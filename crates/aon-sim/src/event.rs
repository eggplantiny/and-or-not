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
        generation: u32,
    ) -> Self {
        Self::unassigned(
            due_tick,
            SIGNAL_ARRIVAL_KIND_ORDER,
            sink.0.0,
            source_driver.0.0,
            Revision(0),
            generation,
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

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathCertificateId(pub u64);

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
    pub const fn s0m3_propagation(
        due_tick: Tick,
        source_driver: DriverId,
        sink: SinkId,
        sample: DriverSample,
    ) -> Self {
        Self {
            key: EventKey::signal_arrival(due_tick, source_driver, sink, 0),
            source_driver,
            sink,
            sample,
            path_certificate: None,
            kind: SignalArrivalKind::Propagation,
        }
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

const fn logic_level_tag(level: LogicLevel) -> u8 {
    match level {
        LogicLevel::Low => 0,
        LogicLevel::High => 1,
        LogicLevel::X => 2,
    }
}
