use crate::{
    ConnectionGeneration, EntityId, FixedSubstrateIndex, FixedVec2, GateId, GateIndex, JunctionId,
    JunctionIndex, NumericError, WireId, WireIndex,
};
use std::ops::Range;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GateType {
    And,
    Or,
    Not,
}

impl GateType {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::And => 0,
            Self::Or => 1,
            Self::Not => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoutingDomain {
    OpenWorld,
    FixedSubstrate(EntityId),
    MobileSubstrate(EntityId),
}

impl RoutingDomain {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::OpenWorld => 0,
            Self::FixedSubstrate(_) => 1,
            Self::MobileSubstrate(_) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GatePort {
    InputA,
    InputB,
    Output,
    Power,
}

impl GatePort {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::InputA => 0,
            Self::InputB => 1,
            Self::Output => 2,
            Self::Power => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GatePortRef {
    pub gate: GateId,
    pub port: GatePort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WireEnd {
    A,
    B,
}

impl WireEnd {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointTarget {
    Free,
    Junction(JunctionId),
    GatePort(GatePortRef),
}

impl EndpointTarget {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::Free => 0,
            Self::Junction(_) => 1,
            Self::GatePort(_) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FixedAabb {
    pub min: FixedVec2,
    pub max: FixedVec2,
}

impl FixedAabb {
    pub const fn new(min: FixedVec2, max: FixedVec2) -> Self {
        Self { min, max }
    }

    pub const fn is_nonempty(self) -> bool {
        self.min.x.0 < self.max.x.0 && self.min.y.0 < self.max.y.0
    }

    pub const fn contains_point(self, point: FixedVec2) -> bool {
        self.min.x.0 <= point.x.0
            && point.x.0 <= self.max.x.0
            && self.min.y.0 <= point.y.0
            && point.y.0 <= self.max.y.0
    }

    pub const fn contains_aabb(self, other: Self) -> bool {
        self.min.x.0 <= other.min.x.0
            && other.max.x.0 <= self.max.x.0
            && self.min.y.0 <= other.min.y.0
            && other.max.y.0 <= self.max.y.0
    }

    pub const fn interior_overlaps(self, other: Self) -> bool {
        self.min.x.0 < other.max.x.0
            && other.min.x.0 < self.max.x.0
            && self.min.y.0 < other.max.y.0
            && other.min.y.0 < self.max.y.0
    }

    pub fn translated(self, origin: FixedVec2) -> Result<Self, NumericError> {
        Ok(Self {
            min: checked_add_point(self.min, origin)?,
            max: checked_add_point(self.max, origin)?,
        })
    }

    pub fn translated_inverse(self, origin: FixedVec2) -> Result<Self, NumericError> {
        Ok(Self {
            min: checked_sub_point(self.min, origin)?,
            max: checked_sub_point(self.max, origin)?,
        })
    }
}

pub(crate) fn checked_add_point(
    left: FixedVec2,
    right: FixedVec2,
) -> Result<FixedVec2, NumericError> {
    Ok(FixedVec2::new(
        left.x
            .checked_add(right.x)
            .map_err(|_| NumericError::Overflow)?,
        left.y
            .checked_add(right.y)
            .map_err(|_| NumericError::Overflow)?,
    ))
}

pub(crate) fn checked_sub_point(
    left: FixedVec2,
    right: FixedVec2,
) -> Result<FixedVec2, NumericError> {
    Ok(FixedVec2::new(
        left.x
            .checked_sub(right.x)
            .map_err(|_| NumericError::Overflow)?,
        left.y
            .checked_sub(right.y)
            .map_err(|_| NumericError::Overflow)?,
    ))
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum TopologyError {
    #[error("canonical topology numeric overflow")]
    NumericOverflow,

    #[error("canonical topology store index exhausted")]
    StoreIndexExhausted,

    #[error("canonical geometry arena offset exhausted")]
    GeometryArenaExhausted,

    #[error("unknown canonical topology store index")]
    UnknownStoreIndex,

    #[error("canonical topology record has already been removed")]
    RemovedRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GateRecord {
    pub id: GateId,
    pub gate_type: GateType,
    pub origin: FixedVec2,
    pub routing_domain: RoutingDomain,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GateStore {
    ids: Vec<GateId>,
    alive: Vec<bool>,
    gate_types: Vec<GateType>,
    origins: Vec<FixedVec2>,
    routing_domains: Vec<RoutingDomain>,
}

impl GateStore {
    pub fn push(
        &mut self,
        id: GateId,
        gate_type: GateType,
        origin: FixedVec2,
        routing_domain: RoutingDomain,
    ) -> Result<GateIndex, TopologyError> {
        let index = store_index(self.ids.len()).map(GateIndex)?;
        self.ids.push(id);
        self.alive.push(true);
        self.gate_types.push(gate_type);
        self.origins.push(origin);
        self.routing_domains.push(routing_domain);
        Ok(index)
    }

    pub fn get(&self, index: GateIndex) -> Option<GateRecord> {
        let index = usize::try_from(index.0).ok()?;
        self.alive.get(index).copied().filter(|alive| *alive)?;
        Some(GateRecord {
            id: *self.ids.get(index)?,
            gate_type: *self.gate_types.get(index)?,
            origin: *self.origins.get(index)?,
            routing_domain: *self.routing_domains.get(index)?,
        })
    }

    pub fn remove(&mut self, index: GateIndex) -> Result<GateRecord, TopologyError> {
        let record = self.get(index).ok_or_else(|| self.index_error(index.0))?;
        self.alive[index.0 as usize] = false;
        Ok(record)
    }

    pub fn iter_alive(&self) -> impl Iterator<Item = (GateIndex, GateRecord)> + '_ {
        (0..self.ids.len()).filter_map(|raw| {
            let raw = u32::try_from(raw).ok()?;
            let index = GateIndex(raw);
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
        self.gate_types.reserve(additional);
        self.origins.reserve(additional);
        self.routing_domains.reserve(additional);
    }

    #[cfg(test)]
    pub fn swap_slots_for_test(
        &mut self,
        first: GateIndex,
        second: GateIndex,
    ) -> Result<(), TopologyError> {
        self.get(first).ok_or_else(|| self.index_error(first.0))?;
        self.get(second).ok_or_else(|| self.index_error(second.0))?;
        let first = first.0 as usize;
        let second = second.0 as usize;
        self.ids.swap(first, second);
        self.alive.swap(first, second);
        self.gate_types.swap(first, second);
        self.origins.swap(first, second);
        self.routing_domains.swap(first, second);
        Ok(())
    }

    fn index_error(&self, raw: u32) -> TopologyError {
        match self.alive.get(raw as usize) {
            Some(false) => TopologyError::RemovedRecord,
            _ => TopologyError::UnknownStoreIndex,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GeometryArena {
    points: Vec<FixedVec2>,
}

impl GeometryArena {
    fn append(&mut self, points: &[FixedVec2]) -> Result<Range<u32>, TopologyError> {
        let start =
            u32::try_from(self.points.len()).map_err(|_| TopologyError::GeometryArenaExhausted)?;
        let end = self
            .points
            .len()
            .checked_add(points.len())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(TopologyError::GeometryArenaExhausted)?;
        self.points.extend_from_slice(points);
        Ok(start..end)
    }

    fn get(&self, range: Range<u32>) -> Option<&[FixedVec2]> {
        self.points
            .get(usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?)
    }

    #[cfg(test)]
    fn reserve_capacity_for_test(&mut self, additional: usize) {
        self.points.reserve(additional);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WireRecord<'a> {
    pub id: WireId,
    pub routing_domain: RoutingDomain,
    pub points: &'a [FixedVec2],
    pub endpoint_a: EndpointTarget,
    pub endpoint_b: EndpointTarget,
    pub connection_generation: ConnectionGeneration,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WireStore {
    ids: Vec<WireId>,
    alive: Vec<bool>,
    routing_domains: Vec<RoutingDomain>,
    geometry_ranges: Vec<Range<u32>>,
    endpoint_a: Vec<EndpointTarget>,
    endpoint_b: Vec<EndpointTarget>,
    connection_generations: Vec<ConnectionGeneration>,
    geometry: GeometryArena,
}

impl WireStore {
    pub fn push(
        &mut self,
        id: WireId,
        routing_domain: RoutingDomain,
        points: &[FixedVec2],
        endpoint_a: EndpointTarget,
        endpoint_b: EndpointTarget,
    ) -> Result<WireIndex, TopologyError> {
        let index = store_index(self.ids.len()).map(WireIndex)?;
        let range = self.geometry.append(points)?;
        self.ids.push(id);
        self.alive.push(true);
        self.routing_domains.push(routing_domain);
        self.geometry_ranges.push(range);
        self.endpoint_a.push(endpoint_a);
        self.endpoint_b.push(endpoint_b);
        self.connection_generations
            .push(ConnectionGeneration::INITIAL);
        Ok(index)
    }

    pub fn get(&self, index: WireIndex) -> Option<WireRecord<'_>> {
        let index = usize::try_from(index.0).ok()?;
        self.alive.get(index).copied().filter(|alive| *alive)?;
        Some(WireRecord {
            id: *self.ids.get(index)?,
            routing_domain: *self.routing_domains.get(index)?,
            points: self
                .geometry
                .get(self.geometry_ranges.get(index)?.clone())?,
            endpoint_a: *self.endpoint_a.get(index)?,
            endpoint_b: *self.endpoint_b.get(index)?,
            connection_generation: *self.connection_generations.get(index)?,
        })
    }

    pub fn remove(&mut self, index: WireIndex) -> Result<WireRecord<'_>, TopologyError> {
        let raw = index.0 as usize;
        if !self.alive.get(raw).copied().unwrap_or(false) {
            return Err(self.index_error(index.0));
        }
        self.alive[raw] = false;
        self.get_removed(index)
    }

    fn get_removed(&self, index: WireIndex) -> Result<WireRecord<'_>, TopologyError> {
        let raw = index.0 as usize;
        Ok(WireRecord {
            id: *self.ids.get(raw).ok_or(TopologyError::UnknownStoreIndex)?,
            routing_domain: *self
                .routing_domains
                .get(raw)
                .ok_or(TopologyError::UnknownStoreIndex)?,
            points: self
                .geometry
                .get(
                    self.geometry_ranges
                        .get(raw)
                        .ok_or(TopologyError::UnknownStoreIndex)?
                        .clone(),
                )
                .ok_or(TopologyError::UnknownStoreIndex)?,
            endpoint_a: *self
                .endpoint_a
                .get(raw)
                .ok_or(TopologyError::UnknownStoreIndex)?,
            endpoint_b: *self
                .endpoint_b
                .get(raw)
                .ok_or(TopologyError::UnknownStoreIndex)?,
            connection_generation: *self
                .connection_generations
                .get(raw)
                .ok_or(TopologyError::UnknownStoreIndex)?,
        })
    }

    pub fn endpoint(&self, index: WireIndex, end: WireEnd) -> Option<EndpointTarget> {
        let record = self.get(index)?;
        Some(match end {
            WireEnd::A => record.endpoint_a,
            WireEnd::B => record.endpoint_b,
        })
    }

    pub fn set_endpoint(
        &mut self,
        index: WireIndex,
        end: WireEnd,
        target: EndpointTarget,
    ) -> Result<(), TopologyError> {
        self.get(index).ok_or_else(|| self.index_error(index.0))?;
        let slot = index.0 as usize;
        match end {
            WireEnd::A => self.endpoint_a[slot] = target,
            WireEnd::B => self.endpoint_b[slot] = target,
        }
        Ok(())
    }

    pub fn advance_generation(&mut self, index: WireIndex) -> Result<(), TopologyError> {
        self.get(index).ok_or_else(|| self.index_error(index.0))?;
        let slot = index.0 as usize;
        self.connection_generations[slot] = self.connection_generations[slot]
            .checked_advance()
            .map_err(|_| TopologyError::NumericOverflow)?;
        Ok(())
    }

    pub fn iter_alive(&self) -> impl Iterator<Item = (WireIndex, WireRecord<'_>)> + '_ {
        (0..self.ids.len()).filter_map(|raw| {
            let raw = u32::try_from(raw).ok()?;
            let index = WireIndex(raw);
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
        self.routing_domains.reserve(additional);
        self.geometry_ranges.reserve(additional);
        self.endpoint_a.reserve(additional);
        self.endpoint_b.reserve(additional);
        self.connection_generations.reserve(additional);
        self.geometry.reserve_capacity_for_test(additional);
    }

    #[cfg(test)]
    pub fn swap_slots_for_test(
        &mut self,
        first: WireIndex,
        second: WireIndex,
    ) -> Result<(), TopologyError> {
        self.get(first).ok_or_else(|| self.index_error(first.0))?;
        self.get(second).ok_or_else(|| self.index_error(second.0))?;
        let first = first.0 as usize;
        let second = second.0 as usize;
        self.ids.swap(first, second);
        self.alive.swap(first, second);
        self.routing_domains.swap(first, second);
        self.geometry_ranges.swap(first, second);
        self.endpoint_a.swap(first, second);
        self.endpoint_b.swap(first, second);
        self.connection_generations.swap(first, second);
        Ok(())
    }

    #[cfg(test)]
    pub fn force_generation_for_test(
        &mut self,
        index: WireIndex,
        generation: ConnectionGeneration,
    ) -> Result<(), TopologyError> {
        self.get(index).ok_or_else(|| self.index_error(index.0))?;
        self.connection_generations[index.0 as usize] = generation;
        Ok(())
    }

    fn index_error(&self, raw: u32) -> TopologyError {
        match self.alive.get(raw as usize) {
            Some(false) => TopologyError::RemovedRecord,
            _ => TopologyError::UnknownStoreIndex,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JunctionRecord {
    pub id: JunctionId,
    pub routing_domain: RoutingDomain,
    pub position: FixedVec2,
    pub connection_generation: ConnectionGeneration,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JunctionStore {
    ids: Vec<JunctionId>,
    alive: Vec<bool>,
    routing_domains: Vec<RoutingDomain>,
    positions: Vec<FixedVec2>,
    connection_generations: Vec<ConnectionGeneration>,
}

impl JunctionStore {
    pub fn push(
        &mut self,
        id: JunctionId,
        routing_domain: RoutingDomain,
        position: FixedVec2,
    ) -> Result<JunctionIndex, TopologyError> {
        let index = store_index(self.ids.len()).map(JunctionIndex)?;
        self.ids.push(id);
        self.alive.push(true);
        self.routing_domains.push(routing_domain);
        self.positions.push(position);
        self.connection_generations
            .push(ConnectionGeneration::INITIAL);
        Ok(index)
    }

    pub fn get(&self, index: JunctionIndex) -> Option<JunctionRecord> {
        let index = usize::try_from(index.0).ok()?;
        self.alive.get(index).copied().filter(|alive| *alive)?;
        Some(JunctionRecord {
            id: *self.ids.get(index)?,
            routing_domain: *self.routing_domains.get(index)?,
            position: *self.positions.get(index)?,
            connection_generation: *self.connection_generations.get(index)?,
        })
    }

    pub fn remove(&mut self, index: JunctionIndex) -> Result<JunctionRecord, TopologyError> {
        let record = self.get(index).ok_or_else(|| self.index_error(index.0))?;
        self.alive[index.0 as usize] = false;
        Ok(record)
    }

    pub fn advance_generation(&mut self, index: JunctionIndex) -> Result<(), TopologyError> {
        self.get(index).ok_or_else(|| self.index_error(index.0))?;
        let slot = index.0 as usize;
        self.connection_generations[slot] = self.connection_generations[slot]
            .checked_advance()
            .map_err(|_| TopologyError::NumericOverflow)?;
        Ok(())
    }

    pub fn iter_alive(&self) -> impl Iterator<Item = (JunctionIndex, JunctionRecord)> + '_ {
        (0..self.ids.len()).filter_map(|raw| {
            let raw = u32::try_from(raw).ok()?;
            let index = JunctionIndex(raw);
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
        self.routing_domains.reserve(additional);
        self.positions.reserve(additional);
        self.connection_generations.reserve(additional);
    }

    #[cfg(test)]
    pub fn swap_slots_for_test(
        &mut self,
        first: JunctionIndex,
        second: JunctionIndex,
    ) -> Result<(), TopologyError> {
        self.get(first).ok_or_else(|| self.index_error(first.0))?;
        self.get(second).ok_or_else(|| self.index_error(second.0))?;
        let first = first.0 as usize;
        let second = second.0 as usize;
        self.ids.swap(first, second);
        self.alive.swap(first, second);
        self.routing_domains.swap(first, second);
        self.positions.swap(first, second);
        self.connection_generations.swap(first, second);
        Ok(())
    }

    #[cfg(test)]
    pub fn force_generation_for_test(
        &mut self,
        index: JunctionIndex,
        generation: ConnectionGeneration,
    ) -> Result<(), TopologyError> {
        self.get(index).ok_or_else(|| self.index_error(index.0))?;
        self.connection_generations[index.0 as usize] = generation;
        Ok(())
    }

    fn index_error(&self, raw: u32) -> TopologyError {
        match self.alive.get(raw as usize) {
            Some(false) => TopologyError::RemovedRecord,
            _ => TopologyError::UnknownStoreIndex,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FixedSubstrateRecord {
    pub id: EntityId,
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FixedSubstrateStore {
    ids: Vec<EntityId>,
    alive: Vec<bool>,
    origins: Vec<FixedVec2>,
    routing_areas: Vec<FixedAabb>,
    footprints: Vec<FixedAabb>,
}

impl FixedSubstrateStore {
    pub fn push(
        &mut self,
        id: EntityId,
        origin: FixedVec2,
        routing_area: FixedAabb,
        footprint: FixedAabb,
    ) -> Result<FixedSubstrateIndex, TopologyError> {
        let index = store_index(self.ids.len()).map(FixedSubstrateIndex)?;
        self.ids.push(id);
        self.alive.push(true);
        self.origins.push(origin);
        self.routing_areas.push(routing_area);
        self.footprints.push(footprint);
        Ok(index)
    }

    pub fn get(&self, index: FixedSubstrateIndex) -> Option<FixedSubstrateRecord> {
        let index = usize::try_from(index.0).ok()?;
        self.alive.get(index).copied().filter(|alive| *alive)?;
        Some(FixedSubstrateRecord {
            id: *self.ids.get(index)?,
            origin: *self.origins.get(index)?,
            routing_area: *self.routing_areas.get(index)?,
            footprint: *self.footprints.get(index)?,
        })
    }

    pub fn remove(
        &mut self,
        index: FixedSubstrateIndex,
    ) -> Result<FixedSubstrateRecord, TopologyError> {
        let record = self.get(index).ok_or_else(|| self.index_error(index.0))?;
        self.alive[index.0 as usize] = false;
        Ok(record)
    }

    pub fn iter_alive(
        &self,
    ) -> impl Iterator<Item = (FixedSubstrateIndex, FixedSubstrateRecord)> + '_ {
        (0..self.ids.len()).filter_map(|raw| {
            let raw = u32::try_from(raw).ok()?;
            let index = FixedSubstrateIndex(raw);
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
        self.origins.reserve(additional);
        self.routing_areas.reserve(additional);
        self.footprints.reserve(additional);
    }

    #[cfg(test)]
    pub fn swap_slots_for_test(
        &mut self,
        first: FixedSubstrateIndex,
        second: FixedSubstrateIndex,
    ) -> Result<(), TopologyError> {
        self.get(first).ok_or_else(|| self.index_error(first.0))?;
        self.get(second).ok_or_else(|| self.index_error(second.0))?;
        let first = first.0 as usize;
        let second = second.0 as usize;
        self.ids.swap(first, second);
        self.alive.swap(first, second);
        self.origins.swap(first, second);
        self.routing_areas.swap(first, second);
        self.footprints.swap(first, second);
        Ok(())
    }

    fn index_error(&self, raw: u32) -> TopologyError {
        match self.alive.get(raw as usize) {
            Some(false) => TopologyError::RemovedRecord,
            _ => TopologyError::UnknownStoreIndex,
        }
    }
}

fn store_index(length: usize) -> Result<u32, TopologyError> {
    u32::try_from(length).map_err(|_| TopologyError::StoreIndexExhausted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fixed;

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    #[test]
    fn aabb_uses_open_interior_overlap_and_checked_translation() {
        let left = FixedAabb::new(point(0, 0), point(10, 10));
        let touching = FixedAabb::new(point(10, 0), point(20, 10));
        let overlapping = FixedAabb::new(point(9, 1), point(20, 9));

        assert!(left.is_nonempty());
        assert!(!left.interior_overlaps(touching));
        assert!(left.interior_overlaps(overlapping));
        assert_eq!(
            left.translated(point(-2, 3)),
            Ok(FixedAabb::new(point(-2, 3), point(8, 13)))
        );
        assert_eq!(
            left.translated(point(i64::MAX, 0)),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn soa_stores_leave_tombstones_and_iterate_live_ids_in_creation_order() {
        let mut gates = GateStore::default();
        let first = gates
            .push(
                GateId(EntityId(1)),
                GateType::And,
                point(0, 0),
                RoutingDomain::OpenWorld,
            )
            .expect("first gate stores");
        gates
            .push(
                GateId(EntityId(3)),
                GateType::Not,
                point(1, 0),
                RoutingDomain::OpenWorld,
            )
            .expect("second gate stores");
        gates.remove(first).expect("first gate removes");

        let ids: Vec<_> = gates.iter_alive().map(|(_, record)| record.id).collect();
        assert_eq!(ids, vec![GateId(EntityId(3))]);
        assert_eq!(gates.live_count(), 1);
    }

    #[test]
    fn wire_geometry_is_arena_backed_and_generation_is_checked() {
        let mut wires = WireStore::default();
        let index = wires
            .push(
                WireId(EntityId(2)),
                RoutingDomain::OpenWorld,
                &[point(0, 0), point(4, 0)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            )
            .expect("wire stores");

        wires
            .set_endpoint(
                index,
                WireEnd::B,
                EndpointTarget::Junction(JunctionId(EntityId(4))),
            )
            .expect("endpoint updates");
        wires
            .advance_generation(index)
            .expect("generation advances");
        let record = wires.get(index).expect("wire remains live");
        assert_eq!(record.points, &[point(0, 0), point(4, 0)]);
        assert_eq!(record.connection_generation, ConnectionGeneration(1));
        assert_eq!(
            record.endpoint_b,
            EndpointTarget::Junction(JunctionId(EntityId(4)))
        );
    }
}
