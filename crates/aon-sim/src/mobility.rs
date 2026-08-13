use crate::topology::{EndpointTarget, JunctionStore, RoutingDomain, WireEnd, WireStore};
use crate::{
    Fixed, FixedAabb, FixedVec2, JunctionId, LogicLevel, MobileId, MobileSubstrateIndex,
    NumericError, SinkId, WireId, polyline_length, round_div_nearest_even, segment_length,
};
use crate::{
    geometry::canonical_polyline_points,
    structural_geometry::{
        ExactTurnSide, compare_exact_turn_magnitude, exact_turn_side, point_lies_on_segment,
    },
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Heading {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackPosition {
    Edge {
        edge: WireId,
        offset: Fixed,
        heading: Heading,
    },
    Junction {
        junction: JunctionId,
        incoming_edge: WireId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MobilePort {
    Stop,
    Left,
    Right,
}

impl MobilePort {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::Stop => 0,
            Self::Left => 1,
            Self::Right => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MobilePortRef {
    pub mobile: MobileId,
    pub port: MobilePort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileControlPorts {
    pub stop: SinkId,
    pub left: SinkId,
    pub right: SinkId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileControlSample {
    pub stop: LogicLevel,
    pub left: LogicLevel,
    pub right: LogicLevel,
}

impl MobileControlSample {
    pub const fn grants_stage0_movement(self) -> bool {
        matches!(self.stop, LogicLevel::Low)
            && !matches!(self.left, LogicLevel::X)
            && !matches!(self.right, LogicLevel::X)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionDecisionKind {
    Straight,
    Left,
    Right,
    Reverse,
    MissingRequestedSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileJunctionDecision {
    pub junction: JunctionId,
    pub incoming_edge: WireId,
    pub selected_edge: Option<WireId>,
    pub kind: JunctionDecisionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileMovementObservation {
    pub mobile: MobileId,
    pub start: TrackPosition,
    pub end: TrackPosition,
    pub controls: MobileControlSample,
    pub granted_budget: Fixed,
    pub consumed_budget: Fixed,
    pub junction_decisions: Vec<MobileJunctionDecision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MobileSubstrateRecord {
    pub id: MobileId,
    pub track_position: TrackPosition,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MobileSubstrateStore {
    ids: Vec<MobileId>,
    alive: Vec<bool>,
    track_positions: Vec<TrackPosition>,
    routing_areas: Vec<FixedAabb>,
    footprints: Vec<FixedAabb>,
}

impl MobileSubstrateStore {
    pub fn push(
        &mut self,
        id: MobileId,
        track_position: TrackPosition,
        routing_area: FixedAabb,
        footprint: FixedAabb,
    ) -> Result<MobileSubstrateIndex, TrackGraphError> {
        let raw = u32::try_from(self.ids.len()).map_err(|_| TrackGraphError::NumericOverflow)?;
        self.ids.push(id);
        self.alive.push(true);
        self.track_positions.push(track_position);
        self.routing_areas.push(routing_area);
        self.footprints.push(footprint);
        Ok(MobileSubstrateIndex(raw))
    }

    pub fn get(&self, index: MobileSubstrateIndex) -> Option<MobileSubstrateRecord> {
        let index = usize::try_from(index.0).ok()?;
        self.alive.get(index).copied().filter(|alive| *alive)?;
        Some(MobileSubstrateRecord {
            id: *self.ids.get(index)?,
            track_position: *self.track_positions.get(index)?,
            routing_area: *self.routing_areas.get(index)?,
            footprint: *self.footprints.get(index)?,
        })
    }

    pub fn remove(
        &mut self,
        index: MobileSubstrateIndex,
    ) -> Result<MobileSubstrateRecord, TrackGraphError> {
        let record = self
            .get(index)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        self.alive[index.0 as usize] = false;
        Ok(record)
    }

    pub fn set_track_position(
        &mut self,
        index: MobileSubstrateIndex,
        id: MobileId,
        position: TrackPosition,
    ) -> Result<(), TrackGraphError> {
        let current = self
            .get(index)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        if current.id != id {
            return Err(TrackGraphError::InvalidCanonicalState);
        }
        self.track_positions[index.0 as usize] = position;
        Ok(())
    }

    pub fn iter_alive(
        &self,
    ) -> impl Iterator<Item = (MobileSubstrateIndex, MobileSubstrateRecord)> + '_ {
        (0..self.ids.len()).filter_map(|raw| {
            let raw = u32::try_from(raw).ok()?;
            let index = MobileSubstrateIndex(raw);
            self.get(index).map(|record| (index, record))
        })
    }

    pub fn live_count(&self) -> u64 {
        self.alive.iter().filter(|alive| **alive).count() as u64
    }

    #[cfg(test)]
    pub fn reserve_capacity_for_test(&mut self, additional: usize) {
        self.ids.reserve(additional);
        self.alive.reserve(additional);
        self.track_positions.reserve(additional);
        self.routing_areas.reserve(additional);
        self.footprints.reserve(additional);
    }

    #[cfg(test)]
    pub fn swap_slots_for_test(
        &mut self,
        first: MobileSubstrateIndex,
        second: MobileSubstrateIndex,
    ) -> Result<(), TrackGraphError> {
        self.get(first)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        self.get(second)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        let first = first.0 as usize;
        let second = second.0 as usize;
        self.ids.swap(first, second);
        self.alive.swap(first, second);
        self.track_positions.swap(first, second);
        self.routing_areas.swap(first, second);
        self.footprints.swap(first, second);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackEdge {
    pub id: WireId,
    pub length: Fixed,
    pub points: Vec<FixedVec2>,
    pub junction_a: Option<JunctionId>,
    pub junction_b: Option<JunctionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TrackIncident {
    pub edge: WireId,
    pub end: WireEnd,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TrackGraph {
    edges: BTreeMap<WireId, TrackEdge>,
    incidents: BTreeMap<JunctionId, Vec<TrackIncident>>,
    junctions: BTreeMap<JunctionId, FixedVec2>,
}

impl TrackGraph {
    pub fn compile(wires: &WireStore, junctions: &JunctionStore) -> Result<Self, TrackGraphError> {
        let live_junctions: BTreeMap<_, _> = junctions
            .iter_alive()
            .filter(|(_, record)| record.routing_domain == RoutingDomain::OpenWorld)
            .map(|(_, record)| (record.id, record.position))
            .collect();
        let mut graph = Self {
            junctions: live_junctions.clone(),
            ..Self::default()
        };
        for (_, wire) in wires
            .iter_alive()
            .filter(|(_, record)| record.routing_domain == RoutingDomain::OpenWorld)
        {
            let points = canonical_polyline_points(wire.points);
            let Some((&endpoint_a, remaining)) = points.split_first() else {
                return Err(TrackGraphError::InvalidCanonicalState);
            };
            let Some(&endpoint_b) = remaining.last() else {
                return Err(TrackGraphError::InvalidCanonicalState);
            };
            let length = polyline_length(&points)?;
            if length.0 <= 0 {
                return Err(TrackGraphError::InvalidCanonicalState);
            }
            let junction_a = connected_junction(wire.endpoint_a, endpoint_a, &live_junctions)?;
            let junction_b = connected_junction(wire.endpoint_b, endpoint_b, &live_junctions)?;
            let edge = TrackEdge {
                id: wire.id,
                length,
                points,
                junction_a,
                junction_b,
            };
            if graph.edges.insert(wire.id, edge).is_some() {
                return Err(TrackGraphError::InvalidCanonicalState);
            }
            for (junction, end) in [(junction_a, WireEnd::A), (junction_b, WireEnd::B)] {
                if let Some(junction) = junction {
                    graph
                        .incidents
                        .entry(junction)
                        .or_default()
                        .push(TrackIncident { edge: wire.id, end });
                }
            }
        }
        for incidents in graph.incidents.values_mut() {
            incidents.sort_unstable();
        }
        graph.validate()?;
        Ok(graph)
    }

    #[cfg(test)]
    pub fn edge(&self, id: WireId) -> Option<&TrackEdge> {
        self.edges.get(&id)
    }

    #[cfg(test)]
    pub fn incidents(&self, junction: JunctionId) -> &[TrackIncident] {
        self.incidents.get(&junction).map_or(&[], Vec::as_slice)
    }

    fn validate(&self) -> Result<(), TrackGraphError> {
        for (id, edge) in &self.edges {
            if edge.id != *id
                || edge.points.len() < 2
                || edge.length.0 <= 0
                || polyline_length(&edge.points)? != edge.length
            {
                return Err(TrackGraphError::InvalidCanonicalState);
            }
            for (junction, end) in [(edge.junction_a, WireEnd::A), (edge.junction_b, WireEnd::B)] {
                if let Some(junction) = junction
                    && !self.incidents.get(&junction).is_some_and(|incidents| {
                        incidents.contains(&TrackIncident { edge: *id, end })
                    })
                {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
            }
        }
        for (junction, incidents) in &self.incidents {
            if incidents.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(TrackGraphError::InvalidCanonicalState);
            }
            for incident in incidents {
                let edge = self
                    .edges
                    .get(&incident.edge)
                    .ok_or(TrackGraphError::InvalidCanonicalState)?;
                let endpoint_junction = match incident.end {
                    WireEnd::A => edge.junction_a,
                    WireEnd::B => edge.junction_b,
                };
                if endpoint_junction != Some(*junction) {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
            }
        }
        Ok(())
    }

    pub fn locate(&self, point: FixedVec2) -> Result<Option<TrackPosition>, TrackGraphError> {
        let explicit_incident = self
            .junctions
            .iter()
            .filter(|(_, position)| **position == point)
            .flat_map(|(junction, _)| self.incidents.get(junction).into_iter().flatten().copied())
            .min();
        if let Some(incident) = explicit_incident {
            return self.position_leaving_incident(incident).map(Some);
        }
        for edge in self.edges.values() {
            let mut cumulative = Fixed::ZERO;
            for segment in edge.points.windows(2) {
                let start = segment[0];
                let end = segment[1];
                let length = segment_length(start, end)?;
                if point_lies_on_segment(point, start, end)? {
                    let local_offset = segment_offset(start, end, point, length)?;
                    if project_segment_offset(start, end, local_offset, length)? != point {
                        continue;
                    }
                    let offset = cumulative.checked_add(local_offset)?;
                    let heading = if offset == edge.length {
                        Heading::Reverse
                    } else {
                        Heading::Forward
                    };
                    return Ok(Some(TrackPosition::Edge {
                        edge: edge.id,
                        offset,
                        heading,
                    }));
                }
                cumulative = cumulative.checked_add(length)?;
            }
            if cumulative != edge.length {
                return Err(TrackGraphError::InvalidCanonicalState);
            }
        }
        Ok(None)
    }

    pub fn world_position(&self, position: TrackPosition) -> Result<FixedVec2, TrackGraphError> {
        match position {
            TrackPosition::Edge { edge, offset, .. } => {
                let edge = self
                    .edges
                    .get(&edge)
                    .ok_or(TrackGraphError::InvalidCanonicalState)?;
                if offset.0 < 0 || offset.0 > edge.length.0 {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                let mut cumulative = Fixed::ZERO;
                for segment in edge.points.windows(2) {
                    let length = segment_length(segment[0], segment[1])?;
                    let end = cumulative.checked_add(length)?;
                    if offset <= end {
                        let local = offset.checked_sub(cumulative)?;
                        return project_segment_offset(segment[0], segment[1], local, length);
                    }
                    cumulative = end;
                }
                Err(TrackGraphError::InvalidCanonicalState)
            }
            TrackPosition::Junction {
                junction,
                incoming_edge,
            } => {
                if self.incidents.get(&junction).map_or(0, |items| {
                    items
                        .iter()
                        .filter(|item| item.edge == incoming_edge)
                        .count()
                }) != 1
                {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                self.junctions
                    .get(&junction)
                    .copied()
                    .ok_or(TrackGraphError::InvalidCanonicalState)
            }
        }
    }

    /// Resolves a canonical Track position to its physical Wire and offset from stored endpoint A.
    ///
    /// Heading does not change the attachment point. A Junction position remains attached to its
    /// unique incoming incident: endpoint A maps to zero and endpoint B maps to the Wire length.
    pub(crate) fn power_attachment_position(
        &self,
        position: TrackPosition,
    ) -> Result<(WireId, Fixed), TrackGraphError> {
        match position {
            TrackPosition::Edge { edge, offset, .. } => {
                let record = self
                    .edges
                    .get(&edge)
                    .ok_or(TrackGraphError::InvalidCanonicalState)?;
                if record.id != edge
                    || record.length.0 <= 0
                    || offset.0 < 0
                    || offset.0 > record.length.0
                {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                Ok((edge, offset))
            }
            TrackPosition::Junction {
                junction,
                incoming_edge,
            } => {
                if !self.junctions.contains_key(&junction) {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                let mut matching = self
                    .incidents
                    .get(&junction)
                    .into_iter()
                    .flatten()
                    .filter(|incident| incident.edge == incoming_edge);
                let incident = matching
                    .next()
                    .copied()
                    .ok_or(TrackGraphError::InvalidCanonicalState)?;
                if matching.next().is_some() {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                let edge = self
                    .edges
                    .get(&incoming_edge)
                    .ok_or(TrackGraphError::InvalidCanonicalState)?;
                let endpoint_junction = match incident.end {
                    WireEnd::A => edge.junction_a,
                    WireEnd::B => edge.junction_b,
                };
                if edge.id != incoming_edge
                    || edge.length.0 <= 0
                    || endpoint_junction != Some(junction)
                {
                    return Err(TrackGraphError::InvalidCanonicalState);
                }
                let offset = match incident.end {
                    WireEnd::A => Fixed::ZERO,
                    WireEnd::B => edge.length,
                };
                Ok((incoming_edge, offset))
            }
        }
    }

    pub fn stage_movement(
        &self,
        mobile: MobileId,
        start: TrackPosition,
        controls: MobileControlSample,
        granted_budget: Fixed,
    ) -> Result<MobileMovementObservation, TrackGraphError> {
        self.stage_movement_with_power(mobile, start, controls, granted_budget, None)
    }

    pub(crate) fn stage_powered_movement(
        &self,
        mobile: MobileId,
        start: TrackPosition,
        controls: MobileControlSample,
        granted_budget: Fixed,
        powered_edges: &BTreeSet<WireId>,
    ) -> Result<MobileMovementObservation, TrackGraphError> {
        self.stage_movement_with_power(mobile, start, controls, granted_budget, Some(powered_edges))
    }

    fn stage_movement_with_power(
        &self,
        mobile: MobileId,
        start: TrackPosition,
        controls: MobileControlSample,
        granted_budget: Fixed,
        powered_edges: Option<&BTreeSet<WireId>>,
    ) -> Result<MobileMovementObservation, TrackGraphError> {
        if granted_budget.0 < 0 {
            return Err(TrackGraphError::InvalidCanonicalState);
        }
        let mut position = start;
        let mut remaining = granted_budget;
        let mut junction_decisions = Vec::new();
        while remaining.0 > 0 {
            match position {
                TrackPosition::Edge {
                    edge,
                    offset,
                    heading,
                } => {
                    let record = self
                        .edges
                        .get(&edge)
                        .ok_or(TrackGraphError::InvalidCanonicalState)?;
                    if offset.0 < 0 || offset.0 > record.length.0 {
                        return Err(TrackGraphError::InvalidCanonicalState);
                    }
                    if record.junction_a.is_none() && record.junction_b.is_none() {
                        let (offset, heading) =
                            advance_reflecting_edge(record.length, offset, heading, remaining)?;
                        position = TrackPosition::Edge {
                            edge,
                            offset,
                            heading,
                        };
                        remaining = Fixed::ZERO;
                        continue;
                    }
                    let (distance, endpoint_offset, junction) = match heading {
                        Heading::Forward => (
                            record.length.checked_sub(offset)?,
                            record.length,
                            record.junction_b,
                        ),
                        Heading::Reverse => (offset, Fixed::ZERO, record.junction_a),
                    };
                    match remaining.cmp(&distance) {
                        Ordering::Less => {
                            let next_offset = match heading {
                                Heading::Forward => offset.checked_add(remaining)?,
                                Heading::Reverse => offset.checked_sub(remaining)?,
                            };
                            position = TrackPosition::Edge {
                                edge,
                                offset: next_offset,
                                heading,
                            };
                            remaining = Fixed::ZERO;
                        }
                        Ordering::Equal => {
                            position = junction.map_or(
                                TrackPosition::Edge {
                                    edge,
                                    offset: endpoint_offset,
                                    heading,
                                },
                                |junction| TrackPosition::Junction {
                                    junction,
                                    incoming_edge: edge,
                                },
                            );
                            remaining = Fixed::ZERO;
                        }
                        Ordering::Greater => {
                            remaining = remaining.checked_sub(distance)?;
                            if let Some(junction) = junction {
                                position = TrackPosition::Junction {
                                    junction,
                                    incoming_edge: edge,
                                };
                            } else {
                                position = TrackPosition::Edge {
                                    edge,
                                    offset: endpoint_offset,
                                    heading: opposite_heading(heading),
                                };
                            }
                        }
                    }
                }
                TrackPosition::Junction {
                    junction,
                    incoming_edge,
                } => {
                    let (selected, kind) =
                        self.select_junction_incident(junction, incoming_edge, controls)?;
                    junction_decisions.push(MobileJunctionDecision {
                        junction,
                        incoming_edge,
                        selected_edge: selected.map(|incident| incident.edge),
                        kind,
                    });
                    let Some(selected) = selected else {
                        break;
                    };
                    if powered_edges.is_some_and(|edges| !edges.contains(&selected.edge)) {
                        break;
                    }
                    position = self.position_leaving_incident(selected)?;
                }
            }
        }
        let consumed_budget = granted_budget.checked_sub(remaining)?;
        Ok(MobileMovementObservation {
            mobile,
            start,
            end: position,
            controls,
            granted_budget,
            consumed_budget,
            junction_decisions,
        })
    }

    fn select_junction_incident(
        &self,
        junction: JunctionId,
        incoming_edge: WireId,
        controls: MobileControlSample,
    ) -> Result<(Option<TrackIncident>, JunctionDecisionKind), TrackGraphError> {
        let incidents = self
            .incidents
            .get(&junction)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        let incoming_matches: Vec<_> = incidents
            .iter()
            .copied()
            .filter(|incident| incident.edge == incoming_edge)
            .collect();
        let [incoming] = incoming_matches.as_slice() else {
            return Err(TrackGraphError::InvalidCanonicalState);
        };
        if matches!(controls.left, LogicLevel::High) && matches!(controls.right, LogicLevel::High) {
            return Ok((Some(*incoming), JunctionDecisionKind::Reverse));
        }

        let incoming_vector = self.incoming_vector(*incoming)?;
        let mut candidates = Vec::new();
        for incident in incidents
            .iter()
            .copied()
            .filter(|incident| incident.edge != incoming_edge)
        {
            let outgoing = self.outgoing_vector(incident)?;
            candidates.push((
                incident,
                outgoing,
                exact_turn_side(incoming_vector, outgoing)?,
            ));
        }

        if matches!(controls.left, LogicLevel::Low) && matches!(controls.right, LogicLevel::Low) {
            if candidates.is_empty() {
                return Ok((Some(*incoming), JunctionDecisionKind::Reverse));
            }
            let mut best = candidates[0];
            for candidate in candidates.into_iter().skip(1) {
                if candidate_is_better(incoming_vector, candidate, best, TurnPreference::Minimum)? {
                    best = candidate;
                }
            }
            let kind = match best.2 {
                ExactTurnSide::Left => JunctionDecisionKind::Left,
                ExactTurnSide::Right => JunctionDecisionKind::Right,
                ExactTurnSide::Straight => JunctionDecisionKind::Straight,
                ExactTurnSide::Reverse => JunctionDecisionKind::Reverse,
            };
            return Ok((Some(best.0), kind));
        }

        let (side, kind) = if matches!(controls.left, LogicLevel::High) {
            (ExactTurnSide::Left, JunctionDecisionKind::Left)
        } else if matches!(controls.right, LogicLevel::High) {
            (ExactTurnSide::Right, JunctionDecisionKind::Right)
        } else {
            return Ok((None, JunctionDecisionKind::MissingRequestedSide));
        };
        let mut filtered = candidates
            .into_iter()
            .filter(|candidate| candidate.2 == side);
        let Some(mut best) = filtered.next() else {
            return Ok((None, JunctionDecisionKind::MissingRequestedSide));
        };
        for candidate in filtered {
            if candidate_is_better(incoming_vector, candidate, best, TurnPreference::Maximum)? {
                best = candidate;
            }
        }
        Ok((Some(best.0), kind))
    }

    fn position_leaving_incident(
        &self,
        incident: TrackIncident,
    ) -> Result<TrackPosition, TrackGraphError> {
        let edge = self
            .edges
            .get(&incident.edge)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        Ok(match incident.end {
            WireEnd::A => TrackPosition::Edge {
                edge: incident.edge,
                offset: Fixed::ZERO,
                heading: Heading::Forward,
            },
            WireEnd::B => TrackPosition::Edge {
                edge: incident.edge,
                offset: edge.length,
                heading: Heading::Reverse,
            },
        })
    }

    fn incoming_vector(&self, incident: TrackIncident) -> Result<(i128, i128), TrackGraphError> {
        let outgoing = self.outgoing_vector(incident)?;
        Ok((-outgoing.0, -outgoing.1))
    }

    fn outgoing_vector(&self, incident: TrackIncident) -> Result<(i128, i128), TrackGraphError> {
        let edge = self
            .edges
            .get(&incident.edge)
            .ok_or(TrackGraphError::InvalidCanonicalState)?;
        let (start, next) = match incident.end {
            WireEnd::A => (edge.points[0], edge.points[1]),
            WireEnd::B => {
                let last = edge.points.len() - 1;
                (edge.points[last], edge.points[last - 1])
            }
        };
        Ok((
            i128::from(next.x.0) - i128::from(start.x.0),
            i128::from(next.y.0) - i128::from(start.y.0),
        ))
    }

    pub(crate) fn edge_ids(&self) -> impl Iterator<Item = WireId> + '_ {
        self.edges.keys().copied()
    }
}

#[derive(Clone, Copy)]
enum TurnPreference {
    Minimum,
    Maximum,
}

fn candidate_is_better(
    incoming: (i128, i128),
    candidate: (TrackIncident, (i128, i128), ExactTurnSide),
    current: (TrackIncident, (i128, i128), ExactTurnSide),
    preference: TurnPreference,
) -> Result<bool, TrackGraphError> {
    let angle_order = compare_exact_turn_magnitude(incoming, candidate.1, current.1)?;
    Ok(match (preference, angle_order) {
        (TurnPreference::Minimum, Ordering::Less)
        | (TurnPreference::Maximum, Ordering::Greater) => true,
        (_, Ordering::Equal) => candidate.0 < current.0,
        _ => false,
    })
}

const fn opposite_heading(heading: Heading) -> Heading {
    match heading {
        Heading::Forward => Heading::Reverse,
        Heading::Reverse => Heading::Forward,
    }
}

fn advance_reflecting_edge(
    length: Fixed,
    offset: Fixed,
    heading: Heading,
    distance: Fixed,
) -> Result<(Fixed, Heading), TrackGraphError> {
    if length.0 <= 0 || offset.0 < 0 || offset.0 > length.0 || distance.0 < 0 {
        return Err(TrackGraphError::InvalidCanonicalState);
    }
    if distance.0 == 0 {
        return Ok((offset, heading));
    }

    let length = i128::from(length.0);
    let period = length
        .checked_mul(2)
        .ok_or(TrackGraphError::NumericOverflow)?;
    let phase = match heading {
        Heading::Forward => i128::from(offset.0),
        Heading::Reverse => period
            .checked_sub(i128::from(offset.0))
            .ok_or(TrackGraphError::NumericOverflow)?,
    };
    let phase = phase
        .checked_add(i128::from(distance.0))
        .ok_or(TrackGraphError::NumericOverflow)?
        .rem_euclid(period);
    let (offset, heading) = if phase == 0 {
        (0, Heading::Reverse)
    } else if phase <= length {
        (phase, Heading::Forward)
    } else {
        (
            period
                .checked_sub(phase)
                .ok_or(TrackGraphError::NumericOverflow)?,
            Heading::Reverse,
        )
    };
    Ok((
        Fixed(i64::try_from(offset).map_err(|_| TrackGraphError::NumericOverflow)?),
        heading,
    ))
}

fn segment_offset(
    start: FixedVec2,
    end: FixedVec2,
    point: FixedVec2,
    length: Fixed,
) -> Result<Fixed, TrackGraphError> {
    let dx = i128::from(end.x.0) - i128::from(start.x.0);
    let dy = i128::from(end.y.0) - i128::from(start.y.0);
    let (numerator, denominator) = if dx != 0 {
        (
            (i128::from(point.x.0) - i128::from(start.x.0)).unsigned_abs(),
            dx.unsigned_abs(),
        )
    } else if dy != 0 {
        (
            (i128::from(point.y.0) - i128::from(start.y.0)).unsigned_abs(),
            dy.unsigned_abs(),
        )
    } else {
        return Err(TrackGraphError::InvalidCanonicalState);
    };
    let numerator = i128::from(length.0)
        .checked_mul(i128::try_from(numerator).map_err(|_| TrackGraphError::NumericOverflow)?)
        .ok_or(TrackGraphError::NumericOverflow)?;
    let offset = round_div_nearest_even(
        numerator,
        i128::try_from(denominator).map_err(|_| TrackGraphError::NumericOverflow)?,
    )?;
    let offset = i64::try_from(offset).map_err(|_| TrackGraphError::NumericOverflow)?;
    if offset < 0 || offset > length.0 {
        return Err(TrackGraphError::InvalidCanonicalState);
    }
    Ok(Fixed(offset))
}

fn project_segment_offset(
    start: FixedVec2,
    end: FixedVec2,
    offset: Fixed,
    length: Fixed,
) -> Result<FixedVec2, TrackGraphError> {
    if length.0 <= 0 || offset.0 < 0 || offset.0 > length.0 {
        return Err(TrackGraphError::InvalidCanonicalState);
    }
    let project_axis = |start: i64, end: i64| -> Result<Fixed, TrackGraphError> {
        let delta = i128::from(end) - i128::from(start);
        let scaled = delta
            .checked_mul(i128::from(offset.0))
            .ok_or(TrackGraphError::NumericOverflow)?;
        let relative = round_div_nearest_even(scaled, i128::from(length.0))?;
        let coordinate = i128::from(start)
            .checked_add(relative)
            .ok_or(TrackGraphError::NumericOverflow)?;
        i64::try_from(coordinate)
            .map(Fixed)
            .map_err(|_| TrackGraphError::NumericOverflow)
    };
    Ok(FixedVec2::new(
        project_axis(start.x.0, end.x.0)?,
        project_axis(start.y.0, end.y.0)?,
    ))
}

fn connected_junction(
    target: EndpointTarget,
    endpoint: FixedVec2,
    live_junctions: &BTreeMap<JunctionId, FixedVec2>,
) -> Result<Option<JunctionId>, TrackGraphError> {
    let EndpointTarget::Junction(junction) = target else {
        return Ok(None);
    };
    let position = live_junctions
        .get(&junction)
        .ok_or(TrackGraphError::InvalidCanonicalState)?;
    if *position != endpoint {
        return Err(TrackGraphError::InvalidCanonicalState);
    }
    Ok(Some(junction))
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum TrackGraphError {
    #[error("canonical Track Graph numeric range exhausted")]
    NumericOverflow,

    #[error("canonical Track Graph input invariant violated")]
    InvalidCanonicalState,
}

impl From<NumericError> for TrackGraphError {
    fn from(_: NumericError) -> Self {
        Self::NumericOverflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionGeneration, EntityId, JunctionIndex, WireIndex};

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn graph_fixture() -> (WireStore, JunctionStore) {
        let junction = JunctionId(EntityId(40));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(10, 0))
            .expect("junction");
        junctions
            .push(
                JunctionId(EntityId(41)),
                RoutingDomain::FixedSubstrate(EntityId(7)),
                point(10, 0),
            )
            .expect("fixed junction");

        let mut wires = WireStore::default();
        wires
            .push(
                WireId(EntityId(9)),
                RoutingDomain::OpenWorld,
                &[point(10, 0), point(20, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("right edge");
        wires
            .push(
                WireId(EntityId(3)),
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(10, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            )
            .expect("left edge");
        wires
            .push(
                WireId(EntityId(1)),
                RoutingDomain::FixedSubstrate(EntityId(7)),
                &[point(0, 0), point(10, 0)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("circuit wire");
        (wires, junctions)
    }

    fn turn_graph() -> TrackGraph {
        let junction = JunctionId(EntityId(40));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(0, 0))
            .expect("junction");
        let mut wires = WireStore::default();
        for (id, points, endpoint_a, endpoint_b) in [
            (
                2,
                [point(-10, 0), point(0, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            ),
            (
                3,
                [point(0, 0), point(10, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            ),
            (
                4,
                [point(0, 0), point(0, 10)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            ),
            (
                5,
                [point(0, 0), point(0, -10)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            ),
        ] {
            wires
                .push(
                    WireId(EntityId(id)),
                    RoutingDomain::OpenWorld,
                    &points,
                    endpoint_a,
                    endpoint_b,
                )
                .expect("turn edge");
        }
        TrackGraph::compile(&wires, &junctions).expect("turn graph")
    }

    #[test]
    fn graph_uses_only_open_world_edges_and_explicit_live_junction_bindings() {
        let (wires, junctions) = graph_fixture();
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        assert_eq!(
            graph.edge_ids().collect::<Vec<_>>(),
            [WireId(EntityId(3)), WireId(EntityId(9)),]
        );
        assert_eq!(
            graph.incidents(JunctionId(EntityId(40))),
            [
                TrackIncident {
                    edge: WireId(EntityId(3)),
                    end: WireEnd::B,
                },
                TrackIncident {
                    edge: WireId(EntityId(9)),
                    end: WireEnd::A,
                },
            ]
        );
        assert_eq!(
            graph.edge(WireId(EntityId(3))),
            Some(TrackEdge {
                id: WireId(EntityId(3)),
                length: Fixed(10),
                points: vec![point(0, 0), point(10, 0)],
                junction_a: None,
                junction_b: Some(JunctionId(EntityId(40))),
            })
            .as_ref()
        );
    }

    #[test]
    fn graph_is_invariant_to_dense_store_layout() {
        let (mut wires, mut junctions) = graph_fixture();
        let expected = TrackGraph::compile(&wires, &junctions).expect("baseline graph");
        wires
            .swap_slots_for_test(WireIndex(0), WireIndex(2))
            .expect("wire slots swap");
        junctions
            .swap_slots_for_test(JunctionIndex(0), JunctionIndex(1))
            .expect("junction slots swap");
        let actual = TrackGraph::compile(&wires, &junctions).expect("permuted graph");
        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_or_removed_explicit_junction_binding_is_not_geometric_connectivity() {
        let (mut wires, mut junctions) = graph_fixture();
        junctions
            .remove(JunctionIndex(0))
            .expect("junction removes");
        assert_eq!(
            TrackGraph::compile(&wires, &junctions),
            Err(TrackGraphError::InvalidCanonicalState)
        );

        let (_, baseline_junctions) = graph_fixture();
        wires
            .set_endpoint(WireIndex(0), WireEnd::A, EndpointTarget::Free)
            .expect("binding clears");
        let graph = TrackGraph::compile(&wires, &baseline_junctions).expect("unbound graph");
        assert_eq!(graph.incidents(JunctionId(EntityId(40))).len(), 1);
    }

    #[test]
    fn track_position_and_heading_are_identity_based_canonical_values() {
        let position = TrackPosition::Edge {
            edge: WireId(EntityId(3)),
            offset: Fixed(5),
            heading: Heading::Forward,
        };
        assert_ne!(
            position,
            TrackPosition::Edge {
                edge: WireId(EntityId(3)),
                offset: Fixed(5),
                heading: Heading::Reverse,
            }
        );
        assert_eq!(
            TrackPosition::Junction {
                junction: JunctionId(EntityId(40)),
                incoming_edge: WireId(EntityId(3)),
            },
            TrackPosition::Junction {
                junction: JunctionId(EntityId(40)),
                incoming_edge: WireId(EntityId(3)),
            }
        );
    }

    #[test]
    fn power_attachment_uses_stored_a_offset_for_edge_positions_independent_of_heading() {
        let (wires, junctions) = graph_fixture();
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        let edge = WireId(EntityId(3));

        for heading in [Heading::Forward, Heading::Reverse] {
            for offset in [Fixed::ZERO, Fixed(4), Fixed(10)] {
                assert_eq!(
                    graph.power_attachment_position(TrackPosition::Edge {
                        edge,
                        offset,
                        heading,
                    }),
                    Ok((edge, offset))
                );
            }
        }
        assert_eq!(
            graph.power_attachment_position(TrackPosition::Edge {
                edge,
                offset: Fixed(-1),
                heading: Heading::Forward,
            }),
            Err(TrackGraphError::InvalidCanonicalState)
        );
        assert_eq!(
            graph.power_attachment_position(TrackPosition::Edge {
                edge,
                offset: Fixed(11),
                heading: Heading::Reverse,
            }),
            Err(TrackGraphError::InvalidCanonicalState)
        );
        assert_eq!(
            graph.power_attachment_position(TrackPosition::Edge {
                edge: WireId(EntityId(99)),
                offset: Fixed::ZERO,
                heading: Heading::Forward,
            }),
            Err(TrackGraphError::InvalidCanonicalState)
        );
    }

    #[test]
    fn power_attachment_maps_junction_incident_a_to_zero_and_b_to_edge_length() {
        let (wires, junctions) = graph_fixture();
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        let junction = JunctionId(EntityId(40));

        assert_eq!(
            graph.power_attachment_position(TrackPosition::Junction {
                junction,
                incoming_edge: WireId(EntityId(9)),
            }),
            Ok((WireId(EntityId(9)), Fixed::ZERO)),
            "Wire 9 stores the Junction at endpoint A"
        );
        assert_eq!(
            graph.power_attachment_position(TrackPosition::Junction {
                junction,
                incoming_edge: WireId(EntityId(3)),
            }),
            Ok((WireId(EntityId(3)), Fixed(10))),
            "Wire 3 stores the Junction at endpoint B"
        );
    }

    #[test]
    fn power_attachment_rejects_malformed_junction_positions() {
        let (wires, junctions) = graph_fixture();
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");

        for position in [
            TrackPosition::Junction {
                junction: JunctionId(EntityId(99)),
                incoming_edge: WireId(EntityId(3)),
            },
            TrackPosition::Junction {
                junction: JunctionId(EntityId(40)),
                incoming_edge: WireId(EntityId(99)),
            },
        ] {
            assert_eq!(
                graph.power_attachment_position(position),
                Err(TrackGraphError::InvalidCanonicalState)
            );
        }
    }

    #[test]
    fn power_attachment_is_invariant_to_dense_wire_and_junction_layout() {
        let (mut wires, mut junctions) = graph_fixture();
        let position = TrackPosition::Junction {
            junction: JunctionId(EntityId(40)),
            incoming_edge: WireId(EntityId(9)),
        };
        let baseline = TrackGraph::compile(&wires, &junctions)
            .expect("baseline graph")
            .power_attachment_position(position)
            .expect("baseline attachment");

        wires
            .swap_slots_for_test(WireIndex(0), WireIndex(2))
            .expect("wire slots swap");
        junctions
            .swap_slots_for_test(JunctionIndex(0), JunctionIndex(1))
            .expect("junction slots swap");
        let permuted = TrackGraph::compile(&wires, &junctions)
            .expect("permuted graph")
            .power_attachment_position(position)
            .expect("permuted attachment");
        assert_eq!(permuted, baseline);
    }

    #[test]
    fn generation_does_not_create_a_second_track_identity() {
        let (mut wires, junctions) = graph_fixture();
        wires
            .force_generation_for_test(WireIndex(0), ConnectionGeneration(9))
            .expect("generation changes");
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        assert_eq!(
            graph.edge_ids().collect::<Vec<_>>(),
            [WireId(EntityId(3)), WireId(EntityId(9)),]
        );
    }

    #[test]
    fn placement_uses_wire_id_segment_order_and_exact_reversible_projection() {
        let (wires, junctions) = graph_fixture();
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        assert_eq!(
            graph.locate(point(10, 0)),
            Ok(Some(TrackPosition::Edge {
                edge: WireId(EntityId(3)),
                offset: Fixed(10),
                heading: Heading::Reverse,
            }))
        );
        assert_eq!(
            graph.locate(point(15, 0)),
            Ok(Some(TrackPosition::Edge {
                edge: WireId(EntityId(9)),
                offset: Fixed(5),
                heading: Heading::Forward,
            }))
        );
        assert_eq!(graph.locate(point(15, 1)), Ok(None));
        assert_eq!(
            graph.world_position(TrackPosition::Edge {
                edge: WireId(EntityId(9)),
                offset: Fixed(5),
                heading: Heading::Forward,
            }),
            Ok(point(15, 0))
        );

        let mut diagonal_wires = WireStore::default();
        diagonal_wires
            .push(
                WireId(EntityId(1)),
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(3, 4), point(6, 8)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("diagonal edge");
        let diagonal = TrackGraph::compile(&diagonal_wires, &JunctionStore::default())
            .expect("diagonal graph");
        assert_eq!(
            diagonal.locate(point(3, 4)),
            Ok(Some(TrackPosition::Edge {
                edge: WireId(EntityId(1)),
                offset: Fixed(5),
                heading: Heading::Forward,
            }))
        );
    }

    #[test]
    fn placement_at_an_explicit_junction_prefers_its_lowest_incident_over_a_lower_free_edge() {
        let junction = JunctionId(EntityId(40));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(10, 0))
            .expect("junction");
        let mut wires = WireStore::default();
        wires
            .push(
                WireId(EntityId(1)),
                RoutingDomain::OpenWorld,
                &[point(10, 0), point(10, -10)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("lower free edge");
        wires
            .push(
                WireId(EntityId(3)),
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(10, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            )
            .expect("bound incident");
        wires
            .push(
                WireId(EntityId(4)),
                RoutingDomain::OpenWorld,
                &[point(10, 0), point(20, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("second bound incident");
        let graph = TrackGraph::compile(&wires, &junctions).expect("graph");
        assert_eq!(
            graph.locate(point(10, 0)),
            Ok(Some(TrackPosition::Edge {
                edge: WireId(EntityId(3)),
                offset: Fixed(10),
                heading: Heading::Reverse,
            }))
        );
    }

    #[test]
    fn coincident_explicit_junctions_choose_the_lowest_incident_across_all_bindings() {
        let lower_junction = JunctionId(EntityId(40));
        let higher_junction = JunctionId(EntityId(41));
        let mut junctions = JunctionStore::default();
        junctions
            .push(lower_junction, RoutingDomain::OpenWorld, point(10, 0))
            .expect("lower Junction");
        junctions
            .push(higher_junction, RoutingDomain::OpenWorld, point(10, 0))
            .expect("coincident higher Junction");
        let mut wires = WireStore::default();
        wires
            .push(
                WireId(EntityId(1)),
                RoutingDomain::OpenWorld,
                &[point(10, 0), point(10, -10)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("lower free edge");
        wires
            .push(
                WireId(EntityId(4)),
                RoutingDomain::OpenWorld,
                &[point(10, 0), point(20, 0)],
                EndpointTarget::Junction(lower_junction),
                EndpointTarget::Free,
            )
            .expect("incident on the lower JunctionId");
        wires
            .push(
                WireId(EntityId(3)),
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(10, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(higher_junction),
            )
            .expect("lower incident on the higher JunctionId");

        let graph = TrackGraph::compile(&wires, &junctions).expect("coincident Junction graph");
        assert_eq!(
            graph.locate(point(10, 0)),
            Ok(Some(TrackPosition::Edge {
                edge: WireId(EntityId(3)),
                offset: Fixed(10),
                heading: Heading::Reverse,
            }))
        );
    }

    #[test]
    fn c14_control_table_selects_straight_left_right_reverse_and_stops_on_x() {
        let graph = turn_graph();
        let start = TrackPosition::Junction {
            junction: JunctionId(EntityId(40)),
            incoming_edge: WireId(EntityId(2)),
        };
        let cases = [
            (
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::Low,
                    right: LogicLevel::Low,
                },
                WireId(EntityId(3)),
                JunctionDecisionKind::Straight,
                TrackPosition::Edge {
                    edge: WireId(EntityId(3)),
                    offset: Fixed(5),
                    heading: Heading::Forward,
                },
            ),
            (
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::High,
                    right: LogicLevel::Low,
                },
                WireId(EntityId(4)),
                JunctionDecisionKind::Left,
                TrackPosition::Edge {
                    edge: WireId(EntityId(4)),
                    offset: Fixed(5),
                    heading: Heading::Forward,
                },
            ),
            (
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::Low,
                    right: LogicLevel::High,
                },
                WireId(EntityId(5)),
                JunctionDecisionKind::Right,
                TrackPosition::Edge {
                    edge: WireId(EntityId(5)),
                    offset: Fixed(5),
                    heading: Heading::Forward,
                },
            ),
            (
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::High,
                    right: LogicLevel::High,
                },
                WireId(EntityId(2)),
                JunctionDecisionKind::Reverse,
                TrackPosition::Edge {
                    edge: WireId(EntityId(2)),
                    offset: Fixed(5),
                    heading: Heading::Reverse,
                },
            ),
        ];
        for (controls, selected, kind, end) in cases {
            let observation = graph
                .stage_movement(MobileId(EntityId(99)), start, controls, Fixed(5))
                .expect("C-14 movement");
            assert_eq!(observation.end, end);
            assert_eq!(observation.consumed_budget, Fixed(5));
            assert_eq!(observation.junction_decisions.len(), 1);
            assert_eq!(
                observation.junction_decisions[0].selected_edge,
                Some(selected)
            );
            assert_eq!(observation.junction_decisions[0].kind, kind);
        }

        let mut stopped_controls = vec![
            MobileControlSample {
                stop: LogicLevel::High,
                left: LogicLevel::Low,
                right: LogicLevel::Low,
            },
            MobileControlSample {
                stop: LogicLevel::X,
                left: LogicLevel::High,
                right: LogicLevel::High,
            },
            MobileControlSample {
                stop: LogicLevel::Low,
                left: LogicLevel::High,
                right: LogicLevel::X,
            },
        ];
        for right in [LogicLevel::Low, LogicLevel::High, LogicLevel::X] {
            stopped_controls.push(MobileControlSample {
                stop: LogicLevel::Low,
                left: LogicLevel::X,
                right,
            });
        }
        for left in [LogicLevel::Low, LogicLevel::High] {
            stopped_controls.push(MobileControlSample {
                stop: LogicLevel::Low,
                left,
                right: LogicLevel::X,
            });
        }
        for controls in stopped_controls {
            assert!(!controls.grants_stage0_movement());
            let observation = graph
                .stage_movement(MobileId(EntityId(99)), start, controls, Fixed::ZERO)
                .expect("stopped C-14 movement");
            assert_eq!(observation.end, start);
            assert_eq!(observation.consumed_budget, Fixed::ZERO);
            assert!(observation.junction_decisions.is_empty());
        }
    }

    #[test]
    fn movement_spends_budget_across_edges_bounces_at_dead_ends_and_stops_for_missing_side() {
        let junction = JunctionId(EntityId(40));
        let incoming = WireId(EntityId(1));
        let outgoing = WireId(EntityId(2));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(3, 0))
            .expect("junction");
        let mut wires = WireStore::default();
        wires
            .push(
                incoming,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(3, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            )
            .expect("incoming edge");
        wires
            .push(
                outgoing,
                RoutingDomain::OpenWorld,
                &[point(3, 0), point(7, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("outgoing edge");
        let graph = TrackGraph::compile(&wires, &junctions).expect("track graph");
        let go_straight = MobileControlSample {
            stop: LogicLevel::Low,
            left: LogicLevel::Low,
            right: LogicLevel::Low,
        };
        let observation = graph
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Edge {
                    edge: incoming,
                    offset: Fixed(1),
                    heading: Heading::Forward,
                },
                go_straight,
                Fixed(9),
            )
            .expect("multi-edge movement");
        assert_eq!(observation.consumed_budget, Fixed(9));
        assert_eq!(observation.junction_decisions.len(), 1);
        assert_eq!(
            observation.end,
            TrackPosition::Edge {
                edge: outgoing,
                offset: Fixed(1),
                heading: Heading::Reverse,
            }
        );

        for (left, right) in [
            (LogicLevel::High, LogicLevel::Low),
            (LogicLevel::Low, LogicLevel::High),
        ] {
            let missing_side = graph
                .stage_movement(
                    MobileId(EntityId(99)),
                    TrackPosition::Junction {
                        junction,
                        incoming_edge: incoming,
                    },
                    MobileControlSample {
                        stop: LogicLevel::Low,
                        left,
                        right,
                    },
                    Fixed(1),
                )
                .expect("missing-side decision");
            assert_eq!(missing_side.consumed_budget, Fixed::ZERO);
            assert_eq!(missing_side.junction_decisions.len(), 1);
            assert_eq!(
                missing_side.junction_decisions[0].kind,
                JunctionDecisionKind::MissingRequestedSide
            );
            assert_eq!(missing_side.junction_decisions[0].selected_edge, None);
        }
    }

    #[test]
    fn isolated_edge_reflection_fast_forwards_extreme_budgets_exactly() {
        let edge = WireId(EntityId(1));
        let mut wires = WireStore::default();
        wires
            .push(
                edge,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(1, 0)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("one-unit isolated edge");
        let graph = TrackGraph::compile(&wires, &JunctionStore::default()).expect("isolated graph");
        let controls = MobileControlSample {
            stop: LogicLevel::Low,
            left: LogicLevel::Low,
            right: LogicLevel::Low,
        };

        let odd = graph
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Edge {
                    edge,
                    offset: Fixed::ZERO,
                    heading: Heading::Forward,
                },
                controls,
                Fixed(i64::MAX),
            )
            .expect("extreme odd budget fast-forwards");
        assert_eq!(odd.consumed_budget, Fixed(i64::MAX));
        assert_eq!(
            odd.end,
            TrackPosition::Edge {
                edge,
                offset: Fixed(1),
                heading: Heading::Forward,
            }
        );
        assert!(odd.junction_decisions.is_empty());

        let even = graph
            .stage_movement(MobileId(EntityId(99)), odd.end, controls, Fixed(i64::MAX))
            .expect("second extreme budget fast-forwards");
        assert_eq!(
            even.end,
            TrackPosition::Edge {
                edge,
                offset: Fixed::ZERO,
                heading: Heading::Reverse,
            }
        );

        assert_eq!(
            advance_reflecting_edge(
                Fixed(i64::MAX),
                Fixed::ZERO,
                Heading::Forward,
                Fixed(i64::MAX),
            ),
            Ok((Fixed(i64::MAX), Heading::Forward)),
            "2 * edge length is widened before reflection-period arithmetic"
        );
        assert_eq!(
            advance_reflecting_edge(
                Fixed(i64::MAX),
                Fixed(i64::MAX - 1),
                Heading::Forward,
                Fixed(i64::MAX),
            ),
            Ok((Fixed(1), Heading::Reverse))
        );

        for length in 1_i64..=9 {
            for offset in 0..=length {
                for heading in [Heading::Forward, Heading::Reverse] {
                    for distance in 1_i64..=4 * length {
                        let mut expected_offset = offset;
                        let mut expected_heading = heading;
                        for _ in 0..distance {
                            match expected_heading {
                                Heading::Forward if expected_offset == length => {
                                    expected_heading = Heading::Reverse;
                                    expected_offset -= 1;
                                }
                                Heading::Forward => expected_offset += 1,
                                Heading::Reverse if expected_offset == 0 => {
                                    expected_heading = Heading::Forward;
                                    expected_offset += 1;
                                }
                                Heading::Reverse => expected_offset -= 1,
                            }
                        }
                        assert_eq!(
                            advance_reflecting_edge(
                                Fixed(length),
                                Fixed(offset),
                                heading,
                                Fixed(distance),
                            ),
                            Ok((Fixed(expected_offset), expected_heading)),
                            "length={length}, offset={offset}, heading={heading:?}, distance={distance}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn maximum_profile_ratio_bounds_exact_degree_one_junction_observations() {
        let junction = JunctionId(EntityId(40));
        let edge = WireId(EntityId(1));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(0, 0))
            .expect("degree-one Junction");
        let mut wires = WireStore::default();
        wires
            .push(
                edge,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(1, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("one-quantum edge");
        let graph = TrackGraph::compile(&wires, &junctions).expect("degree-one graph");
        let budget = Fixed(crate::MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA);

        let observation = graph
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Edge {
                    edge,
                    offset: Fixed(1),
                    heading: Heading::Reverse,
                },
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::Low,
                    right: LogicLevel::Low,
                },
                budget,
            )
            .expect("maximum valid ratio completes with exact observations");
        assert_eq!(observation.consumed_budget, budget);
        assert_eq!(
            observation.end,
            TrackPosition::Edge {
                edge,
                offset: Fixed(1),
                heading: Heading::Forward,
            }
        );
        assert_eq!(
            observation.junction_decisions.len(),
            usize::try_from(crate::MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA / 2)
                .expect("the frozen bound fits usize")
        );
        assert!(observation.junction_decisions.iter().all(|decision| {
            *decision
                == MobileJunctionDecision {
                    junction,
                    incoming_edge: edge,
                    selected_edge: Some(edge),
                    kind: JunctionDecisionKind::Reverse,
                }
        }));
    }

    #[test]
    fn one_sample_drives_multiple_junctions_and_degree_one_low_low_reverses() {
        let first_junction = JunctionId(EntityId(40));
        let second_junction = JunctionId(EntityId(41));
        let incoming = WireId(EntityId(1));
        let middle = WireId(EntityId(2));
        let outgoing = WireId(EntityId(3));
        let mut junctions = JunctionStore::default();
        junctions
            .push(first_junction, RoutingDomain::OpenWorld, point(2, 0))
            .expect("first Junction");
        junctions
            .push(second_junction, RoutingDomain::OpenWorld, point(4, 0))
            .expect("second Junction");
        let mut wires = WireStore::default();
        wires
            .push(
                incoming,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(2, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(first_junction),
            )
            .expect("incoming edge");
        wires
            .push(
                middle,
                RoutingDomain::OpenWorld,
                &[point(2, 0), point(4, 0)],
                EndpointTarget::Junction(first_junction),
                EndpointTarget::Junction(second_junction),
            )
            .expect("middle edge");
        wires
            .push(
                outgoing,
                RoutingDomain::OpenWorld,
                &[point(4, 0), point(8, 0)],
                EndpointTarget::Junction(second_junction),
                EndpointTarget::Free,
            )
            .expect("outgoing edge");
        let graph = TrackGraph::compile(&wires, &junctions).expect("two-Junction graph");
        let controls = MobileControlSample {
            stop: LogicLevel::Low,
            left: LogicLevel::Low,
            right: LogicLevel::Low,
        };

        let observation = graph
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Edge {
                    edge: incoming,
                    offset: Fixed(1),
                    heading: Heading::Forward,
                },
                controls,
                Fixed(5),
            )
            .expect("one budget crosses two Junctions");
        assert_eq!(observation.controls, controls);
        assert_eq!(observation.consumed_budget, Fixed(5));
        assert_eq!(
            observation.end,
            TrackPosition::Edge {
                edge: outgoing,
                offset: Fixed(2),
                heading: Heading::Forward,
            }
        );
        assert_eq!(
            observation
                .junction_decisions
                .iter()
                .map(|decision| (decision.junction, decision.selected_edge, decision.kind))
                .collect::<Vec<_>>(),
            [
                (first_junction, Some(middle), JunctionDecisionKind::Straight,),
                (
                    second_junction,
                    Some(outgoing),
                    JunctionDecisionKind::Straight,
                ),
            ]
        );

        let mut degree_one_wires = WireStore::default();
        degree_one_wires
            .push(
                incoming,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(2, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(first_junction),
            )
            .expect("degree-one edge");
        let mut degree_one_junctions = JunctionStore::default();
        degree_one_junctions
            .push(first_junction, RoutingDomain::OpenWorld, point(2, 0))
            .expect("degree-one Junction");
        let degree_one = TrackGraph::compile(&degree_one_wires, &degree_one_junctions)
            .expect("degree-one graph");
        let reverse = degree_one
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Junction {
                    junction: first_junction,
                    incoming_edge: incoming,
                },
                controls,
                Fixed(1),
            )
            .expect("LOW/LOW degree-one reversal");
        assert_eq!(
            reverse.end,
            TrackPosition::Edge {
                edge: incoming,
                offset: Fixed(1),
                heading: Heading::Reverse,
            }
        );
        assert_eq!(reverse.junction_decisions.len(), 1);
        assert_eq!(
            reverse.junction_decisions[0],
            MobileJunctionDecision {
                junction: first_junction,
                incoming_edge: incoming,
                selected_edge: Some(incoming),
                kind: JunctionDecisionKind::Reverse,
            }
        );
    }

    #[test]
    fn powered_movement_stops_at_an_unpowered_junction_edge_boundary() {
        let junction = JunctionId(EntityId(40));
        let incoming = WireId(EntityId(1));
        let outgoing = WireId(EntityId(2));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(2, 0))
            .expect("Junction");
        let mut wires = WireStore::default();
        wires
            .push(
                incoming,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(2, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            )
            .expect("powered incoming Track");
        wires
            .push(
                outgoing,
                RoutingDomain::OpenWorld,
                &[point(2, 0), point(6, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("candidate outgoing Track");
        let graph = TrackGraph::compile(&wires, &junctions).expect("two-edge Track graph");
        let mobile = MobileId(EntityId(99));
        let start = TrackPosition::Edge {
            edge: incoming,
            offset: Fixed(1),
            heading: Heading::Forward,
        };
        let controls = MobileControlSample {
            stop: LogicLevel::Low,
            left: LogicLevel::Low,
            right: LogicLevel::Low,
        };
        let boundary = TrackPosition::Junction {
            junction,
            incoming_edge: incoming,
        };

        let stopped = graph
            .stage_powered_movement(
                mobile,
                start,
                controls,
                Fixed(3),
                &BTreeSet::from([incoming]),
            )
            .expect("powered movement stops at the first unpowered edge");
        assert_eq!(stopped.end, boundary);
        assert_eq!(stopped.granted_budget, Fixed(3));
        assert_eq!(stopped.consumed_budget, Fixed(1));
        assert_eq!(
            stopped.junction_decisions,
            vec![MobileJunctionDecision {
                junction,
                incoming_edge: incoming,
                selected_edge: Some(outgoing),
                kind: JunctionDecisionKind::Straight,
            }]
        );

        let entered = graph
            .stage_powered_movement(
                mobile,
                start,
                controls,
                Fixed(3),
                &BTreeSet::from([incoming, outgoing]),
            )
            .expect("power on the selected next edge permits entry");
        assert_eq!(
            entered.end,
            TrackPosition::Edge {
                edge: outgoing,
                offset: Fixed(2),
                heading: Heading::Forward,
            }
        );
        assert_eq!(entered.consumed_budget, Fixed(3));
        assert_eq!(entered.junction_decisions, stopped.junction_decisions);
    }

    #[test]
    fn negative_dot_collinear_alternative_is_a_reverse_direction_candidate() {
        let junction = JunctionId(EntityId(40));
        let incoming = WireId(EntityId(10));
        let alternative = WireId(EntityId(11));
        let mut junctions = JunctionStore::default();
        junctions
            .push(junction, RoutingDomain::OpenWorld, point(0, 0))
            .expect("Junction");
        let mut wires = WireStore::default();
        wires
            .push(
                incoming,
                RoutingDomain::OpenWorld,
                &[point(-10, 0), point(0, 0)],
                EndpointTarget::Free,
                EndpointTarget::Junction(junction),
            )
            .expect("incoming edge");
        wires
            .push(
                alternative,
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(-3, 0)],
                EndpointTarget::Junction(junction),
                EndpointTarget::Free,
            )
            .expect("reverse-direction alternative");
        let graph = TrackGraph::compile(&wires, &junctions).expect("reverse alternative graph");

        let observation = graph
            .stage_movement(
                MobileId(EntityId(99)),
                TrackPosition::Junction {
                    junction,
                    incoming_edge: incoming,
                },
                MobileControlSample {
                    stop: LogicLevel::Low,
                    left: LogicLevel::Low,
                    right: LogicLevel::Low,
                },
                Fixed(1),
            )
            .expect("reverse-direction alternative is selectable");
        assert_eq!(observation.junction_decisions.len(), 1);
        assert_eq!(
            observation.junction_decisions[0],
            MobileJunctionDecision {
                junction,
                incoming_edge: incoming,
                selected_edge: Some(alternative),
                kind: JunctionDecisionKind::Reverse,
            }
        );
        assert_eq!(
            observation.end,
            TrackPosition::Edge {
                edge: alternative,
                offset: Fixed(1),
                heading: Heading::Forward,
            }
        );
    }

    #[test]
    fn exact_angular_order_and_entity_ties_are_invariant_to_wire_store_permutation() {
        let compile = |reverse: bool, include_straight: bool| {
            let junction = JunctionId(EntityId(40));
            let mut junctions = JunctionStore::default();
            junctions
                .push(junction, RoutingDomain::OpenWorld, point(0, 0))
                .expect("junction");
            let mut definitions = vec![
                (10, point(-10, 0), point(0, 0), WireEnd::B),
                (1, point(0, 0), point(20, 20), WireEnd::A),
                (2, point(0, 0), point(2, 2), WireEnd::A),
                (3, point(0, 0), point(0, 10), WireEnd::A),
                (4, point(0, 0), point(2, -2), WireEnd::A),
                (5, point(0, 0), point(0, -10), WireEnd::A),
            ];
            if include_straight {
                definitions.push((6, point(0, 0), point(10, 0), WireEnd::A));
            }
            if reverse {
                definitions.reverse();
            }
            let mut wires = WireStore::default();
            for (id, start, end, junction_end) in definitions {
                let (endpoint_a, endpoint_b) = match junction_end {
                    WireEnd::A => (EndpointTarget::Junction(junction), EndpointTarget::Free),
                    WireEnd::B => (EndpointTarget::Free, EndpointTarget::Junction(junction)),
                };
                wires
                    .push(
                        WireId(EntityId(id)),
                        RoutingDomain::OpenWorld,
                        &[start, end],
                        endpoint_a,
                        endpoint_b,
                    )
                    .expect("turn edge");
            }
            TrackGraph::compile(&wires, &junctions).expect("turn graph")
        };
        let start = TrackPosition::Junction {
            junction: JunctionId(EntityId(40)),
            incoming_edge: WireId(EntityId(10)),
        };
        let choose = |graph: &TrackGraph, left, right| {
            graph
                .stage_movement(
                    MobileId(EntityId(99)),
                    start,
                    MobileControlSample {
                        stop: LogicLevel::Low,
                        left,
                        right,
                    },
                    Fixed(1),
                )
                .expect("exact turn")
                .junction_decisions[0]
        };

        for reverse in [false, true] {
            let graph = compile(reverse, true);
            assert_eq!(
                choose(&graph, LogicLevel::High, LogicLevel::Low).selected_edge,
                Some(WireId(EntityId(3))),
                "greatest left turn wins"
            );
            assert_eq!(
                choose(&graph, LogicLevel::Low, LogicLevel::High).selected_edge,
                Some(WireId(EntityId(5))),
                "greatest right turn wins"
            );
            assert_eq!(
                choose(&graph, LogicLevel::Low, LogicLevel::Low).selected_edge,
                Some(WireId(EntityId(6))),
                "zero-angle straight wins"
            );

            let tied = compile(reverse, false);
            assert_eq!(
                choose(&tied, LogicLevel::Low, LogicLevel::Low).selected_edge,
                Some(WireId(EntityId(1))),
                "equal 45-degree directions of different lengths tie by the smaller WireId"
            );
        }
    }
}
