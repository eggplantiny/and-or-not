use crate::contract::{ContractValidationError, SimulationContract};
use crate::profile::{ProfileBundle, ProfileValidationError};
use crate::{
    EntityRegistry, InitialWorld, RenderSnapshot, Revision, ScenarioManifest, SimulationError,
    StageFeatureSet, StateHash, Tick, canonical,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationPackage {
    scenario_id: String,
    initial_world: InitialWorld,
    required_features: StageFeatureSet,
    contract: SimulationContract,
    profiles: ProfileBundle,
}

impl SimulationPackage {
    pub fn new(
        scenario_id: impl Into<String>,
        initial_world: InitialWorld,
        required_features: StageFeatureSet,
        contract: SimulationContract,
        profiles: ProfileBundle,
    ) -> Self {
        Self {
            scenario_id: scenario_id.into(),
            initial_world,
            required_features,
            contract,
            profiles,
        }
    }

    pub(crate) fn from_artifacts(scenario: ScenarioManifest, profiles: ProfileBundle) -> Self {
        let contract = SimulationContract {
            semantics_version: scenario.semantics_version(),
            numeric_profile_hash: scenario.profiles().numeric().profile_hash(),
            physical_scale_profile_hash: scenario.profiles().physical_scale().profile_hash(),
            balance_profile_hash: scenario.profiles().balance().profile_hash(),
        };
        Self::new(
            scenario.scenario_id(),
            scenario.initial_world().clone(),
            scenario.required_features(),
            contract,
            profiles,
        )
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.contract
    }

    pub const fn semantics_version(&self) -> crate::SemanticsVersion {
        self.contract.semantics_version
    }

    pub const fn profiles(&self) -> &ProfileBundle {
        &self.profiles
    }

    pub const fn required_features(&self) -> StageFeatureSet {
        self.required_features
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandEnvelope {
    _s0_private: (),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepReport {
    pub completed_tick: Tick,
    pub next_tick: Tick,
    pub state_hash: StateHash,
}

struct CanonicalWorld {
    next_tick: Tick,
    topology_revision: Revision,
    contract: SimulationContract,
    entities: EntityRegistry,
}

pub struct Simulation {
    scenario_id: String,
    canonical: CanonicalWorld,
    profiles: ProfileBundle,
}

impl Simulation {
    pub fn new(package: SimulationPackage) -> Result<Self, SimulationError> {
        if let Some(feature) = package.required_features.first_enabled() {
            return Err(SimulationError::UnsupportedStageFeature { feature });
        }

        package.profiles.validate().map_err(SimulationError::from)?;
        package
            .contract
            .validate_profiles(&package.profiles)
            .map_err(SimulationError::from)?;

        let entities = match package.initial_world {
            InitialWorld::Empty => EntityRegistry::new(),
        };

        Ok(Self {
            scenario_id: package.scenario_id,
            canonical: CanonicalWorld {
                next_tick: Tick(0),
                topology_revision: Revision(0),
                contract: package.contract,
                entities,
            },
            profiles: package.profiles,
        })
    }

    pub fn step(&mut self, commands: &[CommandEnvelope]) -> Result<StepReport, SimulationError> {
        if !commands.is_empty() {
            return Err(SimulationError::CommandsUnsupported);
        }

        let completed_tick = self.canonical.next_tick;
        self.canonical.next_tick = completed_tick.checked_add(Tick(1))?;
        let state_hash = self.state_hash();

        Ok(StepReport {
            completed_tick,
            next_tick: self.canonical.next_tick,
            state_hash,
        })
    }

    pub fn write_render_snapshot(&self, output: &mut RenderSnapshot) {
        output.write_empty(
            &self.scenario_id,
            self.canonical.next_tick,
            self.state_hash(),
        );
    }

    pub fn state_hash(&self) -> StateHash {
        canonical::state_hash(
            &self.canonical.contract,
            self.canonical.next_tick,
            self.canonical.topology_revision,
            &self.canonical.entities,
        )
    }

    pub const fn next_tick(&self) -> Tick {
        self.canonical.next_tick
    }

    pub const fn topology_revision(&self) -> Revision {
        self.canonical.topology_revision
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.canonical.contract
    }

    pub const fn profiles(&self) -> &ProfileBundle {
        &self.profiles
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }
}

impl From<ProfileValidationError> for SimulationError {
    fn from(error: ProfileValidationError) -> Self {
        Self::InvalidProfile { error }
    }
}

impl From<ContractValidationError> for SimulationError {
    fn from(error: ContractValidationError) -> Self {
        match error {
            ContractValidationError::Profile(error) => Self::InvalidProfile { error },
            ContractValidationError::ProfileHashMismatch {
                profile,
                expected,
                actual,
            } => Self::ProfileHashMismatch {
                profile,
                expected,
                actual,
            },
            ContractValidationError::UnsupportedSemanticsVersion { actual } => {
                Self::UnsupportedSemanticsVersion { actual }
            }
            ContractValidationError::UnsupportedHashAlgorithm { actual } => {
                Self::UnsupportedHashAlgorithm { actual }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{BalanceProfile, NumericProfile, PhysicalScaleProfile};
    use crate::{ProfileKind, SemanticsVersion};

    fn package() -> SimulationPackage {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical"),
            balance: BalanceProfile::stage0_alpha("balance"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        SimulationPackage::new(
            "empty",
            InitialWorld::Empty,
            StageFeatureSet::none(),
            contract,
            profiles,
        )
    }

    #[test]
    fn tick_overflow_is_typed_and_does_not_wrap() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        simulation.canonical.next_tick = Tick(u64::MAX);

        assert_eq!(simulation.step(&[]), Err(SimulationError::NumericOverflow));
        assert_eq!(simulation.next_tick(), Tick(u64::MAX));
    }

    #[test]
    fn contract_hash_mismatch_rejects_simulation_start() {
        let mut package = package();
        package.contract.balance_profile_hash = crate::ProfileHash::default();

        assert!(matches!(
            Simulation::new(package),
            Err(SimulationError::ProfileHashMismatch {
                profile: ProfileKind::Balance,
                ..
            })
        ));
    }

    #[test]
    fn unsupported_stage_feature_rejects_simulation_start() {
        let mut package = package();
        package.required_features.signal = true;

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::UnsupportedStageFeature { feature: "signal" })
        );
    }

    #[test]
    fn valid_simulation_exposes_immutable_contract_and_profiles() {
        let simulation = Simulation::new(package()).expect("test package is valid");

        assert_eq!(
            simulation.contract().semantics_version,
            SemanticsVersion::AonV1
        );
        assert_eq!(simulation.profiles().numeric.fixed_one, crate::FIXED_ONE);
        assert_eq!(simulation.topology_revision(), Revision(0));
    }
}
