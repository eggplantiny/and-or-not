use crate::cell_buffer::TextPanel;
use aon_sim::{
    CommandAcceptance, CommandRejection, DriveVector, DriverSample, EndpointTarget, EntityId,
    FixedAabb, FixedSubstrateRenderRecord, FixedVec2, GatePort, GatePortRef, GateRenderRecord,
    LogicLevel, MainCoreRenderRecord, MobilePortRef, MobileRenderRecord, RenderSnapshot,
    RoutingDomain, SignalArrivalKind, StepReport, Tick, WireEnd, WireRenderRecord,
};

/// A presentation-stable selection namespace.
///
/// This deliberately mirrors the semantic target of a pick instead of retaining a renderer or
/// Bevy entity. Gate-port and Wire-end selections inspect their canonical parent record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InspectorTarget {
    Entity(EntityId),
    GatePort(GatePortRef),
    MobilePort(MobilePortRef),
    WireEnd { wire: aon_sim::WireId, end: WireEnd },
}

impl InspectorTarget {
    pub const fn parent_entity(self) -> EntityId {
        match self {
            Self::Entity(entity) => entity,
            Self::GatePort(port) => port.gate.entity_id(),
            Self::MobilePort(port) => port.mobile.entity_id(),
            Self::WireEnd { wire, .. } => wire.entity_id(),
        }
    }
}

/// Stable identity for a command result retained in a [`StepReport`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandResultKey {
    pub completed_tick: Tick,
    pub ordinal: u64,
}

/// Host-owned selection state supplied to the snapshot-only inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectorSelection {
    pub target: InspectorTarget,
    /// The latest retained command known by the host to concern this selection, if any.
    pub latest_command: Option<CommandResultKey>,
}

/// Stable identity for one raw due Arrival in a completed Tick's preserved drain order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArrivalSelection {
    pub completed_tick: Tick,
    pub observation_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorHostState {
    #[default]
    Paused,
    Running,
    Faulted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorRate {
    Quarter,
    #[default]
    One,
    Four,
}

/// All data needed to build the S0-M6 inspector.
///
/// The input is intentionally limited to owned read projections and host-only values. In
/// particular, it cannot carry a reference to Core's `Simulation` or any of its stores.
#[derive(Clone, Copy, Debug)]
pub struct InspectorInput<'a> {
    pub snapshot: &'a RenderSnapshot,
    pub retained_reports: &'a [StepReport],
    pub host_state: InspectorHostState,
    pub rate: InspectorRate,
    pub selection: Option<InspectorSelection>,
    pub selected_arrival: Option<ArrivalSelection>,
}

pub fn inspector_panel(input: InspectorInput<'_>) -> TextPanel {
    TextPanel::new("Inspector", inspector_lines(input))
}

pub fn inspector_lines(input: InspectorInput<'_>) -> Vec<String> {
    let mut lines = session_lines(input);
    lines.push(String::new());
    append_selection(&mut lines, input);
    lines.push(String::new());
    append_arrival(&mut lines, input);
    lines
}

fn session_lines(input: InspectorInput<'_>) -> Vec<String> {
    let contract = input.snapshot.contract();
    let completed_tick = input.retained_reports.last().map_or_else(
        || "-".to_owned(),
        |report| report.completed_tick.0.to_string(),
    );
    vec![
        format!("session.scenario_id={}", input.snapshot.scenario_id()),
        format!("session.next_tick={}", input.snapshot.next_tick().0),
        format!("session.completed_tick={completed_tick}"),
        format!("session.state_hash={}", input.snapshot.state_hash()),
        format!("session.semantics={}", contract.semantics_version),
        format!(
            "session.numeric_profile_hash={}",
            contract.numeric_profile_hash
        ),
        format!(
            "session.physical_scale_profile_hash={}",
            contract.physical_scale_profile_hash
        ),
        format!(
            "session.balance_profile_hash={}",
            contract.balance_profile_hash
        ),
        format!(
            "session.topology_revision={}",
            input.snapshot.topology_revision().0
        ),
        format!("session.host_state={}", host_state_name(input.host_state)),
        format!("session.rate={}", rate_name(input.rate)),
    ]
}

fn append_selection(lines: &mut Vec<String>, input: InspectorInput<'_>) {
    let Some(selection) = input.selection else {
        lines.push("selection=none".to_owned());
        lines.push("command=none".to_owned());
        return;
    };

    lines.push(format!("selection={}", target_text(selection.target)));
    let entity = selection.target.parent_entity();
    if let Some(core) = input
        .snapshot
        .main_core()
        .filter(|record| record.id.entity_id() == entity)
    {
        append_main_core(lines, core);
    } else if let Some(gate) = input
        .snapshot
        .gates()
        .iter()
        .find(|record| record.id.entity_id() == entity)
    {
        append_gate(lines, gate);
    } else if let Some(wire) = input
        .snapshot
        .wires()
        .iter()
        .find(|record| record.id.entity_id() == entity)
    {
        append_wire(lines, wire);
    } else if let Some(junction) = input
        .snapshot
        .junctions()
        .iter()
        .find(|record| record.id.entity_id() == entity)
    {
        lines.push(format!("junction.id={}", junction.id.entity_id().0));
        lines.push(format!(
            "junction.domain={}",
            routing_domain_text(junction.routing_domain)
        ));
        lines.push(format!(
            "junction.position={}",
            fixed_vec2_text(junction.position)
        ));
        lines.push(format!(
            "junction.connection_generation={}",
            junction.connection_generation.0
        ));
    } else if let Some(substrate) = input
        .snapshot
        .fixed_substrates()
        .iter()
        .find(|record| record.id == entity)
    {
        append_fixed_substrate(lines, substrate);
    } else if let Some(mobile) = input
        .snapshot
        .mobiles()
        .iter()
        .find(|record| record.id.entity_id() == entity)
    {
        append_mobile(lines, mobile);
    } else {
        lines.push("selection.live=false".to_owned());
    }

    append_command(lines, input.retained_reports, selection);
}

fn append_main_core(lines: &mut Vec<String>, core: &MainCoreRenderRecord) {
    lines.push(format!("main_core.id={}", core.id.entity_id().0));
    lines.push(format!(
        "main_core.position={}",
        fixed_vec2_text(core.position)
    ));
    lines.push(format!("main_core.capacity={}", core.capacity.0));
    lines.push(format!("main_core.integrity={}", core.integrity.0));
    lines.push(format!("main_core.heat_energy={}", core.heat_energy.0));
}

fn append_gate(lines: &mut Vec<String>, gate: &GateRenderRecord) {
    lines.push(format!("gate.id={}", gate.id.entity_id().0));
    lines.push(format!("gate.type={}", gate_type_name(gate.gate_type)));
    lines.push(format!("gate.origin={}", fixed_vec2_text(gate.origin)));
    lines.push(format!(
        "gate.domain={}",
        routing_domain_text(gate.routing_domain)
    ));
    lines.push(format!(
        "gate.input_a.sink={}",
        gate.ports.input_a.sink.entity_id().0
    ));
    lines.push(format!(
        "gate.input_a.level={}",
        logic_name(gate.input_a_level)
    ));
    lines.push(format!(
        "gate.input_a.external={}",
        driver_sample_text(gate.input_a_external_sample)
    ));
    match (
        gate.ports.input_b,
        gate.input_b_level,
        gate.input_b_external_sample,
    ) {
        (Some(port), Some(level), Some(sample)) => {
            lines.push(format!("gate.input_b.sink={}", port.sink.entity_id().0));
            lines.push(format!("gate.input_b.level={}", logic_name(level)));
            lines.push(format!(
                "gate.input_b.external={}",
                driver_sample_text(sample)
            ));
        }
        _ => {
            lines.push("gate.input_b.sink=-".to_owned());
            lines.push("gate.input_b.level=-".to_owned());
            lines.push("gate.input_b.external=-".to_owned());
        }
    }
    lines.push(format!(
        "gate.output={}",
        driver_sample_text(gate.output_sample)
    ));
    lines.push(format!(
        "gate.current_output={}",
        logic_name(gate.current_output)
    ));
    lines.push(format!(
        "gate.desired_output={}",
        logic_name(gate.desired_output)
    ));
    lines.push(format!(
        "gate.pending_generation={}",
        gate.pending_generation
    ));
    lines.push(format!(
        "gate.pending_due_tick={}",
        optional_tick_text(gate.pending_due_tick)
    ));
    lines.push(format!(
        "gate.pending_level={}",
        gate.pending_level.map_or("-", logic_name)
    ));
    lines.push(format!(
        "gate.pending_energy={}",
        gate.pending_switch_energy
            .map_or_else(|| "-".to_owned(), |energy| energy.0.to_string())
    ));
    lines.push(format!(
        "gate.cancelled_heat={}",
        gate.cancelled_switching_heat.0
    ));
}

fn append_mobile(lines: &mut Vec<String>, mobile: &MobileRenderRecord) {
    lines.push(format!("mobile.id={}", mobile.id.entity_id().0));
    lines.push(format!("mobile.track_position={:?}", mobile.track_position));
    lines.push(format!(
        "mobile.world_position={}",
        fixed_vec2_text(mobile.world_position)
    ));
    lines.push(format!("mobile.stop={}", logic_name(mobile.stop)));
    lines.push(format!("mobile.left={}", logic_name(mobile.left)));
    lines.push(format!("mobile.right={}", logic_name(mobile.right)));
    lines.push(format!(
        "mobile.sinks={}/{}/{}",
        mobile.ports.stop.entity_id().0,
        mobile.ports.left.entity_id().0,
        mobile.ports.right.entity_id().0
    ));
}

fn append_wire(lines: &mut Vec<String>, wire: &WireRenderRecord) {
    lines.push(format!("wire.id={}", wire.id.entity_id().0));
    lines.push(format!(
        "wire.domain={}",
        routing_domain_text(wire.routing_domain)
    ));
    lines.push(format!("wire.points={}", points_text(&wire.points)));
    lines.push(format!(
        "wire.endpoint_a={}",
        endpoint_text(wire.endpoint_a)
    ));
    lines.push(format!(
        "wire.endpoint_b={}",
        endpoint_text(wire.endpoint_b)
    ));
    lines.push(format!(
        "wire.connection_generation={}",
        wire.connection_generation.0
    ));
    lines.push(format!(
        "wire.active_drive={}",
        drive_vector_text(wire.active_drive)
    ));
    lines.push(format!(
        "wire.previous_drive={}",
        drive_vector_text(wire.previous_drive)
    ));
    lines.push(format!(
        "wire.active_level={}",
        logic_name(wire.active_level)
    ));
    lines.push(format!(
        "wire.previous_level={}",
        logic_name(wire.previous_level)
    ));
}

fn append_fixed_substrate(lines: &mut Vec<String>, substrate: &FixedSubstrateRenderRecord) {
    lines.push(format!("fixed_substrate.id={}", substrate.id.0));
    lines.push(format!(
        "fixed_substrate.origin={}",
        fixed_vec2_text(substrate.origin)
    ));
    lines.push(format!(
        "fixed_substrate.routing_area={}",
        fixed_aabb_text(substrate.routing_area)
    ));
    lines.push(format!(
        "fixed_substrate.footprint={}",
        fixed_aabb_text(substrate.footprint)
    ));
}

fn append_command(lines: &mut Vec<String>, reports: &[StepReport], selection: InspectorSelection) {
    let explicit = selection.latest_command.and_then(|key| {
        reports
            .iter()
            .rev()
            .find(|report| report.completed_tick == key.completed_tick)
            .and_then(|report| find_command_result(report, key.ordinal))
            .map(|result| (key.completed_tick, result))
    });
    let automatic = selection.latest_command.is_none().then(|| {
        reports.iter().rev().find_map(|report| {
            report
                .command_acceptances
                .iter()
                .rev()
                .find(|acceptance| {
                    acceptance.created_entity == Some(selection.target.parent_entity())
                })
                .map(|acceptance| (report.completed_tick, CommandResult::Accepted(acceptance)))
        })
    });
    let result = explicit.or_else(|| automatic.flatten());

    let Some((completed_tick, result)) = result else {
        if let Some(key) = selection.latest_command {
            lines.push(format!(
                "command=not-retained completed_tick={} ordinal={}",
                key.completed_tick.0, key.ordinal
            ));
        } else {
            lines.push("command=none".to_owned());
        }
        return;
    };

    lines.push(format!("command.completed_tick={}", completed_tick.0));
    match result {
        CommandResult::Accepted(acceptance) => append_acceptance(lines, acceptance),
        CommandResult::Rejected(rejection) => append_rejection(lines, rejection),
    }
}

fn append_acceptance(lines: &mut Vec<String>, acceptance: &CommandAcceptance) {
    lines.push("command.outcome=accepted".to_owned());
    lines.push(format!("command.target_tick={}", acceptance.target_tick.0));
    lines.push(format!("command.ordinal={}", acceptance.ordinal));
    lines.push(format!(
        "command.created_entity={}",
        acceptance
            .created_entity
            .map_or_else(|| "-".to_owned(), |entity| entity.0.to_string())
    ));
}

fn append_rejection(lines: &mut Vec<String>, rejection: &CommandRejection) {
    lines.push("command.outcome=rejected".to_owned());
    lines.push(format!("command.target_tick={}", rejection.target_tick.0));
    lines.push(format!("command.ordinal={}", rejection.ordinal));
    lines.push(format!("command.reason={:?}", rejection.reason));
}

fn append_arrival(lines: &mut Vec<String>, input: InspectorInput<'_>) {
    let Some(selection) = input.selected_arrival else {
        lines.push("arrival=none".to_owned());
        return;
    };
    let Some(report) = input
        .retained_reports
        .iter()
        .rev()
        .find(|report| report.completed_tick == selection.completed_tick)
    else {
        lines.push(format!(
            "arrival=not-retained completed_tick={} index={}",
            selection.completed_tick.0, selection.observation_index
        ));
        return;
    };
    let Some(arrival) = report.signal_arrivals.get(selection.observation_index) else {
        lines.push(format!(
            "arrival=not-retained completed_tick={} index={}",
            selection.completed_tick.0, selection.observation_index
        ));
        return;
    };

    lines.push(format!(
        "arrival.completed_tick={}",
        report.completed_tick.0
    ));
    lines.push(format!("arrival.index={}", selection.observation_index));
    lines.push(format!("arrival.due_tick={}", arrival.due_tick.0));
    lines.push(format!("arrival.kind={}", arrival_kind_name(arrival.kind)));
    lines.push(format!(
        "arrival.source_driver={}",
        arrival.source_driver.entity_id().0
    ));
    lines.push(format!("arrival.sink={}", arrival.sink.entity_id().0));
    lines.push(format!(
        "arrival.sample={}",
        driver_sample_text(arrival.sample)
    ));
    lines.push(format!(
        "arrival.counters.applied={}",
        report.signal_counters.signal_arrivals_applied
    ));
    lines.push(format!(
        "arrival.counters.topology_sync_staged={}",
        report.signal_counters.topology_sync_arrivals_staged
    ));
    lines.push(format!(
        "arrival.counters.invalid_path={}",
        report.signal_counters.invalid_path_arrivals
    ));
    lines.push(format!(
        "arrival.counters.stale_revision={}",
        report.signal_counters.stale_revision_arrivals
    ));
    lines.push(format!(
        "arrival.counters.idempotent={}",
        report.signal_counters.idempotent_signal_arrivals
    ));
}

enum CommandResult<'a> {
    Accepted(&'a CommandAcceptance),
    Rejected(&'a CommandRejection),
}

fn find_command_result(report: &StepReport, ordinal: u64) -> Option<CommandResult<'_>> {
    report
        .command_acceptances
        .iter()
        .find(|acceptance| acceptance.ordinal == ordinal)
        .map(CommandResult::Accepted)
        .or_else(|| {
            report
                .command_rejections
                .iter()
                .find(|rejection| rejection.ordinal == ordinal)
                .map(CommandResult::Rejected)
        })
}

fn target_text(target: InspectorTarget) -> String {
    match target {
        InspectorTarget::Entity(entity) => format!("entity:{}", entity.0),
        InspectorTarget::GatePort(port) => format!(
            "gate-port:{}:{}",
            port.gate.entity_id().0,
            gate_port_name(port.port)
        ),
        InspectorTarget::MobilePort(port) => {
            format!("mobile-port:{}:{:?}", port.mobile.entity_id().0, port.port)
        }
        InspectorTarget::WireEnd { wire, end } => {
            format!("wire-end:{}:{}", wire.entity_id().0, wire_end_name(end))
        }
    }
}

fn driver_sample_text(sample: DriverSample) -> String {
    format!(
        "driver:{} level:{} strength:{} revision:{} emitted_at:{}",
        sample.driver_id.entity_id().0,
        logic_name(sample.level),
        sample.strength.0,
        sample.revision.0,
        sample.emitted_at.0
    )
}

fn drive_vector_text(vector: DriveVector) -> String {
    format!(
        "high:{} low:{} unknown:{}",
        vector.high, vector.low, vector.unknown
    )
}

fn points_text(points: &[FixedVec2]) -> String {
    let points = points
        .iter()
        .copied()
        .map(fixed_vec2_text)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{points}]")
}

fn fixed_vec2_text(point: FixedVec2) -> String {
    format!("({},{})", point.x.0, point.y.0)
}

fn fixed_aabb_text(aabb: FixedAabb) -> String {
    format!(
        "min:{} max:{}",
        fixed_vec2_text(aabb.min),
        fixed_vec2_text(aabb.max)
    )
}

fn routing_domain_text(domain: RoutingDomain) -> String {
    match domain {
        RoutingDomain::OpenWorld => "open-world".to_owned(),
        RoutingDomain::FixedSubstrate(entity) => format!("fixed-substrate:{}", entity.0),
        RoutingDomain::MobileSubstrate(entity) => format!("mobile-substrate:{}", entity.0),
    }
}

fn endpoint_text(endpoint: EndpointTarget) -> String {
    match endpoint {
        EndpointTarget::Free => "free".to_owned(),
        EndpointTarget::Junction(junction) => {
            format!("junction:{}", junction.entity_id().0)
        }
        EndpointTarget::GatePort(port) => format!(
            "gate-port:{}:{}",
            port.gate.entity_id().0,
            gate_port_name(port.port)
        ),
        EndpointTarget::MobilePort(port) => {
            format!("mobile-port:{}:{:?}", port.mobile.entity_id().0, port.port)
        }
        EndpointTarget::MainCoreAnchor(core) => {
            format!("main-core-anchor:{}", core.entity_id().0)
        }
        EndpointTarget::PowerSourceAnchor(source) => {
            format!("power-source-anchor:{}", source.entity_id().0)
        }
        EndpointTarget::WireSensePort(port) => {
            format!("wire-sense-port:{}:{:?}", port.wire.entity_id().0, port.end)
        }
    }
}

const fn host_state_name(state: InspectorHostState) -> &'static str {
    match state {
        InspectorHostState::Paused => "PAUSED",
        InspectorHostState::Running => "RUNNING",
        InspectorHostState::Faulted => "FAULTED",
    }
}

const fn rate_name(rate: InspectorRate) -> &'static str {
    match rate {
        InspectorRate::Quarter => "1/4x",
        InspectorRate::One => "1x",
        InspectorRate::Four => "4x",
    }
}

const fn logic_name(level: LogicLevel) -> &'static str {
    match level {
        LogicLevel::Low => "LOW",
        LogicLevel::High => "HIGH",
        LogicLevel::X => "X",
    }
}

const fn gate_type_name(gate_type: aon_sim::GateType) -> &'static str {
    match gate_type {
        aon_sim::GateType::And => "AND",
        aon_sim::GateType::Or => "OR",
        aon_sim::GateType::Not => "NOT",
    }
}

const fn gate_port_name(port: GatePort) -> &'static str {
    match port {
        GatePort::InputA => "input-a",
        GatePort::InputB => "input-b",
        GatePort::Output => "output",
        GatePort::Power => "power",
    }
}

const fn wire_end_name(end: WireEnd) -> &'static str {
    match end {
        WireEnd::A => "a",
        WireEnd::B => "b",
    }
}

const fn arrival_kind_name(kind: SignalArrivalKind) -> &'static str {
    match kind {
        SignalArrivalKind::Propagation => "PROPAGATION",
        SignalArrivalKind::TopologySync => "TOPOLOGY_SYNC",
    }
}

fn optional_tick_text(tick: Option<Tick>) -> String {
    tick.map_or_else(|| "-".to_owned(), |tick| tick.0.to_string())
}
