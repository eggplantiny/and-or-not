use crate::event::DriverSample;
use crate::{
    DriveStrength, DriverId, Energy, EntityId, GateId, GateType, HeatEnergy, LogicLevel,
    NumericError, Revision, SinkId, Tick, WireId,
};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

const RESERVED_ENDPOINT_ID: u64 = 0;
const FIRST_ENDPOINT_ID: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DriverRole {
    ExternalInputA,
    ExternalInputB,
    GateOutput,
}

impl DriverRole {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::ExternalInputA => 0,
            Self::ExternalInputB => 1,
            Self::GateOutput => 2,
        }
    }

    pub const fn is_external(self) -> bool {
        matches!(self, Self::ExternalInputA | Self::ExternalInputB)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SinkRole {
    InputA,
    InputB,
}

impl SinkRole {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::InputA => 0,
            Self::InputB => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateInputSignalPort {
    pub sink: SinkId,
    pub external_driver: DriverId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSignalPorts {
    pub input_a: GateInputSignalPort,
    pub input_b: Option<GateInputSignalPort>,
    pub output: DriverId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriveVector {
    pub high: u128,
    pub low: u128,
    pub unknown: u128,
}

impl DriveVector {
    pub(crate) fn checked_add_sample(&mut self, sample: DriverSample) -> Result<(), SignalError> {
        let target = match sample.level {
            LogicLevel::Low => &mut self.low,
            LogicLevel::High => &mut self.high,
            LogicLevel::X => &mut self.unknown,
        };
        *target = target
            .checked_add(u128::from(sample.strength.0))
            .ok_or(SignalError::NumericOverflow)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSignalSnapshot {
    pub active: DriveVector,
    pub previous: DriveVector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSignalSnapshot {
    pub ports: GateSignalPorts,
    pub current_output: LogicLevel,
    pub desired_output: LogicLevel,
    pub pending_generation: u32,
    pub pending_due_tick: Option<Tick>,
    pub pending_level: Option<LogicLevel>,
    pub pending_switch_energy: Option<Energy>,
    pub cancelled_switching_heat: HeatEnergy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverChangeRecord {
    pub driver: DriverId,
    pub previous: DriverSample,
    pub current: DriverSample,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalChangeRecord {
    pub sink: SinkId,
    pub previous: LogicLevel,
    pub current: LogicLevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignalStepCounters {
    pub routes_added: u64,
    pub routes_removed: u64,
    pub routes_retained: u64,
    pub routes_replaced: u64,
    pub driver_transitions_applied: u64,
    pub stale_driver_transitions: u64,
    pub signal_arrivals_applied: u64,
    pub topology_sync_arrivals_staged: u64,
    pub stale_revision_arrivals: u64,
    pub invalid_path_arrivals: u64,
    pub idempotent_signal_arrivals: u64,
    pub sinks_resolved: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotApplyOutcome {
    Applied,
    Idempotent,
    Stale,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum SignalError {
    #[error("canonical signal numeric capacity exhausted")]
    NumericOverflow,

    #[error("canonical signal state invariant violated")]
    InvalidCanonicalState,

    #[error("canonical Driver Revision invariant violated")]
    DriverRevisionInvariantViolation,
}

impl From<NumericError> for SignalError {
    fn from(_: NumericError) -> Self {
        Self::NumericOverflow
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DriverRecord {
    pub id: DriverId,
    pub owner: GateId,
    pub role: DriverRole,
    pub sample: DriverSample,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SinkRecord {
    pub id: SinkId,
    pub owner: GateId,
    pub role: SinkRole,
    pub resolved_level: LogicLevel,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SinkDriverSlot {
    pub driver: DriverId,
    pub sink: SinkId,
    pub level: LogicLevel,
    pub strength: DriveStrength,
    pub revision: Revision,
    pub emitted_at: Tick,
}

impl SinkDriverSlot {
    pub const fn from_sample(sink: SinkId, sample: DriverSample) -> Self {
        Self {
            driver: sample.driver_id,
            sink,
            level: sample.level,
            strength: sample.strength,
            revision: sample.revision,
            emitted_at: sample.emitted_at,
        }
    }

    pub const fn sample(self) -> DriverSample {
        DriverSample {
            level: self.level,
            strength: self.strength,
            revision: self.revision,
            emitted_at: self.emitted_at,
            driver_id: self.driver,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DriverStore {
    next_id: u64,
    slots: Vec<Option<DriverRecord>>,
}

impl Default for DriverStore {
    fn default() -> Self {
        Self {
            next_id: FIRST_ENDPOINT_ID,
            slots: vec![None],
        }
    }
}

impl DriverStore {
    fn allocate(
        &mut self,
        owner: GateId,
        role: DriverRole,
        tick: Tick,
    ) -> Result<DriverId, SignalError> {
        let raw = self.next_id;
        let next = raw.checked_add(1).ok_or(SignalError::NumericOverflow)?;
        let index = usize::try_from(raw).map_err(|_| SignalError::NumericOverflow)?;
        if index != self.slots.len() || raw == RESERVED_ENDPOINT_ID {
            return Err(SignalError::InvalidCanonicalState);
        }
        let id = DriverId(EntityId(raw));
        self.slots.push(Some(DriverRecord {
            id,
            owner,
            role,
            sample: DriverSample {
                level: LogicLevel::Low,
                strength: DriveStrength(0),
                revision: Revision(0),
                emitted_at: tick,
                driver_id: id,
            },
        }));
        self.next_id = next;
        Ok(id)
    }

    fn record(&self, id: DriverId) -> Option<&DriverRecord> {
        let index = usize::try_from(id.entity_id().0).ok()?;
        self.slots.get(index)?.as_ref()
    }

    fn record_mut(&mut self, id: DriverId) -> Option<&mut DriverRecord> {
        let index = usize::try_from(id.entity_id().0).ok()?;
        self.slots.get_mut(index)?.as_mut()
    }

    fn remove(&mut self, id: DriverId) -> Result<DriverRecord, SignalError> {
        let index =
            usize::try_from(id.entity_id().0).map_err(|_| SignalError::InvalidCanonicalState)?;
        self.slots
            .get_mut(index)
            .ok_or(SignalError::InvalidCanonicalState)?
            .take()
            .ok_or(SignalError::InvalidCanonicalState)
    }

    fn next_id(&self) -> DriverId {
        DriverId(EntityId(self.next_id))
    }

    fn allocated_count(&self) -> u64 {
        self.next_id - FIRST_ENDPOINT_ID
    }

    fn canonical_slots(&self) -> impl Iterator<Item = (DriverId, Option<DriverRecord>)> + '_ {
        (FIRST_ENDPOINT_ID..)
            .zip(self.slots.iter().skip(1))
            .map(|(raw, record)| (DriverId(EntityId(raw)), *record))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SinkStore {
    next_id: u64,
    slots: Vec<Option<SinkRecord>>,
}

impl Default for SinkStore {
    fn default() -> Self {
        Self {
            next_id: FIRST_ENDPOINT_ID,
            slots: vec![None],
        }
    }
}

impl SinkStore {
    fn allocate(&mut self, owner: GateId, role: SinkRole) -> Result<SinkId, SignalError> {
        let raw = self.next_id;
        let next = raw.checked_add(1).ok_or(SignalError::NumericOverflow)?;
        let index = usize::try_from(raw).map_err(|_| SignalError::NumericOverflow)?;
        if index != self.slots.len() || raw == RESERVED_ENDPOINT_ID {
            return Err(SignalError::InvalidCanonicalState);
        }
        let id = SinkId(EntityId(raw));
        self.slots.push(Some(SinkRecord {
            id,
            owner,
            role,
            resolved_level: LogicLevel::Low,
            dirty: false,
        }));
        self.next_id = next;
        Ok(id)
    }

    fn record(&self, id: SinkId) -> Option<&SinkRecord> {
        let index = usize::try_from(id.entity_id().0).ok()?;
        self.slots.get(index)?.as_ref()
    }

    fn record_mut(&mut self, id: SinkId) -> Option<&mut SinkRecord> {
        let index = usize::try_from(id.entity_id().0).ok()?;
        self.slots.get_mut(index)?.as_mut()
    }

    fn remove(&mut self, id: SinkId) -> Result<SinkRecord, SignalError> {
        let index =
            usize::try_from(id.entity_id().0).map_err(|_| SignalError::InvalidCanonicalState)?;
        self.slots
            .get_mut(index)
            .ok_or(SignalError::InvalidCanonicalState)?
            .take()
            .ok_or(SignalError::InvalidCanonicalState)
    }

    fn next_id(&self) -> SinkId {
        SinkId(EntityId(self.next_id))
    }

    fn allocated_count(&self) -> u64 {
        self.next_id - FIRST_ENDPOINT_ID
    }

    fn canonical_slots(&self) -> impl Iterator<Item = (SinkId, Option<SinkRecord>)> + '_ {
        (FIRST_ENDPOINT_ID..)
            .zip(self.slots.iter().skip(1))
            .map(|(raw, record)| (SinkId(EntityId(raw)), *record))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GateSignalRecord {
    pub gate: GateId,
    pub gate_type: GateType,
    pub ports: GateSignalPorts,
    pub current_output: LogicLevel,
    pub desired_output: LogicLevel,
    pub pending_generation: u32,
    pub pending_due_tick: Option<Tick>,
    pub pending_level: Option<LogicLevel>,
    pub pending_switch_energy: Option<Energy>,
    pub cancelled_switching_heat: HeatEnergy,
}

impl GateSignalRecord {
    fn snapshot(self) -> GateSignalSnapshot {
        GateSignalSnapshot {
            ports: self.ports,
            current_output: self.current_output,
            desired_output: self.desired_output,
            pending_generation: self.pending_generation,
            pending_due_tick: self.pending_due_tick,
            pending_level: self.pending_level,
            pending_switch_energy: self.pending_switch_energy,
            cancelled_switching_heat: self.cancelled_switching_heat,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SignalWorld {
    drivers: DriverStore,
    sinks: SinkStore,
    gates: BTreeMap<GateId, GateSignalRecord>,
    wires: BTreeMap<WireId, WireSignalSnapshot>,
    slots: BTreeMap<(SinkId, DriverId), SinkDriverSlot>,
}

impl SignalWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn activate_gate(
        &mut self,
        gate: GateId,
        gate_type: GateType,
        tick: Tick,
    ) -> Result<GateSignalPorts, SignalError> {
        if self.gates.contains_key(&gate) {
            return Err(SignalError::InvalidCanonicalState);
        }

        let external_a = self
            .drivers
            .allocate(gate, DriverRole::ExternalInputA, tick)?;
        let external_b = if matches!(gate_type, GateType::And | GateType::Or) {
            Some(
                self.drivers
                    .allocate(gate, DriverRole::ExternalInputB, tick)?,
            )
        } else {
            None
        };
        let output = self.drivers.allocate(gate, DriverRole::GateOutput, tick)?;
        let sink_a = self.sinks.allocate(gate, SinkRole::InputA)?;
        let sink_b = if matches!(gate_type, GateType::And | GateType::Or) {
            Some(self.sinks.allocate(gate, SinkRole::InputB)?)
        } else {
            None
        };
        let ports = GateSignalPorts {
            input_a: GateInputSignalPort {
                sink: sink_a,
                external_driver: external_a,
            },
            input_b: match (sink_b, external_b) {
                (Some(sink), Some(external_driver)) => Some(GateInputSignalPort {
                    sink,
                    external_driver,
                }),
                (None, None) => None,
                _ => return Err(SignalError::InvalidCanonicalState),
            },
            output,
        };
        let previous = self.gates.insert(
            gate,
            GateSignalRecord {
                gate,
                gate_type,
                ports,
                current_output: LogicLevel::Low,
                desired_output: LogicLevel::Low,
                pending_generation: 0,
                pending_due_tick: None,
                pending_level: None,
                pending_switch_energy: None,
                cancelled_switching_heat: HeatEnergy(0),
            },
        );
        if previous.is_some() {
            return Err(SignalError::InvalidCanonicalState);
        }
        Ok(ports)
    }

    pub fn remove_gate(&mut self, gate: GateId) -> Result<(), SignalError> {
        let record = self
            .gates
            .remove(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        let mut drivers = vec![record.ports.input_a.external_driver, record.ports.output];
        let mut sinks = vec![record.ports.input_a.sink];
        if let Some(input_b) = record.ports.input_b {
            drivers.push(input_b.external_driver);
            sinks.push(input_b.sink);
        }
        for driver in &drivers {
            self.drivers.remove(*driver)?;
        }
        for sink in &sinks {
            self.sinks.remove(*sink)?;
        }

        let removed_drivers: BTreeSet<_> = drivers.into_iter().collect();
        let removed_sinks: BTreeSet<_> = sinks.into_iter().collect();
        let mut dirtied = BTreeSet::new();
        self.slots.retain(|(sink, driver), _| {
            let remove = removed_sinks.contains(sink) || removed_drivers.contains(driver);
            if remove && !removed_sinks.contains(sink) {
                dirtied.insert(*sink);
            }
            !remove
        });
        for sink in dirtied {
            let record = self
                .sinks
                .record_mut(sink)
                .ok_or(SignalError::InvalidCanonicalState)?;
            record.dirty = true;
        }
        Ok(())
    }

    pub fn activate_wire(&mut self, wire: WireId) -> Result<(), SignalError> {
        if self
            .wires
            .insert(wire, WireSignalSnapshot::default())
            .is_some()
        {
            return Err(SignalError::InvalidCanonicalState);
        }
        Ok(())
    }

    pub fn remove_wire(&mut self, wire: WireId) -> Result<(), SignalError> {
        self.wires
            .remove(&wire)
            .map(|_| ())
            .ok_or(SignalError::InvalidCanonicalState)
    }

    pub fn driver_frontier(&self) -> DriverId {
        self.drivers.next_id()
    }

    pub fn sink_frontier(&self) -> SinkId {
        self.sinks.next_id()
    }

    pub fn allocated_driver_count(&self) -> u64 {
        self.drivers.allocated_count()
    }

    pub fn allocated_sink_count(&self) -> u64 {
        self.sinks.allocated_count()
    }

    pub fn driver_record(&self, driver: DriverId) -> Option<&DriverRecord> {
        self.drivers.record(driver)
    }

    pub fn driver_sample(&self, driver: DriverId) -> Option<DriverSample> {
        self.drivers.record(driver).map(|record| record.sample)
    }

    pub fn sink_record(&self, sink: SinkId) -> Option<&SinkRecord> {
        self.sinks.record(sink)
    }

    pub fn sink_level(&self, sink: SinkId) -> Option<LogicLevel> {
        self.sinks.record(sink).map(|record| record.resolved_level)
    }

    pub fn sink_driver_sample(&self, sink: SinkId, driver: DriverId) -> Option<DriverSample> {
        self.slots
            .get(&(sink, driver))
            .copied()
            .map(SinkDriverSlot::sample)
    }

    pub fn gate_ports(&self, gate: GateId) -> Option<GateSignalPorts> {
        self.gates.get(&gate).map(|record| record.ports)
    }

    pub fn gate_snapshot(&self, gate: GateId) -> Option<GateSignalSnapshot> {
        self.gates
            .get(&gate)
            .copied()
            .map(GateSignalRecord::snapshot)
    }

    pub fn wire_snapshot(&self, wire: WireId) -> Option<WireSignalSnapshot> {
        self.wires.get(&wire).copied()
    }

    pub fn gate_record(&self, gate: GateId) -> Option<&GateSignalRecord> {
        self.gates.get(&gate)
    }

    pub fn external_driver_status(
        &self,
        driver: DriverId,
        batch_frontier: DriverId,
    ) -> ExternalDriverStatus {
        let raw = driver.entity_id().0;
        if raw == RESERVED_ENDPOINT_ID || raw >= batch_frontier.entity_id().0 {
            return ExternalDriverStatus::Unknown;
        }
        match self.drivers.record(driver) {
            None => ExternalDriverStatus::Removed,
            Some(record) if record.role.is_external() => ExternalDriverStatus::External,
            Some(_) => ExternalDriverStatus::WrongKind,
        }
    }

    pub fn apply_driver_sample(
        &mut self,
        driver: DriverId,
        level: LogicLevel,
        strength: DriveStrength,
        emitted_at: Tick,
    ) -> Result<Option<DriverChangeRecord>, SignalError> {
        let record = self
            .drivers
            .record_mut(driver)
            .ok_or(SignalError::InvalidCanonicalState)?;
        let previous = record.sample;
        if previous.level == level && previous.strength == strength {
            return Ok(None);
        }
        let revision = previous.revision.checked_add(Revision(1))?;
        let current = DriverSample {
            level,
            strength,
            revision,
            emitted_at,
            driver_id: driver,
        };
        record.sample = current;
        if record.role == DriverRole::GateOutput {
            let gate = self
                .gates
                .get_mut(&record.owner)
                .ok_or(SignalError::InvalidCanonicalState)?;
            gate.current_output = level;
        }
        Ok(Some(DriverChangeRecord {
            driver,
            previous,
            current,
        }))
    }

    pub fn apply_slot_sample(
        &mut self,
        sink: SinkId,
        sample: DriverSample,
    ) -> Result<SlotApplyOutcome, SignalError> {
        if self.sinks.record(sink).is_none() || self.drivers.record(sample.driver_id).is_none() {
            return Err(SignalError::InvalidCanonicalState);
        }
        let key = (sink, sample.driver_id);
        let incoming = SinkDriverSlot::from_sample(sink, sample);
        if let Some(existing) = self.slots.get(&key).copied() {
            return match incoming.revision.cmp(&existing.revision) {
                std::cmp::Ordering::Less => Ok(SlotApplyOutcome::Stale),
                std::cmp::Ordering::Equal if incoming == existing => {
                    Ok(SlotApplyOutcome::Idempotent)
                }
                std::cmp::Ordering::Equal => Err(SignalError::DriverRevisionInvariantViolation),
                std::cmp::Ordering::Greater => {
                    self.slots.insert(key, incoming);
                    self.sinks
                        .record_mut(sink)
                        .ok_or(SignalError::InvalidCanonicalState)?
                        .dirty = true;
                    Ok(SlotApplyOutcome::Applied)
                }
            };
        }
        self.slots.insert(key, incoming);
        self.sinks
            .record_mut(sink)
            .ok_or(SignalError::InvalidCanonicalState)?
            .dirty = true;
        Ok(SlotApplyOutcome::Applied)
    }

    pub fn remove_route_slot(
        &mut self,
        sink: SinkId,
        driver: DriverId,
    ) -> Result<bool, SignalError> {
        if self.sinks.record(sink).is_none() {
            if self.slots.contains_key(&(sink, driver)) {
                return Err(SignalError::InvalidCanonicalState);
            }
            return Ok(false);
        }
        let removed = self.slots.remove(&(sink, driver)).is_some();
        self.sinks
            .record_mut(sink)
            .ok_or(SignalError::InvalidCanonicalState)?
            .dirty = true;
        Ok(removed)
    }

    pub fn resolve_dirty(
        &mut self,
        logic_threshold: u64,
    ) -> Result<(Vec<SignalChangeRecord>, u64), SignalError> {
        let dirty: Vec<_> = self
            .sinks
            .canonical_slots()
            .filter_map(|(id, record)| record.filter(|record| record.dirty).map(|_| id))
            .collect();
        let resolved_count =
            u64::try_from(dirty.len()).map_err(|_| SignalError::NumericOverflow)?;
        let mut changes = Vec::new();
        for sink in dirty {
            let mut drive = DriveVector::default();
            for ((slot_sink, _), slot) in self
                .slots
                .range((sink, DriverId(EntityId(0)))..=(sink, DriverId(EntityId(u64::MAX))))
            {
                if *slot_sink != sink {
                    break;
                }
                drive.checked_add_sample(DriverSample {
                    level: slot.level,
                    strength: slot.strength,
                    revision: slot.revision,
                    emitted_at: slot.emitted_at,
                    driver_id: slot.driver,
                })?;
            }
            let resolved = resolve_drive(drive, logic_threshold);
            let record = self
                .sinks
                .record_mut(sink)
                .ok_or(SignalError::InvalidCanonicalState)?;
            let previous = record.resolved_level;
            record.resolved_level = resolved;
            record.dirty = false;
            if previous != resolved {
                changes.push(SignalChangeRecord {
                    sink,
                    previous,
                    current: resolved,
                });
            }
        }
        Ok((changes, resolved_count))
    }

    pub fn set_gate_desired_from_inputs(
        &mut self,
        gate: GateId,
    ) -> Result<LogicLevel, SignalError> {
        let record = *self
            .gates
            .get(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        let input_a = self
            .sink_level(record.ports.input_a.sink)
            .ok_or(SignalError::InvalidCanonicalState)?;
        let input_b = match record.ports.input_b {
            Some(port) => Some(
                self.sink_level(port.sink)
                    .ok_or(SignalError::InvalidCanonicalState)?,
            ),
            None => None,
        };
        let desired = gate_output(record.gate_type, input_a, input_b)?;
        self.gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?
            .desired_output = desired;
        Ok(desired)
    }

    pub fn set_wire_excitations(
        &mut self,
        excitations: &BTreeMap<WireId, DriveVector>,
    ) -> Result<(), SignalError> {
        for (wire, state) in &mut self.wires {
            state.previous = state.active;
            state.active = excitations.get(wire).copied().unwrap_or_default();
        }
        if excitations
            .keys()
            .any(|wire| !self.wires.contains_key(wire))
        {
            return Err(SignalError::InvalidCanonicalState);
        }
        Ok(())
    }

    pub fn add_cancelled_heat(&mut self, gate: GateId, energy: Energy) -> Result<(), SignalError> {
        let record = self
            .gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        record.cancelled_switching_heat = record
            .cancelled_switching_heat
            .checked_add(HeatEnergy(energy.0))?;
        Ok(())
    }

    pub fn advance_pending_generation(&mut self, gate: GateId) -> Result<u32, SignalError> {
        let record = self
            .gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        record.pending_generation = record
            .pending_generation
            .checked_add(1)
            .ok_or(SignalError::NumericOverflow)?;
        Ok(record.pending_generation)
    }

    #[cfg(test)]
    pub(crate) fn force_pending_generation_for_test(
        &mut self,
        gate: GateId,
        generation: u32,
    ) -> Result<(), SignalError> {
        self.gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?
            .pending_generation = generation;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn force_driver_revision_for_test(
        &mut self,
        driver: DriverId,
        revision: Revision,
    ) -> Result<(), SignalError> {
        self.drivers
            .record_mut(driver)
            .ok_or(SignalError::InvalidCanonicalState)?
            .sample
            .revision = revision;
        Ok(())
    }

    pub fn clear_pending(&mut self, gate: GateId) -> Result<(), SignalError> {
        let record = self
            .gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        record.pending_due_tick = None;
        record.pending_level = None;
        record.pending_switch_energy = None;
        Ok(())
    }

    pub fn set_pending(
        &mut self,
        gate: GateId,
        due_tick: Tick,
        level: LogicLevel,
        switch_energy: Energy,
    ) -> Result<(), SignalError> {
        let record = self
            .gates
            .get_mut(&gate)
            .ok_or(SignalError::InvalidCanonicalState)?;
        record.pending_due_tick = Some(due_tick);
        record.pending_level = Some(level);
        record.pending_switch_energy = Some(switch_energy);
        Ok(())
    }

    pub fn iter_gates(&self) -> impl Iterator<Item = GateSignalRecord> + '_ {
        self.gates.values().copied()
    }

    pub fn iter_gate_entries(&self) -> impl Iterator<Item = (GateId, GateSignalRecord)> + '_ {
        self.gates.iter().map(|(key, gate)| (*key, *gate))
    }

    #[cfg(test)]
    pub fn move_gate_key_for_test(&mut self, from: GateId, to: GateId) -> Result<(), SignalError> {
        if self.gates.contains_key(&to) {
            return Err(SignalError::InvalidCanonicalState);
        }
        let gate = self
            .gates
            .remove(&from)
            .ok_or(SignalError::InvalidCanonicalState)?;
        self.gates.insert(to, gate);
        Ok(())
    }

    pub fn iter_wires(&self) -> impl Iterator<Item = (WireId, WireSignalSnapshot)> + '_ {
        self.wires.iter().map(|(id, state)| (*id, *state))
    }

    pub fn iter_slots(&self) -> impl Iterator<Item = SinkDriverSlot> + '_ {
        self.slots.values().copied()
    }

    pub fn iter_slot_entries(
        &self,
    ) -> impl Iterator<Item = ((SinkId, DriverId), SinkDriverSlot)> + '_ {
        self.slots.iter().map(|(key, slot)| (*key, *slot))
    }

    #[cfg(test)]
    pub fn move_slot_key_for_test(
        &mut self,
        from: (SinkId, DriverId),
        to: (SinkId, DriverId),
    ) -> Result<(), SignalError> {
        if self.slots.contains_key(&to) {
            return Err(SignalError::InvalidCanonicalState);
        }
        let slot = self
            .slots
            .remove(&from)
            .ok_or(SignalError::InvalidCanonicalState)?;
        self.slots.insert(to, slot);
        Ok(())
    }

    pub fn canonical_driver_slots(
        &self,
    ) -> impl Iterator<Item = (DriverId, Option<DriverRecord>)> + '_ {
        self.drivers.canonical_slots()
    }

    pub fn canonical_sink_slots(&self) -> impl Iterator<Item = (SinkId, Option<SinkRecord>)> + '_ {
        self.sinks.canonical_slots()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalDriverStatus {
    Unknown,
    Removed,
    WrongKind,
    External,
}

pub(crate) fn resolve_drive(drive: DriveVector, logic_threshold: u64) -> LogicLevel {
    let threshold = u128::from(logic_threshold);
    if drive.unknown >= threshold || (drive.high >= threshold && drive.low >= threshold) {
        LogicLevel::X
    } else if drive.high >= threshold {
        LogicLevel::High
    } else {
        LogicLevel::Low
    }
}

pub(crate) fn gate_output(
    gate_type: GateType,
    input_a: LogicLevel,
    input_b: Option<LogicLevel>,
) -> Result<LogicLevel, SignalError> {
    match gate_type {
        GateType::Not => {
            if input_b.is_some() {
                return Err(SignalError::InvalidCanonicalState);
            }
            Ok(match input_a {
                LogicLevel::Low => LogicLevel::High,
                LogicLevel::High => LogicLevel::Low,
                LogicLevel::X => LogicLevel::X,
            })
        }
        GateType::And => {
            let input_b = input_b.ok_or(SignalError::InvalidCanonicalState)?;
            Ok(
                if input_a == LogicLevel::Low || input_b == LogicLevel::Low {
                    LogicLevel::Low
                } else if input_a == LogicLevel::High && input_b == LogicLevel::High {
                    LogicLevel::High
                } else {
                    LogicLevel::X
                },
            )
        }
        GateType::Or => {
            let input_b = input_b.ok_or(SignalError::InvalidCanonicalState)?;
            Ok(
                if input_a == LogicLevel::High || input_b == LogicLevel::High {
                    LogicLevel::High
                } else if input_a == LogicLevel::Low && input_b == LogicLevel::Low {
                    LogicLevel::Low
                } else {
                    LogicLevel::X
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(raw: u64) -> GateId {
        GateId(EntityId(raw))
    }

    #[test]
    fn endpoint_namespaces_allocate_in_frozen_role_order() {
        let mut world = SignalWorld::new();
        let binary = world
            .activate_gate(gate(41), GateType::And, Tick(7))
            .expect("binary Gate activates");
        assert_eq!(binary.input_a.external_driver, DriverId(EntityId(1)));
        assert_eq!(
            binary.input_b.expect("binary input").external_driver,
            DriverId(EntityId(2))
        );
        assert_eq!(binary.output, DriverId(EntityId(3)));
        assert_eq!(binary.input_a.sink, SinkId(EntityId(1)));
        assert_eq!(
            binary.input_b.expect("binary input").sink,
            SinkId(EntityId(2))
        );

        let unary = world
            .activate_gate(gate(99), GateType::Not, Tick(8))
            .expect("unary Gate activates");
        assert_eq!(unary.input_a.external_driver, DriverId(EntityId(4)));
        assert_eq!(unary.output, DriverId(EntityId(5)));
        assert_eq!(unary.input_b, None);
        assert_eq!(unary.input_a.sink, SinkId(EntityId(3)));
        assert_eq!(world.driver_frontier(), DriverId(EntityId(6)));
        assert_eq!(world.sink_frontier(), SinkId(EntityId(4)));
    }

    #[test]
    fn every_gate_type_activates_with_the_frozen_quiescent_signal_state() {
        let mut world = SignalWorld::new();

        for (raw_gate, gate_type, activation_tick) in [
            (1, GateType::And, Tick(7)),
            (2, GateType::Or, Tick(11)),
            (3, GateType::Not, Tick(13)),
        ] {
            let gate = gate(raw_gate);
            let ports = world
                .activate_gate(gate, gate_type, activation_tick)
                .expect("Gate activates");
            let state = world
                .gate_snapshot(gate)
                .expect("the just-activated Gate is observable");

            assert_eq!(state.ports, ports);
            assert_eq!(state.current_output, LogicLevel::Low);
            assert_eq!(state.desired_output, LogicLevel::Low);
            assert_eq!(state.pending_generation, 0);
            assert_eq!(state.pending_due_tick, None);
            assert_eq!(state.pending_level, None);
            assert_eq!(state.pending_switch_energy, None);
            assert_eq!(state.cancelled_switching_heat, HeatEnergy(0));
            assert_eq!(
                world.driver_sample(ports.output),
                Some(DriverSample {
                    level: LogicLevel::Low,
                    strength: DriveStrength(0),
                    revision: Revision(0),
                    emitted_at: activation_tick,
                    driver_id: ports.output,
                })
            );
        }
    }

    #[test]
    fn removed_endpoints_are_tombstones_and_never_reused() {
        let mut world = SignalWorld::new();
        let first = world
            .activate_gate(gate(1), GateType::Not, Tick(0))
            .expect("first Gate activates");
        world.remove_gate(gate(1)).expect("first Gate removes");
        assert_eq!(
            world.external_driver_status(first.input_a.external_driver, world.driver_frontier()),
            ExternalDriverStatus::Removed
        );
        let second = world
            .activate_gate(gate(2), GateType::Not, Tick(1))
            .expect("second Gate activates");
        assert!(second.input_a.external_driver.entity_id().0 > first.output.entity_id().0);
    }

    #[test]
    fn truth_tables_preserve_x() {
        assert_eq!(
            gate_output(GateType::Not, LogicLevel::X, None),
            Ok(LogicLevel::X)
        );
        assert_eq!(
            gate_output(GateType::And, LogicLevel::Low, Some(LogicLevel::X)),
            Ok(LogicLevel::Low)
        );
        assert_eq!(
            gate_output(GateType::And, LogicLevel::High, Some(LogicLevel::X)),
            Ok(LogicLevel::X)
        );
        assert_eq!(
            gate_output(GateType::Or, LogicLevel::High, Some(LogicLevel::X)),
            Ok(LogicLevel::High)
        );
        assert_eq!(
            gate_output(GateType::Or, LogicLevel::Low, Some(LogicLevel::X)),
            Ok(LogicLevel::X)
        );
    }

    #[test]
    fn sink_resolution_is_wide_simultaneous_and_passively_low() {
        assert_eq!(resolve_drive(DriveVector::default(), 100), LogicLevel::Low);
        assert_eq!(
            resolve_drive(
                DriveVector {
                    high: 100,
                    low: 0,
                    unknown: 0,
                },
                100
            ),
            LogicLevel::High
        );
        assert_eq!(
            resolve_drive(
                DriveVector {
                    high: 100,
                    low: 100,
                    unknown: 0,
                },
                100
            ),
            LogicLevel::X
        );
        assert_eq!(
            resolve_drive(
                DriveVector {
                    high: u128::from(u64::MAX),
                    low: 0,
                    unknown: 100,
                },
                100
            ),
            LogicLevel::X
        );
    }

    #[test]
    fn multi_driver_low_high_x_resolution_is_permutation_invariant() {
        let samples = [
            (LogicLevel::Low, 100_u64),
            (LogicLevel::High, 100_u64),
            (LogicLevel::X, 99_u64),
        ];
        for permutation in [
            [0_usize, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut drive = DriveVector::default();
            for index in permutation {
                let (level, strength) = samples[index];
                drive
                    .checked_add_sample(DriverSample {
                        level,
                        strength: DriveStrength(strength),
                        revision: Revision(0),
                        emitted_at: Tick(0),
                        driver_id: DriverId(EntityId(index as u64 + 1)),
                    })
                    .expect("permuted multi-Driver sum fits");
            }
            assert_eq!(
                drive,
                DriveVector {
                    high: 100,
                    low: 100,
                    unknown: 99,
                }
            );
            assert_eq!(resolve_drive(drive, 100), LogicLevel::X);
        }
    }

    #[test]
    fn drive_accumulator_overflow_is_typed_and_does_not_mutate() {
        let mut vector = DriveVector {
            high: u128::MAX,
            low: 7,
            unknown: 11,
        };
        let before = vector;
        assert_eq!(
            vector.checked_add_sample(DriverSample {
                level: LogicLevel::High,
                strength: DriveStrength(1),
                revision: Revision(0),
                emitted_at: Tick(0),
                driver_id: DriverId(EntityId(1)),
            }),
            Err(SignalError::NumericOverflow)
        );
        assert_eq!(vector, before);
    }

    #[test]
    fn canonical_driver_capacity_cannot_reach_drive_accumulator_overflow() {
        // DriverId 0 is reserved and allocating raw u64::MAX fails while advancing the frontier,
        // so at most u64::MAX - 1 unique Drivers can contribute one u64 strength each.
        let max_unique_drivers = u128::from(u64::MAX - 1);
        let max_strength = u128::from(u64::MAX);
        let maximum_representable_drive = max_unique_drivers
            .checked_mul(max_strength)
            .expect("the canonical Driver/strength product fits u128");

        assert_eq!(
            maximum_representable_drive,
            u128::MAX - 3 * u128::from(u64::MAX)
        );
        assert!(maximum_representable_drive < u128::MAX);
    }

    #[test]
    fn pending_generation_overflow_is_typed_and_does_not_mutate() {
        let mut world = SignalWorld::new();
        world
            .activate_gate(gate(1), GateType::Not, Tick(0))
            .expect("Gate activates");
        world
            .gates
            .get_mut(&gate(1))
            .expect("Gate record exists")
            .pending_generation = u32::MAX;

        assert_eq!(
            world.advance_pending_generation(gate(1)),
            Err(SignalError::NumericOverflow)
        );
        assert_eq!(
            world
                .gate_record(gate(1))
                .expect("Gate record remains")
                .pending_generation,
            u32::MAX
        );
    }

    #[test]
    fn driver_revision_advances_only_for_a_real_sample_change() {
        let mut world = SignalWorld::new();
        let ports = world
            .activate_gate(gate(1), GateType::Not, Tick(4))
            .expect("Gate activates");
        let driver = ports.input_a.external_driver;
        let initial = world.driver_sample(driver).expect("Driver is live");
        assert_eq!(initial.revision, Revision(0));
        assert_eq!(initial.emitted_at, Tick(4));

        assert_eq!(
            world.apply_driver_sample(driver, LogicLevel::Low, DriveStrength(0), Tick(9)),
            Ok(None)
        );
        assert_eq!(world.driver_sample(driver), Some(initial));

        let change = world
            .apply_driver_sample(driver, LogicLevel::High, DriveStrength(100), Tick(9))
            .expect("changed Sample applies")
            .expect("changed Sample is observable");
        assert_eq!(change.previous, initial);
        assert_eq!(change.current.revision, Revision(1));
        assert_eq!(change.current.emitted_at, Tick(9));

        assert_eq!(
            world.apply_driver_sample(driver, LogicLevel::High, DriveStrength(100), Tick(10)),
            Ok(None)
        );
        assert_eq!(world.driver_sample(driver), Some(change.current));

        world
            .force_driver_revision_for_test(driver, Revision(u64::MAX))
            .expect("test revision seed succeeds");
        let before = world.driver_sample(driver).expect("Driver remains live");
        assert_eq!(
            world.apply_driver_sample(driver, LogicLevel::Low, DriveStrength(100), Tick(11)),
            Err(SignalError::NumericOverflow)
        );
        assert_eq!(world.driver_sample(driver), Some(before));
    }

    #[test]
    fn slot_revision_table_is_applied_without_partial_mutation() {
        let mut world = SignalWorld::new();
        let ports = world
            .activate_gate(gate(1), GateType::Not, Tick(0))
            .expect("Gate activates");
        let driver = ports.input_a.external_driver;
        let sink = ports.input_a.sink;
        let initial = world.driver_sample(driver).expect("Driver is live");
        assert_eq!(
            world.apply_slot_sample(sink, initial),
            Ok(SlotApplyOutcome::Applied)
        );
        assert_eq!(world.sink_driver_sample(sink, driver), Some(initial));

        let revision_one = world
            .apply_driver_sample(driver, LogicLevel::High, DriveStrength(100), Tick(1))
            .expect("revision one applies")
            .expect("revision one changes")
            .current;
        assert_eq!(
            world.apply_slot_sample(sink, revision_one),
            Ok(SlotApplyOutcome::Applied)
        );
        let revision_two = world
            .apply_driver_sample(driver, LogicLevel::X, DriveStrength(100), Tick(2))
            .expect("revision two applies")
            .expect("revision two changes")
            .current;
        assert_eq!(
            world.apply_slot_sample(sink, revision_two),
            Ok(SlotApplyOutcome::Applied)
        );
        assert_eq!(
            world.apply_slot_sample(sink, revision_one),
            Ok(SlotApplyOutcome::Stale)
        );
        assert_eq!(world.sink_driver_sample(sink, driver), Some(revision_two));
        assert_eq!(
            world.apply_slot_sample(sink, revision_two),
            Ok(SlotApplyOutcome::Idempotent)
        );

        let conflict = DriverSample {
            strength: DriveStrength(101),
            ..revision_two
        };
        assert_eq!(
            world.apply_slot_sample(sink, conflict),
            Err(SignalError::DriverRevisionInvariantViolation)
        );
        assert_eq!(world.sink_driver_sample(sink, driver), Some(revision_two));
    }

    #[test]
    fn applying_slots_resolves_each_dirty_sink_and_updates_gate_input() {
        let mut world = SignalWorld::new();
        let ports = world
            .activate_gate(gate(1), GateType::Not, Tick(0))
            .expect("Gate activates");
        let sample = DriverSample {
            level: LogicLevel::High,
            strength: DriveStrength(400),
            revision: Revision(0),
            emitted_at: Tick(2),
            driver_id: ports.input_a.external_driver,
        };
        assert_eq!(
            world.apply_slot_sample(ports.input_a.sink, sample),
            Ok(SlotApplyOutcome::Applied)
        );
        let (changes, resolved) = world.resolve_dirty(100).expect("resolve succeeds");
        assert_eq!(changes.len(), 1);
        assert_eq!(resolved, 1);
        assert_eq!(world.sink_level(ports.input_a.sink), Some(LogicLevel::High));
        assert_eq!(
            world
                .set_gate_desired_from_inputs(gate(1))
                .expect("Gate evaluates"),
            LogicLevel::Low
        );
        assert_eq!(
            world.resolve_dirty(100).expect("nothing dirty"),
            (Vec::new(), 0)
        );
    }

    #[test]
    fn wire_excitation_keeps_previous_tick_vector() {
        let mut world = SignalWorld::new();
        let wire = WireId(EntityId(7));
        world.activate_wire(wire).expect("Wire activates");
        let first = DriveVector {
            high: 400,
            low: 0,
            unknown: 0,
        };
        world
            .set_wire_excitations(&BTreeMap::from([(wire, first)]))
            .expect("first excitation applies");
        assert_eq!(
            world.wire_snapshot(wire),
            Some(WireSignalSnapshot {
                active: first,
                previous: DriveVector::default()
            })
        );
        world
            .set_wire_excitations(&BTreeMap::from([(wire, first)]))
            .expect("second excitation applies");
        assert_eq!(
            world.wire_snapshot(wire),
            Some(WireSignalSnapshot {
                active: first,
                previous: first
            })
        );
    }
}
