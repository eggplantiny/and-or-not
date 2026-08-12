use crate::mobility::TrackGraph;
use crate::signal::{SignalWorld, resolve_drive};
use crate::structural::StructuralWorld;
use crate::{
    Capacity, ConnectionGeneration, DriveVector, DriverId, DriverSample, EndpointTarget, EntityId,
    FixedAabb, FixedVec2, GateId, GateSignalPorts, GateType, HeatEnergy, Integrity, JunctionId,
    LogicLevel, MainCoreId, MobileControlPorts, MobileId, Revision, RoutingDomain,
    SimulationContract, SinkId, StateHash, Tick, TrackPosition, WireId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MainCoreRenderRecord {
    pub id: MainCoreId,
    pub position: FixedVec2,
    pub capacity: Capacity,
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedSubstrateRenderRecord {
    pub id: EntityId,
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileRenderRecord {
    pub id: MobileId,
    pub track_position: TrackPosition,
    pub world_position: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
    pub ports: MobileControlPorts,
    pub stop: LogicLevel,
    pub left: LogicLevel,
    pub right: LogicLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateRenderRecord {
    pub id: GateId,
    pub gate_type: GateType,
    pub origin: FixedVec2,
    pub routing_domain: RoutingDomain,
    pub ports: GateSignalPorts,
    pub input_a_level: LogicLevel,
    pub input_b_level: Option<LogicLevel>,
    pub input_a_external_sample: DriverSample,
    pub input_b_external_sample: Option<DriverSample>,
    pub output_sample: DriverSample,
    pub current_output: LogicLevel,
    pub desired_output: LogicLevel,
    pub pending_generation: u32,
    pub pending_due_tick: Option<Tick>,
    pub pending_level: Option<LogicLevel>,
    pub pending_switch_energy: Option<crate::Energy>,
    pub cancelled_switching_heat: crate::HeatEnergy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireRenderRecord {
    pub id: WireId,
    pub routing_domain: RoutingDomain,
    pub points: Vec<FixedVec2>,
    pub endpoint_a: EndpointTarget,
    pub endpoint_b: EndpointTarget,
    pub connection_generation: ConnectionGeneration,
    pub active_drive: DriveVector,
    pub previous_drive: DriveVector,
    pub active_level: LogicLevel,
    pub previous_level: LogicLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JunctionRenderRecord {
    pub id: JunctionId,
    pub routing_domain: RoutingDomain,
    pub position: FixedVec2,
    pub connection_generation: ConnectionGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SignalProbeTarget {
    Driver(DriverId),
    Sink(SinkId),
    GateInputA(GateId),
    GateInputB(GateId),
    GateOutput(GateId),
    Wire(WireId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalProbeValue {
    Driver(DriverSample),
    Sink {
        sink: SinkId,
        level: LogicLevel,
    },
    Wire {
        active_drive: DriveVector,
        previous_drive: DriveVector,
        active_level: LogicLevel,
        previous_level: LogicLevel,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalProbeSample {
    pub target: SignalProbeTarget,
    pub next_tick: Tick,
    pub value: SignalProbeValue,
}

pub(crate) struct RenderSnapshotSource<'a> {
    pub scenario_id: &'a str,
    pub next_tick: Tick,
    pub topology_revision: Revision,
    pub contract: SimulationContract,
    pub state_hash: StateHash,
    pub main_core: Option<&'a crate::MainCoreState>,
    pub structural: &'a StructuralWorld,
    pub signal: &'a SignalWorld,
    pub logic_threshold: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderSnapshot {
    scenario_id: String,
    next_tick: Tick,
    topology_revision: Revision,
    contract: SimulationContract,
    primitive_count: u64,
    state_hash: StateHash,
    main_core: Option<MainCoreRenderRecord>,
    fixed_substrates: Vec<FixedSubstrateRenderRecord>,
    mobiles: Vec<MobileRenderRecord>,
    gates: Vec<GateRenderRecord>,
    wires: Vec<WireRenderRecord>,
    junctions: Vec<JunctionRenderRecord>,
}

impl Default for RenderSnapshot {
    fn default() -> Self {
        Self {
            scenario_id: String::new(),
            next_tick: Tick::default(),
            topology_revision: Revision::default(),
            contract: SimulationContract {
                semantics_version: crate::SemanticsVersion::default(),
                numeric_profile_hash: crate::ProfileHash::default(),
                physical_scale_profile_hash: crate::ProfileHash::default(),
                balance_profile_hash: crate::ProfileHash::default(),
            },
            primitive_count: 0,
            state_hash: StateHash::default(),
            main_core: None,
            fixed_substrates: Vec::new(),
            mobiles: Vec::new(),
            gates: Vec::new(),
            wires: Vec::new(),
            junctions: Vec::new(),
        }
    }
}

impl RenderSnapshot {
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn next_tick(&self) -> Tick {
        self.next_tick
    }

    pub const fn topology_revision(&self) -> Revision {
        self.topology_revision
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.contract
    }

    pub const fn primitive_count(&self) -> u64 {
        self.primitive_count
    }

    pub const fn state_hash(&self) -> StateHash {
        self.state_hash
    }

    pub const fn main_core(&self) -> Option<&MainCoreRenderRecord> {
        self.main_core.as_ref()
    }

    pub fn fixed_substrates(&self) -> &[FixedSubstrateRenderRecord] {
        &self.fixed_substrates
    }

    pub fn mobiles(&self) -> &[MobileRenderRecord] {
        &self.mobiles
    }

    pub fn gates(&self) -> &[GateRenderRecord] {
        &self.gates
    }

    pub fn wires(&self) -> &[WireRenderRecord] {
        &self.wires
    }

    pub fn junctions(&self) -> &[JunctionRenderRecord] {
        &self.junctions
    }

    pub(crate) fn write(&mut self, source: RenderSnapshotSource<'_>) {
        let RenderSnapshotSource {
            scenario_id,
            next_tick,
            topology_revision,
            contract,
            state_hash,
            main_core,
            structural,
            signal,
            logic_threshold,
        } = source;
        self.scenario_id.clear();
        self.scenario_id.push_str(scenario_id);
        self.next_tick = next_tick;
        self.topology_revision = topology_revision;
        self.contract = contract;
        self.primitive_count = structural.live_primitive_count() + u64::from(main_core.is_some());
        self.state_hash = state_hash;
        self.main_core = main_core.map(|core| MainCoreRenderRecord {
            id: core.id(),
            position: core.position(),
            capacity: core.capacity(),
            integrity: core.integrity(),
            heat_energy: core.heat_energy(),
        });

        self.fixed_substrates.clear();
        self.fixed_substrates
            .extend(
                structural
                    .fixed_substrates()
                    .iter_alive()
                    .map(|(_, record)| FixedSubstrateRenderRecord {
                        id: record.id,
                        origin: record.origin,
                        routing_area: record.routing_area,
                        footprint: record.footprint,
                    }),
            );
        self.fixed_substrates
            .sort_unstable_by_key(|record| record.id);

        let track = TrackGraph::compile(structural.wires(), structural.junctions())
            .expect("validated canonical world has a valid Track Graph");
        self.mobiles.clear();
        self.mobiles.extend(
            structural
                .mobile_substrates()
                .iter_alive()
                .map(|(_, record)| {
                    let ports = signal
                        .mobile_ports(record.id)
                        .expect("validated canonical Mobile has control ports");
                    MobileRenderRecord {
                        id: record.id,
                        track_position: record.track_position,
                        world_position: track
                            .world_position(record.track_position)
                            .expect("validated canonical TrackPosition projects"),
                        routing_area: record.routing_area,
                        footprint: record.footprint,
                        ports,
                        stop: signal
                            .sink_level(ports.stop)
                            .expect("validated canonical Mobile STOP Sink exists"),
                        left: signal
                            .sink_level(ports.left)
                            .expect("validated canonical Mobile LEFT Sink exists"),
                        right: signal
                            .sink_level(ports.right)
                            .expect("validated canonical Mobile RIGHT Sink exists"),
                    }
                }),
        );
        self.mobiles
            .sort_unstable_by_key(|record| record.id.entity_id());

        self.gates.clear();
        self.gates
            .extend(structural.gates().iter_alive().map(|(_, record)| {
                let gate_signal = signal
                    .gate_snapshot(record.id)
                    .expect("validated canonical Gate has signal state");
                let input_a_level = signal
                    .sink_level(gate_signal.ports.input_a.sink)
                    .expect("validated canonical Gate input A has a live Sink");
                let input_b_level = gate_signal.ports.input_b.map(|port| {
                    signal
                        .sink_level(port.sink)
                        .expect("validated canonical Gate input B has a live Sink")
                });
                let input_a_external_sample = signal
                    .driver_sample(gate_signal.ports.input_a.external_driver)
                    .expect("validated canonical Gate input A has a live external Driver");
                let input_b_external_sample = gate_signal.ports.input_b.map(|port| {
                    signal
                        .driver_sample(port.external_driver)
                        .expect("validated canonical Gate input B has a live external Driver")
                });
                let output_sample = signal
                    .driver_sample(gate_signal.ports.output)
                    .expect("validated canonical Gate output has a live Driver");
                GateRenderRecord {
                    id: record.id,
                    gate_type: record.gate_type,
                    origin: record.origin,
                    routing_domain: record.routing_domain,
                    ports: gate_signal.ports,
                    input_a_level,
                    input_b_level,
                    input_a_external_sample,
                    input_b_external_sample,
                    output_sample,
                    current_output: gate_signal.current_output,
                    desired_output: gate_signal.desired_output,
                    pending_generation: gate_signal.pending_generation,
                    pending_due_tick: gate_signal.pending_due_tick,
                    pending_level: gate_signal.pending_level,
                    pending_switch_energy: gate_signal.pending_switch_energy,
                    cancelled_switching_heat: gate_signal.cancelled_switching_heat,
                }
            }));
        self.gates
            .sort_unstable_by_key(|record| record.id.entity_id());

        self.wires.clear();
        self.wires
            .extend(structural.wires().iter_alive().map(|(_, record)| {
                let wire_signal = signal
                    .wire_snapshot(record.id)
                    .expect("validated canonical Wire has signal state");
                WireRenderRecord {
                    id: record.id,
                    routing_domain: record.routing_domain,
                    points: record.points.to_vec(),
                    endpoint_a: record.endpoint_a,
                    endpoint_b: record.endpoint_b,
                    connection_generation: record.connection_generation,
                    active_drive: wire_signal.active,
                    previous_drive: wire_signal.previous,
                    active_level: resolve_drive(wire_signal.active, logic_threshold),
                    previous_level: resolve_drive(wire_signal.previous, logic_threshold),
                }
            }));
        self.wires
            .sort_unstable_by_key(|record| record.id.entity_id());

        self.junctions.clear();
        self.junctions
            .extend(
                structural
                    .junctions()
                    .iter_alive()
                    .map(|(_, record)| JunctionRenderRecord {
                        id: record.id,
                        routing_domain: record.routing_domain,
                        position: record.position,
                        connection_generation: record.connection_generation,
                    }),
            );
        self.junctions
            .sort_unstable_by_key(|record| record.id.entity_id());
    }
}

pub(crate) fn sample_signal(
    signal: &SignalWorld,
    logic_threshold: u64,
    next_tick: Tick,
    target: SignalProbeTarget,
) -> Option<SignalProbeSample> {
    let value = match target {
        SignalProbeTarget::Driver(driver) => {
            SignalProbeValue::Driver(signal.driver_sample(driver)?)
        }
        SignalProbeTarget::Sink(sink) => SignalProbeValue::Sink {
            sink,
            level: signal.sink_level(sink)?,
        },
        SignalProbeTarget::GateInputA(gate) => {
            let port = signal.gate_ports(gate)?.input_a;
            SignalProbeValue::Sink {
                sink: port.sink,
                level: signal.sink_level(port.sink)?,
            }
        }
        SignalProbeTarget::GateInputB(gate) => {
            let port = signal.gate_ports(gate)?.input_b?;
            SignalProbeValue::Sink {
                sink: port.sink,
                level: signal.sink_level(port.sink)?,
            }
        }
        SignalProbeTarget::GateOutput(gate) => {
            let driver = signal.gate_ports(gate)?.output;
            SignalProbeValue::Driver(signal.driver_sample(driver)?)
        }
        SignalProbeTarget::Wire(wire) => {
            let wire = signal.wire_snapshot(wire)?;
            SignalProbeValue::Wire {
                active_drive: wire.active,
                previous_drive: wire.previous,
                active_level: resolve_drive(wire.active, logic_threshold),
                previous_level: resolve_drive(wire.previous, logic_threshold),
            }
        }
    };
    Some(SignalProbeSample {
        target,
        next_tick,
        value,
    })
}
