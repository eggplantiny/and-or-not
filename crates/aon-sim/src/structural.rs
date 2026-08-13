use crate::command::{
    BindPortCommand, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, SetExternalDriverCommand,
};
use crate::construction::{
    ConstructionError, ConstructionSite, ConstructionSiteStore, ConstructionTarget,
    required_construction_work,
};
#[cfg(test)]
use crate::identity::FixedSubstrateIndex;
use crate::identity::{
    EntityLocation, EntityRegistry, EntityRegistryError, GateId, GateIndex, JunctionId,
    JunctionIndex, MobileId, MobileSubstrateIndex, WireId, WireIndex,
};
use crate::main_core::MainCoreAnchorView;
use crate::mobility::{
    MobileSubstrateRecord, MobileSubstrateStore, TrackGraph, TrackGraphError, TrackPosition,
};
use crate::path_certificate::PathElementStamp;
use crate::profile::{
    ConstructionProbeProfile, GateFootprint, MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA,
    PhysicalScaleProfile, PortAnchor, PrimitiveIntegrityProfile,
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
use crate::{
    ConstructionSiteId, DamageState, DriverId, EntityId, Fixed, FixedVec2, HeatEnergy, Integrity,
    MainCoreId, NumericError, Tick, polyline_length,
};
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

impl From<ConstructionError> for StructuralError {
    fn from(error: ConstructionError) -> Self {
        match error {
            ConstructionError::ArithmeticOverflow
            | ConstructionError::WorkOutOfRange { .. }
            | ConstructionError::StoreIndexExhausted
            | ConstructionError::Power(_) => Self::NumericOverflow,
            ConstructionError::UnsupportedTarget
            | ConstructionError::NonPositiveExtent { .. }
            | ConstructionError::NegativeLength { .. }
            | ConstructionError::DuplicateContribution { .. }
            | ConstructionError::DuplicateSite { .. }
            | ConstructionError::UnknownStoreIndex { .. }
            | ConstructionError::UnknownSite { .. }
            | ConstructionError::SiteAlreadyReady { .. }
            | ConstructionError::InvalidConstructionAttachment { .. } => {
                Self::InvalidCanonicalState
            }
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

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StructuralDestructionKind {
    Damage = 0,
    TrackSupportLost = 1,
    SubstrateSupportLost = 2,
    ConstructionDependencyLost = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StructuralDestructionRecord {
    pub target: EntityId,
    pub kind: StructuralDestructionKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructuralDestructionBatch {
    pub records: Vec<StructuralDestructionRecord>,
    pub topology_changed: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PowerSourceAnchorView {
    pub id: crate::PowerSourceId,
    pub position: FixedVec2,
}

#[derive(Clone, Copy)]
pub(crate) struct StructuralCommandContext<'a> {
    pub physical: &'a PhysicalScaleProfile,
    pub main_core: Option<MainCoreAnchorView>,
    pub power_sources: &'a [PowerSourceAnchorView],
    pub sensing_enabled: bool,
    pub construction_probe: Option<&'a ConstructionProbeProfile>,
    pub initial_integrity: Option<&'a PrimitiveIntegrityProfile>,
}

#[derive(Default)]
struct PhaseChanges {
    dirty_wires: BTreeSet<WireIndex>,
    dirty_junctions: BTreeSet<JunctionIndex>,
    topology_changed: bool,
}

impl StructuralWorld {
    fn place_construction_site(
        &mut self,
        target: &ConstructionTarget,
        frontier: EntityId,
        context: StructuralCommandContext<'_>,
        sites: &mut ConstructionSiteStore,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        let Some(probe) = context.construction_probe else {
            return Ok(Err(Rejection::UnsupportedPlacement));
        };
        let mut reservation = self.clone();
        let mut ignored_changes = PhaseChanges::default();
        for existing in sites.iter() {
            match reservation.place_construction_target_as_active(
                &existing.target,
                frontier,
                context,
                &mut ignored_changes,
            )? {
                Ok(_) => {}
                Err(_) => return Err(StructuralError::InvalidCanonicalState),
            }
        }
        match reservation.place_construction_target_as_active(
            target,
            frontier,
            context,
            &mut ignored_changes,
        )? {
            Ok(_) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        let required_work = required_construction_work(target, probe)?;
        let id = ConstructionSiteId(self.entities.next_id());
        let index = sites.insert_with_index(ConstructionSite {
            id,
            target: target.clone(),
            required_work,
            completed_work: crate::Energy(0),
            activation_ready: false,
        })?;
        let allocated = self
            .entities
            .allocate(EntityLocation::ConstructionSite(index))?;
        if allocated != id.entity_id() {
            return Err(StructuralError::InvalidCanonicalState);
        }
        Ok(Ok(Some(allocated)))
    }

    fn validate_direct_placement_reservation(
        &self,
        command: &Command,
        sites: &ConstructionSiteStore,
        frontier: EntityId,
        context: StructuralCommandContext<'_>,
    ) -> Result<Result<(), Rejection>, StructuralError> {
        let mut reservation = self.clone();
        let mut changes = PhaseChanges::default();
        for site in sites.iter() {
            match reservation.place_construction_target_as_active(
                &site.target,
                frontier,
                context,
                &mut changes,
            )? {
                Ok(_) => {}
                Err(_) => return Err(StructuralError::InvalidCanonicalState),
            }
        }
        let result = match command {
            Command::PlaceGate(command) => {
                reservation.place_gate(*command, frontier, context.physical, None, &mut changes)?
            }
            Command::PlaceWire(command) => {
                reservation.place_wire(command, frontier, context, &mut changes)?
            }
            Command::PlaceJunction(command) => reservation.place_junction(
                *command,
                frontier,
                context.physical,
                None,
                &mut changes,
            )?,
            Command::PlaceFixedSubstrate(command) => {
                reservation.place_fixed_substrate(*command, context.physical, None)?
            }
            Command::PlaceMobileSubstrate(command) => reservation.place_mobile_substrate(
                *command,
                context.physical,
                None,
                &mut changes,
            )?,
            _ => return Ok(Ok(())),
        };
        Ok(result.map(|_| ()))
    }

    fn place_construction_target_as_active(
        &mut self,
        target: &ConstructionTarget,
        frontier: EntityId,
        context: StructuralCommandContext<'_>,
        changes: &mut PhaseChanges,
    ) -> Result<Result<EntityId, Rejection>, StructuralError> {
        let result = match target {
            ConstructionTarget::Gate {
                gate_type,
                origin,
                routing_domain,
            } => self.place_gate(
                PlaceGateCommand {
                    gate_type: *gate_type,
                    origin: *origin,
                    routing_domain: *routing_domain,
                },
                frontier,
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.gate)),
                changes,
            )?,
            ConstructionTarget::Wire {
                routing_domain,
                points,
                endpoint_a,
                endpoint_b,
            } => self.place_wire(
                &PlaceWireCommand {
                    routing_domain: *routing_domain,
                    points: points.clone(),
                    endpoint_a: *endpoint_a,
                    endpoint_b: *endpoint_b,
                },
                frontier,
                context,
                changes,
            )?,
            ConstructionTarget::Junction {
                routing_domain,
                position,
            } => self.place_junction(
                PlaceJunctionCommand {
                    routing_domain: *routing_domain,
                    position: *position,
                },
                frontier,
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.junction)),
                changes,
            )?,
            ConstructionTarget::FixedSubstrate {
                origin,
                routing_area,
                footprint,
            } => self.place_fixed_substrate(
                PlaceFixedSubstrateCommand {
                    origin: *origin,
                    routing_area: *routing_area,
                    footprint: *footprint,
                },
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.fixed_substrate)),
            )?,
        };
        Ok(match result {
            Ok(Some(id)) => Ok(id),
            Ok(None) => return Err(StructuralError::InvalidCanonicalState),
            Err(reason) => Err(reason),
        })
    }

    fn activate_ready_sites(
        &mut self,
        sites: &mut ConstructionSiteStore,
        mut signal: Option<&mut SignalWorld>,
        tick: Tick,
        context: StructuralCommandContext<'_>,
        changes: &mut PhaseChanges,
    ) -> Result<(), StructuralError> {
        let ready = sites
            .iter()
            .filter(|site| site.activation_ready)
            .map(|site| site.id)
            .collect::<Vec<_>>();
        for id in ready {
            let site = sites
                .get(id)
                .cloned()
                .ok_or(StructuralError::InvalidCanonicalState)?;
            let location = self
                .entities
                .location(id.entity_id())
                .copied()
                .ok_or(StructuralError::InvalidCanonicalState)?;
            let EntityLocation::ConstructionSite(index) = location else {
                return Err(StructuralError::InvalidCanonicalState);
            };
            if sites.remove_by_index(index)? != site {
                return Err(StructuralError::InvalidCanonicalState);
            }
            self.entities.remove(id.entity_id())?;
            let frontier = self.entities.next_id();
            let created = match self.place_construction_target_as_active(
                &site.target,
                frontier,
                context,
                changes,
            )? {
                Ok(created) => created,
                Err(_) => return Err(StructuralError::InvalidCanonicalState),
            };
            if let Some(signal) = signal.as_deref_mut() {
                activate_construction_target_signal(
                    signal,
                    &site.target,
                    created,
                    tick,
                    context.sensing_enabled,
                )?;
            }
        }
        Ok(())
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn new_with_main_core_registry_entry() -> Result<(Self, MainCoreId), StructuralError>
    {
        let mut world = Self::new();
        let id = MainCoreId(world.entities.allocate(EntityLocation::MainCore)?);
        Ok((world, id))
    }

    pub(crate) fn new_with_main_core_and_power_source_registry_entries(
        source_count: usize,
    ) -> Result<(Self, MainCoreId, Vec<crate::PowerSourceId>), StructuralError> {
        let (mut world, core) = Self::new_with_main_core_registry_entry()?;
        let mut sources = Vec::with_capacity(source_count);
        for index in 0..source_count {
            let index = u32::try_from(index).map_err(|_| StructuralError::NumericOverflow)?;
            let id = world
                .entities
                .allocate(EntityLocation::PowerSource(crate::PowerSourceIndex(index)))?;
            sources.push(crate::PowerSourceId(id));
        }
        Ok((world, core, sources))
    }

    pub(crate) fn new_with_main_core_power_source_and_enemy_registry_entries(
        source_count: usize,
        enemy_count: usize,
    ) -> Result<
        (
            Self,
            MainCoreId,
            Vec<crate::PowerSourceId>,
            Vec<crate::EnemyId>,
        ),
        StructuralError,
    > {
        let (mut world, core, sources) =
            Self::new_with_main_core_and_power_source_registry_entries(source_count)?;
        let mut enemies = Vec::with_capacity(enemy_count);
        for index in 0..enemy_count {
            let index = u32::try_from(index).map_err(|_| StructuralError::NumericOverflow)?;
            let id = world
                .entities
                .allocate(EntityLocation::Enemy(crate::EnemyIndex(index)))?;
            enemies.push(crate::EnemyId(id));
        }
        Ok((world, core, sources, enemies))
    }

    #[cfg(test)]
    pub fn apply_phase0(
        &mut self,
        tick: Tick,
        commands: &[CommandEnvelope],
        physical: &PhysicalScaleProfile,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        self.apply_phase0_internal(
            tick,
            commands,
            StructuralCommandContext {
                physical,
                main_core: None,
                power_sources: &[],
                sensing_enabled: false,
                construction_probe: None,
                initial_integrity: None,
            },
            None,
            None,
        )
    }

    #[cfg(test)]
    pub fn apply_phase0_with_signal(
        &mut self,
        signal: &mut SignalWorld,
        tick: Tick,
        commands: &[CommandEnvelope],
        physical: &PhysicalScaleProfile,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        let mut signal_working = signal.clone();
        let report = self.apply_phase0_internal(
            tick,
            commands,
            StructuralCommandContext {
                physical,
                main_core: None,
                power_sources: &[],
                sensing_enabled: false,
                construction_probe: None,
                initial_integrity: None,
            },
            Some(&mut signal_working),
            None,
        )?;
        *signal = signal_working;
        Ok(report)
    }

    /// S1-M4 Phase-0 entry point. Construction Sites are cloned with the structural/signal
    /// candidate and committed only after the complete batch succeeds.
    pub(crate) fn apply_phase0_s1m4(
        &mut self,
        signal: &mut SignalWorld,
        sites: &mut ConstructionSiteStore,
        tick: Tick,
        commands: &[CommandEnvelope],
        context: StructuralCommandContext<'_>,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        let mut signal_working = signal.clone();
        let mut sites_working = sites.clone();
        let report = self.apply_phase0_internal(
            tick,
            commands,
            context,
            Some(&mut signal_working),
            Some(&mut sites_working),
        )?;
        *signal = signal_working;
        *sites = sites_working;
        Ok(report)
    }

    fn apply_phase0_internal(
        &mut self,
        tick: Tick,
        commands: &[CommandEnvelope],
        context: StructuralCommandContext<'_>,
        mut signal: Option<&mut SignalWorld>,
        mut sites: Option<&mut ConstructionSiteStore>,
    ) -> Result<StructuralPhaseReport, StructuralError> {
        let mut working = self.clone();
        let mut report = StructuralPhaseReport::default();
        let mut changes = PhaseChanges::default();
        if let Some(sites) = sites.as_deref_mut() {
            working.activate_ready_sites(
                sites,
                signal.as_deref_mut(),
                tick,
                context,
                &mut changes,
            )?;
        }
        let batch_frontier = working.entities.next_id();
        let driver_frontier = signal.as_deref().map(SignalWorld::driver_frontier);
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
                command => working.apply_command(
                    command,
                    batch_frontier,
                    context,
                    sites.as_deref_mut(),
                    &mut changes,
                ),
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
                    context.sensing_enabled,
                    context.construction_probe.is_some(),
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
        context: StructuralCommandContext<'_>,
        sites: Option<&mut ConstructionSiteStore>,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        if is_direct_placement(command)
            && let Some(sites) = sites.as_deref()
        {
            match self.validate_direct_placement_reservation(command, sites, frontier, context)? {
                Ok(()) => {}
                Err(reason) => return Ok(Err(reason)),
            }
        }
        match command {
            Command::PlaceGate(command) => self.place_gate(
                *command,
                frontier,
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.gate)),
                changes,
            ),
            Command::PlaceWire(command) => self.place_wire(command, frontier, context, changes),
            Command::PlaceJunction(command) => self.place_junction(
                *command,
                frontier,
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.junction)),
                changes,
            ),
            Command::PlaceFixedSubstrate(command) => self.place_fixed_substrate(
                *command,
                context.physical,
                damage_state(context.initial_integrity.map(|table| table.fixed_substrate)),
            ),
            Command::PlaceMobileSubstrate(command) => self.place_mobile_substrate(
                *command,
                context.physical,
                damage_state(
                    context
                        .initial_integrity
                        .map(|table| table.mobile_substrate),
                ),
                changes,
            ),
            Command::RemoveEntity(command) => {
                self.remove_entity(*command, frontier, sites, changes)
            }
            Command::BindPort(command) => self.bind_port(*command, frontier, context, changes),
            Command::SetExternalDriver(_) => Ok(Err(Rejection::UnsupportedCommand)),
            Command::PlaceConstructionSite(command) => match sites {
                Some(sites) => {
                    self.place_construction_site(&command.target, frontier, context, sites)
                }
                None => Ok(Err(Rejection::UnsupportedPlacement)),
            },
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

    pub(crate) fn damage_state(
        &self,
        id: EntityId,
    ) -> Option<(crate::ThermalObjectKind, DamageState)> {
        match self.entities.location(id).copied()? {
            EntityLocation::Gate(index) => self
                .gates
                .get(index)?
                .damage_state
                .map(|state| (crate::ThermalObjectKind::Gate, state)),
            EntityLocation::Wire(index) => self
                .wires
                .get(index)?
                .damage_state
                .map(|state| (crate::ThermalObjectKind::Wire, state)),
            EntityLocation::Junction(index) => self
                .junctions
                .get(index)?
                .damage_state
                .map(|state| (crate::ThermalObjectKind::Junction, state)),
            EntityLocation::FixedSubstrate(index) => self
                .fixed_substrates
                .get(index)?
                .damage_state
                .map(|state| (crate::ThermalObjectKind::FixedSubstrate, state)),
            EntityLocation::MobileSubstrate(index) => self
                .mobile_substrates
                .get(index)?
                .damage_state
                .map(|state| (crate::ThermalObjectKind::MobileSubstrate, state)),
            _ => None,
        }
    }

    pub(crate) fn damageable_structural_states(
        &self,
    ) -> impl Iterator<Item = (EntityId, crate::ThermalObjectKind, DamageState)> + '_ {
        self.entities
            .iter_alive()
            .filter_map(|(id, _)| self.damage_state(id).map(|(kind, state)| (id, kind, state)))
    }

    pub(crate) fn set_damage_state(
        &mut self,
        id: EntityId,
        state: DamageState,
    ) -> Result<(), StructuralError> {
        match self.entities.location(id).copied() {
            Some(EntityLocation::Gate(index)) => self.gates.set_damage_state(index, state)?,
            Some(EntityLocation::Wire(index)) => self.wires.set_damage_state(index, state)?,
            Some(EntityLocation::Junction(index)) => {
                self.junctions.set_damage_state(index, state)?
            }
            Some(EntityLocation::FixedSubstrate(index)) => {
                self.fixed_substrates.set_damage_state(index, state)?
            }
            Some(EntityLocation::MobileSubstrate(index)) => {
                self.mobile_substrates.set_damage_state(index, state)?
            }
            _ => return Err(StructuralError::InvalidCanonicalState),
        }
        Ok(())
    }

    pub(crate) fn remove_enemy_registry_entry(
        &mut self,
        id: crate::EnemyId,
        expected_index: crate::EnemyIndex,
    ) -> Result<(), StructuralError> {
        if self.entities.location(id.entity_id()) != Some(&EntityLocation::Enemy(expected_index)) {
            return Err(StructuralError::InvalidCanonicalState);
        }
        self.entities.remove(id.entity_id())?;
        Ok(())
    }

    /// Selects the lowest-ID live Site whose closed reserved geometry touches the Phase-1 Mobile
    /// footprint. The caller supplies the already-resolved Phase-1 world origin.
    pub(crate) fn smallest_intersecting_site(
        &self,
        sites: &ConstructionSiteStore,
        mobile_world_origin: FixedVec2,
        mobile_local_footprint: FixedAabb,
        track_graph: &TrackGraph,
        physical: &PhysicalScaleProfile,
    ) -> Result<Option<ConstructionSiteId>, StructuralError> {
        let mobile = mobile_local_footprint.translated(mobile_world_origin)?;
        for site in sites.iter() {
            if self.construction_target_intersects_aabb(
                &site.target,
                mobile,
                track_graph,
                physical,
            )? {
                return Ok(Some(site.id));
            }
        }
        Ok(None)
    }

    /// Resolves geometry stored in a routing domain into the world frame of the supplied Track
    /// snapshot.
    ///
    /// Open-world and Fixed-Substrate routed points are already absolute. Mobile-Substrate
    /// internal geometry is stored substrate-local and follows the substrate's Track position.
    pub(crate) fn routing_domain_points_world(
        &self,
        domain: RoutingDomain,
        points: &[FixedVec2],
        track_graph: &TrackGraph,
    ) -> Result<Vec<FixedVec2>, StructuralError> {
        let RoutingDomain::MobileSubstrate(id) = domain else {
            return Ok(points.to_vec());
        };
        let Some(EntityLocation::MobileSubstrate(index)) = self.entities.location(id).copied()
        else {
            return Err(StructuralError::InvalidCanonicalState);
        };
        let mobile = self
            .mobile_substrates
            .get(index)
            .ok_or(StructuralError::InvalidCanonicalState)?;
        let origin = track_graph.world_position(mobile.track_position)?;
        points
            .iter()
            .copied()
            .map(|point| checked_add_point(origin, point).map_err(StructuralError::from))
            .collect()
    }

    fn routing_domain_point_world(
        &self,
        domain: RoutingDomain,
        point: FixedVec2,
        track_graph: &TrackGraph,
    ) -> Result<FixedVec2, StructuralError> {
        self.routing_domain_points_world(domain, &[point], track_graph)?
            .into_iter()
            .next()
            .ok_or(StructuralError::InvalidCanonicalState)
    }

    fn construction_target_intersects_aabb(
        &self,
        target: &ConstructionTarget,
        aabb: FixedAabb,
        track_graph: &TrackGraph,
        physical: &PhysicalScaleProfile,
    ) -> Result<bool, StructuralError> {
        match target {
            ConstructionTarget::Gate {
                gate_type,
                origin,
                routing_domain,
            } => Ok(closed_aabb_intersects(
                gate_aabb(
                    self.routing_domain_point_world(*routing_domain, *origin, track_graph)?,
                    *gate_type,
                    physical,
                )?,
                aabb,
            )),
            ConstructionTarget::Junction {
                routing_domain,
                position,
            } => Ok(aabb.contains_point(self.routing_domain_point_world(
                *routing_domain,
                *position,
                track_graph,
            )?)),
            ConstructionTarget::FixedSubstrate {
                origin, footprint, ..
            } => Ok(closed_aabb_intersects(footprint.translated(*origin)?, aabb)),
            ConstructionTarget::Wire {
                routing_domain,
                points,
                ..
            } => {
                let radius = physical.wire_body_radius;
                if radius.0 < 0 {
                    return Err(StructuralError::InvalidCanonicalState);
                }
                let points =
                    self.routing_domain_points_world(*routing_domain, points, track_graph)?;
                for segment in points.windows(2) {
                    if aabb.contains_point(segment[0])
                        || aabb.contains_point(segment[1])
                        || aabb_edges(aabb).into_iter().try_fold(false, |hit, edge| {
                            if hit {
                                return Ok(true);
                            }
                            crate::swept_circle_intersects_wire_body(
                                edge[0],
                                edge[1],
                                radius,
                                segment,
                                Fixed::ZERO,
                            )
                            .map_err(|_| StructuralError::InvalidCanonicalState)
                        })?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Applies the Phase-0 structural destruction closure atomically. Enemy records are handled
    /// by their dedicated store; Enemy pending IDs are intentionally left in `pending`.
    pub(crate) fn apply_pending_structural_destructions(
        &mut self,
        signal: &mut SignalWorld,
        sites: &mut ConstructionSiteStore,
        pending: &mut BTreeSet<EntityId>,
    ) -> Result<StructuralDestructionBatch, StructuralError> {
        let mut working = self.clone();
        let mut signal_working = signal.clone();
        let mut sites_working = sites.clone();
        let mut pending_working = pending.clone();
        let batch = working.apply_pending_structural_destructions_internal(
            &mut signal_working,
            &mut sites_working,
            &mut pending_working,
        )?;
        *self = working;
        *signal = signal_working;
        *sites = sites_working;
        *pending = pending_working;
        Ok(batch)
    }

    fn apply_pending_structural_destructions_internal(
        &mut self,
        signal: &mut SignalWorld,
        sites: &mut ConstructionSiteStore,
        pending: &mut BTreeSet<EntityId>,
    ) -> Result<StructuralDestructionBatch, StructuralError> {
        let mut causes = BTreeMap::<EntityId, StructuralDestructionKind>::new();
        for &target in pending.iter() {
            match self.entities.location(target).copied() {
                Some(
                    EntityLocation::Gate(_)
                    | EntityLocation::Wire(_)
                    | EntityLocation::Junction(_)
                    | EntityLocation::FixedSubstrate(_)
                    | EntityLocation::MobileSubstrate(_),
                ) => {
                    causes.insert(target, StructuralDestructionKind::Damage);
                }
                Some(EntityLocation::Enemy(_)) => {}
                _ => return Err(StructuralError::InvalidCanonicalState),
            }
        }
        if causes.is_empty() {
            return Ok(StructuralDestructionBatch::default());
        }

        loop {
            let before = causes.len();
            for (_, mobile) in self.mobile_substrates.iter_alive() {
                let support = track_support(mobile.track_position);
                if causes.contains_key(&support) {
                    insert_min_cause(
                        &mut causes,
                        mobile.id.entity_id(),
                        StructuralDestructionKind::TrackSupportLost,
                    );
                }
            }
            for (_, gate) in self.gates.iter_alive() {
                cascade_substrate_dependency(&mut causes, gate.id.entity_id(), gate.routing_domain);
            }
            for (_, wire) in self.wires.iter_alive() {
                cascade_substrate_dependency(&mut causes, wire.id.entity_id(), wire.routing_domain);
            }
            for (_, junction) in self.junctions.iter_alive() {
                cascade_substrate_dependency(
                    &mut causes,
                    junction.id.entity_id(),
                    junction.routing_domain,
                );
            }
            for site in sites.iter() {
                if construction_target_dependencies(&site.target)
                    .into_iter()
                    .any(|dependency| causes.contains_key(&dependency))
                {
                    insert_min_cause(
                        &mut causes,
                        site.id.entity_id(),
                        StructuralDestructionKind::ConstructionDependencyLost,
                    );
                }
            }
            if causes.len() == before {
                break;
            }
        }

        let graph = destruction_dependency_graph(self, sites, &causes)?;
        let order = dependent_first_scc_order(&graph)?;
        let mut changes = PhaseChanges::default();
        for component in order {
            for target in component {
                let location = self
                    .entities
                    .location(target)
                    .copied()
                    .ok_or(StructuralError::InvalidCanonicalState)?;
                match location {
                    EntityLocation::Gate(index) => {
                        self.remove_gate(index, target, &mut changes)?;
                        signal.remove_gate(GateId(target))?;
                    }
                    EntityLocation::Wire(index) => {
                        self.remove_wire(index, target, &mut changes)?;
                        signal.remove_wire(WireId(target))?;
                    }
                    EntityLocation::Junction(index) => {
                        self.remove_junction(index, target, &mut changes)?;
                    }
                    EntityLocation::FixedSubstrate(index) => {
                        self.fixed_substrates.remove(index)?;
                        self.entities.remove(target)?;
                    }
                    EntityLocation::MobileSubstrate(index) => {
                        self.mobile_substrates.remove(index)?;
                        self.entities.remove(target)?;
                        signal.remove_mobile(MobileId(target))?;
                        changes.topology_changed = true;
                    }
                    EntityLocation::ConstructionSite(index) => {
                        let removed = sites.remove_by_index(index)?;
                        if removed.id.entity_id() != target {
                            return Err(StructuralError::InvalidCanonicalState);
                        }
                        self.entities.remove(target)?;
                    }
                    _ => return Err(StructuralError::InvalidCanonicalState),
                }
                pending.remove(&target);
            }
        }
        for index in changes.dirty_wires {
            if self.wires.get(index).is_some() {
                self.wires.advance_generation(index)?;
            }
        }
        for index in changes.dirty_junctions {
            if self.junctions.get(index).is_some() {
                self.junctions.advance_generation(index)?;
            }
        }
        let records = causes
            .into_iter()
            .map(|(target, kind)| StructuralDestructionRecord { target, kind })
            .collect();
        Ok(StructuralDestructionBatch {
            records,
            topology_changed: changes.topology_changed,
        })
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
    pub(crate) fn remove_registry_entry_for_test(
        &mut self,
        id: EntityId,
    ) -> Result<EntityLocation, StructuralError> {
        self.entities.remove(id).map_err(StructuralError::from)
    }

    #[cfg(test)]
    pub(crate) fn relocate_main_core_registry_for_test(
        &mut self,
    ) -> Result<MainCoreId, StructuralError> {
        self.entities.remove(crate::FIRST_ENTITY_ID)?;
        Ok(MainCoreId(
            self.entities.allocate(EntityLocation::MainCore)?,
        ))
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

const fn damage_state(integrity: Option<u64>) -> Option<DamageState> {
    match integrity {
        Some(integrity) => Some(DamageState::new(Integrity(integrity), HeatEnergy(0))),
        None => None,
    }
}

const fn is_direct_placement(command: &Command) -> bool {
    matches!(
        command,
        Command::PlaceGate(_)
            | Command::PlaceWire(_)
            | Command::PlaceJunction(_)
            | Command::PlaceFixedSubstrate(_)
            | Command::PlaceMobileSubstrate(_)
    )
}

fn construction_target_depends_on(target: &ConstructionTarget, entity: EntityId) -> bool {
    let domain_depends = match target {
        ConstructionTarget::Gate { routing_domain, .. }
        | ConstructionTarget::Wire { routing_domain, .. }
        | ConstructionTarget::Junction { routing_domain, .. } => {
            routing_domain_entity(*routing_domain) == Some(entity)
        }
        ConstructionTarget::FixedSubstrate { .. } => false,
    };
    domain_depends
        || matches!(
            target,
            ConstructionTarget::Wire {
                endpoint_a,
                endpoint_b,
                ..
            } if endpoint_entity(*endpoint_a) == Some(entity)
                || endpoint_entity(*endpoint_b) == Some(entity)
        )
}

fn construction_target_dependencies(target: &ConstructionTarget) -> Vec<EntityId> {
    let mut dependencies = Vec::new();
    match target {
        ConstructionTarget::Gate { routing_domain, .. }
        | ConstructionTarget::Junction { routing_domain, .. } => {
            if let Some(owner) = routing_domain_entity(*routing_domain) {
                dependencies.push(owner);
            }
        }
        ConstructionTarget::Wire {
            routing_domain,
            endpoint_a,
            endpoint_b,
            ..
        } => {
            if let Some(owner) = routing_domain_entity(*routing_domain) {
                dependencies.push(owner);
            }
            if let Some(owner) = endpoint_entity(*endpoint_a) {
                dependencies.push(owner);
            }
            if let Some(owner) = endpoint_entity(*endpoint_b) {
                dependencies.push(owner);
            }
        }
        ConstructionTarget::FixedSubstrate { .. } => {}
    }
    dependencies.sort_unstable();
    dependencies.dedup();
    dependencies
}

const fn track_support(position: TrackPosition) -> EntityId {
    match position {
        TrackPosition::Edge { edge, .. } => edge.entity_id(),
        TrackPosition::Junction { junction, .. } => junction.entity_id(),
    }
}

fn insert_min_cause(
    causes: &mut BTreeMap<EntityId, StructuralDestructionKind>,
    target: EntityId,
    cause: StructuralDestructionKind,
) {
    causes
        .entry(target)
        .and_modify(|current| *current = (*current).min(cause))
        .or_insert(cause);
}

fn cascade_substrate_dependency(
    causes: &mut BTreeMap<EntityId, StructuralDestructionKind>,
    dependent: EntityId,
    domain: RoutingDomain,
) {
    if routing_domain_entity(domain).is_some_and(|owner| causes.contains_key(&owner)) {
        insert_min_cause(
            causes,
            dependent,
            StructuralDestructionKind::SubstrateSupportLost,
        );
    }
}

fn destruction_dependency_graph(
    world: &StructuralWorld,
    sites: &ConstructionSiteStore,
    causes: &BTreeMap<EntityId, StructuralDestructionKind>,
) -> Result<BTreeMap<EntityId, BTreeSet<EntityId>>, StructuralError> {
    let mut graph = causes
        .keys()
        .copied()
        .map(|id| (id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for &dependent in causes.keys() {
        let dependencies = match world.entities.location(dependent).copied() {
            Some(EntityLocation::Gate(index)) => vec![routing_domain_entity(
                world
                    .gates
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?
                    .routing_domain,
            )],
            Some(EntityLocation::Wire(index)) => vec![routing_domain_entity(
                world
                    .wires
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?
                    .routing_domain,
            )],
            Some(EntityLocation::Junction(index)) => vec![routing_domain_entity(
                world
                    .junctions
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?
                    .routing_domain,
            )],
            Some(EntityLocation::MobileSubstrate(index)) => vec![Some(track_support(
                world
                    .mobile_substrates
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?
                    .track_position,
            ))],
            Some(EntityLocation::ConstructionSite(index)) => {
                let site = sites
                    .get_by_index(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?;
                construction_target_dependencies(&site.target)
                    .into_iter()
                    .map(Some)
                    .collect()
            }
            Some(EntityLocation::FixedSubstrate(_)) => Vec::new(),
            _ => return Err(StructuralError::InvalidCanonicalState),
        };
        let edges = graph
            .get_mut(&dependent)
            .ok_or(StructuralError::InvalidCanonicalState)?;
        for dependency in dependencies.into_iter().flatten() {
            if causes.contains_key(&dependency) {
                edges.insert(dependency);
            }
        }
    }
    Ok(graph)
}

fn dependent_first_scc_order(
    graph: &BTreeMap<EntityId, BTreeSet<EntityId>>,
) -> Result<Vec<Vec<EntityId>>, StructuralError> {
    let mut visited = BTreeSet::new();
    let mut finish = Vec::with_capacity(graph.len());
    for &start in graph.keys() {
        if visited.contains(&start) {
            continue;
        }
        let mut stack = vec![(start, false)];
        while let Some((node, exiting)) = stack.pop() {
            if exiting {
                finish.push(node);
                continue;
            }
            if !visited.insert(node) {
                continue;
            }
            stack.push((node, true));
            let neighbors = graph
                .get(&node)
                .ok_or(StructuralError::InvalidCanonicalState)?;
            for &neighbor in neighbors.iter().rev() {
                if !visited.contains(&neighbor) {
                    stack.push((neighbor, false));
                }
            }
        }
    }
    let mut reverse = graph
        .keys()
        .copied()
        .map(|id| (id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for (&source, targets) in graph {
        for &target in targets {
            reverse
                .get_mut(&target)
                .ok_or(StructuralError::InvalidCanonicalState)?
                .insert(source);
        }
    }
    visited.clear();
    let mut components = Vec::<Vec<EntityId>>::new();
    for &start in finish.iter().rev() {
        if !visited.insert(start) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            for &neighbor in reverse
                .get(&node)
                .ok_or(StructuralError::InvalidCanonicalState)?
                .iter()
                .rev()
            {
                if visited.insert(neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    let component_of = components
        .iter()
        .enumerate()
        .flat_map(|(index, members)| members.iter().map(move |&id| (id, index)))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = vec![BTreeSet::<usize>::new(); components.len()];
    let mut incoming = vec![0_usize; components.len()];
    for (&source, targets) in graph {
        let source_component = component_of[&source];
        for &target in targets {
            let target_component = component_of[&target];
            if source_component != target_component
                && outgoing[source_component].insert(target_component)
            {
                incoming[target_component] = incoming[target_component]
                    .checked_add(1)
                    .ok_or(StructuralError::NumericOverflow)?;
            }
        }
    }
    let mut ready = components
        .iter()
        .enumerate()
        .filter(|(index, _)| incoming[*index] == 0)
        .map(|(index, members)| (members[0], index))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(components.len());
    while let Some(&(minimum, index)) = ready.iter().next() {
        ready.remove(&(minimum, index));
        ordered.push(components[index].clone());
        for &target in &outgoing[index] {
            incoming[target] = incoming[target]
                .checked_sub(1)
                .ok_or(StructuralError::InvalidCanonicalState)?;
            if incoming[target] == 0 {
                ready.insert((components[target][0], target));
            }
        }
    }
    if ordered.len() != components.len() {
        return Err(StructuralError::InvalidCanonicalState);
    }
    Ok(ordered)
}

const fn routing_domain_entity(domain: RoutingDomain) -> Option<EntityId> {
    match domain {
        RoutingDomain::OpenWorld => None,
        RoutingDomain::FixedSubstrate(id) | RoutingDomain::MobileSubstrate(id) => Some(id),
    }
}

const fn endpoint_entity(target: EndpointTarget) -> Option<EntityId> {
    match target {
        EndpointTarget::Free => None,
        EndpointTarget::Junction(id) => Some(id.entity_id()),
        EndpointTarget::GatePort(reference) => Some(reference.gate.entity_id()),
        EndpointTarget::MobilePort(reference) => Some(reference.mobile.entity_id()),
        EndpointTarget::MainCoreAnchor(id) => Some(id.entity_id()),
        EndpointTarget::PowerSourceAnchor(id) => Some(id.entity_id()),
        EndpointTarget::WireSensePort(reference) => Some(reference.wire.entity_id()),
    }
}

fn activate_construction_target_signal(
    signal: &mut SignalWorld,
    target: &ConstructionTarget,
    created: EntityId,
    tick: Tick,
    sensing_enabled: bool,
) -> Result<(), StructuralError> {
    match target {
        ConstructionTarget::Gate { gate_type, .. } => {
            signal.activate_gate(GateId(created), *gate_type, tick)?;
        }
        ConstructionTarget::Wire { .. } => {
            let wire = WireId(created);
            signal.activate_wire(wire)?;
            if sensing_enabled {
                signal.activate_wire_sensing(wire, tick)?;
            }
        }
        ConstructionTarget::Junction { .. } | ConstructionTarget::FixedSubstrate { .. } => {}
    }
    Ok(())
}

const fn closed_aabb_intersects(left: FixedAabb, right: FixedAabb) -> bool {
    left.min.x.0 <= right.max.x.0
        && right.min.x.0 <= left.max.x.0
        && left.min.y.0 <= right.max.y.0
        && right.min.y.0 <= left.max.y.0
}

const fn aabb_edges(aabb: FixedAabb) -> [[FixedVec2; 2]; 4] {
    let top_right = FixedVec2::new(aabb.max.x, aabb.min.y);
    let bottom_left = FixedVec2::new(aabb.min.x, aabb.max.y);
    [
        [aabb.min, top_right],
        [top_right, aabb.max],
        [aabb.max, bottom_left],
        [bottom_left, aabb.min],
    ]
}

fn apply_signal_lifecycle(
    signal: &mut SignalWorld,
    command: &Command,
    created_entity: Option<EntityId>,
    removed_location: Option<EntityLocation>,
    tick: Tick,
    sensing_enabled: bool,
    construction_enabled: bool,
) -> Result<(), StructuralError> {
    match command {
        Command::PlaceGate(command) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            signal.activate_gate(GateId(id), command.gate_type, tick)?;
        }
        Command::PlaceWire(_) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            let wire = WireId(id);
            signal.activate_wire(wire)?;
            if sensing_enabled {
                signal.activate_wire_sensing(wire, tick)?;
            }
        }
        Command::PlaceMobileSubstrate(_) => {
            let id = created_entity.ok_or(StructuralError::InvalidCanonicalState)?;
            if construction_enabled {
                signal.activate_mobile_with_build(MobileId(id), tick)?;
            } else {
                signal.activate_mobile(MobileId(id))?;
            }
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
        | Command::SetExternalDriver(_)
        | Command::PlaceConstructionSite(_) => {}
    }
    Ok(())
}

impl StructuralWorld {
    fn place_mobile_substrate(
        &mut self,
        command: PlaceMobileSubstrateCommand,
        physical: &PhysicalScaleProfile,
        damage_state: Option<DamageState>,
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
        let index = self.mobile_substrates.push_with_damage(
            id,
            track_position,
            command.routing_area,
            command.footprint,
            damage_state,
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
        damage_state: Option<DamageState>,
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
        let index = self.fixed_substrates.push_with_damage(
            id,
            command.origin,
            command.routing_area,
            command.footprint,
            damage_state,
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
        damage_state: Option<DamageState>,
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
        let index = self.gates.push_with_damage(
            id,
            command.gate_type,
            command.origin,
            command.routing_domain,
            damage_state,
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
        damage_state: Option<DamageState>,
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
        let index = self.junctions.push_with_damage(
            id,
            command.routing_domain,
            command.position,
            damage_state,
        )?;
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
            EndpointTarget::MainCoreAnchor(core) => core.entity_id(),
            EndpointTarget::PowerSourceAnchor(source) => source.entity_id(),
            EndpointTarget::WireSensePort(reference) => reference.wire.entity_id(),
        };
        self.reference_location(id, frontier).err()
    }
}

impl StructuralWorld {
    fn place_wire(
        &mut self,
        command: &PlaceWireCommand,
        frontier: EntityId,
        context: StructuralCommandContext<'_>,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        let physical = context.physical;
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
            context,
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
            context,
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
        let index = self.wires.push_with_damage(
            id,
            command.routing_domain,
            &command.points,
            command.endpoint_a,
            command.endpoint_b,
            damage_state(context.initial_integrity.map(|table| table.wire)),
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
        context: StructuralCommandContext<'_>,
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
                let Some(anchor) =
                    gate_port_anchor(gate.gate_type, reference.port, context.physical)
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
            EndpointTarget::MainCoreAnchor(core) => {
                let location = match self.reference_location(core.entity_id(), frontier) {
                    Ok(location) => location,
                    Err(reason) => return Ok(Err(reason)),
                };
                if location != EntityLocation::MainCore {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                let Some(anchor) = context.main_core else {
                    return Err(StructuralError::InvalidCanonicalState);
                };
                if anchor.id != core
                    || domain != RoutingDomain::OpenWorld
                    || anchor.position != endpoint
                {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                Ok(Ok(()))
            }
            // Power Sources are generator-owned infrastructure. Their exact structural view is
            // wired into Phase 0 by the S1-M2 world adapter; until then a player command must not
            // be allowed to invent or approximate an anchor position.
            EndpointTarget::PowerSourceAnchor(source) => {
                let location = match self.reference_location(source.entity_id(), frontier) {
                    Ok(location) => location,
                    Err(reason) => return Ok(Err(reason)),
                };
                if !matches!(location, EntityLocation::PowerSource(_)) {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                let Some(anchor) = context
                    .power_sources
                    .iter()
                    .find(|anchor| anchor.id == source)
                else {
                    return Err(StructuralError::InvalidCanonicalState);
                };
                // A Power Source anchor is the sole explicit bridge between routing domains.
                // The enclosing Wire has already passed its own domain/pitch/bounds validation;
                // the bridge still requires the exact immutable Source coordinate.
                if anchor.position != endpoint {
                    return Ok(Err(Rejection::InvalidEndpoint));
                }
                Ok(Ok(()))
            }
            EndpointTarget::WireSensePort(reference) => {
                let location = match self.reference_location(reference.wire.entity_id(), frontier) {
                    Ok(location) => location,
                    Err(reason) => return Ok(Err(reason)),
                };
                let EntityLocation::Wire(index) = location else {
                    return Ok(Err(Rejection::InvalidEndpoint));
                };
                let owner = self
                    .wires
                    .get(index)
                    .ok_or(StructuralError::InvalidCanonicalState)?;
                let owner_endpoint = match reference.end {
                    WireEnd::A => owner.points[0],
                    WireEnd::B => *owner
                        .points
                        .last()
                        .ok_or(StructuralError::InvalidCanonicalState)?,
                };
                if owner.routing_domain != domain || owner_endpoint != endpoint {
                    return Ok(Err(Rejection::InvalidPortBinding));
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
        context: StructuralCommandContext<'_>,
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
        if matches!(
            command.target,
            EndpointTarget::WireSensePort(reference) if reference.wire == command.wire
        ) {
            return Ok(Err(Rejection::InvalidPortBinding));
        }
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
        match self.validate_wire_gate_contacts(
            wire.points,
            wire.routing_domain,
            context.physical,
        )? {
            Ok(()) => {}
            Err(reason) => return Ok(Err(reason)),
        }
        match self.validate_endpoint(
            command.target,
            endpoint,
            wire.routing_domain,
            frontier,
            context,
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
        sites: Option<&mut ConstructionSiteStore>,
        changes: &mut PhaseChanges,
    ) -> Result<Result<Option<EntityId>, Rejection>, StructuralError> {
        let location = match self.reference_location(command.target, frontier) {
            Ok(location) => location,
            Err(reason) => return Ok(Err(reason)),
        };
        if !matches!(location, EntityLocation::ConstructionSite(_))
            && sites.as_deref().is_some_and(|sites| {
                sites
                    .iter()
                    .any(|site| construction_target_depends_on(&site.target, command.target))
            })
        {
            return Ok(Err(Rejection::ConstructionDependencyInUse));
        }
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
            EntityLocation::ConstructionSite(index) => {
                let Some(sites) = sites else {
                    return Err(StructuralError::InvalidCanonicalState);
                };
                let site = sites.remove_by_index(index)?;
                if site.id.entity_id() != command.target {
                    return Err(StructuralError::InvalidCanonicalState);
                }
                self.entities.remove(command.target)?;
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
        let wire_id = WireId(id);
        let affected: Vec<_> = self
            .wires
            .iter_alive()
            .filter_map(|(wire_index, wire)| {
                if wire.id == wire_id {
                    return None;
                }
                let a = matches!(
                    wire.endpoint_a,
                    EndpointTarget::WireSensePort(reference) if reference.wire == wire_id
                );
                let b = matches!(
                    wire.endpoint_b,
                    EndpointTarget::WireSensePort(reference) if reference.wire == wire_id
                );
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
    use crate::topology::WireSensePortRef;
    use crate::{
        BindPortCommand, Command, CommandEnvelope, ConnectionGeneration, PlaceJunctionCommand,
        PlaceWireCommand, RemoveEntityCommand,
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

    #[test]
    fn wire_sense_binding_requires_an_exact_live_other_wire_endpoint() {
        let physical = reference_physical();
        let mut world = StructuralWorld::new();
        let report = world
            .apply_phase0(
                Tick(0),
                &[
                    envelope(
                        0,
                        0,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![point(0, 0), point(WORLD_PITCH, 0)],
                            endpoint_a: EndpointTarget::Free,
                            endpoint_b: EndpointTarget::Free,
                        }),
                    ),
                    envelope(
                        0,
                        1,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![point(WORLD_PITCH, 0), point(2 * WORLD_PITCH, 0)],
                            endpoint_a: EndpointTarget::Free,
                            endpoint_b: EndpointTarget::Free,
                        }),
                    ),
                ],
                &physical,
            )
            .expect("two endpoint-touching Wires place");
        assert_eq!(report.acceptances.len(), 2);

        let report = world
            .apply_phase0(
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::BindPort(BindPortCommand {
                        wire: WireId(EntityId(2)),
                        end: WireEnd::A,
                        target: EndpointTarget::WireSensePort(WireSensePortRef {
                            wire: WireId(EntityId(1)),
                            end: WireEnd::B,
                        }),
                    }),
                )],
                &physical,
            )
            .expect("exact Sense endpoint binding is an ordinary command");
        assert_eq!(report.acceptances.len(), 1);
        assert!(report.rejections.is_empty());

        let report = world
            .apply_phase0(
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::BindPort(BindPortCommand {
                        wire: WireId(EntityId(1)),
                        end: WireEnd::A,
                        target: EndpointTarget::WireSensePort(WireSensePortRef {
                            wire: WireId(EntityId(1)),
                            end: WireEnd::A,
                        }),
                    }),
                )],
                &physical,
            )
            .expect("self binding rejects without aborting");
        assert_eq!(report.rejections[0].reason, Rejection::InvalidPortBinding);
    }

    #[test]
    fn removing_sense_owner_detaches_incident_wire_endpoint() {
        let physical = reference_physical();
        let mut world = StructuralWorld::new();
        world
            .apply_phase0(
                Tick(0),
                &[envelope(
                    0,
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
            .expect("Sense owner places");
        world
            .apply_phase0(
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::PlaceWire(PlaceWireCommand {
                        routing_domain: RoutingDomain::OpenWorld,
                        points: vec![point(WORLD_PITCH, 0), point(2 * WORLD_PITCH, 0)],
                        endpoint_a: EndpointTarget::WireSensePort(WireSensePortRef {
                            wire: WireId(EntityId(1)),
                            end: WireEnd::B,
                        }),
                        endpoint_b: EndpointTarget::Free,
                    }),
                )],
                &physical,
            )
            .expect("incident Sense-bound Wire places");
        world
            .apply_phase0(
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::RemoveEntity(RemoveEntityCommand {
                        target: EntityId(1),
                    }),
                )],
                &physical,
            )
            .expect("owner removal succeeds");
        let EntityLocation::Wire(index) = *world
            .entities
            .location(EntityId(2))
            .expect("incident Wire remains live")
        else {
            panic!("fixture EntityId 2 is a Wire");
        };
        assert_eq!(
            world
                .wires
                .get(index)
                .expect("Wire record exists")
                .endpoint_a,
            EndpointTarget::Free
        );
    }

    fn s1m4_context<'a>(
        physical: &'a PhysicalScaleProfile,
        balance: &'a crate::BalanceProfile,
    ) -> StructuralCommandContext<'a> {
        StructuralCommandContext {
            physical,
            main_core: None,
            power_sources: &[],
            sensing_enabled: true,
            construction_probe: balance.construction_probe.as_ref(),
            initial_integrity: balance
                .contact_damage_probe
                .as_ref()
                .map(|probe| &probe.initial_integrity),
        }
    }

    fn substrate_target(origin_x: i64) -> ConstructionTarget {
        ConstructionTarget::FixedSubstrate {
            origin: point(origin_x, 0),
            routing_area: FixedAabb::new(point(0, 0), point(WORLD_PITCH, WORLD_PITCH)),
            footprint: FixedAabb::new(point(0, 0), point(WORLD_PITCH, WORLD_PITCH)),
        }
    }

    fn s1m4_world_after(commands: Vec<Command>) -> StructuralWorld {
        let physical = reference_physical();
        let balance = crate::BalanceProfile::construction_contact_damage_alpha("damage-mapping");
        let context = s1m4_context(&physical, &balance);
        let mut world = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let mut sites = ConstructionSiteStore::default();
        for (tick, command) in commands.into_iter().enumerate() {
            let tick = u64::try_from(tick).expect("bounded fixture Tick fits u64");
            let report = world
                .apply_phase0_s1m4(
                    &mut signal,
                    &mut sites,
                    Tick(tick),
                    &[envelope(tick, 0, command)],
                    context,
                )
                .expect("S1-M4 structural fixture command is valid");
            assert_eq!(report.acceptances.len(), 1);
            assert!(report.rejections.is_empty());
        }
        world
    }

    fn assert_damage_state(
        world: &StructuralWorld,
        id: EntityId,
        kind: crate::ThermalObjectKind,
        integrity: u64,
    ) {
        assert_eq!(
            world.damage_state(id),
            Some((kind, DamageState::new(Integrity(integrity), HeatEnergy(0)),))
        );
    }

    #[test]
    fn construction_site_reserves_geometry_and_activates_with_fresh_damageable_id() {
        let physical = reference_physical();
        let balance = crate::BalanceProfile::construction_contact_damage_alpha("site-lifecycle");
        let context = s1m4_context(&physical, &balance);
        let mut world = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let mut sites = ConstructionSiteStore::default();
        let target = substrate_target(0);

        let placed = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceConstructionSite(crate::PlaceConstructionSiteCommand {
                        target: target.clone(),
                    }),
                )],
                context,
            )
            .expect("Site placement succeeds");
        assert_eq!(placed.acceptances[0].created_entity, Some(EntityId(1)));
        assert_eq!(sites.slot_count(), 1);

        let duplicate_site = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::PlaceConstructionSite(crate::PlaceConstructionSiteCommand {
                        target: target.clone(),
                    }),
                )],
                context,
            )
            .expect("Site/Site reservation rejection is non-fatal");
        assert_eq!(
            duplicate_site.rejections[0].reason,
            Rejection::GeometryOverlap
        );
        assert_eq!(sites.slot_count(), 1);

        let blocked = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                        origin: point(0, 0),
                        routing_area: FixedAabb::new(point(0, 0), point(WORLD_PITCH, WORLD_PITCH)),
                        footprint: FixedAabb::new(point(0, 0), point(WORLD_PITCH, WORLD_PITCH)),
                    }),
                )],
                context,
            )
            .expect("reservation rejection is non-fatal");
        assert_eq!(blocked.rejections[0].reason, Rejection::GeometryOverlap);

        let site = ConstructionSiteId(EntityId(1));
        assert_eq!(sites.get(site).unwrap().required_work, crate::Energy(1));
        crate::apply_construction_work(
            &mut sites,
            &[crate::ConstructionWorkContribution {
                site,
                builder: MobileId(EntityId(99)),
                granted_work: crate::Energy(1),
            }],
        )
        .expect("Work marks Site ready");
        world
            .apply_phase0_s1m4(&mut signal, &mut sites, Tick(3), &[], context)
            .expect("ready Site activates");
        assert!(sites.is_empty());
        assert_eq!(world.entities.location(EntityId(1)), None);
        let Some(EntityLocation::FixedSubstrate(index)) =
            world.entities.location(EntityId(2)).copied()
        else {
            panic!("fresh active target ID is allocated");
        };
        assert_eq!(
            world.fixed_substrates.get(index).unwrap().damage_state,
            Some(DamageState::pristine(Integrity(20)))
        );
    }

    #[test]
    fn phase0_remove_entity_cancels_a_live_site_and_preserves_its_tombstone() {
        let physical = reference_physical();
        let balance = crate::BalanceProfile::construction_contact_damage_alpha("site-cancel");
        let context = s1m4_context(&physical, &balance);
        let mut world = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let mut sites = ConstructionSiteStore::default();

        let placed = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceConstructionSite(crate::PlaceConstructionSiteCommand {
                        target: substrate_target(0),
                    }),
                )],
                context,
            )
            .expect("Site placement succeeds");
        assert_eq!(placed.acceptances[0].created_entity, Some(EntityId(1)));
        assert_eq!(sites.slot_count(), 1);

        let cancelled = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::RemoveEntity(RemoveEntityCommand {
                        target: EntityId(1),
                    }),
                )],
                context,
            )
            .expect("Site cancellation is an ordinary Phase-0 command");
        assert_eq!(cancelled.acceptances.len(), 1);
        assert_eq!(cancelled.acceptances[0].created_entity, None);
        assert!(cancelled.rejections.is_empty());
        assert!(sites.is_empty());
        assert_eq!(world.entities.location(EntityId(1)), None);
        assert_eq!(
            world.entities.canonical_slots().collect::<Vec<_>>(),
            vec![(EntityId(1), None)]
        );
        assert_eq!(world.entities.next_id(), EntityId(2));
        assert_eq!(world.fixed_substrates.iter_alive().count(), 0);

        let next = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::PlaceJunction(PlaceJunctionCommand {
                        routing_domain: RoutingDomain::OpenWorld,
                        position: point(2 * WORLD_PITCH, 0),
                    }),
                )],
                context,
            )
            .expect("the next unrelated allocation succeeds");
        assert_eq!(next.acceptances[0].created_entity, Some(EntityId(2)));
        assert_eq!(world.entities.location(EntityId(1)), None);
        assert!(sites.is_empty());
        assert_eq!(world.fixed_substrates.iter_alive().count(), 0);
    }

    #[test]
    fn construction_work_overflow_rejects_phase0_atomically_before_identity_or_site_mutation() {
        let physical = reference_physical();
        let balance = crate::BalanceProfile::construction_contact_damage_alpha("site-overflow");
        let context = s1m4_context(&physical, &balance);
        let mut world = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let mut sites = ConstructionSiteStore::default();
        let before_world = world.clone();
        let before_signal = signal.clone();
        let before_sites = sites.clone();
        let quantum = physical.wire_geometry_quantum.0;
        let max_aligned = i64::MAX - i64::MAX.rem_euclid(quantum);

        let result = world.apply_phase0_s1m4(
            &mut signal,
            &mut sites,
            Tick(0),
            &[envelope(
                0,
                0,
                Command::PlaceConstructionSite(crate::PlaceConstructionSiteCommand {
                    target: ConstructionTarget::Wire {
                        routing_domain: RoutingDomain::OpenWorld,
                        points: vec![point(i64::MIN, 0), point(max_aligned, 0)],
                        endpoint_a: EndpointTarget::Free,
                        endpoint_b: EndpointTarget::Free,
                    },
                }),
            )],
            context,
        );

        assert_eq!(result, Err(StructuralError::NumericOverflow));
        assert_eq!(world, before_world);
        assert_eq!(signal, before_signal);
        assert_eq!(sites, before_sites);
        assert_eq!(world.entities.next_id(), EntityId(1));
        assert_eq!(sites.slot_count(), 0);
    }

    #[test]
    fn all_five_structural_kinds_receive_exact_v5_integrity_and_zero_heat() {
        let circuit_pitch = reference_physical().circuit_routing_pitch.0;
        let substrate_bounds = FixedAabb::new(
            point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
            point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
        );
        let substrate = || {
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: substrate_bounds,
                footprint: substrate_bounds,
            })
        };

        let fixed = s1m4_world_after(vec![substrate()]);
        assert_damage_state(
            &fixed,
            EntityId(1),
            crate::ThermalObjectKind::FixedSubstrate,
            20,
        );

        let gate = s1m4_world_after(vec![
            substrate(),
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
            }),
        ]);
        assert_damage_state(&gate, EntityId(2), crate::ThermalObjectKind::Gate, 10);

        let junction = s1m4_world_after(vec![
            substrate(),
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                position: point(2 * circuit_pitch, 0),
            }),
        ]);
        assert_damage_state(
            &junction,
            EntityId(2),
            crate::ThermalObjectKind::Junction,
            10,
        );

        let track = || {
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(4 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            })
        };
        let wire = s1m4_world_after(vec![track()]);
        assert_damage_state(&wire, EntityId(1), crate::ThermalObjectKind::Wire, 10);

        let mobile_bounds = FixedAabb::new(
            point(-4 * circuit_pitch, -4 * circuit_pitch),
            point(4 * circuit_pitch, 4 * circuit_pitch),
        );
        let mobile = s1m4_world_after(vec![
            track(),
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(2 * WORLD_PITCH, 0),
                routing_area: mobile_bounds,
                footprint: mobile_bounds,
            }),
        ]);
        assert_damage_state(
            &mobile,
            EntityId(2),
            crate::ThermalObjectKind::MobileSubstrate,
            20,
        );
    }

    #[test]
    fn wire_site_mobile_intersection_uses_the_exact_rounded_corner() {
        let mut physical = reference_physical();
        physical.wire_body_radius = Fixed(5);
        let world = StructuralWorld::new();
        let track_graph = TrackGraph::compile(world.wires(), world.junctions())
            .expect("empty Track graph compiles");
        let target = ConstructionTarget::Wire {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![point(-10, 0), point(0, 0)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        };

        let exact_three_four_five_tangent = FixedAabb::new(point(3, 4), point(4, 5));
        assert_eq!(
            world.construction_target_intersects_aabb(
                &target,
                exact_three_four_five_tangent,
                &track_graph,
                &physical,
            ),
            Ok(true),
        );

        let outside_the_rounded_corner = FixedAabb::new(point(4, 4), point(5, 5));
        assert_eq!(
            world.construction_target_intersects_aabb(
                &target,
                outside_the_rounded_corner,
                &track_graph,
                &physical,
            ),
            Ok(false),
        );
    }

    #[test]
    fn site_dependency_blocks_player_removal_and_damage_cascade_cancels_site() {
        let physical = reference_physical();
        let balance = crate::BalanceProfile::construction_contact_damage_alpha("site-cascade");
        let context = s1m4_context(&physical, &balance);
        let mut world = StructuralWorld::new();
        let mut signal = SignalWorld::new();
        let mut sites = ConstructionSiteStore::default();
        let area = FixedAabb::new(
            point(-2 * WORLD_PITCH, -2 * WORLD_PITCH),
            point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
        );
        world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                        origin: point(0, 0),
                        routing_area: area,
                        footprint: area,
                    }),
                )],
                context,
            )
            .unwrap();
        world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(1),
                &[envelope(
                    1,
                    0,
                    Command::PlaceGate(PlaceGateCommand {
                        gate_type: GateType::Not,
                        origin: point(-WORLD_PITCH, 0),
                        routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                    }),
                )],
                context,
            )
            .unwrap();
        world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::PlaceConstructionSite(crate::PlaceConstructionSiteCommand {
                        target: ConstructionTarget::Junction {
                            routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                            position: point(WORLD_PITCH, 0),
                        },
                    }),
                )],
                context,
            )
            .unwrap();

        let blocked = world
            .apply_phase0_s1m4(
                &mut signal,
                &mut sites,
                Tick(3),
                &[envelope(
                    3,
                    0,
                    Command::RemoveEntity(RemoveEntityCommand {
                        target: EntityId(1),
                    }),
                )],
                context,
            )
            .unwrap();
        assert_eq!(
            blocked.rejections[0].reason,
            Rejection::ConstructionDependencyInUse
        );

        let mut pending = BTreeSet::from([EntityId(1)]);
        let destroyed = world
            .apply_pending_structural_destructions(&mut signal, &mut sites, &mut pending)
            .expect("damage closure bypasses player-facing dependency rejection");
        assert!(pending.is_empty());
        assert!(sites.is_empty());
        assert!(destroyed.topology_changed);
        assert_eq!(
            destroyed.records,
            vec![
                StructuralDestructionRecord {
                    target: EntityId(1),
                    kind: StructuralDestructionKind::Damage,
                },
                StructuralDestructionRecord {
                    target: EntityId(2),
                    kind: StructuralDestructionKind::SubstrateSupportLost,
                },
                StructuralDestructionRecord {
                    target: EntityId(3),
                    kind: StructuralDestructionKind::ConstructionDependencyLost,
                },
            ]
        );
        assert_eq!(world.live_primitive_count(), 0);
        assert_eq!(signal.gate_ports(GateId(EntityId(2))), None);
    }

    #[test]
    fn destruction_scc_order_is_dependent_first_and_cycle_stable() {
        let graph = BTreeMap::from([
            (EntityId(1), BTreeSet::from([EntityId(2)])),
            (EntityId(2), BTreeSet::from([EntityId(1), EntityId(3)])),
            (EntityId(3), BTreeSet::new()),
        ]);
        assert_eq!(
            dependent_first_scc_order(&graph).unwrap(),
            vec![vec![EntityId(1), EntityId(2)], vec![EntityId(3)]]
        );
    }
}
