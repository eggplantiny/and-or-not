use crate::{ConnectionGeneration, JunctionId, WireId};
use std::ops::Range;
use thiserror::Error;

pub(crate) const RESERVED_PATH_CERTIFICATE_ID: PathCertificateId = PathCertificateId(0);
pub(crate) const FIRST_PATH_CERTIFICATE_ID: PathCertificateId = PathCertificateId(1);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathCertificateId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PathElementStamp {
    Wire {
        id: WireId,
        generation: ConnectionGeneration,
    },
    Junction {
        id: JunctionId,
        generation: ConnectionGeneration,
    },
}

impl PathElementStamp {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::Wire { .. } => 0,
            Self::Junction { .. } => 1,
        }
    }

    pub(crate) const fn entity_id(self) -> crate::EntityId {
        match self {
            Self::Wire { id, .. } => id.entity_id(),
            Self::Junction { id, .. } => id.entity_id(),
        }
    }

    pub(crate) const fn generation(self) -> ConnectionGeneration {
        match self {
            Self::Wire { generation, .. } | Self::Junction { generation, .. } => generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathCertificate {
    pub(crate) id: PathCertificateId,
    pub(crate) element_range: Range<u32>,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum PathCertificateError {
    #[error("path certificate ID 0 is reserved")]
    ReservedCertificateId,

    #[error("path certificate ID allocator exhausted")]
    CertificateIdExhausted,

    #[error("path certificate slot index exhausted")]
    CertificateSlotIndexExhausted,

    #[error("path certificate element range exhausted")]
    ElementRangeExhausted,

    #[error("unknown path certificate {id:?}")]
    UnknownCertificate { id: PathCertificateId },

    #[error("path certificate {id:?} has already been consumed")]
    ConsumedCertificate { id: PathCertificateId },

    #[error("path certificate batch plan no longer matches the arena")]
    StaleBatchPlan,

    #[error("path certificate frontier or slot layout is invalid")]
    InvalidSlotLayout,

    #[error("path certificate slot {slot:?} stores mismatched ID {actual:?}")]
    CertificateIdMismatch {
        slot: PathCertificateId,
        actual: PathCertificateId,
    },

    #[error("path certificate {id:?} has an invalid element range")]
    InvalidElementRange { id: PathCertificateId },

    #[error("live path certificate {id:?} overlaps another live element range")]
    OverlappingElementRange { id: PathCertificateId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathCertificateBatchPlan {
    expected_next_id: u64,
    expected_slot_len: usize,
    expected_element_len: usize,
    next_id: u64,
    element_end: u32,
    ids: Vec<PathCertificateId>,
    ranges: Vec<Range<u32>>,
}

impl PathCertificateBatchPlan {
    pub(crate) fn ids(&self) -> &[PathCertificateId] {
        &self.ids
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathCertificateArena {
    next_id: u64,
    certificates: Vec<Option<PathCertificate>>,
    elements: Vec<PathElementStamp>,
    #[cfg(test)]
    certificate_frontier_limit: u64,
    #[cfg(test)]
    element_frontier_limit: u32,
}

impl Default for PathCertificateArena {
    fn default() -> Self {
        Self::new()
    }
}

impl PathCertificateArena {
    pub(crate) fn new() -> Self {
        Self {
            next_id: FIRST_PATH_CERTIFICATE_ID.0,
            certificates: vec![None],
            elements: Vec::new(),
            #[cfg(test)]
            certificate_frontier_limit: u64::MAX,
            #[cfg(test)]
            element_frontier_limit: u32::MAX,
        }
    }

    pub(crate) const fn frontier(&self) -> PathCertificateId {
        PathCertificateId(self.next_id)
    }

    pub(crate) const fn allocated_count(&self) -> u64 {
        self.next_id - FIRST_PATH_CERTIFICATE_ID.0
    }

    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.certificates
            .iter()
            .skip(1)
            .filter(|certificate| certificate.is_some())
            .count()
    }

    pub(crate) fn canonical_slots(
        &self,
    ) -> impl Iterator<Item = (PathCertificateId, Option<&PathCertificate>)> + '_ {
        (FIRST_PATH_CERTIFICATE_ID.0..self.next_id)
            .zip(self.certificates.iter().skip(1))
            .map(|(id, certificate)| (PathCertificateId(id), certificate.as_ref()))
    }

    pub(crate) fn elements(
        &self,
        id: PathCertificateId,
    ) -> Result<&[PathElementStamp], PathCertificateError> {
        let certificate = self.live_certificate(id)?;
        self.element_slice(certificate)
    }

    #[cfg(test)]
    pub(crate) fn allocate_batch(
        &mut self,
        paths: &[&[PathElementStamp]],
    ) -> Result<Vec<PathCertificateId>, PathCertificateError> {
        let plan = self.preflight_batch(paths)?;
        let ids = plan.ids.clone();
        self.allocate_preflighted(paths, plan)?;
        Ok(ids)
    }

    pub(crate) fn preflight_batch(
        &self,
        paths: &[&[PathElementStamp]],
    ) -> Result<PathCertificateBatchPlan, PathCertificateError> {
        self.validate_shape()?;

        let certificate_count =
            u64::try_from(paths.len()).map_err(|_| PathCertificateError::CertificateIdExhausted)?;
        let next_id = self
            .next_id
            .checked_add(certificate_count)
            .ok_or(PathCertificateError::CertificateIdExhausted)?;
        #[cfg(test)]
        if next_id > self.certificate_frontier_limit {
            return Err(PathCertificateError::CertificateIdExhausted);
        }

        let expected_slot_len = self.certificates.len();
        let _next_slot_len = expected_slot_len
            .checked_add(paths.len())
            .ok_or(PathCertificateError::CertificateSlotIndexExhausted)?;

        let mut element_end = u32::try_from(self.elements.len())
            .map_err(|_| PathCertificateError::ElementRangeExhausted)?;
        let mut ranges = Vec::with_capacity(paths.len());
        for path in paths {
            let element_count = u32::try_from(path.len())
                .map_err(|_| PathCertificateError::ElementRangeExhausted)?;
            let start = element_end;
            element_end = element_end
                .checked_add(element_count)
                .ok_or(PathCertificateError::ElementRangeExhausted)?;
            #[cfg(test)]
            if element_end > self.element_frontier_limit {
                return Err(PathCertificateError::ElementRangeExhausted);
            }
            ranges.push(start..element_end);
        }

        let ids = (self.next_id..next_id).map(PathCertificateId).collect();
        Ok(PathCertificateBatchPlan {
            expected_next_id: self.next_id,
            expected_slot_len,
            expected_element_len: self.elements.len(),
            next_id,
            element_end,
            ids,
            ranges,
        })
    }

    pub(crate) fn allocate_preflighted(
        &mut self,
        paths: &[&[PathElementStamp]],
        plan: PathCertificateBatchPlan,
    ) -> Result<(), PathCertificateError> {
        if self.next_id != plan.expected_next_id
            || self.certificates.len() != plan.expected_slot_len
            || self.elements.len() != plan.expected_element_len
            || paths.len() != plan.ids.len()
            || paths.len() != plan.ranges.len()
        {
            return Err(PathCertificateError::StaleBatchPlan);
        }

        for (range, path) in plan.ranges.iter().zip(paths) {
            let expected_len = usize::try_from(range.end - range.start)
                .map_err(|_| PathCertificateError::ElementRangeExhausted)?;
            if expected_len != path.len() {
                return Err(PathCertificateError::StaleBatchPlan);
            }
        }

        for ((id, range), path) in plan
            .ids
            .iter()
            .copied()
            .zip(plan.ranges.iter().cloned())
            .zip(paths)
        {
            self.certificates.push(Some(PathCertificate {
                id,
                element_range: range,
            }));
            self.elements.extend_from_slice(path);
        }
        debug_assert_eq!(
            u32::try_from(self.elements.len()).ok(),
            Some(plan.element_end)
        );
        self.next_id = plan.next_id;
        Ok(())
    }

    pub(crate) fn consume(
        &mut self,
        id: PathCertificateId,
    ) -> Result<Vec<PathElementStamp>, PathCertificateError> {
        let index = self.live_index(id)?;
        let certificate = self
            .certificates
            .get(index)
            .ok_or(PathCertificateError::InvalidSlotLayout)?
            .as_ref()
            .ok_or(PathCertificateError::ConsumedCertificate { id })?;
        if certificate.id != id {
            return Err(PathCertificateError::CertificateIdMismatch {
                slot: id,
                actual: certificate.id,
            });
        }
        let elements = self.element_slice(certificate)?.to_vec();
        *self
            .certificates
            .get_mut(index)
            .ok_or(PathCertificateError::InvalidSlotLayout)? = None;
        Ok(elements)
    }

    pub(crate) fn validate_shape(&self) -> Result<(), PathCertificateError> {
        if self.next_id == RESERVED_PATH_CERTIFICATE_ID.0
            || self.certificates.first() != Some(&None)
            || usize::try_from(self.next_id).ok() != Some(self.certificates.len())
        {
            return Err(PathCertificateError::InvalidSlotLayout);
        }

        let mut live_ranges = Vec::new();
        for (slot_id, certificate) in self.canonical_slots() {
            let Some(certificate) = certificate else {
                continue;
            };
            if certificate.id != slot_id {
                return Err(PathCertificateError::CertificateIdMismatch {
                    slot: slot_id,
                    actual: certificate.id,
                });
            }
            self.element_slice(certificate)?;
            if !certificate.element_range.is_empty() {
                live_ranges.push((certificate.element_range.clone(), slot_id));
            }
        }
        live_ranges.sort_unstable_by_key(|(range, id)| (range.start, range.end, *id));
        let mut previous_end = 0_u32;
        for (range, id) in live_ranges {
            if range.start < previous_end {
                return Err(PathCertificateError::OverlappingElementRange { id });
            }
            previous_end = range.end;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_frontier_limits_for_test(
        &mut self,
        certificate_frontier_limit: u64,
        element_frontier_limit: u32,
    ) {
        self.certificate_frontier_limit = certificate_frontier_limit;
        self.element_frontier_limit = element_frontier_limit;
    }

    fn live_index(&self, id: PathCertificateId) -> Result<usize, PathCertificateError> {
        if id == RESERVED_PATH_CERTIFICATE_ID {
            return Err(PathCertificateError::ReservedCertificateId);
        }
        if id.0 >= self.next_id {
            return Err(PathCertificateError::UnknownCertificate { id });
        }
        usize::try_from(id.0).map_err(|_| PathCertificateError::CertificateSlotIndexExhausted)
    }

    fn live_certificate(
        &self,
        id: PathCertificateId,
    ) -> Result<&PathCertificate, PathCertificateError> {
        let index = self.live_index(id)?;
        let certificate = self
            .certificates
            .get(index)
            .ok_or(PathCertificateError::InvalidSlotLayout)?
            .as_ref()
            .ok_or(PathCertificateError::ConsumedCertificate { id })?;
        if certificate.id != id {
            return Err(PathCertificateError::CertificateIdMismatch {
                slot: id,
                actual: certificate.id,
            });
        }
        Ok(certificate)
    }

    fn element_slice(
        &self,
        certificate: &PathCertificate,
    ) -> Result<&[PathElementStamp], PathCertificateError> {
        if certificate.element_range.start > certificate.element_range.end {
            return Err(PathCertificateError::InvalidElementRange { id: certificate.id });
        }
        let start = usize::try_from(certificate.element_range.start)
            .map_err(|_| PathCertificateError::InvalidElementRange { id: certificate.id })?;
        let end = usize::try_from(certificate.element_range.end)
            .map_err(|_| PathCertificateError::InvalidElementRange { id: certificate.id })?;
        self.elements
            .get(start..end)
            .ok_or(PathCertificateError::InvalidElementRange { id: certificate.id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityId, JunctionId, WireId};

    const fn wire(id: u64, generation: u64) -> PathElementStamp {
        PathElementStamp::Wire {
            id: WireId(EntityId(id)),
            generation: ConnectionGeneration(generation),
        }
    }

    const fn junction(id: u64, generation: u64) -> PathElementStamp {
        PathElementStamp::Junction {
            id: JunctionId(EntityId(id)),
            generation: ConnectionGeneration(generation),
        }
    }

    #[test]
    fn zero_is_reserved_and_empty_local_certificates_are_live() {
        let mut arena = PathCertificateArena::new();
        assert_eq!(arena.frontier(), FIRST_PATH_CERTIFICATE_ID);
        assert_eq!(arena.allocated_count(), 0);
        assert_eq!(
            arena.consume(RESERVED_PATH_CERTIFICATE_ID),
            Err(PathCertificateError::ReservedCertificateId)
        );

        let ids = arena.allocate_batch(&[&[]]).expect("empty path allocates");
        assert_eq!(ids, [PathCertificateId(1)]);
        assert_eq!(arena.frontier(), PathCertificateId(2));
        assert_eq!(arena.allocated_count(), 1);
        assert_eq!(arena.elements(ids[0]), Ok([].as_slice()));
        assert_eq!(arena.live_count(), 1);
        assert_eq!(arena.validate_shape(), Ok(()));
    }

    #[test]
    fn batch_allocation_preserves_path_order_and_assigns_contiguous_ids() {
        let mut arena = PathCertificateArena::new();
        let first = [wire(7, 2), junction(8, 3), wire(9, 5)];
        let second = [wire(11, 13)];
        let ids = arena
            .allocate_batch(&[first.as_slice(), second.as_slice()])
            .expect("batch allocates");

        assert_eq!(ids, [PathCertificateId(1), PathCertificateId(2)]);
        assert_eq!(arena.elements(ids[0]), Ok(first.as_slice()));
        assert_eq!(arena.elements(ids[1]), Ok(second.as_slice()));
        assert_eq!(
            arena
                .canonical_slots()
                .map(|(id, certificate)| (id, certificate.is_some()))
                .collect::<Vec<_>>(),
            [(PathCertificateId(1), true), (PathCertificateId(2), true)]
        );
    }

    #[test]
    fn consumption_leaves_a_tombstone_and_never_reuses_the_id() {
        let mut arena = PathCertificateArena::new();
        let path = [wire(4, 1)];
        let first = arena
            .allocate_batch(&[path.as_slice()])
            .expect("first certificate allocates")[0];

        assert_eq!(arena.consume(first), Ok(path.to_vec()));
        assert_eq!(
            arena.consume(first),
            Err(PathCertificateError::ConsumedCertificate { id: first })
        );
        assert_eq!(arena.live_count(), 0);
        assert_eq!(
            arena
                .canonical_slots()
                .next()
                .map(|(_, value)| value.is_none()),
            Some(true)
        );

        let replacement = arena.allocate_batch(&[&[]]).expect("replacement allocates")[0];
        assert_eq!(replacement, PathCertificateId(2));
        assert_eq!(arena.frontier(), PathCertificateId(3));
    }

    #[test]
    fn batch_preflight_failure_is_locally_atomic() {
        let mut arena = PathCertificateArena::new();
        arena.set_frontier_limits_for_test(2, 1);
        let baseline = arena.clone();
        let too_many_certificates = arena.allocate_batch(&[&[], &[]]);
        assert_eq!(
            too_many_certificates,
            Err(PathCertificateError::CertificateIdExhausted)
        );
        assert_eq!(arena, baseline);

        let too_many_elements = arena.allocate_batch(&[&[wire(1, 0), wire(2, 0)]]);
        assert_eq!(
            too_many_elements,
            Err(PathCertificateError::ElementRangeExhausted)
        );
        assert_eq!(arena, baseline);
    }

    #[test]
    fn exact_frontier_limits_succeed_and_the_next_allocation_is_atomic() {
        let mut arena = PathCertificateArena::new();
        arena.set_frontier_limits_for_test(3, 1);
        let one_element = [wire(1, 0)];

        assert_eq!(
            arena.allocate_batch(&[one_element.as_slice()]),
            Ok(vec![PathCertificateId(1)])
        );
        assert_eq!(arena.frontier(), PathCertificateId(2));
        assert_eq!(
            arena.elements(PathCertificateId(1)),
            Ok(one_element.as_slice())
        );

        let element_boundary = arena.clone();
        assert_eq!(
            arena.allocate_batch(&[one_element.as_slice()]),
            Err(PathCertificateError::ElementRangeExhausted)
        );
        assert_eq!(arena, element_boundary);

        assert_eq!(arena.allocate_batch(&[&[]]), Ok(vec![PathCertificateId(2)]));
        assert_eq!(arena.frontier(), PathCertificateId(3));

        let certificate_boundary = arena.clone();
        assert_eq!(
            arena.allocate_batch(&[&[]]),
            Err(PathCertificateError::CertificateIdExhausted)
        );
        assert_eq!(arena, certificate_boundary);
    }

    #[test]
    fn a_stale_plan_cannot_partially_mutate_the_arena() {
        let mut arena = PathCertificateArena::new();
        let path = [wire(3, 0)];
        let paths = [path.as_slice()];
        let plan = arena.preflight_batch(&paths).expect("plan succeeds");
        arena
            .allocate_batch(&[&[]])
            .expect("intervening allocation succeeds");
        let baseline = arena.clone();

        assert_eq!(
            arena.allocate_preflighted(&paths, plan),
            Err(PathCertificateError::StaleBatchPlan)
        );
        assert_eq!(arena, baseline);
    }

    #[test]
    fn element_tags_and_accessors_are_frozen() {
        let wire = wire(17, 19);
        let junction = junction(23, 29);
        assert_eq!(wire.canonical_tag(), 0);
        assert_eq!(junction.canonical_tag(), 1);
        assert_eq!(wire.entity_id(), EntityId(17));
        assert_eq!(junction.entity_id(), EntityId(23));
        assert_eq!(wire.generation(), ConnectionGeneration(19));
        assert_eq!(junction.generation(), ConnectionGeneration(29));
        assert!(wire < junction);
    }
}
