use crate::contract::{ContractValidationError, SimulationContract};
use crate::profile::{ProfileBundle, ProfileValidationError};
use crate::structural::{StructuralError, StructuralWorld};
use crate::{
    CommandAcceptance, CommandEnvelope, CommandRejection, InitialWorld, RenderSnapshot, Revision,
    ScenarioManifest, SimulationError, StageFeatureSet, StateHash, Tick, canonical,
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
pub struct StepReport {
    pub completed_tick: Tick,
    pub next_tick: Tick,
    pub state_hash: StateHash,
    pub command_acceptances: Vec<CommandAcceptance>,
    pub command_rejections: Vec<CommandRejection>,
    pub topology_changed: bool,
}

struct CanonicalWorld {
    next_tick: Tick,
    topology_revision: Revision,
    contract: SimulationContract,
    structural: StructuralWorld,
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

        let structural = match package.initial_world {
            InitialWorld::Empty => StructuralWorld::new(),
        };

        Ok(Self {
            scenario_id: package.scenario_id,
            canonical: CanonicalWorld {
                next_tick: Tick(0),
                topology_revision: Revision(0),
                contract: package.contract,
                structural,
            },
            profiles: package.profiles,
        })
    }

    pub fn step(&mut self, commands: &[CommandEnvelope]) -> Result<StepReport, SimulationError> {
        let completed_tick = self.canonical.next_tick;
        let next_tick = completed_tick.checked_add(Tick(1))?;
        let mut structural = self.canonical.structural.clone();
        let phase =
            structural.apply_phase0(completed_tick, commands, &self.profiles.physical_scale)?;
        let topology_revision = if phase.topology_changed {
            self.canonical.topology_revision.checked_add(Revision(1))?
        } else {
            self.canonical.topology_revision
        };

        self.canonical.structural = structural;
        self.canonical.topology_revision = topology_revision;
        self.canonical.next_tick = next_tick;
        let state_hash = self.state_hash();

        Ok(StepReport {
            completed_tick,
            next_tick,
            state_hash,
            command_acceptances: phase.acceptances,
            command_rejections: phase.rejections,
            topology_changed: phase.topology_changed,
        })
    }

    pub fn write_render_snapshot(&self, output: &mut RenderSnapshot) {
        output.write(
            &self.scenario_id,
            self.canonical.next_tick,
            self.canonical.structural.live_primitive_count(),
            self.state_hash(),
        );
    }

    pub fn state_hash(&self) -> StateHash {
        canonical::state_hash(
            &self.canonical.contract,
            self.canonical.next_tick,
            self.canonical.topology_revision,
            &self.canonical.structural,
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

impl From<StructuralError> for SimulationError {
    fn from(error: StructuralError) -> Self {
        match error {
            StructuralError::NumericOverflow => Self::NumericOverflow,
            StructuralError::InvalidCanonicalState => Self::InvalidCanonicalState,
        }
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
        let before_hash = simulation.state_hash();

        assert_eq!(simulation.step(&[]), Err(SimulationError::NumericOverflow));
        assert_eq!(simulation.next_tick(), Tick(u64::MAX));
        assert_eq!(simulation.state_hash(), before_hash);
    }

    #[test]
    fn topology_revision_overflow_rolls_back_tick_and_structural_changes() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        simulation.canonical.topology_revision = Revision(u64::MAX);
        let before_hash = simulation.state_hash();
        let command = crate::CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: crate::Command::PlaceJunction(crate::PlaceJunctionCommand {
                routing_domain: crate::RoutingDomain::OpenWorld,
                position: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
            }),
        };

        assert_eq!(
            simulation.step(&[command]),
            Err(SimulationError::NumericOverflow)
        );
        assert_eq!(simulation.next_tick(), Tick(0));
        assert_eq!(simulation.topology_revision(), Revision(u64::MAX));
        assert_eq!(simulation.state_hash(), before_hash);
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
