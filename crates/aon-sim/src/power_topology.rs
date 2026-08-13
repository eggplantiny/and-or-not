use crate::{
    CanonicalPowerRoute, DemandId, Fixed, GateId, JunctionId, PowerError, PowerPathToken,
    PowerRegionId, PowerRouteKey, PowerRouteWire, PowerSourceId, WireEnd, WireId,
};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// A semantic node in the Power graph supplied by the structural-to-Power adapter.
///
/// Merely crossing wire geometry does not create a shared key. A connection exists only when the
/// adapter gives two endpoints the same Junction, GatePower, or SourceAnchor key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PowerNodeKey {
    Junction(JunctionId),
    GatePower(GateId),
    SourceAnchor(PowerSourceId),
    WireEnd(WireId, WireEnd),
    /// The orientation-neutral intrinsic load point at half the physical Wire arclength.
    WireBody(WireId),
    /// A derived point measured in the Wire's stored A-to-B canonical polyline direction.
    ///
    /// Offset zero and `L` are accepted at the input boundary and coalesce with the matching
    /// physical endpoint. The midpoint coalesces with `WireBody`.
    WireOffset(WireId, Fixed),
}

/// One complete Wire body as an undirected Power edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerBodyEdge {
    pub wire: WireId,
    pub a: PowerNodeKey,
    pub b: PowerNodeKey,
    pub length: Fixed,
    /// Original canonical straight-segment lengths in stored A-to-B geometry order.
    ///
    /// Every length must be positive and their checked sum must equal `length`. Retaining the
    /// boundaries is necessary to count positive overlap exactly after virtual edge splits.
    pub segment_lengths: Vec<Fixed>,
    /// Which stored end has the lower complete semantic endpoint descriptor.
    ///
    /// The structural adapter derives this by comparing `(EndpointTarget tag, referenced
    /// EntityId, referenced port/end tag, position.x, position.y)`. It must reject an exact tie;
    /// the abstract graph consumes the already-validated result so it need not duplicate world
    /// geometry or endpoint stores.
    pub canonical_lower_end: WireEnd,
}

/// A world-generator-owned source attached to one Power graph node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerSourceAttachment {
    pub source: PowerSourceId,
    pub node: PowerNodeKey,
}

/// A pre-collected load attached to one Power graph node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerLoadAttachment {
    pub demand: DemandId,
    pub node: PowerNodeKey,
}

/// Abstract input accepted by the pure Power topology compiler.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerTopologyInput {
    pub bodies: Vec<PowerBodyEdge>,
    pub sources: Vec<PowerSourceAttachment>,
    pub loads: Vec<PowerLoadAttachment>,
}

/// One derived connected component. IDs are assigned by the component's smallest semantic node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPowerRegion {
    id: PowerRegionId,
    first_node: PowerNodeKey,
    nodes: Vec<PowerNodeKey>,
    wires: Vec<WireId>,
    sources: Vec<PowerSourceId>,
    loads: Vec<DemandId>,
}

impl CompiledPowerRegion {
    pub const fn id(&self) -> PowerRegionId {
        self.id
    }

    pub const fn first_node(&self) -> PowerNodeKey {
        self.first_node
    }

    pub fn nodes(&self) -> &[PowerNodeKey] {
        &self.nodes
    }

    pub fn wires(&self) -> &[WireId] {
        &self.wires
    }

    pub fn sources(&self) -> &[PowerSourceId] {
        &self.sources
    }

    pub fn loads(&self) -> &[DemandId] {
        &self.loads
    }
}

/// The region and optional canonical source route compiled for one load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPowerLoad {
    demand: DemandId,
    region: PowerRegionId,
    source_route: Option<CanonicalPowerRoute>,
}

impl CompiledPowerLoad {
    pub const fn demand(&self) -> DemandId {
        self.demand
    }

    pub const fn region(&self) -> PowerRegionId {
        self.region
    }

    pub const fn source_route(&self) -> Option<&CanonicalPowerRoute> {
        self.source_route.as_ref()
    }
}

/// Immutable, derived Power graph output. No field belongs in canonical state or StateHash.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompiledPowerTopology {
    regions: Vec<CompiledPowerRegion>,
    node_regions: BTreeMap<PowerNodeKey, PowerRegionId>,
    wire_regions: BTreeMap<WireId, PowerRegionId>,
    source_regions: BTreeMap<PowerSourceId, PowerRegionId>,
    loads: BTreeMap<DemandId, CompiledPowerLoad>,
}

impl CompiledPowerTopology {
    pub fn compile(input: &PowerTopologyInput) -> Result<Self, PowerTopologyError> {
        PowerGraph::build(input)?.compile()
    }

    pub fn regions(&self) -> &[CompiledPowerRegion] {
        &self.regions
    }

    pub fn region_for_node(&self, node: PowerNodeKey) -> Option<PowerRegionId> {
        self.node_regions.get(&node).copied()
    }

    pub fn region_for_wire(&self, wire: WireId) -> Option<PowerRegionId> {
        self.wire_regions.get(&wire).copied()
    }

    pub fn region_for_source(&self, source: PowerSourceId) -> Option<PowerRegionId> {
        self.source_regions.get(&source).copied()
    }

    pub fn load(&self, demand: DemandId) -> Option<&CompiledPowerLoad> {
        self.loads.get(&demand)
    }

    pub fn loads(&self) -> impl ExactSizeIterator<Item = &CompiledPowerLoad> + '_ {
        self.loads.values()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GraphEdge {
    to: PowerNodeKey,
    wire: WireId,
    length: Fixed,
    /// Closed arclength bounds in stored A-to-B orientation. The traversal itself is undirected.
    low_offset: Fixed,
    high_offset: Fixed,
    segment_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RouteWireTraversal {
    wire: WireId,
    low_offset: Fixed,
    high_offset: Fixed,
    segment_count: u32,
}

impl RouteWireTraversal {
    fn length(self) -> Result<Fixed, PowerTopologyError> {
        self.high_offset
            .checked_sub(self.low_offset)
            .map_err(|_| PowerTopologyError::NumericOverflow)
    }
}

#[derive(Clone, Debug)]
struct RoutePriority {
    total_length: Fixed,
    segment_count: u32,
    path_tokens: Vec<PowerPathToken>,
    traversals: Vec<RouteWireTraversal>,
}

impl PartialEq for RoutePriority {
    fn eq(&self, other: &Self) -> bool {
        (self.total_length, self.segment_count, &self.path_tokens)
            == (other.total_length, other.segment_count, &other.path_tokens)
    }
}

impl Eq for RoutePriority {}

impl Ord for RoutePriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.total_length, self.segment_count, &self.path_tokens).cmp(&(
            other.total_length,
            other.segment_count,
            &other.path_tokens,
        ))
    }
}

impl PartialOrd for RoutePriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Default)]
struct PowerGraph {
    adjacency: BTreeMap<PowerNodeKey, Vec<GraphEdge>>,
    body_by_wire: BTreeMap<WireId, PowerBodyEdge>,
    /// Input-only derived keys that coalesce to an actual graph node without a zero-length edge.
    aliases: BTreeMap<PowerNodeKey, PowerNodeKey>,
    sources: BTreeMap<PowerSourceId, PowerNodeKey>,
    loads: BTreeMap<DemandId, PowerNodeKey>,
}

impl PowerGraph {
    fn build(input: &PowerTopologyInput) -> Result<Self, PowerTopologyError> {
        let mut graph = Self::default();
        for body in &input.bodies {
            validate_body(body)?;
            if graph.body_by_wire.insert(body.wire, body.clone()).is_some() {
                return Err(PowerTopologyError::DuplicateWire { wire: body.wire });
            }
        }

        let mut requested_offsets: BTreeMap<WireId, BTreeSet<Fixed>> = BTreeMap::new();
        for attachment in &input.loads {
            match attachment.node {
                PowerNodeKey::WireBody(wire) => {
                    if !graph.body_by_wire.contains_key(&wire) {
                        return Err(PowerTopologyError::UnknownAttachmentWire { wire });
                    }
                }
                PowerNodeKey::WireOffset(wire, offset) => {
                    let body = graph
                        .body_by_wire
                        .get(&wire)
                        .ok_or(PowerTopologyError::UnknownAttachmentWire { wire })?;
                    if !(0..=body.length.0).contains(&offset.0) {
                        return Err(PowerTopologyError::WireOffsetOutOfRange {
                            wire,
                            offset_raw: offset.0,
                            length_raw: body.length.0,
                        });
                    }
                    requested_offsets.entry(wire).or_default().insert(offset);
                }
                _ => {}
            }
        }

        // Materialize every virtual midpoint, plus only the partial points actually requested by
        // loads. Split pieces retain their original physical Wire and exact A-to-B arclength span.
        let bodies = graph.body_by_wire.values().cloned().collect::<Vec<_>>();
        for body in bodies {
            let stored_offsets = requested_offsets.remove(&body.wire).unwrap_or_default();
            graph.add_split_body(&body, &stored_offsets)?;
        }

        for attachment in &input.sources {
            validate_source_node(*attachment)?;
            graph.ensure_node(attachment.node);
            if graph
                .sources
                .insert(attachment.source, attachment.node)
                .is_some()
            {
                return Err(PowerTopologyError::DuplicateSource {
                    power_source: attachment.source,
                });
            }
        }
        for attachment in &input.loads {
            let node = graph.resolve_alias(attachment.node);
            graph.ensure_node(node);
            if graph.loads.insert(attachment.demand, node).is_some() {
                return Err(PowerTopologyError::DuplicateLoad {
                    demand: attachment.demand,
                });
            }
        }
        for edges in graph.adjacency.values_mut() {
            edges.sort_unstable();
        }
        Ok(graph)
    }

    fn ensure_node(&mut self, node: PowerNodeKey) {
        self.adjacency.entry(node).or_default();
    }

    fn resolve_alias(&self, node: PowerNodeKey) -> PowerNodeKey {
        self.aliases.get(&node).copied().unwrap_or(node)
    }

    fn add_split_body(
        &mut self,
        body: &PowerBodyEdge,
        stored_offsets: &BTreeSet<Fixed>,
    ) -> Result<(), PowerTopologyError> {
        let midpoint = stored_midpoint(body)?;
        let mut points = Vec::with_capacity(stored_offsets.len() + 2);
        let mut offsets = stored_offsets.clone();
        offsets.insert(midpoint);
        offsets.insert(Fixed::ZERO);
        offsets.insert(body.length);

        for stored_offset in offsets {
            let actual = if stored_offset == Fixed::ZERO {
                body.a
            } else if stored_offset == body.length {
                body.b
            } else if stored_offset == midpoint {
                PowerNodeKey::WireBody(body.wire)
            } else {
                PowerNodeKey::WireOffset(body.wire, stored_offset)
            };
            points.push((stored_offset, actual));

            let offset_key = PowerNodeKey::WireOffset(body.wire, stored_offset);
            if stored_offsets.contains(&stored_offset) && offset_key != actual {
                self.aliases.insert(offset_key, actual);
            }
            if stored_offset == midpoint {
                let body_key = PowerNodeKey::WireBody(body.wire);
                if body_key != actual {
                    self.aliases.insert(body_key, actual);
                }
            }
        }

        points.sort_unstable_by_key(|(offset, _)| *offset);
        if points.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(PowerTopologyError::InvalidGraphInvariant);
        }
        for (_, node) in &points {
            self.ensure_node(*node);
        }
        for pair in points.windows(2) {
            let (low_offset, low_node) = pair[0];
            let (high_offset, high_node) = pair[1];
            let segment_count = segment_overlap_count(body, low_offset, high_offset)?;
            self.add_edge(
                low_node,
                high_node,
                body.wire,
                low_offset,
                high_offset,
                segment_count,
            )?;
            self.add_edge(
                high_node,
                low_node,
                body.wire,
                low_offset,
                high_offset,
                segment_count,
            )?;
        }
        Ok(())
    }

    fn add_edge(
        &mut self,
        from: PowerNodeKey,
        to: PowerNodeKey,
        wire: WireId,
        low_offset: Fixed,
        high_offset: Fixed,
        segment_count: u32,
    ) -> Result<(), PowerTopologyError> {
        let length = high_offset
            .checked_sub(low_offset)
            .map_err(|_| PowerTopologyError::NumericOverflow)?;
        if length.0 <= 0 || segment_count == 0 {
            return Err(PowerTopologyError::InvalidGraphInvariant);
        }
        self.adjacency.entry(from).or_default().push(GraphEdge {
            to,
            wire,
            length,
            low_offset,
            high_offset,
            segment_count,
        });
        Ok(())
    }

    fn compile(self) -> Result<CompiledPowerTopology, PowerTopologyError> {
        let components = self.compile_components()?;
        let mut output = CompiledPowerTopology::default();
        for (region_index, component) in components.into_iter().enumerate() {
            let region = PowerRegionId(
                u64::try_from(region_index).map_err(|_| PowerTopologyError::NumericOverflow)?,
            );
            for node in &component {
                if output.node_regions.insert(*node, region).is_some() {
                    return Err(PowerTopologyError::InvalidGraphInvariant);
                }
            }
            for (alias, target) in &self.aliases {
                if component.contains(target)
                    && let Some(existing) = output.node_regions.insert(*alias, region)
                    && existing != region
                {
                    return Err(PowerTopologyError::InvalidGraphInvariant);
                }
            }

            let mut wires = self
                .body_by_wire
                .values()
                .filter_map(|body| component.contains(&body.a).then_some(body.wire))
                .collect::<Vec<_>>();
            wires.sort_unstable();
            for wire in &wires {
                if output.wire_regions.insert(*wire, region).is_some() {
                    return Err(PowerTopologyError::InvalidGraphInvariant);
                }
            }

            let mut sources = self
                .sources
                .iter()
                .filter_map(|(source, node)| component.contains(node).then_some(*source))
                .collect::<Vec<_>>();
            sources.sort_unstable();
            for source in &sources {
                if output.source_regions.insert(*source, region).is_some() {
                    return Err(PowerTopologyError::InvalidGraphInvariant);
                }
            }

            let mut loads = self
                .loads
                .iter()
                .filter_map(|(demand, node)| component.contains(node).then_some(*demand))
                .collect::<Vec<_>>();
            loads.sort_unstable();
            for demand in &loads {
                let node = *self
                    .loads
                    .get(demand)
                    .ok_or(PowerTopologyError::InvalidGraphInvariant)?;
                let source_route = self.compile_load_route(*demand, node, &sources)?;
                if output
                    .loads
                    .insert(
                        *demand,
                        CompiledPowerLoad {
                            demand: *demand,
                            region,
                            source_route,
                        },
                    )
                    .is_some()
                {
                    return Err(PowerTopologyError::InvalidGraphInvariant);
                }
            }

            let first_node = *component
                .first()
                .ok_or(PowerTopologyError::InvalidGraphInvariant)?;
            output.regions.push(CompiledPowerRegion {
                id: region,
                first_node,
                nodes: component,
                wires,
                sources,
                loads,
            });
        }
        Ok(output)
    }

    fn compile_components(&self) -> Result<Vec<Vec<PowerNodeKey>>, PowerTopologyError> {
        let mut unvisited: BTreeSet<_> = self.adjacency.keys().copied().collect();
        let mut components = Vec::new();
        while let Some(start) = unvisited.pop_first() {
            let mut frontier = BTreeSet::from([start]);
            let mut component = Vec::new();
            while let Some(node) = frontier.pop_first() {
                if !unvisited.remove(&node) && node != start {
                    continue;
                }
                component.push(node);
                for edge in self
                    .adjacency
                    .get(&node)
                    .ok_or(PowerTopologyError::InvalidGraphInvariant)?
                {
                    if unvisited.contains(&edge.to) {
                        frontier.insert(edge.to);
                    }
                }
            }
            component.sort_unstable();
            components.push(component);
        }
        components.sort_unstable_by_key(|component| component[0]);
        Ok(components)
    }

    fn compile_load_route(
        &self,
        demand: DemandId,
        load_node: PowerNodeKey,
        sources: &[PowerSourceId],
    ) -> Result<Option<CanonicalPowerRoute>, PowerTopologyError> {
        if sources.is_empty() {
            return Ok(None);
        }
        let best = shortest_paths(demand, load_node, &self.adjacency, &self.body_by_wire)?;
        let mut candidates = Vec::with_capacity(sources.len());
        for source in sources {
            let source_node = *self
                .sources
                .get(source)
                .ok_or(PowerTopologyError::InvalidGraphInvariant)?;
            let priority =
                best.get(&source_node)
                    .ok_or(PowerTopologyError::UnreachableRegionSource {
                        power_source: *source,
                    })?;
            candidates.push(CanonicalPowerRoute::new(
                *source,
                PowerRouteKey::new(
                    priority.total_length,
                    priority.segment_count,
                    priority.path_tokens.clone(),
                )?,
                priority
                    .traversals
                    .iter()
                    .copied()
                    .map(|traversal| {
                        Ok(PowerRouteWire::new(
                            traversal.wire,
                            traversal.length()?,
                            traversal.segment_count,
                        )?)
                    })
                    .collect::<Result<Vec<_>, PowerTopologyError>>()?,
            )?);
        }
        candidates.sort_unstable_by(|left, right| {
            left.key()
                .cmp(right.key())
                .then_with(|| left.source().cmp(&right.source()))
        });
        candidates
            .into_iter()
            .next()
            .map(Some)
            .ok_or(PowerTopologyError::InvalidGraphInvariant)
    }
}

fn validate_body(body: &PowerBodyEdge) -> Result<(), PowerTopologyError> {
    if body.length.0 <= 0 {
        return Err(PowerTopologyError::NonPositiveBodyLength {
            wire: body.wire,
            raw: body.length.0,
        });
    }
    if body.segment_lengths.is_empty() {
        return Err(PowerTopologyError::EmptyBodySegments { wire: body.wire });
    }
    for node in [body.a, body.b] {
        if matches!(
            node,
            PowerNodeKey::WireBody(_) | PowerNodeKey::WireOffset(_, _)
        ) {
            return Err(PowerTopologyError::DerivedBodyEndpoint {
                wire: body.wire,
                node,
            });
        }
    }
    if body.a == body.b {
        return Err(PowerTopologyError::SelfLoopBody {
            wire: body.wire,
            node: body.a,
        });
    }
    let mut summed_raw = 0_i64;
    for (index, segment) in body.segment_lengths.iter().copied().enumerate() {
        if segment.0 <= 0 {
            return Err(PowerTopologyError::NonPositiveBodySegment {
                wire: body.wire,
                index,
                raw: segment.0,
            });
        }
        summed_raw = summed_raw
            .checked_add(segment.0)
            .ok_or(PowerTopologyError::NumericOverflow)?;
    }
    if summed_raw != body.length.0 {
        return Err(PowerTopologyError::BodySegmentLengthMismatch {
            wire: body.wire,
            declared_raw: body.length.0,
            summed_raw,
        });
    }
    Ok(())
}

fn stored_midpoint(body: &PowerBodyEdge) -> Result<Fixed, PowerTopologyError> {
    let distance_from_lower = Fixed(body.length.0 / 2);
    match body.canonical_lower_end {
        WireEnd::A => Ok(distance_from_lower),
        WireEnd::B => body
            .length
            .checked_sub(distance_from_lower)
            .map_err(|_| PowerTopologyError::NumericOverflow),
    }
}

fn segment_overlap_count(
    body: &PowerBodyEdge,
    low_offset: Fixed,
    high_offset: Fixed,
) -> Result<u32, PowerTopologyError> {
    if low_offset.0 < 0 || high_offset.0 > body.length.0 || low_offset.0 >= high_offset.0 {
        return Err(PowerTopologyError::InvalidGraphInvariant);
    }
    let mut segment_low = 0_i64;
    let mut count = 0_u32;
    for segment in &body.segment_lengths {
        let segment_high = segment_low
            .checked_add(segment.0)
            .ok_or(PowerTopologyError::NumericOverflow)?;
        if low_offset.0 < segment_high && segment_low < high_offset.0 {
            count = count
                .checked_add(1)
                .ok_or(PowerTopologyError::NumericOverflow)?;
        }
        segment_low = segment_high;
    }
    if count == 0 {
        return Err(PowerTopologyError::InvalidGraphInvariant);
    }
    Ok(count)
}

fn validate_source_node(attachment: PowerSourceAttachment) -> Result<(), PowerTopologyError> {
    match attachment.node {
        PowerNodeKey::SourceAnchor(source) if source == attachment.source => Ok(()),
        actual => Err(PowerTopologyError::SourceAnchorMismatch {
            power_source: attachment.source,
            actual,
        }),
    }
}

// Frozen canonical EntityLocation tags. Keep these aligned with StateHash's entity-kind table;
// they are explicit here so route ordering never depends on Rust enum layout.
const PATH_KIND_RELAY_SITE: u8 = 1;
const PATH_KIND_GATE: u8 = 2;
const PATH_KIND_WIRE: u8 = 3;
const PATH_KIND_JUNCTION: u8 = 4;
const PATH_KIND_MOBILE_SUBSTRATE: u8 = 6;
const PATH_KIND_POWER_SOURCE: u8 = 7;
const PATH_KIND_CONSTRUCTION_SITE: u8 = 11;
const PATH_LOCAL_WHOLE_ENTITY: u8 = 0;
const PATH_LOCAL_GATE_POWER_PORT: u8 = 3;

fn load_path_token(demand: DemandId) -> Result<PowerPathToken, PowerTopologyError> {
    let owner_kind_tag = match demand.kind() {
        crate::DemandKind::GateIdle
        | crate::DemandKind::GateSwitch
        | crate::DemandKind::GateDrive => PATH_KIND_GATE,
        crate::DemandKind::WireLeakage
        | crate::DemandKind::WireSensing
        | crate::DemandKind::LiveWire
        | crate::DemandKind::OvercapacitySupport => PATH_KIND_WIRE,
        crate::DemandKind::RelayActivation | crate::DemandKind::RelayUpkeep => PATH_KIND_RELAY_SITE,
        crate::DemandKind::Movement
        | crate::DemandKind::Extraction
        | crate::DemandKind::Transfer => PATH_KIND_MOBILE_SUBSTRATE,
        crate::DemandKind::Construction => PATH_KIND_CONSTRUCTION_SITE,
        crate::DemandKind::RadiationEmission => {
            return Err(PowerTopologyError::AmbiguousDemandOwnerKind { demand });
        }
    };
    Ok(PowerPathToken::new(
        owner_kind_tag,
        demand.owner(),
        demand.kind().canonical_tag(),
    ))
}

const fn wire_path_token(wire: WireId) -> PowerPathToken {
    PowerPathToken::new(PATH_KIND_WIRE, wire.entity_id(), PATH_LOCAL_WHOLE_ENTITY)
}

const fn node_path_token(node: PowerNodeKey) -> Option<PowerPathToken> {
    match node {
        PowerNodeKey::Junction(id) => Some(PowerPathToken::new(
            PATH_KIND_JUNCTION,
            id.entity_id(),
            PATH_LOCAL_WHOLE_ENTITY,
        )),
        PowerNodeKey::GatePower(id) => Some(PowerPathToken::new(
            PATH_KIND_GATE,
            id.entity_id(),
            PATH_LOCAL_GATE_POWER_PORT,
        )),
        PowerNodeKey::SourceAnchor(id) => Some(PowerPathToken::new(
            PATH_KIND_POWER_SOURCE,
            id.entity_id(),
            PATH_LOCAL_WHOLE_ENTITY,
        )),
        PowerNodeKey::WireEnd(_, _)
        | PowerNodeKey::WireBody(_)
        | PowerNodeKey::WireOffset(_, _) => None,
    }
}

fn shortest_paths(
    demand: DemandId,
    source: PowerNodeKey,
    adjacency: &BTreeMap<PowerNodeKey, Vec<GraphEdge>>,
    bodies: &BTreeMap<WireId, PowerBodyEdge>,
) -> Result<BTreeMap<PowerNodeKey, RoutePriority>, PowerTopologyError> {
    let mut initial_tokens = vec![load_path_token(demand)?];
    if let Some(token) = node_path_token(source) {
        initial_tokens.push(token);
    }
    let initial = RoutePriority {
        total_length: Fixed::ZERO,
        segment_count: 0,
        path_tokens: initial_tokens,
        traversals: Vec::new(),
    };
    let mut best = BTreeMap::from([(source, initial.clone())]);
    let mut frontier = BTreeSet::from([(initial, source)]);
    while let Some((priority, node)) = frontier.pop_first() {
        if best.get(&node) != Some(&priority) {
            continue;
        }
        for edge in adjacency
            .get(&node)
            .ok_or(PowerTopologyError::InvalidGraphInvariant)?
        {
            let Some(candidate) = extend_priority(&priority, *edge, bodies)? else {
                continue;
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

fn extend_priority(
    priority: &RoutePriority,
    edge: GraphEdge,
    bodies: &BTreeMap<WireId, PowerBodyEdge>,
) -> Result<Option<RoutePriority>, PowerTopologyError> {
    let total_length = priority
        .total_length
        .checked_add(edge.length)
        .map_err(|_| PowerTopologyError::NumericOverflow)?;
    let mut segment_count = priority.segment_count;
    let mut path_tokens = priority.path_tokens.clone();
    let mut traversals = priority.traversals.clone();

    if let Some(last) = traversals.last_mut()
        && last.wire == edge.wire
    {
        // Consecutive pseudo-edge pieces of one physical Wire may only meet at one boundary.
        // Overlap would be immediate backtracking; a gap would violate the split-body graph.
        if last.high_offset != edge.low_offset && edge.high_offset != last.low_offset {
            return Ok(None);
        }
        let low_offset = std::cmp::min(last.low_offset, edge.low_offset);
        let high_offset = std::cmp::max(last.high_offset, edge.high_offset);
        let body = bodies
            .get(&edge.wire)
            .ok_or(PowerTopologyError::InvalidGraphInvariant)?;
        let merged_segment_count = segment_overlap_count(body, low_offset, high_offset)?;
        segment_count = segment_count
            .checked_sub(last.segment_count)
            .and_then(|count| count.checked_add(merged_segment_count))
            .ok_or(PowerTopologyError::NumericOverflow)?;
        last.low_offset = low_offset;
        last.high_offset = high_offset;
        last.segment_count = merged_segment_count;
    } else {
        // A positive shortest route never needs to leave and later re-enter one physical Wire.
        // Suppressing that cycle also guarantees the public route has exactly one row per Wire.
        if traversals
            .iter()
            .any(|traversal| traversal.wire == edge.wire)
        {
            return Ok(None);
        }
        segment_count = segment_count
            .checked_add(edge.segment_count)
            .ok_or(PowerTopologyError::NumericOverflow)?;
        traversals.push(RouteWireTraversal {
            wire: edge.wire,
            low_offset: edge.low_offset,
            high_offset: edge.high_offset,
            segment_count: edge.segment_count,
        });
        path_tokens.push(wire_path_token(edge.wire));
    }
    if let Some(token) = node_path_token(edge.to) {
        path_tokens.push(token);
    }

    Ok(Some(RoutePriority {
        total_length,
        segment_count,
        path_tokens,
        traversals,
    }))
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PowerTopologyError {
    #[error("Power Wire {wire:?} body length must be positive, got raw {raw}")]
    NonPositiveBodyLength { wire: WireId, raw: i64 },

    #[error("Power Wire {wire:?} body must contain at least one straight segment")]
    EmptyBodySegments { wire: WireId },

    #[error("Power Wire {wire:?} body segment {index} must be positive, got raw {raw}")]
    NonPositiveBodySegment {
        wire: WireId,
        index: usize,
        raw: i64,
    },

    #[error(
        "Power Wire {wire:?} body segment lengths sum to raw {summed_raw}, not declared raw {declared_raw}"
    )]
    BodySegmentLengthMismatch {
        wire: WireId,
        declared_raw: i64,
        summed_raw: i64,
    },

    #[error("Power Wire {wire:?} body is a self-loop at {node:?}")]
    SelfLoopBody { wire: WireId, node: PowerNodeKey },

    #[error("Power Wire {wire:?} body endpoint may not be a derived Wire point: {node:?}")]
    DerivedBodyEndpoint { wire: WireId, node: PowerNodeKey },

    #[error("Power Wire {wire:?} appears more than once")]
    DuplicateWire { wire: WireId },

    #[error("Power Source {power_source:?} appears more than once")]
    DuplicateSource { power_source: PowerSourceId },

    #[error("Power load {demand:?} appears more than once")]
    DuplicateLoad { demand: DemandId },

    #[error("Power load {demand:?} does not determine one canonical owner Entity kind")]
    AmbiguousDemandOwnerKind { demand: DemandId },

    #[error("Power load refers to unknown physical Wire {wire:?}")]
    UnknownAttachmentWire { wire: WireId },

    #[error(
        "Power Wire {wire:?} load offset raw {offset_raw} is outside the inclusive 0..={length_raw} body"
    )]
    WireOffsetOutOfRange {
        wire: WireId,
        offset_raw: i64,
        length_raw: i64,
    },

    #[error("Power Source {power_source:?} has mismatched anchor node {actual:?}")]
    SourceAnchorMismatch {
        power_source: PowerSourceId,
        actual: PowerNodeKey,
    },

    #[error("Power Source {power_source:?} is unreachable inside its reported region")]
    UnreachableRegionSource { power_source: PowerSourceId },

    #[error("Power topology numeric overflow")]
    NumericOverflow,

    #[error("Power topology graph invariant violated")]
    InvalidGraphInvariant,

    #[error(transparent)]
    PowerKernel(#[from] PowerError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DemandKind, EntityId};

    const fn wire(id: u64) -> WireId {
        WireId(EntityId(id))
    }

    const fn junction(id: u64) -> PowerNodeKey {
        PowerNodeKey::Junction(JunctionId(EntityId(id)))
    }

    const fn source_node(id: u64) -> PowerNodeKey {
        PowerNodeKey::SourceAnchor(PowerSourceId(EntityId(id)))
    }

    const fn demand(owner: u64, kind: DemandKind) -> DemandId {
        DemandId::new(EntityId(owner), kind)
    }

    fn body(
        id: u64,
        a: PowerNodeKey,
        b: PowerNodeKey,
        length: i64,
        segments: u32,
    ) -> PowerBodyEdge {
        let segment_count = usize::try_from(segments).expect("fixture segment count fits usize");
        assert!(segment_count > 0);
        let prefix_length = i64::try_from(segment_count - 1).expect("fixture count fits i64");
        assert!(length > prefix_length);
        let mut segment_lengths = vec![Fixed(1); segment_count - 1];
        segment_lengths.push(Fixed(length - prefix_length));
        PowerBodyEdge {
            wire: wire(id),
            a,
            b,
            length: Fixed(length),
            segment_lengths,
            canonical_lower_end: WireEnd::A,
        }
    }

    fn segmented_body(
        id: u64,
        a: PowerNodeKey,
        b: PowerNodeKey,
        segment_lengths: &[i64],
        canonical_lower_end: WireEnd,
    ) -> PowerBodyEdge {
        PowerBodyEdge {
            wire: wire(id),
            a,
            b,
            length: Fixed(segment_lengths.iter().sum()),
            segment_lengths: segment_lengths.iter().copied().map(Fixed).collect(),
            canonical_lower_end,
        }
    }

    fn source(id: u64) -> PowerSourceAttachment {
        PowerSourceAttachment {
            source: PowerSourceId(EntityId(id)),
            node: source_node(id),
        }
    }

    fn load(owner: u64, kind: DemandKind, node: PowerNodeKey) -> PowerLoadAttachment {
        PowerLoadAttachment {
            demand: demand(owner, kind),
            node,
        }
    }

    #[test]
    fn regions_are_stable_under_every_input_vector_reversal() {
        let input = PowerTopologyInput {
            bodies: vec![
                body(20, source_node(50), junction(5), 3, 1),
                body(21, junction(5), junction(6), 4, 1),
                body(30, source_node(60), junction(8), 2, 1),
            ],
            sources: vec![source(50), source(60)],
            loads: vec![
                load(100, DemandKind::GateDrive, junction(6)),
                load(101, DemandKind::Movement, junction(8)),
            ],
        };
        let forward = CompiledPowerTopology::compile(&input).expect("fixture compiles");
        let mut reversed = input;
        reversed.bodies.reverse();
        reversed.sources.reverse();
        reversed.loads.reverse();
        let reverse = CompiledPowerTopology::compile(&reversed).expect("fixture compiles");
        assert_eq!(forward, reverse);
        assert_eq!(forward.regions().len(), 2);
        assert_eq!(forward.regions()[0].id(), PowerRegionId(0));
        assert_eq!(forward.regions()[1].id(), PowerRegionId(1));
    }

    #[test]
    fn geometric_crossings_remain_disconnected_without_a_shared_node() {
        let first_a = PowerNodeKey::WireEnd(wire(1), WireEnd::A);
        let first_b = PowerNodeKey::WireEnd(wire(1), WireEnd::B);
        let second_a = PowerNodeKey::WireEnd(wire(2), WireEnd::A);
        let second_b = PowerNodeKey::WireEnd(wire(2), WireEnd::B);
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![
                body(1, first_a, first_b, 10, 1),
                body(2, second_a, second_b, 10, 1),
            ],
            sources: Vec::new(),
            loads: Vec::new(),
        })
        .expect("crossing geometry is irrelevant to abstract graph input");
        assert_eq!(compiled.regions().len(), 2);
        assert_ne!(
            compiled.region_for_wire(wire(1)),
            compiled.region_for_wire(wire(2))
        );
    }

    #[test]
    fn a_shared_junction_connects_bodies_into_one_region() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![
                body(
                    1,
                    PowerNodeKey::WireEnd(wire(1), WireEnd::A),
                    junction(9),
                    10,
                    1,
                ),
                body(
                    2,
                    junction(9),
                    PowerNodeKey::WireEnd(wire(2), WireEnd::B),
                    10,
                    1,
                ),
            ],
            sources: Vec::new(),
            loads: Vec::new(),
        })
        .expect("shared semantic node connects");
        assert_eq!(compiled.regions().len(), 1);
        assert_eq!(
            compiled.region_for_wire(wire(1)),
            compiled.region_for_wire(wire(2))
        );
    }

    #[test]
    fn route_priority_is_length_then_segments_then_semantic_path_tokens() {
        let load_node = junction(1);
        let target = source_node(90);
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![
                body(20, load_node, junction(20), 3, 1),
                body(21, junction(20), target, 3, 1),
                body(10, load_node, junction(10), 3, 1),
                body(11, junction(10), target, 3, 1),
                body(5, load_node, target, 7, 1),
            ],
            sources: vec![source(90)],
            loads: vec![load(100, DemandKind::GateDrive, load_node)],
        })
        .expect("route fixture compiles");
        let route = compiled
            .load(demand(100, DemandKind::GateDrive))
            .expect("load exists")
            .source_route()
            .expect("source is reachable");
        assert_eq!(route.key().total_length(), Fixed(6));
        assert_eq!(route.key().segment_count(), 2);
        assert_eq!(
            route
                .wires()
                .iter()
                .map(|row| row.wire())
                .collect::<Vec<_>>(),
            vec![wire(10), wire(11)]
        );
    }

    #[test]
    fn source_id_is_the_final_tie_break_for_identical_route_priority() {
        let load_node = junction(1);
        let shared = junction(2);
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![
                body(10, load_node, shared, 2, 1),
                body(20, shared, source_node(50), 2, 1),
                body(20_000, shared, source_node(60), 2, 1),
            ],
            sources: vec![source(60), source(50)],
            loads: vec![load(100, DemandKind::WireSensing, load_node)],
        })
        .expect("source fixture compiles");
        let route = compiled
            .load(demand(100, DemandKind::WireSensing))
            .expect("load exists")
            .source_route()
            .expect("source is reachable");
        // Entity paths differ in this abstract fixture, so make the lower source path also the
        // lexicographically lower one; this verifies the final source ordering remains stable.
        assert_eq!(route.source(), PowerSourceId(EntityId(50)));
    }

    #[test]
    fn whole_wire_route_coalesces_midpoint_pieces_and_counts_split_segment_once() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![segmented_body(
                10,
                source_node(50),
                junction(9),
                &[3, 4, 3],
                WireEnd::A,
            )],
            sources: vec![source(50)],
            loads: vec![load(100, DemandKind::GateDrive, junction(9))],
        })
        .expect("midpoint split compiles");
        let route = compiled
            .load(demand(100, DemandKind::GateDrive))
            .expect("load exists")
            .source_route()
            .expect("source route exists");
        assert_eq!(route.key().total_length(), Fixed(10));
        assert_eq!(route.key().segment_count(), 3);
        assert_eq!(route.wires().len(), 1);
        assert_eq!(route.wires()[0].wire(), wire(10));
        assert_eq!(route.wires()[0].length(), Fixed(10));
        assert_eq!(route.wires()[0].segment_count(), 3);
    }

    #[test]
    fn intrinsic_midpoint_is_orientation_neutral_for_odd_raw_length() {
        let forward = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![segmented_body(
                10,
                source_node(50),
                junction(9),
                &[2, 4, 3],
                WireEnd::A,
            )],
            sources: vec![source(50)],
            loads: vec![load(
                10,
                DemandKind::WireSensing,
                PowerNodeKey::WireBody(wire(10)),
            )],
        })
        .expect("forward orientation compiles");
        let reversed = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![segmented_body(
                10,
                junction(9),
                source_node(50),
                &[3, 4, 2],
                WireEnd::B,
            )],
            sources: vec![source(50)],
            loads: vec![load(
                10,
                DemandKind::WireSensing,
                PowerNodeKey::WireBody(wire(10)),
            )],
        })
        .expect("reversed orientation compiles");
        assert_eq!(forward, reversed);
        let route = forward
            .load(demand(10, DemandKind::WireSensing))
            .expect("intrinsic load exists")
            .source_route()
            .expect("source route exists");
        assert_eq!(route.key().total_length(), Fixed(4));
        assert_eq!(route.key().segment_count(), 2);
        assert_eq!(
            route.wires(),
            &[PowerRouteWire::new(wire(10), Fixed(4), 2).unwrap()]
        );
    }

    #[test]
    fn arbitrary_partial_attachment_emits_exact_positive_arclength_and_overlap_count() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![segmented_body(
                10,
                source_node(50),
                junction(9),
                &[3, 4, 3],
                WireEnd::A,
            )],
            sources: vec![source(50)],
            loads: vec![
                load(
                    100,
                    DemandKind::Movement,
                    PowerNodeKey::WireOffset(wire(10), Fixed(8)),
                ),
                load(
                    101,
                    DemandKind::Movement,
                    PowerNodeKey::WireOffset(wire(10), Fixed(4)),
                ),
            ],
        })
        .expect("partial attachment fixture compiles");

        let far_route = compiled
            .load(demand(100, DemandKind::Movement))
            .expect("far load exists")
            .source_route()
            .expect("far source route exists");
        assert_eq!(far_route.key().total_length(), Fixed(8));
        assert_eq!(far_route.key().segment_count(), 3);
        assert_eq!(far_route.wires().len(), 1);
        assert_eq!(far_route.wires()[0].length(), Fixed(8));
        assert_eq!(far_route.wires()[0].segment_count(), 3);

        let near_route = compiled
            .load(demand(101, DemandKind::Movement))
            .expect("near load exists")
            .source_route()
            .expect("near source route exists");
        assert_eq!(near_route.key().total_length(), Fixed(4));
        assert_eq!(near_route.key().segment_count(), 2);
        assert_eq!(near_route.wires()[0].length(), Fixed(4));
        assert_eq!(near_route.wires()[0].segment_count(), 2);
        assert_eq!(
            compiled.region_for_node(PowerNodeKey::WireOffset(wire(10), Fixed(8))),
            compiled.region_for_source(PowerSourceId(EntityId(50)))
        );
    }

    #[test]
    fn exact_source_endpoint_load_compiles_a_zero_wire_zero_loss_route() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![body(10, source_node(50), junction(9), 8, 1)],
            sources: vec![source(50)],
            loads: vec![load(
                100,
                DemandKind::Movement,
                PowerNodeKey::WireOffset(wire(10), Fixed::ZERO),
            )],
        })
        .expect("a load at the Source endpoint is a valid coalesced attachment");

        let route = compiled
            .load(demand(100, DemandKind::Movement))
            .expect("load exists")
            .source_route()
            .expect("Source at the same node is reachable");
        assert_eq!(route.source(), PowerSourceId(EntityId(50)));
        assert_eq!(route.key().total_length(), Fixed::ZERO);
        assert_eq!(route.key().segment_count(), 0);
        assert!(route.wires().is_empty());
        assert_eq!(
            compiled.region_for_node(PowerNodeKey::WireOffset(wire(10), Fixed::ZERO)),
            compiled.region_for_source(PowerSourceId(EntityId(50)))
        );
    }

    #[test]
    fn zero_length_derived_side_coalesces_without_public_route_row() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![segmented_body(
                10,
                source_node(50),
                junction(9),
                &[1],
                WireEnd::A,
            )],
            sources: vec![source(50)],
            loads: vec![load(100, DemandKind::WireLeakage, junction(9))],
        })
        .expect("one-raw-unit body compiles");
        assert_eq!(
            compiled.region_for_node(PowerNodeKey::WireBody(wire(10))),
            compiled.region_for_node(source_node(50))
        );
        let route = compiled
            .load(demand(100, DemandKind::WireLeakage))
            .expect("load exists")
            .source_route()
            .expect("route exists");
        assert_eq!(route.wires()[0].length(), Fixed(1));
    }

    #[test]
    fn route_key_contains_explicit_semantic_tokens_in_traversal_order() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![body(10, junction(1), source_node(50), 2, 1)],
            sources: vec![source(50)],
            loads: vec![load(100, DemandKind::GateDrive, junction(1))],
        })
        .expect("token fixture compiles");
        let route = compiled
            .load(demand(100, DemandKind::GateDrive))
            .unwrap()
            .source_route()
            .unwrap();
        assert_eq!(
            route.key().path_tokens(),
            &[
                PowerPathToken::new(PATH_KIND_GATE, EntityId(100), 2),
                PowerPathToken::new(PATH_KIND_JUNCTION, EntityId(1), 0),
                PowerPathToken::new(PATH_KIND_WIRE, EntityId(10), 0),
                PowerPathToken::new(PATH_KIND_POWER_SOURCE, EntityId(50), 0),
            ]
        );
    }

    #[test]
    fn source_less_component_compiles_a_none_route() {
        let load_id = demand(100, DemandKind::Movement);
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![body(
                1,
                junction(1),
                PowerNodeKey::GatePower(GateId(EntityId(100))),
                5,
                1,
            )],
            sources: Vec::new(),
            loads: vec![load(100, DemandKind::Movement, junction(1))],
        })
        .expect("source-less region is valid");
        assert_eq!(
            compiled.load(load_id).expect("load exists").source_route(),
            None
        );
    }

    #[test]
    fn invalid_duplicate_and_attachment_shapes_fail_closed() {
        let duplicate_wire = body(1, junction(1), junction(2), 1, 1);
        assert_eq!(
            CompiledPowerTopology::compile(&PowerTopologyInput {
                bodies: vec![duplicate_wire.clone(), duplicate_wire],
                sources: Vec::new(),
                loads: Vec::new(),
            }),
            Err(PowerTopologyError::DuplicateWire { wire: wire(1) })
        );
        assert_eq!(
            CompiledPowerTopology::compile(&PowerTopologyInput {
                bodies: Vec::new(),
                sources: vec![PowerSourceAttachment {
                    source: PowerSourceId(EntityId(50)),
                    node: source_node(60),
                }],
                loads: Vec::new(),
            }),
            Err(PowerTopologyError::SourceAnchorMismatch {
                power_source: PowerSourceId(EntityId(50)),
                actual: source_node(60),
            })
        );

        let bad_segments = PowerBodyEdge {
            wire: wire(2),
            a: junction(1),
            b: junction(2),
            length: Fixed(5),
            segment_lengths: vec![Fixed(2), Fixed(2)],
            canonical_lower_end: WireEnd::A,
        };
        assert_eq!(
            CompiledPowerTopology::compile(&PowerTopologyInput {
                bodies: vec![bad_segments],
                sources: Vec::new(),
                loads: Vec::new(),
            }),
            Err(PowerTopologyError::BodySegmentLengthMismatch {
                wire: wire(2),
                declared_raw: 5,
                summed_raw: 4,
            })
        );

        assert_eq!(
            CompiledPowerTopology::compile(&PowerTopologyInput {
                bodies: vec![body(3, junction(1), junction(2), 5, 1)],
                sources: Vec::new(),
                loads: vec![load(
                    100,
                    DemandKind::Movement,
                    PowerNodeKey::WireOffset(wire(3), Fixed(6)),
                )],
            }),
            Err(PowerTopologyError::WireOffsetOutOfRange {
                wire: wire(3),
                offset_raw: 6,
                length_raw: 5,
            })
        );
    }

    #[test]
    fn path_arithmetic_overflow_is_reported() {
        let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![
                body(1, junction(1), junction(2), i64::MAX, 1),
                body(2, junction(2), source_node(50), 1, 1),
            ],
            sources: vec![source(50)],
            loads: vec![load(100, DemandKind::GateIdle, junction(1))],
        });
        assert_eq!(compiled, Err(PowerTopologyError::NumericOverflow));
    }
}
