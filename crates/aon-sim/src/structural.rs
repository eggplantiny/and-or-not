use crate::command::{
    BindPortCommand, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, SetExternalDriverCommand,
};
#[cfg(test)]
use crate::identity::FixedSubstrateIndex;
use crate::identity::{
    EntityLocation, EntityRegistry, EntityRegistryError, GateId, GateIndex, JunctionId,
    JunctionIndex, MobileId, MobileSubstrateIndex, WireId, WireIndex,
};
use crate::mobility::{
    MobileSubstrateRecord, MobileSubstrateStore, TrackGraph, TrackGraphError, TrackPosition,
};
use crate::path_certificate::PathElementStamp;
use crate::profile::{
    GateFootprint, MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA, PhysicalScaleProfile, PortAnchor,
};
use crate::signal::{ExternalDriverStatus, SignalError, SignalWorld};
use crate::structural_geometry::{
    parallel_segments_are_too_close, point_is_strict_segment_interior,
    segment_intersects_aabb_interior, segment_overlaps_aabb_boundary,
    segment_touches_aabb_boundary, segments_have_positive_collinear_overlap,
};
use crate::topology::{
    EndpointTarget, FixedAabb, FixedSubstrateRecord, FixedSubstrateStore, GatePort, GateStore,
    GateType, JunctionStore, RoutingDomain, TopologyError, WireEnd, WireRecord, WireStore,
    checked_add_point, checked_sub_point,
};
use crate::{DriverId, EntityId, Fixed, FixedVec2, NumericError, Tick, polyline_length};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

type Rejection = CommandRejectionReason;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum StructuralError {
    #[error("canonical structural numeric capacity exhausted")]
    NumericOverflow,

    #[error("canonical structural invariant violated")]
    InvalidCanonicalState,
}

impl From<NumericError> for StructuralError {
    fn from(_: NumericError) -> Self {
        Self::NumericOverflow
    }
}

impl From<TopologyError> for StructuralError {
    fn from(error: TopologyError) -> Self {
        match error {
            TopologyError::NumericOverflow
            | TopologyError::StoreIndexExhausted
            | TopologyError::GeometryArenaExhausted => Self::NumericOverflow,
            TopologyError::UnknownStoreIndex | TopologyError::RemovedRecord => {
                Self::InvalidCanonicalState
            }
        }
    }
}

impl From<EntityRegistryError> for StructuralError {
    fn from(error: EntityRegistryError) -> Self {
        match error {
            EntityRegistryError::EntityIdExhausted => Self::NumericOverflow,
            EntityRegistryError::ReservedEntityId
            | EntityRegistryError::UnknownEntity(_)
            | EntityRegistryError::RemovedEntity(_) => Self::InvalidCanonicalState,
        }
    }
}

impl From<SignalError> for StructuralError {
    fn from(error: SignalError) -> Self {
        match error {
            SignalError::NumericOverflow => Self::NumericOverflow,
            SignalError::InvalidCanonicalState | SignalError::DriverRevisionInvariantViolation => {
                Self::InvalidCanonicalState
            }
        }
    }
}

impl From<TrackGraphError> for StructuralError {
    fn from(error: TrackGraphError) -> Self {
        match error {
            TrackGraphError::NumericOverflow => Self::NumericOverflow,
            TrackGraphError::InvalidCanonicalState => Self::InvalidCanonicalState,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExternalDriverUpdate {
    pub driver: DriverId,
    pub level: crate::LogicLevel,
    pub strength: crate::DriveStrength,
    pub ordinal: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructuralPhaseReport {
    pub acceptances: Vec<CommandAcceptance>,
    pub rejections: Vec<CommandRejection>,
    pub topology_changed: bool,
    pub external_driver_updates: Vec<ExternalDriverUpdate>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructuralWorld {
    entities: EntityRegistry,
    gates: GateStore,
    wires: WireStore,
    junctions: JunctionStore,
    fixed_substrates: FixedSubstrateStore,
    mobile_substrates: MobileSubstrateStore,
}

#[derive(Default)]
struct PhaseChanges {
    dirty_wires: BTreeSet<WireIndex>,
    dirty_junctions: BTreeSet<JunctionIndex>,
    topology_changed: bool,
}

impl StructuralWorld {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn apply_phase0(
        &mut self,
        tick: Tick,
        commands: &[CommandEnvelope],
        physical: &PhysicalScaleProfile,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        self.apply_phase0_internal(tick, commands, physical, None)
    }

    pub fn apply_phase0_with_signal(
        &mut self,
        signal: &mut SignalWorld,
        tick: Tick,
        commands: &[CommandEnvelope],
        physical: &PhysicalScaleProfile,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        let mut signal_working = signal.clone();
        let report =
            self.apply_phase0_internal(tick, commands, physical, Some(&mut signal_working))?;
        *signal = signal_working;
        Ok(report)
    }

    fn apply_phase0_internal(
        &mut self,
        tick: Tick,
        commands: &[CommandEnvelope],
        physical: &PhysicalScaleProfile,
        mut signal: Option<&mut SignalWorld>,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        let mut working = self.clone();
        let batch_frontier = self.entities.next_id();
        let driver_frontier = signal.as_deref().map(SignalWorld::driver_frontier);
        let mut report = StructuralPhaseReport::default();
        let mut ordinal_counts = BTreeMap::<u64, usize>::new();
        let mut external_updates = BTreeMap::<DriverId, ExternalDriverUpdate>::new();

        for envelope in commands {
            if envelope.target_tick == tick {
                let count = ordinal_counts.entry(envelope.ordinal).or_default();
                *count = count
                    .checked_add(1)
                    .ok_or(StructuralError::NumericOverflow)?;
            } else {
                report.rejections.push(CommandRejection {
                    target_tick: envelope.target_tick,
                    ordinal: envelope.ordinal,
                    reason: Rejection::WrongTick,
                });
            }
        }

        let mut ordered: Vec<_> = commands
            .iter()
            .filter(|envelope| envelope.target_tick == tick)
            .collect();
        ordered.sort_unstable_by_key(|envelope| envelope.ordinal);

        let mut changes = PhaseChanges::default();
        for envelope in ordered {
            if ordinal_counts.get(&envelope.ordinal).copied().unwrap_or(0) > 1 {
                report.rejections.push(CommandRejection {
                    target_tick: envelope.target_tick,
                    ordinal: envelope.ordinal,
                    reason: Rejection::DuplicateOrdinal,
                });
                continue;
            }

            let removed_location = match &envelope.command {
                Command::RemoveEntity(command) => {
                    working.entities.location(command.target).copied()
                }
                _ => None,
            };
            let result = match &envelope.command {
                Command::SetExternalDriver(command) => match signal.as_deref_mut() {
                    Some(signal) => apply_external_driver_command(
                        signal,
                        *command,
                        driver_frontier.ok_or(StructuralError::InvalidCanonicalState)?,
                        envelope.ordinal,
                        &mut external_updates,
                    ),
                    None => Ok(Err(Rejection::UnsupportedCommand)),
                },
                command => working.apply_command(command, batch_frontier, physical, &mut changes),
            }?;

            match result {
                Ok(created_entity) => report.acceptances.push(CommandAcceptance {
                    target_tick: envelope.target_tick,
                    ordinal: envelope.ordinal,
                    created_entity,
                }),
                Err(reason) => report.rejections.push(CommandRejection {
                    target_tick: envelope.target_tick,
                    ordinal: envelope.ordinal,
                    reason,
                }),
            }

            if result.is_ok()
                && !matches!(envelope.command, Command::SetExternalDriver(_))
                && let Some(signal) = signal.as_deref_mut()
            {
                apply_signal_lifecycle(
                    signal,
                    &envelope.command,
                    result.ok().flatten(),
                    removed_location,
                    tick,
                )?;
            }
        }

        for index in changes.dirty_wires {
            if working.wires.get(index).is_some() {
                working.wires.advance_generation(index)?;
            }
        }
        for index in changes.dirty_junctions {
            if working.junctions.get(index).is_some() {
                working.junctions.advance_generation(index)?;
            }
        }

        report
            .acceptances
            .sort_unstable_by_key(|result| (result.target_tick, result.ordinal));
        report
            .rejections
            .sort_unstable_by_key(|result| (result.target_tick, result.ordinal));
        if let Some(signal) = signal.as_deref() {
            external_updates.retain(|driver, _| {
                matches!(
                    signal.external_driver_status(*driver, signal.driver_frontier()),
                    ExternalDriverStatus::External
                )
            });
        }
        report.external_driver_updates = external_updates.into_values().collect();
        report
            .external_driver_updates
            .sort_unstable_by_key(|update| update.ordinal);
        report.topology_changed = changes.topology_changed;
        *self = working;
        Ok(report)
    }

    fn apply_command(
        &mut self,
        command: &Command,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        match command {
            Command::PlaceGate(command) => self.place_gate(*command, frontier, physical, changes),
            Command::PlaceWire(command) => self.place_wire(command, frontier, physical, changes),
            Command::PlaceJunction(command) => {
                self.place_junction(*command, frontier, physical, changes)
            }
            Command::PlaceFixedSubstrate(command) => self.place_fixed_substrate(*command, physical),
            Command::PlaceMobileSubstrate(command) => {
                self.place_mobile_substrate(*command, physical, changes)
            }
            Command::RemoveEntity(command) => self.remove_entity(*command, frontier, changes),
            Command::BindPort(command) => self.bind_port(*command, frontier, physical, changes),
            Command::SetExternalDriver(_) => Ok(Err(Rejection::UnsupportedCommand)),
        }
    }

    pub const fn entities(&self) -> &EntityRegistry {
        &self.entities
    }

    pub const fn gates(&self) -> &GateStore {
        &self.gates
    }

    pub const fn wires(&self) -> &WireStore {
        &self.wires
    }

    pub const fn junctions(&self) -> &JunctionStore {
        &self.junctions
    }

    pub const fn fixed_substrates(&self) -> &FixedSubstrateStore {
        &self.fixed_substrates
    }

    pub const fn mobile_substrates(&self) -> &MobileSubstrateStore {
        &self.mobile_substrates
    }

    pub(crate) fn commit_mobile_positions(
        &mut self,
        positions: &[(MobileSubstrateIndex, MobileId, TrackPosition)],
    ) -> Result<(), StructuralError> {
        for &(index, id, position) in positions {
            self.mobile_substrates
                .set_track_position(index, id, position)?;
        }
        Ok(())
    }

    pub(crate) fn path_element_is_current(&self, stamp: PathElementStamp) -> bool {
        let entity = stamp.entity_id();
        match (stamp, self.entities.location(entity).copied()) {
            (PathElementStamp::Wire { id, generation }, Some(EntityLocation::Wire(index))) => {
                self.wires.get(index).is_some_and(|record| {
                    record.id == id && record.connection_generation == generation
                })
            }
            (
                PathElementStamp::Junction { id, generation },
                Some(EntityLocation::Junction(index)),
            ) => self.junctions.get(index).is_some_and(|record| {
                record.id == id && record.connection_generation == generation
            }),
            _ => false,
        }
    }

    pub fn live_primitive_count(&self) -> u64 {
        self.gates.live_count()
            + self.wires.live_count()
            + self.junctions.live_count()
            + self.fixed_substrates.live_count()
            + self.mobile_substrates.live_count()
    }

    #[cfg(test)]
    pub(crate) fn reserve_layout_capacity_for_test(&mut self, additional: usize) {
        self.entities.reserve_capacity_for_test(additional);
        self.gates.reserve_capacity_for_test(additional);
        self.wires.reserve_capacity_for_test(additional);
        self.junctions.reserve_capacity_for_test(additional);
        self.fixed_substrates.reserve_capacity_for_test(additional);
        self.mobile_substrates.reserve_capacity_for_test(additional);
    }

    #[cfg(test)]
    pub(crate) fn swap_gate_slots_for_test(
        &mut self,
        first: GateIndex,
        second: GateIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .gates
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .gates
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id.entity_id()) != Some(&EntityLocation::Gate(first))
            || self.entities.location(second_id.entity_id()) != Some(&EntityLocation::Gate(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.gates.swap_slots_for_test(first, second)?;
        self.entities
            .update_location(first_id.entity_id(), EntityLocation::Gate(second))?;
        self.entities
            .update_location(second_id.entity_id(), EntityLocation::Gate(first))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn swap_wire_slots_for_test(
        &mut self,
        first: WireIndex,
        second: WireIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .wires
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .wires
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id.entity_id()) != Some(&EntityLocation::Wire(first))
            || self.entities.location(second_id.entity_id()) != Some(&EntityLocation::Wire(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.wires.swap_slots_for_test(first, second)?;
        self.entities
            .update_location(first_id.entity_id(), EntityLocation::Wire(second))?;
        self.entities
            .update_location(second_id.entity_id(), EntityLocation::Wire(first))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn swap_wire_registry_locations_for_test(
        &mut self,
        first: WireIndex,
        second: WireIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .wires
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .wires
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id.entity_id()) != Some(&EntityLocation::Wire(first))
            || self.entities.location(second_id.entity_id()) != Some(&EntityLocation::Wire(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.entities
            .update_location(first_id.entity_id(), EntityLocation::Wire(second))?;
        self.entities
            .update_location(second_id.entity_id(), EntityLocation::Wire(first))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn swap_junction_slots_for_test(
        &mut self,
        first: JunctionIndex,
        second: JunctionIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .junctions
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .junctions
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id.entity_id()) != Some(&EntityLocation::Junction(first))
            || self.entities.location(second_id.entity_id())
                != Some(&EntityLocation::Junction(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.junctions.swap_slots_for_test(first, second)?;
        self.entities
            .update_location(first_id.entity_id(), EntityLocation::Junction(second))?;
        self.entities
            .update_location(second_id.entity_id(), EntityLocation::Junction(first))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn swap_fixed_substrate_slots_for_test(
        &mut self,
        first: FixedSubstrateIndex,
        second: FixedSubstrateIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .fixed_substrates
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .fixed_substrates
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id) != Some(&EntityLocation::FixedSubstrate(first))
            || self.entities.location(second_id) != Some(&EntityLocation::FixedSubstrate(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.fixed_substrates.swap_slots_for_test(first, second)?;
        self.entities
            .update_location(first_id, EntityLocation::FixedSubstrate(second))?;
        self.entities
            .update_location(second_id, EntityLocation::FixedSubstrate(first))?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn swap_mobile_substrate_slots_for_test(
        &mut self,
        first: MobileSubstrateIndex,
        second: MobileSubstrateIndex,
    ) -> Result<(), StructuralError> {
        let first_id = self
            .mobile_substrates
            .get(first)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        let second_id = self
            .mobile_substrates
            .get(second)
            .ok_or(StructuralError::InvalidCanonicalState)?
            .id;
        if self.entities.location(first_id.entity_id())
            != Some(&EntityLocation::MobileSubstrate(first))
            || self.entities.location(second_id.entity_id())
                != Some(&EntityLocation::MobileSubstrate(second))
        {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.mobile_substrates.swap_slots_for_test(first, second)?;
        self.entities.update_location(
            first_id.entity_id(),
            EntityLocation::MobileSubstrate(second),
        )?;
        self.entities.update_location(
            second_id.entity_id(),
            EntityLocation::MobileSubstrate(first),
        )?;
        Ok(())
    }
}

fn apply_external_driver_command(
    signal: &SignalWorld,
    command: SetExternalDriverCommand,
    batch_frontier: DriverId,
    ordinal: u64,
    updates: &mut BTreeMap<DriverId, ExternalDriverUpdate>,
) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
    let reason = match signal.external_driver_status(command.driver, batch_frontier) {
        ExternalDriverStatus::Unknown => Some(Rejection::UnknownDriver),
        ExternalDriverStatus::Removed => Some(Rejection::RemovedDriver),
        ExternalDriverStatus::WrongKind => Some(Rejection::InvalidDriverKind),
        ExternalDriverStatus::External => None,
    };
    if let Some(reason) = reason {
        return Ok(Err(reason));
    }
    updates.insert(
        command.driver,
        ExternalDriverUpdate {
            driver: command.driver,
            level: command.level,
            strength: command.strength,
            ordinal,
        },
    );
    Ok(Ok(None))
}

fn apply_signal_lifecycle(
    signal: &mut SignalWorld,
    command: &Command,
    created_entity: Option<EntityId>,
    removed_location: Option<EntityLocation>,
    tick: Tick,
) -> Result<(), StructuralError> {
    match command {
        Command::PlaceGate(command) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            signal.activate_gate(GateId(id), command.gate_type, tick)?;
        }
        Command::PlaceWire(_) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            signal.activate_wire(WireId(id))?;
        }
        Command::PlaceMobileSubstrate(_) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            signal.activate_mobile(MobileId(id))?;
        }
        Command::RemoveEntity(command) => match removed_location {
            Some(EntityLocation::Gate(_)) => signal.remove_gate(GateId(command.target))?,
            Some(EntityLocation::Wire(_)) => signal.remove_wire(WireId(command.target))?,
            Some(EntityLocation::MobileSubstrate(_)) => {
                signal.remove_mobile(MobileId(command.target))?;
            }
            _ => {}
        },
        Command::PlaceJunction(_)
        | Command::PlaceFixedSubstrate(_)
        | Command::BindPort(_)
        | Command::SetExternalDriver(_) => {}
    }
    Ok(())
}

impl StructuralWorld {
    fn place_mobile_substrate(
        &mut self,
        command: PlaceMobileSubstrateCommand,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if !command.routing_area.is_nonempty() || !command.footprint.is_nonempty() {
            return Ok(Err(Rejection::InvalidGeometryShape));
        }
        if !point_is_quantized(command.origin, physical.wire_geometry_quantum)
            || !aabb_is_quantized(command.routing_area, physical.wire_geometry_quantum)
            || !aabb_is_quantized(command.footprint, physical.wire_geometry_quantum)
        {
            return Ok(Err(Rejection::InvalidGeometryQuantum));
        }
        if !aabb_is_quantized(command.routing_area, physical.circuit_routing_pitch) {
            return Ok(Err(Rejection::InvalidRoutingPitch));
        }
        if !command.footprint.contains_aabb(command.routing_area) {
            return Ok(Err(Rejection::SubstrateBoundsViolation));
        }
        if physical.world_routing_pitch.0 / physical.wire_geometry_quantum.0
            > MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA
        {
            return Ok(Err(Rejection::UnsupportedPlacement));
        }

        let track = TrackGraph::compile(&self.wires, &self.junctions)?;
        let Some(track_position) = track.locate(command.origin)? else {
            return Ok(Err(Rejection::UnsupportedPlacement));
        };
        let id = MobileId(self.entities.next_id());
        let index = self.mobile_substrates.push(
            id,
            track_position,
            command.routing_area,
            command.footprint,
        )?;
        let allocated = self
            .entities
            .allocate(EntityLocation::MobileSubstrate(index))?;
        if allocated != id.entity_id() {
            return Err(StructuralError::InvalidCanonicalState);
        }
        changes.topology_changed = true;
        Ok(Ok(Some(allocated)))
    }

    fn place_fixed_substrate(
        &mut self,
        command: PlaceFixedSubstrateCommand,
        physical: &PhysicalScaleProfile,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if !command.routing_area.is_nonempty() || !command.footprint.is_nonempty() {
            return Ok(Err(Rejection::InvalidGeometryShape));
        }
        if !point_is_quantized(command.origin, physical.wire_geometry_quantum)
            || !aabb_is_quantized(command.routing_area, physical.wire_geometry_quantum)
            || !aabb_is_quantized(command.footprint, physical.wire_geometry_quantum)
        {
            return Ok(Err(Rejection::InvalidGeometryQuantum));
        }
        if !point_is_quantized(command.origin, physical.world_routing_pitch)
            || !aabb_is_quantized(command.routing_area, physical.circuit_routing_pitch)
        {
            return Ok(Err(Rejection::InvalidRoutingPitch));
        }
        if !command.footprint.contains_aabb(command.routing_area) {
            return Ok(Err(Rejection::SubstrateBoundsViolation));
        }

        let world_footprint = command.footprint.translated(command.origin)?;
        for (_, existing) in self.fixed_substrates.iter_alive() {
            let existing_world = existing.footprint.translated(existing.origin)?;
            if world_footprint.interior_overlaps(existing_world) {
                return Ok(Err(Rejection::GeometryOverlap));
            }
        }

        let id = self.entities.next_id();
        let index = self.fixed_substrates.push(
            id,
            command.origin,
            command.routing_area,
            command.footprint,
        )?;
        let allocated = self
            .entities
            .allocate(EntityLocation::FixedSubstrate(index))?;
        if allocated != id {
            return Err(StructuralError::InvalidCanonicalState);
        }
        Ok(Ok(Some(id)))
    }

    fn place_gate(
        &mut self,
        command: PlaceGateCommand,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if let Some(reason) =
            self.routing_domain_lifecycle_rejection(command.routing_domain, frontier)
        {
            return Ok(Err(reason));
        }
        if !point_is_quantized(command.origin, physical.wire_geometry_quantum) {
            return Ok(Err(Rejection::InvalidGeometryQuantum));
        }
        let (local_origin, routing_area) = match command.routing_domain {
            RoutingDomain::OpenWorld => return Ok(Err(Rejection::UnsupportedPlacement)),
            RoutingDomain::FixedSubstrate(substrate_id) => {
                let substrate = match self.fixed_substrate_reference(substrate_id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                (
                    checked_sub_point(command.origin, substrate.origin)?,
                    substrate.routing_area,
                )
            }
            RoutingDomain::MobileSubstrate(substrate_id) => {
                let substrate = match self.mobile_substrate_reference(substrate_id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                (command.origin, substrate.routing_area)
            }
        };
        if !point_is_quantized(local_origin, physical.circuit_routing_pitch) {
            return Ok(Err(Rejection::InvalidRoutingPitch));
        }

        let local_footprint = gate_aabb(local_origin, command.gate_type, physical)?;
        if !routing_area.contains_aabb(local_footprint) {
            return Ok(Err(Rejection::SubstrateBoundsViolation));
        }
        let domain_footprint = gate_aabb(command.origin, command.gate_type, physical)?;
        for (_, existing) in self.gates.iter_alive() {
            if existing.routing_domain == command.routing_domain
                && domain_footprint.interior_overlaps(gate_aabb(
                    existing.origin,
                    existing.gate_type,
                    physical,
                )?)
            {
                return Ok(Err(Rejection::GeometryOverlap));
            }
        }
        for (_, wire) in self.wires.iter_alive() {
            match validate_wire_gate_contact(
                wire.points,
                wire.routing_domain,
                command.gate_type,
                command.origin,
                command.routing_domain,
                physical,
            )? {
                Ok(()) => {}
                Err(reason) => return Ok(Err(reason)),
            }
        }

        let id = GateId(self.entities.next_id());
        let index = self.gates.push(
            id,
            command.gate_type,
            command.origin,
            command.routing_domain,
        )?;
        let allocated = self.entities.allocate(EntityLocation::Gate(index))?;
        if allocated != id.entity_id() {
            return Err(StructuralError::InvalidCanonicalState);
        }
        changes.topology_changed = true;
        Ok(Ok(Some(allocated)))
    }

    fn place_junction(
        &mut self,
        command: PlaceJunctionCommand,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if let Some(reason) =
            self.routing_domain_lifecycle_rejection(command.routing_domain, frontier)
        {
            return Ok(Err(reason));
        }
        if !point_is_quantized(command.position, physical.wire_geometry_quantum) {
            return Ok(Err(Rejection::InvalidGeometryQuantum));
        }
        match self.validate_routed_point(
            command.position,
            command.routing_domain,
            frontier,
            physical,
        )? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        for (_, wire) in self.wires.iter_alive() {
            if wire.routing_domain != command.routing_domain {
                continue;
            }
            for segment in wire.points.windows(2) {
                if point_is_strict_segment_interior(command.position, segment[0], segment[1])? {
                    return Ok(Err(Rejection::GeometryOverlap));
                }
            }
        }

        let id = JunctionId(self.entities.next_id());
        let index = self
            .junctions
            .push(id, command.routing_domain, command.position)?;
        let allocated = self.entities.allocate(EntityLocation::Junction(index))?;
        if allocated != id.entity_id() {
            return Err(StructuralError::InvalidCanonicalState);
        }
        changes.topology_changed = true;
        Ok(Ok(Some(allocated)))
    }

    fn fixed_substrate_reference(
        &self,
        id: EntityId,
        frontier: EntityId,
    ) -> Result<Result<FixedSubstrateRecord, Rejection>, StructuralError> {
        let location = match self.reference_location(id, frontier) {
            Ok(location) => location,
            Err(reason) => return Ok(Err(reason)),
        };
        let EntityLocation::FixedSubstrate(index) = location else {
            return Ok(Err(Rejection::InvalidRoutingDomain));
        };
        self.fixed_substrates
            .get(index)
            .map(Ok)
            .ok_or(StructuralError::InvalidCanonicalState)
    }

    fn mobile_substrate_reference(
        &self,
        id: EntityId,
        frontier: EntityId,
    ) -> Result<Result<MobileSubstrateRecord, Rejection>, StructuralError> {
        let location = match self.reference_location(id, frontier) {
            Ok(location) => location,
            Err(reason) => return Ok(Err(reason)),
        };
        let EntityLocation::MobileSubstrate(index) = location else {
            return Ok(Err(Rejection::InvalidRoutingDomain));
        };
        self.mobile_substrates
            .get(index)
            .map(Ok)
            .ok_or(StructuralError::InvalidCanonicalState)
    }

    fn validate_routed_point(
        &self,
        point: FixedVec2,
        domain: RoutingDomain,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
    ) -> Result<Result<(), Rejection>, StructuralError> {
        match domain {
            RoutingDomain::OpenWorld => {
                if point_is_quantized(point, physical.world_routing_pitch) {
                    Ok(Ok(()))
                } else {
                    Ok(Err(Rejection::InvalidRoutingPitch))
                }
            }
            RoutingDomain::FixedSubstrate(id) => {
                let substrate = match self.fixed_substrate_reference(id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                let local = checked_sub_point(point, substrate.origin)?;
                if !point_is_quantized(local, physical.circuit_routing_pitch) {
                    return Ok(Err(Rejection::InvalidRoutingPitch));
                }
                if !substrate.routing_area.contains_point(local) {
                    return Ok(Err(Rejection::SubstrateBoundsViolation));
                }
                Ok(Ok(()))
            }
            RoutingDomain::MobileSubstrate(id) => {
                let substrate = match self.mobile_substrate_reference(id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                if !point_is_quantized(point, physical.circuit_routing_pitch) {
                    return Ok(Err(Rejection::InvalidRoutingPitch));
                }
                if !substrate.routing_area.contains_point(point) {
                    return Ok(Err(Rejection::SubstrateBoundsViolation));
                }
                Ok(Ok(()))
            }
        }
    }

    fn validate_wire_endpoint(
        &self,
        point: FixedVec2,
        domain: RoutingDomain,
        frontier: EntityId,
    ) -> Result<Result<(), Rejection>, StructuralError> {
        match domain {
            RoutingDomain::OpenWorld => Ok(Ok(())),
            RoutingDomain::FixedSubstrate(id) => {
                let substrate = match self.fixed_substrate_reference(id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                let local = checked_sub_point(point, substrate.origin)?;
                if !substrate.routing_area.contains_point(local) {
                    return Ok(Err(Rejection::SubstrateBoundsViolation));
                }
                Ok(Ok(()))
            }
            RoutingDomain::MobileSubstrate(id) => {
                let substrate = match self.mobile_substrate_reference(id, frontier)? {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                if !substrate.routing_area.contains_point(point) {
                    return Ok(Err(Rejection::SubstrateBoundsViolation));
                }
                Ok(Ok(()))
            }
        }
    }

    fn reference_location(
        &self,
        id: EntityId,
        frontier: EntityId,
    ) -> Result<EntityLocation, Rejection> {
        if id.0 == 0 || id.0 >= frontier.0 {
            return Err(Rejection::UnknownEntity);
        }
        self.entities
            .location(id)
            .copied()
            .ok_or(Rejection::RemovedEntity)
    }

    fn routing_domain_lifecycle_rejection(
        &self,
        domain: RoutingDomain,
        frontier: EntityId,
    ) -> Option<Rejection> {
        let id = match domain {
            RoutingDomain::OpenWorld => return None,
            RoutingDomain::FixedSubstrate(id) | RoutingDomain::MobileSubstrate(id) => id,
        };
        self.reference_location(id, frontier).err()
    }

    fn endpoint_lifecycle_rejection(
        &self,
        target: EndpointTarget,
        frontier: EntityId,
    ) -> Option<Rejection> {
        let id = match target {
            EndpointTarget::Free => return None,
            EndpointTarget::Junction(id) => id.entity_id(),
            EndpointTarget::GatePort(reference) => reference.gate.entity_id(),
            EndpointTarget::MobilePort(reference) => reference.mobile.entity_id(),
        };
        self.reference_location(id, frontier).err()
    }
}

impl StructuralWorld {
    fn place_wire(
        &mut self,
        command: &PlaceWireCommand,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if command.points.len() < 2 || u32::try_from(command.points.len()).is_err() {
            return Ok(Err(Rejection::InvalidGeometryShape));
        }
        if let Some(reason) =
            self.routing_domain_lifecycle_rejection(command.routing_domain, frontier)
        {
            return Ok(Err(reason));
        }
        if let Some(reason) = self.endpoint_lifecycle_rejection(command.endpoint_a, frontier) {
            return Ok(Err(reason));
        }
        if let Some(reason) = self.endpoint_lifecycle_rejection(command.endpoint_b, frontier) {
            return Ok(Err(reason));
        }
        if command
            .points
            .iter()
            .any(|&point| !point_is_quantized(point, physical.wire_geometry_quantum))
        {
            return Ok(Err(Rejection::InvalidGeometryQuantum));
        }
        if command
            .points
            .windows(2)
            .any(|segment| segment[0] == segment[1])
        {
            return Ok(Err(Rejection::ZeroLengthSegment));
        }
        for (index, &point) in command.points.iter().enumerate() {
            let is_endpoint = index == 0 || index + 1 == command.points.len();
            let validation = if is_endpoint {
                self.validate_wire_endpoint(point, command.routing_domain, frontier)?
            } else {
                self.validate_routed_point(point, command.routing_domain, frontier, physical)?
            };
            match validation {
                Ok(()) => {}
                Err(reason) => return Ok(Err(reason)),
            }
        }
        // Length is derived rather than stored, but every canonical Wire must be able to
        // reproduce it with the canonical Fixed representation. Treat exhaustion as a fatal
        // checked-arithmetic error before any structural mutation can be committed.
        polyline_length(&command.points)?;
        for first in 0..command.points.len() - 1 {
            for second in first + 1..command.points.len() - 1 {
                if segments_have_positive_collinear_overlap(
                    command.points[first],
                    command.points[first + 1],
                    command.points[second],
                    command.points[second + 1],
                )? {
                    return Ok(Err(Rejection::GeometryOverlap));
                }
                // Consecutive pieces are one continuous centerline. Non-adjacent pieces of the
                // same Wire still obey the routing-pitch clearance rule, including a first/last
                // pair unless the physical endpoints themselves coincide.
                if second > first + 1
                    && parallel_segments_are_too_close(
                        command.points[first],
                        command.points[first + 1],
                        command.points[second],
                        command.points[second + 1],
                        routing_pitch(command.routing_domain, physical),
                    )?
                    && !segments_share_endpoint_from_points(
                        &command.points,
                        first,
                        &command.points,
                        second,
                    )
                {
                    return Ok(Err(Rejection::InsufficientSpacing));
                }
            }
        }

        for (_, existing) in self.wires.iter_alive() {
            if command.routing_domain != existing.routing_domain {
                continue;
            }
            for (new_index, new_segment) in command.points.windows(2).enumerate() {
                for (old_index, old_segment) in existing.points.windows(2).enumerate() {
                    if segments_have_positive_collinear_overlap(
                        new_segment[0],
                        new_segment[1],
                        old_segment[0],
                        old_segment[1],
                    )? {
                        return Ok(Err(Rejection::GeometryOverlap));
                    }
                    if parallel_segments_are_too_close(
                        new_segment[0],
                        new_segment[1],
                        old_segment[0],
                        old_segment[1],
                        routing_pitch(command.routing_domain, physical),
                    )? && !segments_share_physical_wire_endpoint(
                        command, new_index, existing, old_index,
                    ) {
                        return Ok(Err(Rejection::InsufficientSpacing));
                    }
                }
            }
        }

        for (_, junction) in self.junctions.iter_alive() {
            if junction.routing_domain != command.routing_domain {
                continue;
            }
            for segment in command.points.windows(2) {
                if point_is_strict_segment_interior(junction.position, segment[0], segment[1])? {
                    return Ok(Err(Rejection::GeometryOverlap));
                }
            }
        }
        match self.validate_wire_gate_contacts(&command.points, command.routing_domain, physical)? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }

        match self.validate_endpoint(
            command.endpoint_a,
            command.points[0],
            command.routing_domain,
            frontier,
            physical,
        )? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        match self.validate_endpoint(
            command.endpoint_b,
            *command
                .points
                .last()
                .ok_or(StructuralError::InvalidCanonicalState)?,
            command.routing_domain,
            frontier,
            physical,
        )? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        if open_world_wire_binds_same_junction(
            command.routing_domain,
            command.endpoint_a,
            command.endpoint_b,
        ) {
            return Ok(Err(Rejection::InvalidPortBinding));
        }

        let id = WireId(self.entities.next_id());
        let index = self.wires.push(
            id,
            command.routing_domain,
            &command.points,
            command.endpoint_a,
            command.endpoint_b,
        )?;
        let allocated = self.entities.allocate(EntityLocation::Wire(index))?;
        if allocated != id.entity_id() {
            return Err(StructuralError::InvalidCanonicalState);
        }
        mark_target_junction(
            command.endpoint_a,
            &self.entities,
            &mut changes.dirty_junctions,
        )?;
        mark_target_junction(
            command.endpoint_b,
            &self.entities,
            &mut changes.dirty_junctions,
        )?;
        changes.topology_changed = true;
        Ok(Ok(Some(allocated)))
    }

    fn validate_endpoint(
        &self,
        target: EndpointTarget,
        endpoint: FixedVec2,
        domain: RoutingDomain,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
    ) -> Result<Result<(), Rejection>, StructuralError> {
        match target {
            EndpointTarget::Free => Ok(Ok(())),
            EndpointTarget::Junction(id) => {
                let location = match self.reference_location(id.entity_id(), frontier) {
                    Ok(location) => location,
                    Err(reason) => return Ok(Err(reason)),
                };
                let EntityLocation::Junction(index) = location else {
                    return Ok(Err(Rejection::InvalidEndpoint));
                };
                let junction = self
                    .junctions
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?;
                if junction.routing_domain != domain || junction.position != endpoint {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                Ok(Ok(()))
            }
            EndpointTarget::GatePort(reference) => {
                let location = match self.reference_location(reference.gate.entity_id(), frontier) {
                    Ok(location) => location,
                    Err(reason) => return Ok(Err(reason)),
                };
                let EntityLocation::Gate(index) = location else {
                    return Ok(Err(Rejection::InvalidEndpoint));
                };
                let gate = self
                    .gates
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?;
                if gate.routing_domain != domain {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                let Some(anchor) = gate_port_anchor(gate.gate_type, reference.port, physical)
                else {
                    return Ok(Err(Rejection::InvalidPort));
                };
                let anchor = checked_add_point(gate.origin, FixedVec2::new(anchor.x, anchor.y))?;
                if anchor != endpoint {
                    return Ok(Err(Rejection::InvalidPortBinding));
                }
                Ok(Ok(()))
            }
            EndpointTarget::MobilePort(reference) => {
                let substrate = match self
                    .mobile_substrate_reference(reference.mobile.entity_id(), frontier)?
                {
                    Ok(substrate) => substrate,
                    Err(reason) => return Ok(Err(reason)),
                };
                if domain != RoutingDomain::MobileSubstrate(reference.mobile.entity_id())
                    || !substrate.routing_area.contains_point(endpoint)
                {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                Ok(Ok(()))
            }
        }
    }

    fn validate_wire_gate_contacts(
        &self,
        points: &[FixedVec2],
        routing_domain: RoutingDomain,
        physical: &PhysicalScaleProfile,
    ) -> Result<Result<(), Rejection>, StructuralError> {
        for (_, gate) in self.gates.iter_alive() {
            match validate_wire_gate_contact(
                points,
                routing_domain,
                gate.gate_type,
                gate.origin,
                gate.routing_domain,
                physical,
            )? {
                Ok(()) => {}
                Err(reason) => return Ok(Err(reason)),
            }
        }
        Ok(Ok(()))
    }

    fn bind_port(
        &mut self,
        command: BindPortCommand,
        frontier: EntityId,
        physical: &PhysicalScaleProfile,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if self.wire_is_occupied(command.wire) {
            return Ok(Err(Rejection::TrackOccupied));
        }
        let location = match self.reference_location(command.wire.entity_id(), frontier) {
            Ok(location) => location,
            Err(reason) => return Ok(Err(reason)),
        };
        if let Some(reason) = self.endpoint_lifecycle_rejection(command.target, frontier) {
            return Ok(Err(reason));
        }
        let EntityLocation::Wire(index) = location else {
            return Ok(Err(Rejection::InvalidPortBinding));
        };
        let wire = self
            .wires
            .get(index)
            .ok_or(StructuralError::InvalidCanonicalState)?;
        let old_target = self
            .wires
            .endpoint(index, command.end)
            .ok_or(StructuralError::InvalidCanonicalState)?;
        if old_target == command.target {
            return Ok(Ok(None));
        }
        let endpoint = match command.end {
            WireEnd::A => wire.points[0],
            WireEnd::B => *wire
                .points
                .last()
                .ok_or(StructuralError::InvalidCanonicalState)?,
        };
        match self.validate_wire_gate_contacts(wire.points, wire.routing_domain, physical)? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        match self.validate_endpoint(
            command.target,
            endpoint,
            wire.routing_domain,
            frontier,
            physical,
        )? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        let (endpoint_a, endpoint_b) = match command.end {
            WireEnd::A => (command.target, wire.endpoint_b),
            WireEnd::B => (wire.endpoint_a, command.target),
        };
        if open_world_wire_binds_same_junction(wire.routing_domain, endpoint_a, endpoint_b) {
            return Ok(Err(Rejection::InvalidPortBinding));
        }

        mark_target_junction(old_target, &self.entities, &mut changes.dirty_junctions)?;
        mark_target_junction(command.target, &self.entities, &mut changes.dirty_junctions)?;
        self.wires
            .set_endpoint(index, command.end, command.target)?;
        changes.dirty_wires.insert(index);
        changes.topology_changed = true;
        Ok(Ok(None))
    }

    fn remove_entity(
        &mut self,
        command: RemoveEntityCommand,
        frontier: EntityId,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        let location = match self.reference_location(command.target, frontier) {
            Ok(location) => location,
            Err(reason) => return Ok(Err(reason)),
        };
        match location {
            EntityLocation::Gate(index) => self.remove_gate(index, command.target, changes)?,
            EntityLocation::Wire(index) => {
                if self.wire_is_occupied(WireId(command.target)) {
                    return Ok(Err(Rejection::TrackOccupied));
                }
                self.remove_wire(index, command.target, changes)?
            }
            EntityLocation::Junction(index) => {
                if self.junction_is_occupied(JunctionId(command.target))? {
                    return Ok(Err(Rejection::TrackOccupied));
                }
                self.remove_junction(index, command.target, changes)?
            }
            EntityLocation::FixedSubstrate(index) => {
                if self.substrate_is_in_use(command.target) {
                    return Ok(Err(Rejection::SubstrateInUse));
                }
                self.fixed_substrates.remove(index)?;
                self.entities.remove(command.target)?;
            }
            EntityLocation::MobileSubstrate(index) => {
                if self.mobile_substrate_is_in_use(command.target) {
                    return Ok(Err(Rejection::SubstrateInUse));
                }
                self.mobile_substrates.remove(index)?;
                self.entities.remove(command.target)?;
                changes.topology_changed = true;
            }
            _ => return Ok(Err(Rejection::UnsupportedCommand)),
        }
        Ok(Ok(None))
    }

    fn remove_gate(
        &mut self,
        index: GateIndex,
        id: EntityId,
        changes: &mut PhaseChanges,
    ) -> Result<(), StructuralError> {
        let gate_id = GateId(id);
        let affected: Vec<_> = self
            .wires
            .iter_alive()
            .filter_map(|(wire_index, wire)| {
                let a = matches!(wire.endpoint_a, EndpointTarget::GatePort(reference) if reference.gate == gate_id);
                let b = matches!(wire.endpoint_b, EndpointTarget::GatePort(reference) if reference.gate == gate_id);
                (a || b).then_some((wire_index, a, b))
            })
            .collect();
        for (wire_index, endpoint_a, endpoint_b) in affected {
            if endpoint_a {
                self.wires
                    .set_endpoint(wire_index, WireEnd::A, EndpointTarget::Free)?;
            }
            if endpoint_b {
                self.wires
                    .set_endpoint(wire_index, WireEnd::B, EndpointTarget::Free)?;
            }
            changes.dirty_wires.insert(wire_index);
        }
        self.gates.remove(index)?;
        self.entities.remove(id)?;
        changes.topology_changed = true;
        Ok(())
    }

    fn remove_junction(
        &mut self,
        index: JunctionIndex,
        id: EntityId,
        changes: &mut PhaseChanges,
    ) -> Result<(), StructuralError> {
        let junction_id = JunctionId(id);
        let affected: Vec<_> = self
            .wires
            .iter_alive()
            .filter_map(|(wire_index, wire)| {
                let a = wire.endpoint_a == EndpointTarget::Junction(junction_id);
                let b = wire.endpoint_b == EndpointTarget::Junction(junction_id);
                (a || b).then_some((wire_index, a, b))
            })
            .collect();
        for (wire_index, endpoint_a, endpoint_b) in affected {
            if endpoint_a {
                self.wires
                    .set_endpoint(wire_index, WireEnd::A, EndpointTarget::Free)?;
            }
            if endpoint_b {
                self.wires
                    .set_endpoint(wire_index, WireEnd::B, EndpointTarget::Free)?;
            }
            changes.dirty_wires.insert(wire_index);
        }
        self.junctions.remove(index)?;
        self.entities.remove(id)?;
        changes.topology_changed = true;
        Ok(())
    }

    fn remove_wire(
        &mut self,
        index: WireIndex,
        id: EntityId,
        changes: &mut PhaseChanges,
    ) -> Result<(), StructuralError> {
        let wire = self
            .wires
            .get(index)
            .ok_or(StructuralError::InvalidCanonicalState)?;
        let endpoints = (wire.endpoint_a, wire.endpoint_b);
        mark_target_junction(endpoints.0, &self.entities, &mut changes.dirty_junctions)?;
        mark_target_junction(endpoints.1, &self.entities, &mut changes.dirty_junctions)?;
        self.wires.remove(index)?;
        self.entities.remove(id)?;
        changes.topology_changed = true;
        Ok(())
    }

    fn substrate_is_in_use(&self, substrate: EntityId) -> bool {
        let domain = RoutingDomain::FixedSubstrate(substrate);
        self.gates
            .iter_alive()
            .any(|(_, record)| record.routing_domain == domain)
            || self
                .wires
                .iter_alive()
                .any(|(_, record)| record.routing_domain == domain)
            || self
                .junctions
                .iter_alive()
                .any(|(_, record)| record.routing_domain == domain)
    }

    fn mobile_substrate_is_in_use(&self, substrate: EntityId) -> bool {
        let domain = RoutingDomain::MobileSubstrate(substrate);
        self.gates
            .iter_alive()
            .any(|(_, record)| record.routing_domain == domain)
            || self
                .wires
                .iter_alive()
                .any(|(_, record)| record.routing_domain == domain)
            || self
                .junctions
                .iter_alive()
                .any(|(_, record)| record.routing_domain == domain)
    }

    fn wire_is_occupied(&self, wire: WireId) -> bool {
        self.mobile_substrates
            .iter_alive()
            .any(|(_, mobile)| match mobile.track_position {
                TrackPosition::Edge { edge, .. } => edge == wire,
                TrackPosition::Junction { incoming_edge, .. } => incoming_edge == wire,
            })
    }

    fn junction_is_occupied(&self, junction: JunctionId) -> Result<bool, StructuralError> {
        for (_, mobile) in self.mobile_substrates.iter_alive() {
            match mobile.track_position {
                TrackPosition::Junction {
                    junction: occupied, ..
                } => {
                    if occupied == junction {
                        return Ok(true);
                    }
                }
                TrackPosition::Edge { edge, offset, .. } => {
                    let Some(EntityLocation::Wire(index)) =
                        self.entities.location(edge.entity_id()).copied()
                    else {
                        return Err(StructuralError::InvalidCanonicalState);
                    };
                    let wire = self
                        .wires
                        .get(index)
                        .ok_or(StructuralError::InvalidCanonicalState)?;
                    let length = polyline_length(wire.points)?;
                    if (offset == Fixed::ZERO
                        && wire.endpoint_a == EndpointTarget::Junction(junction))
                        || (offset == length
                            && wire.endpoint_b == EndpointTarget::Junction(junction))
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

fn point_is_quantized(point: FixedVec2, quantum: Fixed) -> bool {
    quantum.0 > 0 && point.x.0.rem_euclid(quantum.0) == 0 && point.y.0.rem_euclid(quantum.0) == 0
}

fn open_world_wire_binds_same_junction(
    domain: RoutingDomain,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> bool {
    domain == RoutingDomain::OpenWorld
        && matches!(
            (endpoint_a, endpoint_b),
            (EndpointTarget::Junction(a), EndpointTarget::Junction(b)) if a == b
        )
}

fn aabb_is_quantized(aabb: FixedAabb, quantum: Fixed) -> bool {
    point_is_quantized(aabb.min, quantum) && point_is_quantized(aabb.max, quantum)
}

fn gate_footprint(gate_type: GateType, physical: &PhysicalScaleProfile) -> GateFootprint {
    match gate_type {
        GateType::And => physical.gate_footprints.and_gate,
        GateType::Or => physical.gate_footprints.or_gate,
        GateType::Not => physical.gate_footprints.not_gate,
    }
}

fn gate_aabb(
    origin: FixedVec2,
    gate_type: GateType,
    physical: &PhysicalScaleProfile,
) -> Result<FixedAabb, StructuralError> {
    let footprint = gate_footprint(gate_type, physical);
    let half = FixedVec2::new(Fixed(footprint.width.0 / 2), Fixed(footprint.height.0 / 2));
    Ok(FixedAabb::new(
        checked_sub_point(origin, half)?,
        checked_add_point(origin, half)?,
    ))
}

fn routing_pitch(domain: RoutingDomain, physical: &PhysicalScaleProfile) -> Fixed {
    match domain {
        RoutingDomain::OpenWorld => physical.world_routing_pitch,
        RoutingDomain::FixedSubstrate(_) | RoutingDomain::MobileSubstrate(_) => {
            physical.circuit_routing_pitch
        }
    }
}

fn segment_physical_endpoints(
    points: &[FixedVec2],
    segment_index: usize,
) -> [Option<FixedVec2>; 2] {
    [
        (segment_index == 0).then_some(points[0]),
        (segment_index + 2 == points.len()).then(|| points[points.len() - 1]),
    ]
}

fn segments_share_physical_wire_endpoint(
    command: &PlaceWireCommand,
    command_segment: usize,
    existing: WireRecord<'_>,
    existing_segment: usize,
) -> bool {
    segments_share_endpoint_from_points(
        &command.points,
        command_segment,
        existing.points,
        existing_segment,
    )
}

fn segments_share_endpoint_from_points(
    first_points: &[FixedVec2],
    first_segment: usize,
    second_points: &[FixedVec2],
    second_segment: usize,
) -> bool {
    let new_endpoints = segment_physical_endpoints(first_points, first_segment);
    let existing_endpoints = segment_physical_endpoints(second_points, second_segment);
    new_endpoints.into_iter().flatten().any(|new_endpoint| {
        existing_endpoints
            .into_iter()
            .flatten()
            .any(|existing_endpoint| existing_endpoint == new_endpoint)
    })
}

fn mark_target_junction(
    target: EndpointTarget,
    entities: &EntityRegistry,
    dirty: &mut BTreeSet<JunctionIndex>,
) -> Result<(), StructuralError> {
    let EndpointTarget::Junction(id) = target else {
        return Ok(());
    };
    let Some(EntityLocation::Junction(index)) = entities.location(id.entity_id()).copied() else {
        return Err(StructuralError::InvalidCanonicalState);
    };
    dirty.insert(index);
    Ok(())
}

fn gate_port_anchor(
    gate_type: GateType,
    port: GatePort,
    physical: &PhysicalScaleProfile,
) -> Option<PortAnchor> {
    match gate_type {
        GateType::And => binary_anchor(physical.gate_port_anchors.and_gate, port),
        GateType::Or => binary_anchor(physical.gate_port_anchors.or_gate, port),
        GateType::Not => match port {
            GatePort::InputA => Some(physical.gate_port_anchors.not_gate.input),
            GatePort::InputB => None,
            GatePort::Output => Some(physical.gate_port_anchors.not_gate.output),
            GatePort::Power => Some(physical.gate_port_anchors.not_gate.power),
        },
    }
}

fn binary_anchor(anchors: crate::BinaryGatePortAnchors, port: GatePort) -> Option<PortAnchor> {
    Some(match port {
        GatePort::InputA => anchors.input_a,
        GatePort::InputB => anchors.input_b,
        GatePort::Output => anchors.output,
        GatePort::Power => anchors.power,
    })
}

fn validate_wire_gate_contact(
    points: &[FixedVec2],
    wire_domain: RoutingDomain,
    gate_type: GateType,
    gate_origin: FixedVec2,
    gate_domain: RoutingDomain,
    physical: &PhysicalScaleProfile,
) -> Result<Result<(), Rejection>, StructuralError> {
    if wire_domain != gate_domain
        && (matches!(wire_domain, RoutingDomain::MobileSubstrate(_))
            || matches!(gate_domain, RoutingDomain::MobileSubstrate(_)))
    {
        return Ok(Ok(()));
    }
    let aabb = gate_aabb(gate_origin, gate_type, physical)?;
    for (segment_index, segment) in points.windows(2).enumerate() {
        if segment_intersects_aabb_interior(segment[0], segment[1], aabb)?
            || segment_overlaps_aabb_boundary(segment[0], segment[1], aabb)?
        {
            return Ok(Err(Rejection::GeometryOverlap));
        }
        if segment_touches_aabb_boundary(segment[0], segment[1], aabb)?
            && !gate_boundary_contact_is_profile_anchor(
                points,
                segment_index,
                wire_domain,
                gate_type,
                gate_origin,
                gate_domain,
                physical,
            )?
        {
            return Ok(Err(Rejection::InvalidPortBinding));
        }
    }
    Ok(Ok(()))
}

fn gate_boundary_contact_is_profile_anchor(
    points: &[FixedVec2],
    segment_index: usize,
    wire_domain: RoutingDomain,
    gate_type: GateType,
    gate_origin: FixedVec2,
    gate_domain: RoutingDomain,
    physical: &PhysicalScaleProfile,
) -> Result<bool, StructuralError> {
    if wire_domain != gate_domain {
        return Ok(false);
    }
    if segment_index == 0 && point_is_gate_anchor(points[0], gate_type, gate_origin, physical)? {
        return Ok(true);
    }
    if segment_index + 2 == points.len()
        && point_is_gate_anchor(
            *points
                .last()
                .ok_or(StructuralError::InvalidCanonicalState)?,
            gate_type,
            gate_origin,
            physical,
        )?
    {
        return Ok(true);
    }
    Ok(false)
}

fn point_is_gate_anchor(
    point: FixedVec2,
    gate_type: GateType,
    gate_origin: FixedVec2,
    physical: &PhysicalScaleProfile,
) -> Result<bool, StructuralError> {
    for port in [
        GatePort::InputA,
        GatePort::InputB,
        GatePort::Output,
        GatePort::Power,
    ] {
        let Some(anchor) = gate_port_anchor(gate_type, port, physical) else {
            continue;
        };
        let anchor = checked_add_point(gate_origin, FixedVec2::new(anchor.x, anchor.y))?;
        if point == anchor {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BindPortCommand, Command, CommandEnvelope, ConnectionGeneration, PlaceJunctionCommand,
        PlaceWireCommand,
    };

    const WORLD_PITCH: i64 = 65_536;

    const fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn envelope(tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
        CommandEnvelope {
            target_tick: Tick(tick),
            ordinal,
            command,
        }
    }

    fn reference_physical() -> PhysicalScaleProfile {
        PhysicalScaleProfile::stage0_alpha("structural-test")
    }

    fn world_with_free_wire() -> StructuralWorld {
        let mut world = StructuralWorld::new();
        let physical = reference_physical();
        world
            .apply_phase0(
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceJunction(PlaceJunctionCommand {
                        routing_domain: RoutingDomain::OpenWorld,
                        position: point(0, 0),
                    }),
                )],
                &physical,
            )
            .expect("junction placement succeeds");
        world
            .apply_phase0(
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::PlaceWire(PlaceWireCommand {
                        routing_domain: RoutingDomain::OpenWorld,
                        points: vec![point(0, 0), point(WORLD_PITCH, 0)],
                        endpoint_a: EndpointTarget::Free,
                        endpoint_b: EndpointTarget::Free,
                    }),
                )],
                &physical,
            )
            .expect("wire placement succeeds");
        world
    }

    fn bind_to_fixture_junction() -> CommandEnvelope {
        envelope(
            2,
            0,
            Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(2)),
                end: WireEnd::A,
                target: EndpointTarget::Junction(JunctionId(EntityId(1))),
            }),
        )
    }

    #[test]
    fn entity_id_exhaustion_rolls_back_the_structural_transaction() {
        let mut world = StructuralWorld::new();
        world.entities.force_next_id_for_test(EntityId(u64::MAX));
        let before = world.clone();

        assert_eq!(
            world.apply_phase0(
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceJunction(PlaceJunctionCommand {
                        routing_domain: RoutingDomain::OpenWorld,
                        position: point(0, 0),
                    }),
                )],
                &reference_physical(),
            ),
            Err(StructuralError::NumericOverflow)
        );
        assert_eq!(world, before);
    }

    #[test]
    fn wire_generation_overflow_rolls_back_the_structural_transaction() {
        let mut world = world_with_free_wire();
        let EntityLocation::Wire(index) = *world
            .entities
            .location(EntityId(2))
            .expect("fixture wire is live")
        else {
            panic!("fixture EntityId 2 must be a Wire");
        };
        world
            .wires
            .force_generation_for_test(index, ConnectionGeneration(u64::MAX))
            .expect("fixture generation is set");
        let before = world.clone();

        assert_eq!(
            world.apply_phase0(
                Tick(2),
                &[bind_to_fixture_junction()],
                &reference_physical(),
            ),
            Err(StructuralError::NumericOverflow)
        );
        assert_eq!(world, before);
    }

    #[test]
    fn junction_generation_overflow_rolls_back_after_wire_preflight_work() {
        let mut world = world_with_free_wire();
        let EntityLocation::Junction(index) = *world
            .entities
            .location(EntityId(1))
            .expect("fixture junction is live")
        else {
            panic!("fixture EntityId 1 must be a Junction");
        };
        world
            .junctions
            .force_generation_for_test(index, ConnectionGeneration(u64::MAX))
            .expect("fixture generation is set");
        let before = world.clone();

        assert_eq!(
            world.apply_phase0(
                Tick(2),
                &[bind_to_fixture_junction()],
                &reference_physical(),
            ),
            Err(StructuralError::NumericOverflow)
        );
        assert_eq!(world, before);
    }
}
