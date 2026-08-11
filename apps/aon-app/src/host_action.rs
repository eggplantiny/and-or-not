use crate::pacing::HostRate;
use crate::presenter::{PickTarget, ViewMode};
use aon_sim::{Command, SignalProbeTarget};
use std::collections::VecDeque;

/// Ordered, presentation-originated intent. Raw platform input never crosses
/// the host/Core boundary directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    Pause,
    Resume,
    SetRate(HostRate),
    SingleStep,
    Reset,
    QueueEdit(Command),
    SetView(ViewMode),
    Select(PickTarget),
    ClearSelection,
    AddProbe(SignalProbeTarget),
    RemoveProbe(SignalProbeTarget),
    ClearPreview,
}

/// Stable FIFO used between input collection and the single-owner host step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostActionQueue {
    actions: VecDeque<HostAction>,
}

impl HostActionQueue {
    pub fn push(&mut self, action: HostAction) {
        self.actions.push_back(action);
    }

    pub fn pop_front(&mut self) -> Option<HostAction> {
        self.actions.pop_front()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = HostAction> + '_ {
        self.actions.drain(..)
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn clear(&mut self) {
        self.actions.clear();
    }
}
