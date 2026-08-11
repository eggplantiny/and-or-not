use crate::{
    InitialWorld, ProfileBundle, ProfileHash, RenderSnapshot, ScenarioManifest, SimulationError,
    StateHash, canonical,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationPackage {
    scenario_id: String,
    semantics_version: String,
    initial_world: InitialWorld,
    profiles: ProfileBundle,
}

impl SimulationPackage {
    pub(crate) fn from_bootstrap_artifacts(
        scenario: ScenarioManifest,
        profiles: ProfileBundle,
    ) -> Self {
        let initial_world = scenario.initial_world().clone();

        Self {
            scenario_id: scenario.scenario_id().to_owned(),
            semantics_version: scenario.semantics_version().to_owned(),
            initial_world,
            profiles,
        }
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub fn semantics_version(&self) -> &str {
        &self.semantics_version
    }

    pub fn profiles(&self) -> &ProfileBundle {
        &self.profiles
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandEnvelope {
    _bootstrap_private: (),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepReport {
    pub completed_tick: u64,
    pub next_tick: u64,
    pub state_hash: StateHash,
}

pub struct Simulation {
    scenario_id: String,
    semantics_version: String,
    initial_world: InitialWorld,
    numeric_profile_hash: ProfileHash,
    physical_scale_profile_hash: ProfileHash,
    balance_profile_hash: ProfileHash,
    next_tick: u64,
}

impl Simulation {
    pub fn new(package: SimulationPackage) -> Result<Self, SimulationError> {
        let numeric_profile_hash = package.profiles.numeric().canonical_hash();
        let physical_scale_profile_hash = package.profiles.physical_scale().canonical_hash();
        let balance_profile_hash = package.profiles.balance().canonical_hash();

        Ok(Self {
            scenario_id: package.scenario_id,
            semantics_version: package.semantics_version,
            initial_world: package.initial_world,
            numeric_profile_hash,
            physical_scale_profile_hash,
            balance_profile_hash,
            next_tick: 0,
        })
    }

    pub fn step(&mut self, commands: &[CommandEnvelope]) -> Result<StepReport, SimulationError> {
        if !commands.is_empty() {
            return Err(SimulationError::CommandsUnsupported);
        }

        let completed_tick = self.next_tick;
        self.next_tick = completed_tick
            .checked_add(1)
            .ok_or(SimulationError::TickOverflow)?;
        let state_hash = self.state_hash();

        Ok(StepReport {
            completed_tick,
            next_tick: self.next_tick,
            state_hash,
        })
    }

    pub fn write_render_snapshot(&self, output: &mut RenderSnapshot) {
        output.write_empty(&self.scenario_id, self.next_tick, self.state_hash());
    }

    pub fn state_hash(&self) -> StateHash {
        canonical::state_hash(
            &self.semantics_version,
            self.numeric_profile_hash,
            self.physical_scale_profile_hash,
            self.balance_profile_hash,
            &self.initial_world,
            self.next_tick,
        )
    }

    pub const fn next_tick(&self) -> u64 {
        self.next_tick
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }
}

#[cfg(test)]
mod tests {
    use super::Simulation;
    use crate::{ArtifactBytes, SimulationError, decode_package};

    const SCENARIO: &[u8] = br#"{
        "schemaVersion": 0,
        "scenarioId": "empty",
        "semanticsVersion": "bootstrap-v0",
        "initialWorld": { "kind": "empty" },
        "profiles": {
            "numeric": { "path": "numeric.json", "profileId": "n" },
            "physicalScale": { "path": "physical.json", "profileId": "p" },
            "balance": { "path": "balance.json", "profileId": "b" }
        }
    }"#;
    const NUMERIC: &[u8] = br#"{"schemaVersion":0,"profileId":"n","kind":"numeric"}"#;
    const PHYSICAL: &[u8] = br#"{"schemaVersion":0,"profileId":"p","kind":"physical-scale"}"#;
    const BALANCE: &[u8] = br#"{"schemaVersion":0,"profileId":"b","kind":"balance"}"#;

    fn simulation() -> Simulation {
        let package = decode_package(ArtifactBytes {
            scenario: SCENARIO,
            numeric_profile: NUMERIC,
            physical_scale_profile: PHYSICAL,
            balance_profile: BALANCE,
        })
        .expect("test package is valid");
        Simulation::new(package).expect("test simulation is valid")
    }

    #[test]
    fn tick_overflow_is_typed_and_does_not_wrap() {
        let mut simulation = simulation();
        simulation.next_tick = u64::MAX;

        assert_eq!(simulation.step(&[]), Err(SimulationError::TickOverflow));
        assert_eq!(simulation.next_tick(), u64::MAX);
    }
}
