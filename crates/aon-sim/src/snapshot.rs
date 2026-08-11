use crate::{StateHash, Tick};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderSnapshot {
    scenario_id: String,
    next_tick: Tick,
    primitive_count: u64,
    state_hash: StateHash,
}

impl RenderSnapshot {
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn next_tick(&self) -> Tick {
        self.next_tick
    }

    pub const fn primitive_count(&self) -> u64 {
        self.primitive_count
    }

    pub const fn state_hash(&self) -> StateHash {
        self.state_hash
    }

    pub(crate) fn write(
        &mut self,
        scenario_id: &str,
        next_tick: Tick,
        primitive_count: u64,
        state_hash: StateHash,
    ) {
        self.scenario_id.clear();
        self.scenario_id.push_str(scenario_id);
        self.next_tick = next_tick;
        self.primitive_count = primitive_count;
        self.state_hash = state_hash;
    }
}
