use crate::editor::{
    EditIntent, EditScopeError, GhostPreview, PendingCommandError, PendingCommands,
};
use crate::host_action::{HostAction, HostActionQueue};
use crate::pacing::{HostRate, HostRunMode, PacingError, TickPacer};
use crate::presenter::{PickTarget, ViewMode};
use crate::probe::{ProbeError, ProbeId, ProbeRack, ProbeTarget};
use aon_sim::{
    Command, CommandEnvelope, DriveStrength, EntityId, RenderSnapshot, Replay, ReplayError,
    Simulation, SimulationError, SimulationPackage, StateHash, StepReport, Tick,
};
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LaboratorySessionId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaboratorySessionMode {
    #[default]
    Interactive,
    ReplayPlayback,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LaboratoryFault {
    #[error("Core simulation fault: {0}")]
    Simulation(SimulationError),

    #[error("Replay execution fault: {0}")]
    Replay(ReplayError),
}

/// Pure host-side owner of one canonical Simulation and all session-scoped
/// observation state. A Bevy FixedUpdate wrapper can own this value as its
/// only mutable Core access point.
pub struct LaboratorySession {
    package: SimulationPackage,
    simulation: Simulation,
    replay: Option<Replay>,
    session_mode: LaboratorySessionMode,
    session_id: LaboratorySessionId,
    pacer: TickPacer,
    pending_commands: PendingCommands,
    edit_log: Vec<CommandEnvelope>,
    probes: ProbeRack,
    latest_snapshot: RenderSnapshot,
    reports: Vec<StepReport>,
    hash_trace: Vec<StateHash>,
    view: ViewMode,
    selection: Option<PickTarget>,
    hover: Option<PickTarget>,
    preview: Option<GhostPreview>,
    fault: Option<LaboratoryFault>,
}

impl LaboratorySession {
    pub fn new(package: SimulationPackage) -> Result<Self, LaboratoryError> {
        let simulation = Simulation::new(package.clone())?;
        Ok(Self::from_validated_parts(
            package,
            simulation,
            None,
            LaboratorySessionMode::Interactive,
        ))
    }

    pub fn from_replay(
        package: SimulationPackage,
        replay: Replay,
    ) -> Result<Self, LaboratoryError> {
        let simulation = Simulation::new(package.clone())?;
        replay.validate_against(&simulation)?;
        Ok(Self::from_validated_parts(
            package,
            simulation,
            Some(replay),
            LaboratorySessionMode::ReplayPlayback,
        ))
    }

    fn from_validated_parts(
        package: SimulationPackage,
        simulation: Simulation,
        replay: Option<Replay>,
        session_mode: LaboratorySessionMode,
    ) -> Self {
        let initial_hash = simulation.state_hash();
        let mut latest_snapshot = RenderSnapshot::default();
        simulation.write_render_snapshot(&mut latest_snapshot);
        Self {
            package,
            simulation,
            replay,
            session_mode,
            session_id: LaboratorySessionId::default(),
            pacer: TickPacer::default(),
            pending_commands: PendingCommands::default(),
            edit_log: Vec::new(),
            probes: ProbeRack::default(),
            latest_snapshot,
            reports: Vec::new(),
            hash_trace: vec![initial_hash],
            view: ViewMode::Network,
            selection: None,
            hover: None,
            preview: None,
            fault: None,
        }
    }

    pub const fn session_id(&self) -> LaboratorySessionId {
        self.session_id
    }

    pub const fn session_mode(&self) -> LaboratorySessionMode {
        self.session_mode
    }

    pub const fn pacer(&self) -> &TickPacer {
        &self.pacer
    }

    pub fn set_mode(&mut self, mode: HostRunMode) {
        if self.fault.is_some() || (mode == HostRunMode::Running && self.replay_complete()) {
            self.pacer.set_mode(HostRunMode::Paused);
        } else {
            self.pacer.set_mode(mode);
        }
    }

    pub fn set_rate(&mut self, rate: HostRate) {
        self.pacer.set_rate(rate);
    }

    pub fn request_single_step(&mut self) -> Result<(), LaboratoryError> {
        self.ensure_not_faulted()?;
        if self.replay_complete() {
            return Err(LaboratoryError::ReplayComplete);
        }
        Ok(self.pacer.request_single_step()?)
    }

    pub fn queue_edit(&mut self, intent: EditIntent) -> Result<u64, LaboratoryError> {
        self.ensure_editable()?;
        let target_tick = self.simulation.next_tick();
        let command = Command::from(intent);
        let ordinal = self.pending_commands.queue(target_tick, command.clone())?;
        self.edit_log.push(CommandEnvelope {
            target_tick,
            ordinal,
            command,
        });
        Ok(ordinal)
    }

    /// Validates a raw Command at the host boundary before allocating an
    /// ordinal. This ordering is what makes playback/out-of-scope refusal
    /// observationally side-effect free.
    pub fn queue_command(&mut self, command: Command) -> Result<u64, LaboratoryError> {
        self.ensure_editable()?;
        let intent = EditIntent::try_from(command)?;
        self.queue_edit(intent)
    }

    pub fn preview_edit(&self, intent: EditIntent) -> GhostPreview {
        self.pending_commands
            .preview(self.simulation.next_tick(), intent)
    }

    pub fn set_preview_edit(&mut self, intent: EditIntent) -> Result<(), LaboratoryError> {
        self.ensure_editable()?;
        self.preview = Some(self.preview_edit(intent));
        Ok(())
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    pub const fn preview(&self) -> Option<&GhostPreview> {
        self.preview.as_ref()
    }

    pub const fn pending_commands(&self) -> &PendingCommands {
        &self.pending_commands
    }

    pub fn edit_log(&self) -> &[CommandEnvelope] {
        &self.edit_log
    }

    pub fn probes(&self) -> &ProbeRack {
        &self.probes
    }

    pub fn add_probe(&mut self, target: ProbeTarget) -> Result<ProbeId, LaboratoryError> {
        Ok(self.probes.add_validated(&self.simulation, target)?)
    }

    pub fn remove_probe(&mut self, id: ProbeId) -> bool {
        self.probes.remove(id)
    }

    pub fn remove_probe_target(&mut self, target: ProbeTarget) -> Result<(), LaboratoryError> {
        let id = self
            .probes
            .traces()
            .find_map(|(id, trace)| (trace.target() == target).then_some(id))
            .ok_or(ProbeError::UnknownTarget)?;
        self.probes.remove(id);
        Ok(())
    }

    pub fn state_hash(&self) -> StateHash {
        self.latest_snapshot.state_hash()
    }

    pub const fn next_tick(&self) -> Tick {
        self.latest_snapshot.next_tick()
    }

    pub fn nominal_external_drive_strength(&self) -> DriveStrength {
        DriveStrength(self.simulation.profiles().balance().nominal_gate_drive)
    }

    pub const fn latest_snapshot(&self) -> &RenderSnapshot {
        &self.latest_snapshot
    }

    pub fn render_snapshot(&self, output: &mut RenderSnapshot) {
        output.clone_from(&self.latest_snapshot);
    }

    pub fn reports(&self) -> &[StepReport] {
        &self.reports
    }

    pub fn latest_report(&self) -> Option<&StepReport> {
        self.reports.last()
    }

    pub fn hash_trace(&self) -> &[StateHash] {
        &self.hash_trace
    }

    pub const fn view(&self) -> ViewMode {
        self.view
    }

    pub fn set_view(&mut self, view: ViewMode) -> Result<(), LaboratoryError> {
        if let ViewMode::Circuit { substrate } = view
            && !self.substrate_is_live(substrate)
        {
            return Err(LaboratoryError::InvalidCircuitView { substrate });
        }
        self.view = view;
        Ok(())
    }

    pub const fn selection(&self) -> Option<PickTarget> {
        self.selection
    }

    pub const fn set_selection(&mut self, selection: Option<PickTarget>) {
        self.selection = selection;
    }

    pub const fn hover(&self) -> Option<PickTarget> {
        self.hover
    }

    pub const fn set_hover(&mut self, hover: Option<PickTarget>) {
        self.hover = hover;
    }

    pub const fn fault(&self) -> Option<&LaboratoryFault> {
        self.fault.as_ref()
    }

    pub const fn is_faulted(&self) -> bool {
        self.fault.is_some()
    }

    pub fn replay_final_next_tick(&self) -> Option<Tick> {
        self.replay.as_ref().map(Replay::final_next_tick)
    }

    pub fn apply_host_action(&mut self, action: HostAction) -> Result<(), LaboratoryError> {
        match action {
            HostAction::Pause => self.set_mode(HostRunMode::Paused),
            HostAction::Resume => {
                self.ensure_not_faulted()?;
                self.set_mode(HostRunMode::Running);
            }
            HostAction::SetRate(rate) => self.set_rate(rate),
            HostAction::SingleStep => self.request_single_step()?,
            HostAction::Reset => self.reset()?,
            HostAction::QueueEdit(command) => {
                self.queue_command(command)?;
                self.clear_preview();
            }
            HostAction::SetView(view) => self.set_view(view)?,
            HostAction::Select(target) => self.set_selection(Some(target)),
            HostAction::ClearSelection => self.set_selection(None),
            HostAction::AddProbe(target) => {
                self.add_probe(target)?;
            }
            HostAction::RemoveProbe(target) => self.remove_probe_target(target)?,
            HostAction::ClearPreview => self.clear_preview(),
        }
        Ok(())
    }

    /// Drains every queued action, preserving insertion order even when an
    /// earlier action is rejected. Each result corresponds to exactly one
    /// consumed action.
    pub fn drain_host_actions(
        &mut self,
        actions: &mut HostActionQueue,
    ) -> Vec<Result<(), LaboratoryError>> {
        actions
            .drain()
            .map(|action| self.apply_host_action(action))
            .collect()
    }

    pub fn advance_frame(&mut self, elapsed: Duration) -> Result<Vec<StepReport>, LaboratoryError> {
        self.ensure_not_faulted()?;
        if self.replay_complete() {
            self.pacer.set_mode(HostRunMode::Paused);
            return Ok(Vec::new());
        }

        let simulation_hz = self.simulation.profiles().balance().simulation_hz;
        let ticks_due = self.pacer.ticks_due(elapsed, simulation_hz)?;
        usize::try_from(ticks_due).map_err(|_| LaboratoryError::FrameReportCapacityExceeded)?;
        let mut reports = Vec::new();
        for _ in 0..ticks_due {
            if self.replay_complete() {
                self.pacer.set_mode(HostRunMode::Paused);
                break;
            }
            reports.push(self.step_once()?);
        }
        Ok(reports)
    }

    pub fn step_once(&mut self) -> Result<StepReport, LaboratoryError> {
        self.ensure_not_faulted()?;
        if self.replay_complete() {
            self.pacer.set_mode(HostRunMode::Paused);
            return Err(LaboratoryError::ReplayComplete);
        }

        let tick = self.simulation.next_tick();
        let commands = match &self.replay {
            Some(replay) => replay.commands_for_tick(tick).cloned().collect::<Vec<_>>(),
            None => self.pending_commands.commands_for_tick(tick),
        };
        let report = match self.simulation.step(&commands) {
            Ok(report) => report,
            Err(error) => {
                return Err(self.enter_fault(LaboratoryFault::Simulation(error)));
            }
        };

        if self.session_mode == LaboratorySessionMode::Interactive {
            self.pending_commands.discard_tick(tick);
        }
        self.probes.record_step(&self.simulation, &report);
        self.hash_trace.push(report.state_hash);
        self.reports.push(report.clone());
        self.simulation
            .write_render_snapshot(&mut self.latest_snapshot);
        self.normalize_view_after_snapshot();

        if let Some(expected) = self.replay_checkpoint(report.next_tick)
            && expected != report.state_hash
        {
            return Err(self.enter_fault(LaboratoryFault::Replay(
                ReplayError::CheckpointDivergence {
                    next_tick: report.next_tick,
                    expected,
                    actual: report.state_hash,
                },
            )));
        }

        if self.replay_complete() {
            let verification = self
                .replay
                .as_ref()
                .expect("ReplayPlayback always retains a Replay")
                .verify_trace(&self.hash_trace);
            if let Err(error) = verification {
                return Err(self.enter_fault(LaboratoryFault::Replay(error)));
            }
            self.pacer.set_mode(HostRunMode::Paused);
        }
        Ok(report)
    }

    pub fn reset(&mut self) -> Result<(), LaboratoryError> {
        let next_session_id = self
            .session_id
            .0
            .checked_add(1)
            .map(LaboratorySessionId)
            .ok_or(LaboratoryError::SessionIdExhausted)?;
        let simulation = Simulation::new(self.package.clone())?;
        if let Some(replay) = &self.replay {
            replay.validate_against(&simulation)?;
        }
        let initial_hash = simulation.state_hash();
        let mut latest_snapshot = RenderSnapshot::default();
        simulation.write_render_snapshot(&mut latest_snapshot);

        self.simulation = simulation;
        self.session_id = next_session_id;
        self.pacer.reset();
        self.pending_commands.clear();
        self.edit_log.clear();
        self.probes.clear();
        self.latest_snapshot = latest_snapshot;
        self.reports.clear();
        self.hash_trace.clear();
        self.hash_trace.push(initial_hash);
        self.view = ViewMode::Network;
        self.selection = None;
        self.hover = None;
        self.preview = None;
        self.fault = None;
        Ok(())
    }

    fn ensure_not_faulted(&self) -> Result<(), LaboratoryError> {
        if let Some(fault) = &self.fault {
            Err(LaboratoryError::SessionFaulted {
                fault: fault.clone(),
            })
        } else {
            Ok(())
        }
    }

    fn ensure_editable(&self) -> Result<(), LaboratoryError> {
        if self.session_mode == LaboratorySessionMode::ReplayPlayback {
            return Err(LaboratoryError::PlaybackReadOnly);
        }
        self.ensure_not_faulted()
    }

    fn replay_complete(&self) -> bool {
        self.replay
            .as_ref()
            .is_some_and(|replay| self.simulation.next_tick() >= replay.final_next_tick())
    }

    fn replay_checkpoint(&self, next_tick: Tick) -> Option<StateHash> {
        let replay = self.replay.as_ref()?;
        replay
            .checkpoints()
            .binary_search_by_key(&next_tick, |checkpoint| checkpoint.next_tick)
            .ok()
            .map(|index| replay.checkpoints()[index].state_hash)
    }

    fn enter_fault(&mut self, fault: LaboratoryFault) -> LaboratoryError {
        self.pacer.set_mode(HostRunMode::Paused);
        self.fault = Some(fault.clone());
        LaboratoryError::Fatal(fault)
    }

    fn substrate_is_live(&self, substrate: EntityId) -> bool {
        self.latest_snapshot
            .fixed_substrates()
            .binary_search_by_key(&substrate, |record| record.id)
            .is_ok()
            || self
                .latest_snapshot
                .mobiles()
                .binary_search_by_key(&substrate, |record| record.id.entity_id())
                .is_ok()
    }

    fn normalize_view_after_snapshot(&mut self) {
        let ViewMode::Circuit { substrate } = self.view else {
            return;
        };
        if !self.substrate_is_live(substrate) {
            self.view = ViewMode::Network;
            self.selection = None;
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LaboratoryError {
    #[error(transparent)]
    Simulation(#[from] SimulationError),

    #[error(transparent)]
    Replay(#[from] ReplayError),

    #[error(transparent)]
    Pacing(#[from] PacingError),

    #[error(transparent)]
    PendingCommand(#[from] PendingCommandError),

    #[error(transparent)]
    EditScope(#[from] EditScopeError),

    #[error(transparent)]
    Probe(#[from] ProbeError),

    #[error(transparent)]
    Fatal(LaboratoryFault),

    #[error("Laboratory is Faulted/Paused: {fault}")]
    SessionFaulted { fault: LaboratoryFault },

    #[error("Replay playback is read-only")]
    PlaybackReadOnly,

    #[error("Replay playback is already at its final nextTick")]
    ReplayComplete,

    #[error("Circuit View substrate {substrate:?} is unknown or removed")]
    InvalidCircuitView { substrate: EntityId },

    #[error("Laboratory replay-session identity space is exhausted")]
    SessionIdExhausted,

    #[error("host cannot allocate one StepReport slot per Tick due in this frame")]
    FrameReportCapacityExceeded,
}
