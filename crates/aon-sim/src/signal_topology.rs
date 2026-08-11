use crate::path_certificate::PathElementStamp;
use crate::profile::Rational;
use crate::signal::{DriveVector, DriverRole, SignalWorld, SinkRole};
use crate::structural::StructuralWorld;
use crate::{
    DriverId, EntityId, FIXED_ONE, Fixed, GateId, GatePort, JunctionId, SinkId, Tick, WireEnd,
    WireId, polyline_length,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SignalNodeKey {
    GatePort(GateId, GatePort),
    Junction(JunctionId),
    FreeEnd(WireId, WireEnd),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RoutePathElement {
    Wire(WireId),
    Junction(JunctionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DriverSinkPair {
    pub driver: DriverId,
    pub sink: SinkId,
}

impl DriverSinkPair {
    pub const fn new(driver: DriverId, sink: SinkId) -> Self {
        Self { driver, sink }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RouteFingerprint {
    pub path_stamps: Vec<PathElementStamp>,
    pub total_length: Fixed,
    pub segment_count: u64,
    pub delay: Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompiledRoute {
    pub driver: DriverId,
    pub sink: SinkId,
    pub total_length: Fixed,
    pub segment_count: u64,
    pub path_key: Vec<RoutePathElement>,
    pub path_stamps: Vec<PathElementStamp>,
    pub wires: Vec<WireId>,
    pub delay: Tick,
}

impl CompiledRoute {
    pub fn pair(&self) -> DriverSinkPair {
        DriverSinkPair::new(self.driver, self.sink)
    }

    pub fn fingerprint(&self) -> RouteFingerprint {
        RouteFingerprint {
            path_stamps: self.path_stamps.clone(),
            total_length: self.total_length,
            segment_count: self.segment_count,
            delay: self.delay,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RouteDiff {
    pub added: Vec<DriverSinkPair>,
    pub removed: Vec<DriverSinkPair>,
    pub retained: Vec<DriverSinkPair>,
    pub replaced: Vec<DriverSinkPair>,
}

impl RouteDiff {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.added.len() + self.removed.len() + self.retained.len() + self.replaced.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DriverLoad {
    pub reachable_sink_count: u64,
    pub component_wire_length: Fixed,
    pub total_load: u64,
    pub gate_delay: Tick,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompiledSignalTopology {
    routes: BTreeMap<DriverSinkPair, CompiledRoute>,
    driver_nodes: BTreeMap<DriverId, SignalNodeKey>,
    sink_nodes: BTreeMap<SinkId, SignalNodeKey>,
    node_components: BTreeMap<SignalNodeKey, u64>,
    wire_components: BTreeMap<WireId, u64>,
    component_wires: BTreeMap<u64, BTreeSet<WireId>>,
    component_wire_lengths: BTreeMap<u64, Fixed>,
    driver_loads: BTreeMap<DriverId, DriverLoad>,
}

impl CompiledSignalTopology {
    pub fn compile(
        structural: &StructuralWorld,
        signal: &SignalWorld,
        balance: &crate::BalanceProfile,
    ) -> Result<Self, SignalTopologyError> {
        let graph = SignalGraph::build(structural, signal)?;
        let mut compiled = Self {
            driver_nodes: graph.driver_nodes.clone(),
            sink_nodes: graph.sink_nodes.clone(),
            ..Self::default()
        };
        compiled.compile_components(&graph)?;

        let mut drivers: Vec<_> = graph.driver_nodes.keys().copied().collect();
        drivers.sort_unstable();
        for driver in drivers {
            let source = *graph
                .driver_nodes
                .get(&driver)
                .ok_or(SignalTopologyError::InvalidCanonicalState)?;
            let best = shortest_paths(source, &graph.adjacency)?;
            let component = *compiled
                .node_components
                .get(&source)
                .ok_or(SignalTopologyError::InvalidCanonicalState)?;
            let mut sinks: Vec<_> = graph
                .sink_nodes
                .iter()
                .filter_map(|(sink, node)| {
                    (compiled.node_components.get(node) == Some(&component))
                        .then_some((*sink, *node))
                })
                .collect();
            sinks.sort_unstable_by_key(|(sink, _)| *sink);
            for (sink, target) in sinks {
                let priority = best
                    .get(&target)
                    .ok_or(SignalTopologyError::InvalidCanonicalState)?;
                let wires: Vec<_> = priority
                    .path_key
                    .iter()
                    .filter_map(|element| match element {
                        RoutePathElement::Wire(wire) => Some(*wire),
                        RoutePathElement::Junction(_) => None,
                    })
                    .collect();
                let path_stamps = priority
                    .path_key
                    .iter()
                    .map(|element| match element {
                        RoutePathElement::Wire(id) => {
                            graph.wire_generations.get(id).copied().map(|generation| {
                                PathElementStamp::Wire {
                                    id: *id,
                                    generation,
                                }
                            })
                        }
                        RoutePathElement::Junction(id) => graph
                            .junction_generations
                            .get(id)
                            .copied()
                            .map(|generation| PathElementStamp::Junction {
                                id: *id,
                                generation,
                            }),
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or(SignalTopologyError::InvalidCanonicalState)?;
                let delay = if wires.is_empty() {
                    Tick(0)
                } else {
                    wire_delay(priority.total_length, balance)?
                };
                let route = CompiledRoute {
                    driver,
                    sink,
                    total_length: priority.total_length,
                    segment_count: priority.segment_count,
                    path_key: priority.path_key.clone(),
                    path_stamps,
                    wires,
                    delay,
                };
                let pair = DriverSinkPair::new(driver, sink);
                if route.pair() != pair || compiled.routes.insert(pair, route).is_some() {
                    return Err(SignalTopologyError::InvalidCanonicalState);
                }
            }

            let reachable_sink_count = u64::try_from(
                compiled
                    .routes
                    .keys()
                    .filter(|pair| pair.driver == driver)
                    .count(),
            )
            .map_err(|_| SignalTopologyError::NumericOverflow)?;
            let component_wire_length = *compiled
                .component_wire_lengths
                .get(&component)
                .ok_or(SignalTopologyError::InvalidCanonicalState)?;
            let total_load = driver_load(reachable_sink_count, component_wire_length, balance)?;
            let delay = gate_delay(total_load, balance)?;
            compiled.driver_loads.insert(
                driver,
                DriverLoad {
                    reachable_sink_count,
                    component_wire_length,
                    total_load,
                    gate_delay: delay,
                },
            );
        }
        Ok(compiled)
    }

    fn compile_components(&mut self, graph: &SignalGraph) -> Result<(), SignalTopologyError> {
        let mut unvisited: BTreeSet<_> = graph.adjacency.keys().copied().collect();
        let mut next_component = 0_u64;
        while let Some(start) = unvisited.pop_first() {
            let component = next_component;
            next_component = next_component
                .checked_add(1)
                .ok_or(SignalTopologyError::NumericOverflow)?;
            let mut frontier = BTreeSet::from([start]);
            let mut wires = BTreeSet::new();
            while let Some(node) = frontier.pop_first() {
                if self.node_components.insert(node, component).is_some() {
                    continue;
                }
                unvisited.remove(&node);
                for edge in graph
                    .adjacency
                    .get(&node)
                    .ok_or(SignalTopologyError::InvalidCanonicalState)?
                {
                    wires.insert(edge.wire);
                    if !self.node_components.contains_key(&edge.to) {
                        frontier.insert(edge.to);
                    }
                }
            }
            let mut length = Fixed::ZERO;
            for wire in &wires {
                let wire_length = graph
                    .wire_lengths
                    .get(wire)
                    .ok_or(SignalTopologyError::InvalidCanonicalState)?;
                length = length
                    .checked_add(*wire_length)
                    .map_err(|_| SignalTopologyError::NumericOverflow)?;
                if self.wire_components.insert(*wire, component).is_some() {
                    return Err(SignalTopologyError::InvalidCanonicalState);
                }
            }
            self.component_wires.insert(component, wires);
            self.component_wire_lengths.insert(component, length);
        }
        Ok(())
    }

    pub fn routes_from(&self, driver: DriverId) -> impl Iterator<Item = &CompiledRoute> + '_ {
        self.routes
            .range(
                DriverSinkPair::new(driver, SinkId(EntityId(0)))
                    ..=DriverSinkPair::new(driver, SinkId(EntityId(u64::MAX))),
            )
            .map(|(_, route)| route)
    }

    pub fn route(&self, pair: DriverSinkPair) -> Option<&CompiledRoute> {
        self.routes.get(&pair)
    }

    #[cfg(test)]
    pub fn canonical_routes(
        &self,
    ) -> impl DoubleEndedIterator<Item = (DriverSinkPair, &CompiledRoute)> + ExactSizeIterator + '_
    {
        self.routes.iter().map(|(pair, route)| (*pair, route))
    }

    pub fn route_diff(&self, newer: &Self) -> RouteDiff {
        let pairs: BTreeSet<_> = self
            .routes
            .keys()
            .chain(newer.routes.keys())
            .copied()
            .collect();
        let mut diff = RouteDiff::default();
        for pair in pairs {
            match (self.routes.get(&pair), newer.routes.get(&pair)) {
                (None, Some(_)) => diff.added.push(pair),
                (Some(_), None) => diff.removed.push(pair),
                (Some(old), Some(new)) if old.fingerprint() == new.fingerprint() => {
                    diff.retained.push(pair);
                }
                (Some(_), Some(_)) => diff.replaced.push(pair),
                (None, None) => {}
            }
        }
        diff
    }

    pub fn driver_load(&self, driver: DriverId) -> Option<DriverLoad> {
        self.driver_loads.get(&driver).copied()
    }

    pub fn wire_excitations(
        &self,
        signal: &SignalWorld,
    ) -> Result<BTreeMap<WireId, DriveVector>, SignalTopologyError> {
        let mut component_drive = BTreeMap::<u64, DriveVector>::new();
        for (_, record) in signal.canonical_driver_slots() {
            let Some(record) = record else {
                continue;
            };
            let node = self
                .driver_nodes
                .get(&record.id)
                .ok_or(SignalTopologyError::InvalidCanonicalState)?;
            let component = self
                .node_components
                .get(node)
                .ok_or(SignalTopologyError::InvalidCanonicalState)?;
            component_drive
                .entry(*component)
                .or_default()
                .checked_add_sample(record.sample)
                .map_err(|error| match error {
                    crate::signal::SignalError::NumericOverflow => {
                        SignalTopologyError::NumericOverflow
                    }
                    crate::signal::SignalError::InvalidCanonicalState => {
                        SignalTopologyError::InvalidCanonicalState
                    }
                    crate::signal::SignalError::DriverRevisionInvariantViolation => {
                        SignalTopologyError::InvalidCanonicalState
                    }
                })?;
        }
        let mut result = BTreeMap::new();
        for (wire, component) in &self.wire_components {
            result.insert(
                *wire,
                component_drive.get(component).copied().unwrap_or_default(),
            );
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum SignalTopologyError {
    #[error("canonical signal topology numeric overflow")]
    NumericOverflow,

    #[error("canonical signal topology invariant violated")]
    InvalidCanonicalState,
}

#[derive(Clone, Debug, Default)]
struct SignalGraph {
    adjacency: BTreeMap<SignalNodeKey, Vec<SignalEdge>>,
    driver_nodes: BTreeMap<DriverId, SignalNodeKey>,
    sink_nodes: BTreeMap<SinkId, SignalNodeKey>,
    wire_lengths: BTreeMap<WireId, Fixed>,
    wire_generations: BTreeMap<WireId, crate::ConnectionGeneration>,
    junction_generations: BTreeMap<JunctionId, crate::ConnectionGeneration>,
}

impl SignalGraph {
    fn build(
        structural: &StructuralWorld,
        signal: &SignalWorld,
    ) -> Result<Self, SignalTopologyError> {
        let mut graph = Self::default();
        let gate_types: BTreeMap<_, _> = structural
            .gates()
            .iter_alive()
            .map(|(_, record)| (record.id, record.gate_type))
            .collect();

        for gate in signal.iter_gates() {
            if gate_types.get(&gate.gate) != Some(&gate.gate_type) {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
            graph.ensure_node(SignalNodeKey::GatePort(gate.gate, GatePort::InputA));
            graph.ensure_node(SignalNodeKey::GatePort(gate.gate, GatePort::Output));
            if gate.ports.input_b.is_some() {
                graph.ensure_node(SignalNodeKey::GatePort(gate.gate, GatePort::InputB));
            }
        }

        for (_, record) in signal.canonical_driver_slots() {
            let Some(record) = record else {
                continue;
            };
            let port = match record.role {
                DriverRole::ExternalInputA => GatePort::InputA,
                DriverRole::ExternalInputB => GatePort::InputB,
                DriverRole::GateOutput => GatePort::Output,
            };
            let node = SignalNodeKey::GatePort(record.owner, port);
            graph.ensure_node(node);
            if graph.driver_nodes.insert(record.id, node).is_some() {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
        }
        for (_, record) in signal.canonical_sink_slots() {
            let Some(record) = record else {
                continue;
            };
            let port = match record.role {
                SinkRole::InputA => GatePort::InputA,
                SinkRole::InputB => GatePort::InputB,
            };
            let node = SignalNodeKey::GatePort(record.owner, port);
            graph.ensure_node(node);
            if graph.sink_nodes.insert(record.id, node).is_some() {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
        }

        for (_, junction) in structural.junctions().iter_alive() {
            graph.ensure_node(SignalNodeKey::Junction(junction.id));
            if graph
                .junction_generations
                .insert(junction.id, junction.connection_generation)
                .is_some()
            {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
        }
        for (_, wire) in structural.wires().iter_alive() {
            let node_a = signal_node_for_endpoint(wire.id, WireEnd::A, wire.endpoint_a);
            let node_b = signal_node_for_endpoint(wire.id, WireEnd::B, wire.endpoint_b);
            graph.ensure_node(node_a);
            graph.ensure_node(node_b);
            let length =
                polyline_length(wire.points).map_err(|_| SignalTopologyError::NumericOverflow)?;
            let segment_count = u64::try_from(
                wire.points
                    .len()
                    .checked_sub(1)
                    .ok_or(SignalTopologyError::InvalidCanonicalState)?,
            )
            .map_err(|_| SignalTopologyError::NumericOverflow)?;
            if graph.wire_lengths.insert(wire.id, length).is_some() {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
            if graph
                .wire_generations
                .insert(wire.id, wire.connection_generation)
                .is_some()
            {
                return Err(SignalTopologyError::InvalidCanonicalState);
            }
            graph.add_edge(node_a, node_b, wire.id, length, segment_count);
            graph.add_edge(node_b, node_a, wire.id, length, segment_count);
        }
        for edges in graph.adjacency.values_mut() {
            edges.sort_unstable();
        }
        Ok(graph)
    }

    fn ensure_node(&mut self, node: SignalNodeKey) {
        self.adjacency.entry(node).or_default();
    }

    fn add_edge(
        &mut self,
        from: SignalNodeKey,
        to: SignalNodeKey,
        wire: WireId,
        length: Fixed,
        segment_count: u64,
    ) {
        self.adjacency.entry(from).or_default().push(SignalEdge {
            to,
            wire,
            length,
            segment_count,
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignalEdge {
    to: SignalNodeKey,
    wire: WireId,
    length: Fixed,
    segment_count: u64,
}

impl Ord for SignalEdge {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.to, self.wire, self.length, self.segment_count).cmp(&(
            other.to,
            other.wire,
            other.length,
            other.segment_count,
        ))
    }
}

impl PartialOrd for SignalEdge {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct RoutePriority {
    total_length: Fixed,
    segment_count: u64,
    path_key: Vec<RoutePathElement>,
}

fn shortest_paths(
    source: SignalNodeKey,
    adjacency: &BTreeMap<SignalNodeKey, Vec<SignalEdge>>,
) -> Result<BTreeMap<SignalNodeKey, RoutePriority>, SignalTopologyError> {
    let mut best = BTreeMap::from([(source, RoutePriority::default())]);
    let mut frontier = BTreeSet::from([(RoutePriority::default(), source)]);
    while let Some((priority, node)) = frontier.pop_first() {
        if best.get(&node) != Some(&priority) {
            continue;
        }
        for edge in adjacency
            .get(&node)
            .ok_or(SignalTopologyError::InvalidCanonicalState)?
        {
            let total_length = priority
                .total_length
                .checked_add(edge.length)
                .map_err(|_| SignalTopologyError::NumericOverflow)?;
            let segment_count = priority
                .segment_count
                .checked_add(edge.segment_count)
                .ok_or(SignalTopologyError::NumericOverflow)?;
            let mut path_key = priority.path_key.clone();
            path_key.push(RoutePathElement::Wire(edge.wire));
            if let SignalNodeKey::Junction(junction) = edge.to {
                path_key.push(RoutePathElement::Junction(junction));
            }
            let candidate = RoutePriority {
                total_length,
                segment_count,
                path_key,
            };
            if best
                .get(&edge.to)
                .is_none_or(|existing| candidate < *existing)
            {
                if let Some(existing) = best.insert(edge.to, candidate.clone()) {
                    frontier.remove(&(existing, edge.to));
                }
                frontier.insert((candidate, edge.to));
            }
        }
    }
    Ok(best)
}

fn signal_node_for_endpoint(
    wire: WireId,
    end: WireEnd,
    endpoint: crate::EndpointTarget,
) -> SignalNodeKey {
    match endpoint {
        crate::EndpointTarget::Free => SignalNodeKey::FreeEnd(wire, end),
        crate::EndpointTarget::Junction(junction) => SignalNodeKey::Junction(junction),
        crate::EndpointTarget::GatePort(reference) if reference.port != GatePort::Power => {
            SignalNodeKey::GatePort(reference.gate, reference.port)
        }
        crate::EndpointTarget::GatePort(_) => SignalNodeKey::FreeEnd(wire, end),
    }
}

pub(crate) fn driver_load(
    reachable_sink_count: u64,
    component_wire_length: Fixed,
    balance: &crate::BalanceProfile,
) -> Result<u64, SignalTopologyError> {
    if component_wire_length.0 < 0 {
        return Err(SignalTopologyError::InvalidCanonicalState);
    }
    let sink_load = balance
        .input_load
        .checked_mul(reachable_sink_count)
        .ok_or(SignalTopologyError::NumericOverflow)?;
    let wire_load = ceil_scaled_length(balance.wire_load_per_wu, component_wire_length)?;
    sink_load
        .checked_add(wire_load)
        .ok_or(SignalTopologyError::NumericOverflow)
}

pub(crate) fn gate_delay(
    load: u64,
    balance: &crate::BalanceProfile,
) -> Result<Tick, SignalTopologyError> {
    let excess = load.saturating_sub(balance.fanout_free_load);
    let penalty = excess.div_ceil(balance.fanout_step);
    let delay = balance
        .gate_base_delay
        .checked_add(penalty)
        .ok_or(SignalTopologyError::NumericOverflow)?;
    Ok(Tick(delay.max(1)))
}

pub(crate) fn switch_energy(
    load: u64,
    balance: &crate::BalanceProfile,
) -> Result<crate::Energy, SignalTopologyError> {
    let multiplier = load
        .checked_add(1)
        .ok_or(SignalTopologyError::NumericOverflow)?;
    balance
        .gate_switch_base_energy
        .checked_mul(multiplier)
        .map(crate::Energy)
        .ok_or(SignalTopologyError::NumericOverflow)
}

pub(crate) fn wire_delay(
    length: Fixed,
    balance: &crate::BalanceProfile,
) -> Result<Tick, SignalTopologyError> {
    if length.0 < 0 {
        return Err(SignalTopologyError::InvalidCanonicalState);
    }
    let length = u64::try_from(length.0).map_err(|_| SignalTopologyError::NumericOverflow)?;
    let linear_n = nonnegative_rational_part(balance.wire_linear_k, true)?;
    let linear_d = nonnegative_rational_part(balance.wire_linear_k, false)?;
    let quadratic_n = nonnegative_rational_part(balance.wire_quadratic_k, true)?;
    let quadratic_d = nonnegative_rational_part(balance.wire_quadratic_k, false)?;
    if quadratic_n == 0 {
        return Err(SignalTopologyError::InvalidCanonicalState);
    }
    let fixed_one =
        u64::try_from(FIXED_ONE).map_err(|_| SignalTopologyError::InvalidCanonicalState)?;

    let linear = U256::from_u64(linear_n)
        .checked_mul_u64(length)?
        .checked_mul_u64(quadratic_d)?
        .checked_mul_u64(fixed_one)?;
    let quadratic = U256::from_u64(quadratic_n)
        .checked_mul_u64(length)?
        .checked_mul_u64(length)?
        .checked_mul_u64(linear_d)?;
    let numerator = linear.checked_add(quadratic)?;
    let denominator = U256::from_u64(linear_d)
        .checked_mul_u64(quadratic_d)?
        .checked_mul_u64(fixed_one)?
        .checked_mul_u64(fixed_one)?;
    let delay = numerator.ceil_div_to_u64(denominator)?.max(1);
    Ok(Tick(delay))
}

fn ceil_scaled_length(coefficient: Rational, length: Fixed) -> Result<u64, SignalTopologyError> {
    let numerator = nonnegative_rational_part(coefficient, true)?;
    let denominator = nonnegative_rational_part(coefficient, false)?;
    let length = u64::try_from(length.0).map_err(|_| SignalTopologyError::NumericOverflow)?;
    let fixed_one =
        u64::try_from(FIXED_ONE).map_err(|_| SignalTopologyError::InvalidCanonicalState)?;
    U256::from_u64(numerator)
        .checked_mul_u64(length)?
        .ceil_div_to_u64(U256::from_u64(denominator).checked_mul_u64(fixed_one)?)
}

fn nonnegative_rational_part(value: Rational, numerator: bool) -> Result<u64, SignalTopologyError> {
    let raw = if numerator {
        value.numerator()
    } else {
        value.denominator()
    };
    u64::try_from(raw).map_err(|_| SignalTopologyError::InvalidCanonicalState)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct U256([u64; 4]);

impl U256 {
    const fn from_u64(value: u64) -> Self {
        Self([value, 0, 0, 0])
    }

    fn checked_add(self, other: Self) -> Result<Self, SignalTopologyError> {
        let mut output = [0_u64; 4];
        let mut carry = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (sum, first_carry) = self.0[index].overflowing_add(other.0[index]);
            let (sum, second_carry) = sum.overflowing_add(u64::from(carry));
            *slot = sum;
            carry = first_carry || second_carry;
        }
        if carry {
            Err(SignalTopologyError::NumericOverflow)
        } else {
            Ok(Self(output))
        }
    }

    fn checked_mul_u64(self, multiplier: u64) -> Result<Self, SignalTopologyError> {
        let mut output = [0_u64; 4];
        let mut carry = 0_u128;
        for (index, slot) in output.iter_mut().enumerate() {
            let product = u128::from(self.0[index]) * u128::from(multiplier) + carry;
            *slot = product as u64;
            carry = product >> 64;
        }
        if carry != 0 {
            Err(SignalTopologyError::NumericOverflow)
        } else {
            Ok(Self(output))
        }
    }

    fn ceil_div_to_u64(self, denominator: Self) -> Result<u64, SignalTopologyError> {
        if denominator == Self::default() {
            return Err(SignalTopologyError::InvalidCanonicalState);
        }
        if self == Self::default() {
            return Ok(0);
        }
        let maximum = denominator.checked_mul_u64(u64::MAX)?;
        if self > maximum {
            return Err(SignalTopologyError::NumericOverflow);
        }
        let mut lower = 0_u64;
        let mut upper = u64::MAX;
        while lower < upper {
            let midpoint = lower + (upper - lower).div_ceil(2);
            if denominator.checked_mul_u64(midpoint)? <= self {
                lower = midpoint;
            } else {
                upper = midpoint - 1;
            }
        }
        let floor = lower;
        if denominator.checked_mul_u64(floor)? == self {
            Ok(floor)
        } else {
            floor
                .checked_add(1)
                .ok_or(SignalTopologyError::NumericOverflow)
        }
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogicLevel;
    use crate::{
        Command, CommandEnvelope, EndpointTarget, FixedAabb, FixedVec2, GateIndex, GatePortRef,
        GateType, JunctionIndex, PhysicalScaleProfile, PlaceFixedSubstrateCommand,
        PlaceGateCommand, PlaceJunctionCommand, PlaceWireCommand, RoutingDomain, WireIndex,
    };

    const ROUTE_FIXTURE_PITCH: i64 = 16_384;

    const fn route_fixture_point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(
            Fixed(x * ROUTE_FIXTURE_PITCH),
            Fixed(y * ROUTE_FIXTURE_PITCH),
        )
    }

    fn apply_route_fixture_phase(
        structural: &mut StructuralWorld,
        signal: &mut SignalWorld,
        tick: u64,
        commands: Vec<Command>,
        reverse_input: bool,
        physical: &PhysicalScaleProfile,
    ) {
        let mut envelopes: Vec<_> = commands
            .into_iter()
            .enumerate()
            .map(|(ordinal, command)| CommandEnvelope {
                target_tick: Tick(tick),
                ordinal: u64::try_from(ordinal).expect("fixture ordinal fits u64"),
                command,
            })
            .collect();
        if reverse_input {
            envelopes.reverse();
        }
        let expected_acceptances = envelopes.len();
        let report = structural
            .apply_phase0_with_signal(signal, Tick(tick), &envelopes, physical)
            .expect("the real route fixture is structurally valid");
        assert_eq!(report.acceptances.len(), expected_acceptances);
        assert!(report.rejections.is_empty());
    }

    fn compiled_route_tie_fixture(
        reverse_input: bool,
        perturb_store_layout: bool,
    ) -> (StructuralWorld, SignalWorld, DriverId, SinkId) {
        let physical = PhysicalScaleProfile::stage0_alpha("compiled-route-tie");
        let mut structural = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let bounds = FixedAabb::new(route_fixture_point(-16, -16), route_fixture_point(16, 16));
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            0,
            vec![Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: route_fixture_point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            })],
            reverse_input,
            &physical,
        );

        let domain = RoutingDomain::FixedSubstrate(EntityId(1));
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            1,
            vec![
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: route_fixture_point(-4, 0),
                    routing_domain: domain,
                }),
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: route_fixture_point(4, 0),
                    routing_domain: domain,
                }),
            ],
            reverse_input,
            &physical,
        );
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            2,
            vec![
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: route_fixture_point(0, 4),
                }),
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: route_fixture_point(0, -4),
                }),
            ],
            reverse_input,
            &physical,
        );

        let source = EndpointTarget::GatePort(GatePortRef {
            gate: GateId(EntityId(2)),
            port: GatePort::Output,
        });
        let target = EndpointTarget::GatePort(GatePortRef {
            gate: GateId(EntityId(3)),
            port: GatePort::InputA,
        });
        let upper = EndpointTarget::Junction(JunctionId(EntityId(4)));
        let lower = EndpointTarget::Junction(JunctionId(EntityId(5)));
        let start = route_fixture_point(-3, 0);
        let end = route_fixture_point(3, 0);
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            3,
            vec![
                // Lower path key and equal segment count, but strictly longer.
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![start, route_fixture_point(0, 6), end],
                    endpoint_a: source,
                    endpoint_b: target,
                }),
                // Equal 10-WU length and lower path key, but five segments.
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![
                        start,
                        route_fixture_point(-2, 0),
                        route_fixture_point(-2, 2),
                        route_fixture_point(2, 2),
                        route_fixture_point(2, 0),
                        end,
                    ],
                    endpoint_a: source,
                    endpoint_b: target,
                }),
                // Equal-length, two-segment upper route. Its path key wins the final tie.
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![start, route_fixture_point(0, 4)],
                    endpoint_a: source,
                    endpoint_b: upper,
                }),
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![route_fixture_point(0, 4), end],
                    endpoint_a: upper,
                    endpoint_b: target,
                }),
                // Geometrically mirrored, otherwise tied two-segment lower route.
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![start, route_fixture_point(0, -4)],
                    endpoint_a: source,
                    endpoint_b: lower,
                }),
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![route_fixture_point(0, -4), end],
                    endpoint_a: lower,
                    endpoint_b: target,
                }),
            ],
            reverse_input,
            &physical,
        );

        if perturb_store_layout {
            structural.reserve_layout_capacity_for_test(128);
            structural
                .swap_gate_slots_for_test(GateIndex(0), GateIndex(1))
                .expect("Gate storage layout can be permuted");
            structural
                .swap_junction_slots_for_test(JunctionIndex(0), JunctionIndex(1))
                .expect("Junction storage layout can be permuted");
            for (first, second) in [(0, 5), (1, 4), (2, 3)] {
                structural
                    .swap_wire_slots_for_test(WireIndex(first), WireIndex(second))
                    .expect("Wire storage layout can be permuted");
            }
        }

        let source_ports = signal
            .gate_ports(GateId(EntityId(2)))
            .expect("source Gate signal ports exist");
        let target_ports = signal
            .gate_ports(GateId(EntityId(3)))
            .expect("target Gate signal ports exist");
        (
            structural,
            signal,
            source_ports.output,
            target_ports.input_a.sink,
        )
    }

    fn actual_route_priority(
        structural: &StructuralWorld,
        path_key: Vec<RoutePathElement>,
    ) -> RoutePriority {
        let mut total_length = Fixed::ZERO;
        let mut segment_count = 0_u64;
        for element in &path_key {
            let RoutePathElement::Wire(wire) = element else {
                continue;
            };
            let record = structural
                .wires()
                .iter_alive()
                .find_map(|(_, record)| (record.id == *wire).then_some(record))
                .expect("candidate Wire exists in the real StructuralWorld");
            total_length = total_length
                .checked_add(polyline_length(record.points).expect("fixture length fits"))
                .expect("fixture route length fits");
            segment_count = segment_count
                .checked_add(
                    u64::try_from(record.points.len() - 1).expect("fixture segment count fits u64"),
                )
                .expect("fixture route segment count fits");
        }
        RoutePriority {
            total_length,
            segment_count,
            path_key,
        }
    }

    fn synthetic_route(
        pair: DriverSinkPair,
        path_key: Vec<RoutePathElement>,
        path_stamps: Vec<PathElementStamp>,
    ) -> CompiledRoute {
        let wires = path_key
            .iter()
            .filter_map(|element| match element {
                RoutePathElement::Wire(wire) => Some(*wire),
                RoutePathElement::Junction(_) => None,
            })
            .collect();
        CompiledRoute {
            driver: pair.driver,
            sink: pair.sink,
            total_length: Fixed(10),
            segment_count: 1,
            path_key,
            path_stamps,
            wires,
            delay: Tick(1),
        }
    }

    fn synthetic_topology(routes: Vec<CompiledRoute>) -> CompiledSignalTopology {
        let mut topology = CompiledSignalTopology::default();
        for route in routes {
            assert!(topology.routes.insert(route.pair(), route).is_none());
        }
        topology
    }

    #[test]
    fn compiled_certificates_cover_empty_single_and_adjacent_gate_port_wires_exactly() {
        let physical = PhysicalScaleProfile::stage0_alpha("certificate-shapes");
        let balance = crate::BalanceProfile::stage0_alpha("certificate-shapes");
        let mut structural = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let bounds = FixedAabb::new(route_fixture_point(-16, -16), route_fixture_point(16, 16));
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            0,
            vec![Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: route_fixture_point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            })],
            false,
            &physical,
        );
        let domain = RoutingDomain::FixedSubstrate(EntityId(1));
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            1,
            vec![
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: route_fixture_point(-4, 0),
                    routing_domain: domain,
                }),
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: route_fixture_point(0, 0),
                    routing_domain: domain,
                }),
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: route_fixture_point(0, 4),
                    routing_domain: domain,
                }),
            ],
            false,
            &physical,
        );
        let source = GateId(EntityId(2));
        let bridge = GateId(EntityId(3));
        let target = GateId(EntityId(4));
        apply_route_fixture_phase(
            &mut structural,
            &mut signal,
            2,
            vec![
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![route_fixture_point(-3, 0), route_fixture_point(-1, 0)],
                    endpoint_a: EndpointTarget::GatePort(GatePortRef {
                        gate: source,
                        port: GatePort::Output,
                    }),
                    endpoint_b: EndpointTarget::GatePort(GatePortRef {
                        gate: bridge,
                        port: GatePort::InputA,
                    }),
                }),
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![
                        route_fixture_point(-1, 0),
                        route_fixture_point(-2, 1),
                        route_fixture_point(-2, 3),
                        route_fixture_point(-1, 4),
                    ],
                    endpoint_a: EndpointTarget::GatePort(GatePortRef {
                        gate: bridge,
                        port: GatePort::InputA,
                    }),
                    endpoint_b: EndpointTarget::GatePort(GatePortRef {
                        gate: target,
                        port: GatePort::InputA,
                    }),
                }),
            ],
            false,
            &physical,
        );

        let compiled = CompiledSignalTopology::compile(&structural, &signal, &balance)
            .expect("certificate-shape topology compiles");
        let source_ports = signal.gate_ports(source).expect("source ports exist");
        let bridge_ports = signal.gate_ports(bridge).expect("bridge ports exist");
        let target_ports = signal.gate_ports(target).expect("target ports exist");

        let local = compiled
            .route(DriverSinkPair::new(
                bridge_ports.input_a.external_driver,
                bridge_ports.input_a.sink,
            ))
            .expect("local Driver reaches its co-located Sink");
        assert!(local.path_stamps.is_empty());
        assert_eq!(local.delay, Tick(0));

        let single = compiled
            .route(DriverSinkPair::new(
                source_ports.output,
                bridge_ports.input_a.sink,
            ))
            .expect("source reaches the bridge over one Wire");
        assert_eq!(
            single.path_stamps,
            vec![PathElementStamp::Wire {
                id: WireId(EntityId(5)),
                generation: crate::ConnectionGeneration(0),
            }]
        );

        let adjacent = compiled
            .route(DriverSinkPair::new(
                source_ports.output,
                target_ports.input_a.sink,
            ))
            .expect("source reaches the target through two Wires sharing one GatePort");
        assert_eq!(
            adjacent.path_stamps,
            vec![
                PathElementStamp::Wire {
                    id: WireId(EntityId(5)),
                    generation: crate::ConnectionGeneration(0),
                },
                PathElementStamp::Wire {
                    id: WireId(EntityId(6)),
                    generation: crate::ConnectionGeneration(0),
                },
            ]
        );
    }

    #[test]
    fn exact_wire_delay_matches_c01_and_c03_lengths() {
        let balance = crate::BalanceProfile::stage0_alpha("signal-topology-delay");
        assert_eq!(wire_delay(Fixed(8 * FIXED_ONE), &balance), Ok(Tick(3)));
        assert_eq!(wire_delay(Fixed(12 * FIXED_ONE), &balance), Ok(Tick(5)));
    }

    #[test]
    fn physical_delay_is_positive_and_local_delay_is_selected_by_compiler() {
        let balance = crate::BalanceProfile::stage0_alpha("positive-delay");
        assert_eq!(wire_delay(Fixed(0), &balance), Ok(Tick(1)));
        assert_eq!(wire_delay(Fixed(1), &balance), Ok(Tick(1)));
    }

    #[test]
    fn load_and_fanout_use_component_length_and_ceil() {
        let balance = crate::BalanceProfile::stage0_alpha("load-delay");
        assert_eq!(driver_load(1, Fixed(FIXED_ONE / 2), &balance), Ok(2));
        assert_eq!(gate_delay(4, &balance), Ok(Tick(1)));
        assert_eq!(gate_delay(5, &balance), Ok(Tick(2)));
        assert_eq!(gate_delay(9, &balance), Ok(Tick(3)));
    }

    #[test]
    fn switch_energy_is_checked() {
        let balance = crate::BalanceProfile::stage0_alpha("switch-energy");
        assert_eq!(switch_energy(0, &balance), Ok(crate::Energy(1)));
        assert_eq!(
            switch_energy(u64::MAX, &balance),
            Err(SignalTopologyError::NumericOverflow)
        );
    }

    #[test]
    fn load_fanout_and_wire_delay_overflow_are_typed() {
        let mut balance = crate::BalanceProfile::stage0_alpha("signal-overflow-boundaries");
        balance.input_load = u64::MAX;
        assert_eq!(
            driver_load(2, Fixed::ZERO, &balance),
            Err(SignalTopologyError::NumericOverflow)
        );

        balance = crate::BalanceProfile::stage0_alpha("signal-overflow-boundaries");
        balance.gate_base_delay = u64::MAX;
        balance.fanout_free_load = 0;
        balance.fanout_step = 1;
        assert_eq!(
            gate_delay(1, &balance),
            Err(SignalTopologyError::NumericOverflow)
        );

        balance = crate::BalanceProfile::stage0_alpha("signal-overflow-boundaries");
        balance.wire_linear_k = Rational::new(i64::MAX, 1).unwrap();
        balance.wire_quadratic_k = Rational::new(i64::MAX, 1).unwrap();
        assert_eq!(
            wire_delay(Fixed(i64::MAX), &balance),
            Err(SignalTopologyError::NumericOverflow)
        );
    }

    #[test]
    fn u256_ceil_div_handles_values_above_u128() {
        let numerator = U256::from_u64(u64::MAX)
            .checked_mul_u64(u64::MAX)
            .and_then(|value| value.checked_mul_u64(17))
            .expect("wide numerator fits");
        let denominator = U256::from_u64(u64::MAX)
            .checked_mul_u64(17)
            .expect("wide denominator fits");
        assert_eq!(numerator.ceil_div_to_u64(denominator), Ok(u64::MAX));
    }

    #[test]
    fn route_priority_prefers_length_then_segments_then_path_key() {
        let short = RoutePriority {
            total_length: Fixed(10),
            segment_count: 99,
            path_key: vec![RoutePathElement::Wire(WireId(EntityId(9)))],
        };
        let long = RoutePriority {
            total_length: Fixed(11),
            segment_count: 1,
            path_key: vec![RoutePathElement::Wire(WireId(EntityId(1)))],
        };
        assert!(short < long);
        let fewer_segments = RoutePriority {
            total_length: Fixed(10),
            segment_count: 2,
            path_key: vec![RoutePathElement::Wire(WireId(EntityId(9)))],
        };
        assert!(fewer_segments < short);
        let lower_path = RoutePriority {
            total_length: Fixed(10),
            segment_count: 2,
            path_key: vec![RoutePathElement::Wire(WireId(EntityId(1)))],
        };
        assert!(lower_path < fewer_segments);
    }

    #[test]
    fn shortest_path_ties_are_independent_of_adjacency_insertion_order() {
        let source = SignalNodeKey::GatePort(GateId(EntityId(1)), GatePort::Output);
        let target = SignalNodeKey::GatePort(GateId(EntityId(2)), GatePort::InputA);
        let low_junction = SignalNodeKey::Junction(JunctionId(EntityId(3)));
        let high_junction = SignalNodeKey::Junction(JunctionId(EntityId(9)));
        let low_first = SignalEdge {
            to: low_junction,
            wire: WireId(EntityId(2)),
            length: Fixed(5),
            segment_count: 1,
        };
        let high_first = SignalEdge {
            to: high_junction,
            wire: WireId(EntityId(7)),
            length: Fixed(5),
            segment_count: 1,
        };
        let direct_more_segments = SignalEdge {
            to: target,
            wire: WireId(EntityId(1)),
            length: Fixed(10),
            segment_count: 3,
        };
        let low_second = SignalEdge {
            to: target,
            wire: WireId(EntityId(8)),
            length: Fixed(5),
            segment_count: 1,
        };
        let high_second = SignalEdge {
            to: target,
            wire: WireId(EntityId(4)),
            length: Fixed(5),
            segment_count: 1,
        };

        let left = BTreeMap::from([
            (source, vec![high_first, direct_more_segments, low_first]),
            (high_junction, vec![high_second]),
            (low_junction, vec![low_second]),
            (target, Vec::new()),
        ]);
        let right = BTreeMap::from([
            (target, Vec::new()),
            (low_junction, vec![low_second]),
            (high_junction, vec![high_second]),
            (source, vec![low_first, direct_more_segments, high_first]),
        ]);

        let expected = RoutePriority {
            total_length: Fixed(10),
            segment_count: 2,
            path_key: vec![
                RoutePathElement::Wire(WireId(EntityId(2))),
                RoutePathElement::Junction(JunctionId(EntityId(3))),
                RoutePathElement::Wire(WireId(EntityId(8))),
            ],
        };
        assert_eq!(shortest_paths(source, &left).unwrap()[&target], expected);
        assert_eq!(shortest_paths(source, &right).unwrap()[&target], expected);
    }

    #[test]
    fn compiled_route_ties_ignore_command_and_store_layout() {
        let balance = crate::BalanceProfile::stage0_alpha("compiled-route-tie");
        let (baseline_structural, baseline_signal, driver, sink) =
            compiled_route_tie_fixture(false, false);
        let (reordered_structural, reordered_signal, reordered_driver, reordered_sink) =
            compiled_route_tie_fixture(true, true);

        assert_ne!(
            baseline_structural, reordered_structural,
            "the second fixture must have a genuinely different SoA storage layout"
        );
        assert_eq!(baseline_signal, reordered_signal);
        assert_eq!((driver, sink), (reordered_driver, reordered_sink));

        let compiled =
            CompiledSignalTopology::compile(&baseline_structural, &baseline_signal, &balance)
                .expect("baseline topology compiles");
        let reordered =
            CompiledSignalTopology::compile(&reordered_structural, &reordered_signal, &balance)
                .expect("reordered topology compiles");
        assert_eq!(compiled, reordered);
        let layout_diff = compiled.route_diff(&reordered);
        assert!(layout_diff.added.is_empty());
        assert!(layout_diff.removed.is_empty());
        assert!(layout_diff.replaced.is_empty());
        assert_eq!(
            layout_diff.retained,
            compiled
                .canonical_routes()
                .map(|(pair, _)| pair)
                .collect::<Vec<_>>()
        );

        let long_low_key = actual_route_priority(
            &baseline_structural,
            vec![RoutePathElement::Wire(WireId(EntityId(6)))],
        );
        let equal_length_more_segments = actual_route_priority(
            &baseline_structural,
            vec![RoutePathElement::Wire(WireId(EntityId(7)))],
        );
        let expected = actual_route_priority(
            &baseline_structural,
            vec![
                RoutePathElement::Wire(WireId(EntityId(8))),
                RoutePathElement::Junction(JunctionId(EntityId(4))),
                RoutePathElement::Wire(WireId(EntityId(9))),
            ],
        );
        let tied_higher_key = actual_route_priority(
            &baseline_structural,
            vec![
                RoutePathElement::Wire(WireId(EntityId(10))),
                RoutePathElement::Junction(JunctionId(EntityId(5))),
                RoutePathElement::Wire(WireId(EntityId(11))),
            ],
        );

        assert!(expected.total_length < long_low_key.total_length);
        assert_eq!(
            expected.total_length,
            equal_length_more_segments.total_length
        );
        assert!(expected.segment_count < equal_length_more_segments.segment_count);
        assert_eq!(expected.total_length, tied_higher_key.total_length);
        assert_eq!(expected.segment_count, tied_higher_key.segment_count);
        assert!(expected.path_key < tied_higher_key.path_key);

        let selected = compiled
            .routes
            .get(&DriverSinkPair::new(driver, sink))
            .expect("source output reaches target input");
        assert_eq!(selected.total_length, expected.total_length);
        assert_eq!(selected.segment_count, expected.segment_count);
        assert_eq!(selected.path_key, expected.path_key);
        assert_eq!(
            selected.wires,
            vec![WireId(EntityId(8)), WireId(EntityId(9))]
        );
        assert_eq!(
            selected.path_stamps,
            vec![
                PathElementStamp::Wire {
                    id: WireId(EntityId(8)),
                    generation: crate::ConnectionGeneration(0),
                },
                PathElementStamp::Junction {
                    id: JunctionId(EntityId(4)),
                    generation: crate::ConnectionGeneration(1),
                },
                PathElementStamp::Wire {
                    id: WireId(EntityId(9)),
                    generation: crate::ConnectionGeneration(0),
                },
            ],
            "selected paths are stamped after the Phase 0-coalesced generation advance"
        );
    }

    #[test]
    fn route_diff_is_four_way_pair_ordered_and_generation_sensitive() {
        let retained = DriverSinkPair::new(DriverId(EntityId(1)), SinkId(EntityId(1)));
        let removed = DriverSinkPair::new(DriverId(EntityId(1)), SinkId(EntityId(2)));
        let added = DriverSinkPair::new(DriverId(EntityId(2)), SinkId(EntityId(1)));
        let generation_replaced = DriverSinkPair::new(DriverId(EntityId(2)), SinkId(EntityId(2)));
        let path_replaced = DriverSinkPair::new(DriverId(EntityId(3)), SinkId(EntityId(1)));

        let wire = |id, generation| PathElementStamp::Wire {
            id: WireId(EntityId(id)),
            generation: crate::ConnectionGeneration(generation),
        };
        let old = synthetic_topology(vec![
            synthetic_route(
                retained,
                vec![RoutePathElement::Wire(WireId(EntityId(10)))],
                vec![wire(10, 0)],
            ),
            synthetic_route(
                removed,
                vec![RoutePathElement::Wire(WireId(EntityId(20)))],
                vec![wire(20, 0)],
            ),
            synthetic_route(
                generation_replaced,
                vec![RoutePathElement::Wire(WireId(EntityId(30)))],
                vec![wire(30, 0)],
            ),
            synthetic_route(
                path_replaced,
                vec![RoutePathElement::Wire(WireId(EntityId(40)))],
                vec![wire(40, 0)],
            ),
        ]);
        let new = synthetic_topology(vec![
            synthetic_route(
                retained,
                vec![RoutePathElement::Wire(WireId(EntityId(10)))],
                vec![wire(10, 0)],
            ),
            synthetic_route(
                added,
                vec![RoutePathElement::Wire(WireId(EntityId(25)))],
                vec![wire(25, 0)],
            ),
            synthetic_route(
                generation_replaced,
                vec![RoutePathElement::Wire(WireId(EntityId(30)))],
                vec![wire(30, 1)],
            ),
            synthetic_route(
                path_replaced,
                vec![RoutePathElement::Wire(WireId(EntityId(41)))],
                vec![wire(41, 0)],
            ),
        ]);

        let diff = old.route_diff(&new);
        assert_eq!(diff.added, vec![added]);
        assert_eq!(diff.removed, vec![removed]);
        assert_eq!(diff.retained, vec![retained]);
        assert_eq!(diff.replaced, vec![generation_replaced, path_replaced]);
        assert_eq!(diff.len(), 5);
        assert!(!diff.is_empty());

        let canonical_pairs: Vec<_> = new
            .canonical_routes()
            .map(|(pair, route)| {
                assert_eq!(route.pair(), pair);
                assert_eq!(new.route(pair), Some(route));
                pair
            })
            .collect();
        assert_eq!(
            canonical_pairs,
            vec![retained, added, generation_replaced, path_replaced]
        );
        assert!(
            CompiledSignalTopology::default()
                .route_diff(&CompiledSignalTopology::default())
                .is_empty()
        );
    }

    #[test]
    fn route_fingerprint_covers_stamps_length_segments_and_delay() {
        let pair = DriverSinkPair::new(DriverId(EntityId(1)), SinkId(EntityId(1)));
        let mut route = synthetic_route(
            pair,
            vec![RoutePathElement::Wire(WireId(EntityId(10)))],
            vec![PathElementStamp::Wire {
                id: WireId(EntityId(10)),
                generation: crate::ConnectionGeneration(0),
            }],
        );
        let baseline = route.fingerprint();

        route.path_stamps[0] = PathElementStamp::Wire {
            id: WireId(EntityId(10)),
            generation: crate::ConnectionGeneration(1),
        };
        assert_ne!(route.fingerprint(), baseline);
        route.path_stamps = baseline.path_stamps.clone();

        route.total_length = Fixed(11);
        assert_ne!(route.fingerprint(), baseline);
        route.total_length = baseline.total_length;

        route.segment_count = 2;
        assert_ne!(route.fingerprint(), baseline);
        route.segment_count = baseline.segment_count;

        route.delay = Tick(2);
        assert_ne!(route.fingerprint(), baseline);
    }

    #[test]
    fn only_explicit_endpoint_targets_share_signal_nodes() {
        let first_wire = WireId(EntityId(10));
        let second_wire = WireId(EntityId(11));
        assert_ne!(
            signal_node_for_endpoint(first_wire, WireEnd::B, crate::EndpointTarget::Free),
            signal_node_for_endpoint(second_wire, WireEnd::A, crate::EndpointTarget::Free),
            "coordinate-equal Free endpoints remain distinct because coordinates are not graph keys"
        );

        let junction = JunctionId(EntityId(12));
        assert_eq!(
            signal_node_for_endpoint(
                first_wire,
                WireEnd::B,
                crate::EndpointTarget::Junction(junction)
            ),
            signal_node_for_endpoint(
                second_wire,
                WireEnd::A,
                crate::EndpointTarget::Junction(junction)
            )
        );

        let port = crate::GatePortRef {
            gate: GateId(EntityId(13)),
            port: GatePort::InputA,
        };
        assert_eq!(
            signal_node_for_endpoint(
                first_wire,
                WireEnd::B,
                crate::EndpointTarget::GatePort(port)
            ),
            signal_node_for_endpoint(
                second_wire,
                WireEnd::A,
                crate::EndpointTarget::GatePort(port)
            )
        );
    }

    #[test]
    fn drive_vector_level_mapping_is_lossless() {
        let mut vector = DriveVector::default();
        for (driver, level, strength) in [
            (1, LogicLevel::High, 7),
            (2, LogicLevel::Low, 11),
            (3, LogicLevel::X, 13),
        ] {
            vector
                .checked_add_sample(crate::event::DriverSample {
                    level,
                    strength: crate::DriveStrength(strength),
                    revision: crate::Revision(0),
                    emitted_at: Tick(0),
                    driver_id: DriverId(EntityId(driver)),
                })
                .expect("sample sum fits");
        }
        assert_eq!(
            vector,
            DriveVector {
                high: 7,
                low: 11,
                unknown: 13
            }
        );
    }
}
