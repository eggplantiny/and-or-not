pub use crate::path_certificate::PathCertificateId;
use crate::path_certificate::{PathCertificateArena, PathCertificateError, PathElementStamp};
use crate::{DriveStrength, DriverId, LogicLevel, Revision, SinkId, Tick};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use thiserror::Error;

pub const DRIVER_TRANSITION_KIND_ORDER: u8 = 0;
pub const SIGNAL_ARRIVAL_KIND_ORDER: u8 = 1;
pub const RESERVED_EVENT_PAYLOAD_ORDER: u64 = 0;
pub const FIRST_EVENT_PAYLOAD_ORDER: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventKey {
    pub due_tick: Tick,
    pub kind_order: u8,
    pub target_id: u64,
    pub source_id: u64,
    pub revision: Revision,
    pub generation: u32,
    pub payload_order: u64,
}

impl EventKey {
    pub const fn unassigned(
        due_tick: Tick,
        kind_order: u8,
        target_id: u64,
        source_id: u64,
        revision: Revision,
        generation: u32,
    ) -> Self {
        Self {
            due_tick,
            kind_order,
            target_id,
            source_id,
            revision,
            generation,
            payload_order: RESERVED_EVENT_PAYLOAD_ORDER,
        }
    }

    pub const fn driver_transition(due_tick: Tick, driver: DriverId, generation: u32) -> Self {
        Self::unassigned(
            due_tick,
            DRIVER_TRANSITION_KIND_ORDER,
            driver.0.0,
            driver.0.0,
            Revision(0),
            generation,
        )
    }

    pub const fn signal_arrival(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        revision: Revision,
    ) -> Self {
        Self::unassigned(
            due_tick,
            SIGNAL_ARRIVAL_KIND_ORDER,
            sink.0.0,
            source_driver.0.0,
            revision,
            0,
        )
    }

    pub const fn with_payload_order(mut self, payload_order: u64) -> Self {
        self.payload_order = payload_order;
        self
    }

    fn candidate_cmp(&self, other: &Self) -> Ordering {
        self.due_tick
            .cmp(&other.due_tick)
            .then_with(|| self.kind_order.cmp(&other.kind_order))
            .then_with(|| self.target_id.cmp(&other.target_id))
            .then_with(|| self.source_id.cmp(&other.source_id))
            .then_with(|| self.revision.cmp(&other.revision))
            .then_with(|| self.generation.cmp(&other.generation))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverTransitionCause {
    ExternalDriver = 0,
    GateOutput = 1,
    GateStrengthResponse = 2,
}

impl DriverTransitionCause {
    pub const fn canonical_tag(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalArrivalKind {
    Propagation = 0,
    TopologySync = 1,
}

impl SignalArrivalKind {
    pub const fn canonical_tag(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverSample {
    pub level: LogicLevel,
    pub strength: DriveStrength,
    pub revision: Revision,
    pub emitted_at: Tick,
    pub driver_id: DriverId,
}

impl DriverSample {
    pub const fn s0m3(
        driver_id: DriverId,
        level: LogicLevel,
        strength: DriveStrength,
        emitted_at: Tick,
    ) -> Self {
        Self {
            level,
            strength,
            revision: Revision(0),
            emitted_at,
            driver_id,
        }
    }

    fn canonical_cmp(&self, other: &Self) -> Ordering {
        logic_level_tag(self.level)
            .cmp(&logic_level_tag(other.level))
            .then_with(|| self.strength.cmp(&other.strength))
            .then_with(|| self.revision.cmp(&other.revision))
            .then_with(|| self.emitted_at.cmp(&other.emitted_at))
            .then_with(|| self.driver_id.cmp(&other.driver_id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverTransition {
    pub key: EventKey,
    pub driver_id: DriverId,
    pub level: LogicLevel,
    pub strength: DriveStrength,
    pub pending_generation: u32,
    pub cause: DriverTransitionCause,
}

impl DriverTransition {
    pub const fn s0m3(
        due_tick: Tick,
        driver_id: DriverId,
        level: LogicLevel,
        strength: DriveStrength,
        pending_generation: u32,
        cause: DriverTransitionCause,
    ) -> Self {
        Self {
            key: EventKey::driver_transition(due_tick, driver_id, pending_generation),
            driver_id,
            level,
            strength,
            pending_generation,
            cause,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalArrival {
    pub key: EventKey,
    pub source_driver: DriverId,
    pub sink: SinkId,
    pub sample: DriverSample,
    pub path_certificate: Option<PathCertificateId>,
    pub kind: SignalArrivalKind,
}

impl SignalArrival {
    pub const fn propagation(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
        path_certificate: PathCertificateId,
    ) -> Self {
        Self::certified(
            due_tick,
            source_driver,
            sink,
            sample,
            path_certificate,
            SignalArrivalKind::Propagation,
        )
    }

    pub const fn topology_sync(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
        path_certificate: PathCertificateId,
    ) -> Self {
        Self::certified(
            due_tick,
            source_driver,
            sink,
            sample,
            path_certificate,
            SignalArrivalKind::TopologySync,
        )
    }

    const fn certified(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
        path_certificate: PathCertificateId,
        kind: SignalArrivalKind,
    ) -> Self {
        Self {
            key: EventKey::signal_arrival(due_tick, source_driver, sink, sample.revision),
            source_driver,
            sink,
            sample,
            path_certificate: Some(path_certificate),
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UncertifiedSignalArrival {
    pub(crate) due_tick: Tick,
    pub(crate) source_driver: DriverId,
    pub(crate) sink: SinkId,
    pub(crate) sample: DriverSample,
    pub(crate) kind: SignalArrivalKind,
    pub(crate) path_elements: Vec<PathElementStamp>,
}

impl UncertifiedSignalArrival {
    pub(crate) fn propagation(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
        path_elements: Vec<PathElementStamp>,
    ) -> Self {
        Self {
            due_tick,
            source_driver,
            sink,
            sample,
            kind: SignalArrivalKind::Propagation,
            path_elements,
        }
    }

    pub(crate) fn topology_sync(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
        path_elements: Vec<PathElementStamp>,
    ) -> Self {
        Self {
            due_tick,
            source_driver,
            sink,
            sample,
            kind: SignalArrivalKind::TopologySync,
            path_elements,
        }
    }

    fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.due_tick
            .cmp(&other.due_tick)
            .then_with(|| self.sink.cmp(&other.sink))
            .then_with(|| self.source_driver.cmp(&other.source_driver))
            .then_with(|| self.sample.revision.cmp(&other.sample.revision))
            .then_with(|| self.source_driver.cmp(&other.source_driver))
            .then_with(|| self.sink.cmp(&other.sink))
            .then_with(|| self.sample.canonical_cmp(&other.sample))
            .then_with(|| self.kind.canonical_tag().cmp(&other.kind.canonical_tag()))
            .then_with(|| compare_path_elements(&self.path_elements, &other.path_elements))
    }
}

pub trait CanonicalEvent: Clone + Eq {
    const KIND_ORDER: u8;

    fn event_key(&self) -> &EventKey;

    fn event_key_mut(&mut self) -> &mut EventKey;

    fn canonical_payload_cmp(&self, other: &Self) -> Ordering;
}

impl CanonicalEvent for DriverTransition {
    const KIND_ORDER: u8 = DRIVER_TRANSITION_KIND_ORDER;

    fn event_key(&self) -> &EventKey {
        &self.key
    }

    fn event_key_mut(&mut self) -> &mut EventKey {
        &mut self.key
    }

    fn canonical_payload_cmp(&self, other: &Self) -> Ordering {
        self.driver_id
            .cmp(&other.driver_id)
            .then_with(|| logic_level_tag(self.level).cmp(&logic_level_tag(other.level)))
            .then_with(|| self.strength.cmp(&other.strength))
            .then_with(|| self.pending_generation.cmp(&other.pending_generation))
            .then_with(|| self.cause.canonical_tag().cmp(&other.cause.canonical_tag()))
    }
}

impl CanonicalEvent for SignalArrival {
    const KIND_ORDER: u8 = SIGNAL_ARRIVAL_KIND_ORDER;

    fn event_key(&self) -> &EventKey {
        &self.key
    }

    fn event_key_mut(&mut self) -> &mut EventKey {
        &mut self.key
    }

    fn canonical_payload_cmp(&self, other: &Self) -> Ordering {
        self.source_driver
            .cmp(&other.source_driver)
            .then_with(|| self.sink.cmp(&other.sink))
            .then_with(|| self.sample.canonical_cmp(&other.sample))
            .then_with(|| self.path_certificate.cmp(&other.path_certificate))
            .then_with(|| self.kind.canonical_tag().cmp(&other.kind.canonical_tag()))
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum EventCalendarError {
    #[error("event payload order 0 is reserved")]
    ReservedPayloadOrder,

    #[error("staged event already has payload order {payload_order}")]
    AssignedStagedPayload { payload_order: u64 },

    #[error("event kind order mismatch: expected {expected}, got {actual}")]
    InvalidKindOrder { expected: u8, actual: u8 },

    #[error("event payload allocator exhausted")]
    PayloadOrderExhausted,

    #[error("duplicate canonical event key {key:?}")]
    DuplicateEventKey { key: EventKey },

    #[error("event due at {due_tick} was retained before current Tick {current_tick}")]
    OverdueEvent { current_tick: Tick, due_tick: Tick },
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum SignalArrivalStagingError {
    #[error("signal arrival source Driver ID 0 is reserved")]
    ReservedSourceDriver,

    #[error("signal arrival target Sink ID 0 is reserved")]
    ReservedSink,

    #[error("signal arrival sample Driver does not match its source Driver")]
    SampleDriverMismatch,

    #[error("signal arrival path contains reserved structural Entity ID 0")]
    ReservedPathElement,

    #[error(transparent)]
    PathCertificate(#[from] PathCertificateError),

    #[error(transparent)]
    EventCalendar(#[from] EventCalendarError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPayloadAllocator {
    next_payload_order: u64,
}

impl Default for EventPayloadAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl EventPayloadAllocator {
    pub const fn new() -> Self {
        Self {
            next_payload_order: FIRST_EVENT_PAYLOAD_ORDER,
        }
    }

    pub const fn from_next_payload_order(
        next_payload_order: u64,
    ) -> Result<Self, EventCalendarError> {
        if next_payload_order == RESERVED_EVENT_PAYLOAD_ORDER {
            return Err(EventCalendarError::ReservedPayloadOrder);
        }
        Ok(Self { next_payload_order })
    }

    pub const fn next_payload_order(&self) -> u64 {
        self.next_payload_order
    }

    pub const fn allocated_count(&self) -> u64 {
        self.next_payload_order - FIRST_EVENT_PAYLOAD_ORDER
    }

    fn frontier_after(&self, count: usize) -> Result<u64, EventCalendarError> {
        let count = u64::try_from(count).map_err(|_| EventCalendarError::PayloadOrderExhausted)?;
        self.next_payload_order
            .checked_add(count)
            .ok_or(EventCalendarError::PayloadOrderExhausted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventCalendar<T> {
    events: BTreeMap<EventKey, T>,
}

impl<T> Default for EventCalendar<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EventCalendar<T> {
    pub const fn new() -> Self {
        Self {
            events: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn canonical_view(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator + '_ {
        self.events.values()
    }

    pub fn canonical_keys(
        &self,
    ) -> impl DoubleEndedIterator<Item = &EventKey> + ExactSizeIterator + '_ {
        self.events.keys()
    }

    pub(crate) fn canonical_entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&EventKey, &T)> + ExactSizeIterator + '_ {
        self.events.iter()
    }

    #[cfg(test)]
    pub(crate) fn move_map_key_for_test(&mut self, from: EventKey, to: EventKey) {
        assert!(!self.events.contains_key(&to));
        let event = self
            .events
            .remove(&from)
            .expect("test source EventKey must exist");
        assert!(self.events.insert(to, event).is_none());
    }
}

impl<T: CanonicalEvent> EventCalendar<T> {
    pub fn stage(
        &mut self,
        allocator: &mut EventPayloadAllocator,
        candidates: impl IntoIterator<Item = T>,
    ) -> Result<usize, EventCalendarError> {
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        for candidate in &candidates {
            validate_unassigned_candidate(candidate)?;
        }

        candidates.sort_by(canonical_candidate_cmp);
        candidates.dedup_by(|left, right| canonical_candidate_cmp(left, right).is_eq());

        let next_frontier = allocator.frontier_after(candidates.len())?;
        let mut payload_order = allocator.next_payload_order;
        for candidate in &mut candidates {
            candidate.event_key_mut().payload_order = payload_order;
            payload_order = payload_order
                .checked_add(1)
                .ok_or(EventCalendarError::PayloadOrderExhausted)?;
        }

        for candidate in &candidates {
            let key = *candidate.event_key();
            if self.events.contains_key(&key) {
                return Err(EventCalendarError::DuplicateEventKey { key });
            }
        }

        let inserted = candidates.len();
        for candidate in candidates {
            self.events.insert(*candidate.event_key(), candidate);
        }
        allocator.next_payload_order = next_frontier;
        Ok(inserted)
    }

    pub fn insert_assigned(&mut self, event: T) -> Result<(), EventCalendarError> {
        validate_assigned_event(&event)?;
        let key = *event.event_key();
        match self.events.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(event);
                Ok(())
            }
            Entry::Occupied(_) => Err(EventCalendarError::DuplicateEventKey { key }),
        }
    }

    pub fn drain_due(&mut self, tick: Tick) -> Result<Vec<T>, EventCalendarError> {
        if let Some(first) = self.events.keys().next()
            && first.due_tick < tick
        {
            return Err(EventCalendarError::OverdueEvent {
                current_tick: tick,
                due_tick: first.due_tick,
            });
        }

        let due_keys: Vec<_> = self
            .events
            .keys()
            .take_while(|key| key.due_tick == tick)
            .copied()
            .collect();
        Ok(due_keys
            .into_iter()
            .filter_map(|key| self.events.remove(&key))
            .collect())
    }
}

pub(crate) fn stage_signal_arrivals(
    calendar: &mut EventCalendar<SignalArrival>,
    payloads: &mut EventPayloadAllocator,
    certificates: &mut PathCertificateArena,
    candidates: impl IntoIterator<Item = UncertifiedSignalArrival>,
) -> Result<usize, SignalArrivalStagingError> {
    let candidates: Vec<_> = candidates.into_iter().collect();
    let mut working_calendar = calendar.clone();
    let mut working_payloads = payloads.clone();
    let mut working_certificates = certificates.clone();
    let inserted = stage_signal_arrivals_inner(
        &mut working_calendar,
        &mut working_payloads,
        &mut working_certificates,
        candidates,
    )?;
    *calendar = working_calendar;
    *payloads = working_payloads;
    *certificates = working_certificates;
    Ok(inserted)
}

fn stage_signal_arrivals_inner(
    calendar: &mut EventCalendar<SignalArrival>,
    payloads: &mut EventPayloadAllocator,
    certificates: &mut PathCertificateArena,
    mut candidates: Vec<UncertifiedSignalArrival>,
) -> Result<usize, SignalArrivalStagingError> {
    for candidate in &candidates {
        validate_uncertified_signal_arrival(candidate)?;
    }
    candidates.sort_by(UncertifiedSignalArrival::canonical_cmp);
    candidates.dedup_by(|left, right| left.canonical_cmp(right).is_eq());

    let paths: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.path_elements.as_slice())
        .collect();
    let certificate_plan = certificates.preflight_batch(&paths)?;
    let certificate_ids = certificate_plan.ids().to_vec();
    let next_payload_order = payloads.frontier_after(candidates.len())?;

    let mut assigned_payload_order = payloads.next_payload_order;
    let mut assigned = Vec::with_capacity(candidates.len());
    for (candidate, certificate_id) in candidates.iter().zip(certificate_ids) {
        let mut arrival = match candidate.kind {
            SignalArrivalKind::Propagation => SignalArrival::propagation(
                candidate.due_tick,
                candidate.source_driver,
                candidate.sink,
                candidate.sample,
                certificate_id,
            ),
            SignalArrivalKind::TopologySync => SignalArrival::topology_sync(
                candidate.due_tick,
                candidate.source_driver,
                candidate.sink,
                candidate.sample,
                certificate_id,
            ),
        };
        arrival.key.payload_order = assigned_payload_order;
        assigned_payload_order = assigned_payload_order
            .checked_add(1)
            .ok_or(EventCalendarError::PayloadOrderExhausted)?;
        validate_assigned_event(&arrival)?;
        if calendar.events.contains_key(&arrival.key) {
            return Err(EventCalendarError::DuplicateEventKey { key: arrival.key }.into());
        }
        assigned.push(arrival);
    }

    certificates.allocate_preflighted(&paths, certificate_plan)?;
    for arrival in assigned {
        let key = arrival.key;
        if calendar.events.insert(key, arrival).is_some() {
            return Err(EventCalendarError::DuplicateEventKey { key }.into());
        }
    }
    payloads.next_payload_order = next_payload_order;
    Ok(paths.len())
}

fn validate_uncertified_signal_arrival(
    candidate: &UncertifiedSignalArrival,
) -> Result<(), SignalArrivalStagingError> {
    if candidate.source_driver.entity_id().0 == 0 {
        return Err(SignalArrivalStagingError::ReservedSourceDriver);
    }
    if candidate.sink.entity_id().0 == 0 {
        return Err(SignalArrivalStagingError::ReservedSink);
    }
    if candidate.sample.driver_id != candidate.source_driver {
        return Err(SignalArrivalStagingError::SampleDriverMismatch);
    }
    if candidate
        .path_elements
        .iter()
        .any(|element| element.entity_id().0 == 0)
    {
        return Err(SignalArrivalStagingError::ReservedPathElement);
    }
    Ok(())
}

fn validate_unassigned_candidate<T: CanonicalEvent>(event: &T) -> Result<(), EventCalendarError> {
    validate_kind_order(event)?;
    let payload_order = event.event_key().payload_order;
    if payload_order != RESERVED_EVENT_PAYLOAD_ORDER {
        return Err(EventCalendarError::AssignedStagedPayload { payload_order });
    }
    Ok(())
}

fn validate_assigned_event<T: CanonicalEvent>(event: &T) -> Result<(), EventCalendarError> {
    validate_kind_order(event)?;
    if event.event_key().payload_order == RESERVED_EVENT_PAYLOAD_ORDER {
        return Err(EventCalendarError::ReservedPayloadOrder);
    }
    Ok(())
}

fn validate_kind_order<T: CanonicalEvent>(event: &T) -> Result<(), EventCalendarError> {
    let actual = event.event_key().kind_order;
    if actual != T::KIND_ORDER {
        return Err(EventCalendarError::InvalidKindOrder {
            expected: T::KIND_ORDER,
            actual,
        });
    }
    Ok(())
}

fn canonical_candidate_cmp<T: CanonicalEvent>(left: &T, right: &T) -> Ordering {
    left.event_key()
        .candidate_cmp(right.event_key())
        .then_with(|| left.canonical_payload_cmp(right))
}

fn compare_path_elements(left: &[PathElementStamp], right: &[PathElementStamp]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        let ordering = left
            .canonical_tag()
            .cmp(&right.canonical_tag())
            .then_with(|| left.entity_id().cmp(&right.entity_id()))
            .then_with(|| left.generation().cmp(&right.generation()));
        if !ordering.is_eq() {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

const fn logic_level_tag(level: LogicLevel) -> u8 {
    match level {
        LogicLevel::Low => 0,
        LogicLevel::High => 1,
        LogicLevel::X => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_certificate::{FIRST_PATH_CERTIFICATE_ID, PathCertificateId, PathElementStamp};
    use crate::{ConnectionGeneration, EntityId, JunctionId, WireId};

    const fn driver(id: u64) -> DriverId {
        DriverId(EntityId(id))
    }

    const fn sink(id: u64) -> SinkId {
        SinkId(EntityId(id))
    }

    const fn sample(
        driver_id: DriverId,
        level: LogicLevel,
        strength: u64,
        revision: u64,
        emitted_at: u64,
    ) -> DriverSample {
        DriverSample {
            level,
            strength: DriveStrength(strength),
            revision: Revision(revision),
            emitted_at: Tick(emitted_at),
            driver_id,
        }
    }

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

    fn propagation(
        due_tick: u64,
        source: u64,
        target: u64,
        revision: u64,
        path_elements: Vec<PathElementStamp>,
    ) -> UncertifiedSignalArrival {
        let source = driver(source);
        UncertifiedSignalArrival::propagation(
            Tick(due_tick),
            source,
            sink(target),
            sample(source, LogicLevel::High, 17, revision, 3),
            path_elements,
        )
    }

    #[test]
    fn certified_constructors_encode_sample_revision_and_live_certificate() {
        let source = driver(7);
        let target = sink(11);
        let sample = sample(source, LogicLevel::X, 13, 5, 3);
        let certificate = PathCertificateId(19);

        let propagation = SignalArrival::propagation(Tick(23), source, target, sample, certificate);
        let sync = SignalArrival::topology_sync(Tick(23), source, target, sample, certificate);

        for arrival in [propagation, sync] {
            assert_eq!(arrival.key.revision, Revision(5));
            assert_eq!(arrival.key.generation, 0);
            assert_eq!(arrival.key.target_id, 11);
            assert_eq!(arrival.key.source_id, 7);
            assert_eq!(arrival.key.payload_order, RESERVED_EVENT_PAYLOAD_ORDER);
            assert_eq!(arrival.path_certificate, Some(certificate));
        }
        assert_eq!(propagation.kind, SignalArrivalKind::Propagation);
        assert_eq!(sync.kind, SignalArrivalKind::TopologySync);
    }

    #[test]
    fn certified_staging_sorts_semantics_deduplicates_and_allocates_both_namespaces() {
        let candidates = [
            propagation(9, 4, 8, 2, vec![wire(31, 1)]),
            propagation(7, 5, 9, 3, vec![wire(41, 2)]),
            propagation(9, 4, 8, 2, vec![wire(31, 1)]),
            propagation(9, 4, 8, 2, vec![wire(31, 1), junction(37, 3), wire(43, 5)]),
        ];
        let mut calendar = EventCalendar::new();
        let mut payloads = EventPayloadAllocator::new();
        let mut certificates = PathCertificateArena::new();

        let inserted =
            stage_signal_arrivals(&mut calendar, &mut payloads, &mut certificates, candidates)
                .expect("canonical SignalArrival batch stages");

        assert_eq!(inserted, 3);
        assert_eq!(calendar.len(), 3);
        assert_eq!(payloads.next_payload_order(), 4);
        assert_eq!(certificates.frontier(), PathCertificateId(4));
        assert_eq!(certificates.allocated_count(), 3);

        let events: Vec<_> = calendar.canonical_view().copied().collect();
        assert_eq!(events[0].key.due_tick, Tick(7));
        assert_eq!(events[0].path_certificate, Some(PathCertificateId(1)));
        assert_eq!(events[0].key.payload_order, 1);
        assert_eq!(events[1].path_certificate, Some(PathCertificateId(2)));
        assert_eq!(events[1].key.payload_order, 2);
        assert_eq!(events[2].path_certificate, Some(PathCertificateId(3)));
        assert_eq!(events[2].key.payload_order, 3);
        assert_eq!(
            certificates.elements(PathCertificateId(2)),
            Ok([wire(31, 1)].as_slice())
        );
        assert_eq!(
            certificates.elements(PathCertificateId(3)),
            Ok([wire(31, 1), junction(37, 3), wire(43, 5)].as_slice())
        );
    }

    #[test]
    fn candidate_permutations_produce_identical_calendars_and_arenas() {
        let first = propagation(13, 7, 19, 11, vec![wire(23, 2)]);
        let second = UncertifiedSignalArrival::topology_sync(
            Tick(13),
            driver(7),
            sink(17),
            sample(driver(7), LogicLevel::Low, 29, 11, 5),
            vec![],
        );

        let mut left_calendar = EventCalendar::new();
        let mut left_payloads = EventPayloadAllocator::new();
        let mut left_certificates = PathCertificateArena::new();
        stage_signal_arrivals(
            &mut left_calendar,
            &mut left_payloads,
            &mut left_certificates,
            [second.clone(), first.clone()],
        )
        .expect("left batch stages");

        let mut right_calendar = EventCalendar::new();
        let mut right_payloads = EventPayloadAllocator::new();
        let mut right_certificates = PathCertificateArena::new();
        stage_signal_arrivals(
            &mut right_calendar,
            &mut right_payloads,
            &mut right_certificates,
            [first, second],
        )
        .expect("right batch stages");

        assert_eq!(left_calendar, right_calendar);
        assert_eq!(left_payloads, right_payloads);
        assert_eq!(left_certificates, right_certificates);
    }

    #[test]
    fn certificate_and_payload_namespaces_are_independent() {
        let mut calendar = EventCalendar::new();
        let mut payloads = EventPayloadAllocator::from_next_payload_order(37)
            .expect("nonzero payload frontier is valid");
        let mut certificates = PathCertificateArena::new();

        stage_signal_arrivals(
            &mut calendar,
            &mut payloads,
            &mut certificates,
            [propagation(5, 2, 3, 1, vec![])],
        )
        .expect("arrival stages");
        let event = calendar.canonical_view().next().expect("one event");

        assert_eq!(event.path_certificate, Some(FIRST_PATH_CERTIFICATE_ID));
        assert_eq!(event.key.payload_order, 37);
        assert_ne!(event.key.payload_order, FIRST_PATH_CERTIFICATE_ID.0);
    }

    #[test]
    fn validation_and_frontier_failures_are_atomic_across_all_three_stores() {
        let mut calendar = EventCalendar::new();
        let mut payloads = EventPayloadAllocator::new();
        let mut certificates = PathCertificateArena::new();
        let invalid = UncertifiedSignalArrival::propagation(
            Tick(7),
            driver(2),
            sink(3),
            sample(driver(4), LogicLevel::High, 5, 1, 0),
            vec![],
        );
        let baseline = (calendar.clone(), payloads.clone(), certificates.clone());

        assert_eq!(
            stage_signal_arrivals(&mut calendar, &mut payloads, &mut certificates, [invalid],),
            Err(SignalArrivalStagingError::SampleDriverMismatch)
        );
        assert_eq!(
            (calendar.clone(), payloads.clone(), certificates.clone()),
            baseline
        );

        certificates.set_frontier_limits_for_test(1, u32::MAX);
        let baseline = (calendar.clone(), payloads.clone(), certificates.clone());
        assert_eq!(
            stage_signal_arrivals(
                &mut calendar,
                &mut payloads,
                &mut certificates,
                [propagation(7, 2, 3, 1, vec![])],
            ),
            Err(SignalArrivalStagingError::PathCertificate(
                PathCertificateError::CertificateIdExhausted
            ))
        );
        assert_eq!((calendar, payloads, certificates), baseline);
    }

    #[test]
    fn payload_exhaustion_does_not_allocate_a_certificate() {
        let mut calendar = EventCalendar::new();
        let mut payloads = EventPayloadAllocator::from_next_payload_order(u64::MAX)
            .expect("maximum nonzero frontier is representable");
        let mut certificates = PathCertificateArena::new();
        let baseline = (calendar.clone(), payloads.clone(), certificates.clone());

        assert_eq!(
            stage_signal_arrivals(
                &mut calendar,
                &mut payloads,
                &mut certificates,
                [propagation(7, 2, 3, 1, vec![])],
            ),
            Err(SignalArrivalStagingError::EventCalendar(
                EventCalendarError::PayloadOrderExhausted
            ))
        );
        assert_eq!((calendar, payloads, certificates), baseline);
    }
}
