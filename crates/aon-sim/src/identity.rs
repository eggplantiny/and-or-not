use crate::numeric::EntityId;
use thiserror::Error;

pub const RESERVED_ENTITY_ID: EntityId = EntityId(0);
pub const FIRST_ENTITY_ID: EntityId = EntityId(1);

macro_rules! typed_entity_id {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub EntityId);

        impl $name {
            pub const fn entity_id(self) -> EntityId {
                self.0
            }
        }

        impl From<EntityId> for $name {
            fn from(id: EntityId) -> Self {
                Self(id)
            }
        }

        impl From<$name> for EntityId {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

typed_entity_id!(GateId);
typed_entity_id!(WireId);
typed_entity_id!(JunctionId);
typed_entity_id!(DriverId);
typed_entity_id!(SinkId);
typed_entity_id!(RelaySiteId);
typed_entity_id!(MobileId);

macro_rules! dense_index {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            pub const fn as_u32(self) -> u32 {
                self.0
            }
        }
    };
}

dense_index!(RelaySiteIndex);
dense_index!(GateIndex);
dense_index!(WireIndex);
dense_index!(JunctionIndex);
dense_index!(FixedSubstrateIndex);
dense_index!(MobileSubstrateIndex);
dense_index!(PowerSourceIndex);
dense_index!(QuartzIndex);
dense_index!(DepositIndex);
dense_index!(EnemyIndex);
dense_index!(ConstructionSiteIndex);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityLocation {
    MainCore,
    RelaySite(RelaySiteIndex),
    Gate(GateIndex),
    Wire(WireIndex),
    Junction(JunctionIndex),
    FixedSubstrate(FixedSubstrateIndex),
    MobileSubstrate(MobileSubstrateIndex),
    PowerSource(PowerSourceIndex),
    Quartz(QuartzIndex),
    Deposit(DepositIndex),
    Enemy(EnemyIndex),
    ConstructionSite(ConstructionSiteIndex),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum EntityRegistryError {
    #[error("entity ID 0 is reserved")]
    ReservedEntityId,

    #[error("unknown entity ID {0:?}")]
    UnknownEntity(EntityId),

    #[error("entity ID {0:?} has already been removed")]
    RemovedEntity(EntityId),

    #[error("canonical entity ID allocator exhausted")]
    EntityIdExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityRegistry {
    next_id: u64,
    locations: Vec<Option<EntityLocation>>,
}

impl Default for EntityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_id: FIRST_ENTITY_ID.0,
            locations: vec![None],
        }
    }

    pub fn allocate(&mut self, location: EntityLocation) -> Result<EntityId, EntityRegistryError> {
        let id = EntityId(self.next_id);
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(EntityRegistryError::EntityIdExhausted)?;

        self.locations.push(Some(location));
        self.next_id = next_id;
        Ok(id)
    }

    pub fn remove(&mut self, id: EntityId) -> Result<EntityLocation, EntityRegistryError> {
        let slot = self.slot_mut(id)?;
        slot.take().ok_or(EntityRegistryError::RemovedEntity(id))
    }

    pub fn update_location(
        &mut self,
        id: EntityId,
        location: EntityLocation,
    ) -> Result<(), EntityRegistryError> {
        let slot = self.slot_mut(id)?;
        let Some(current) = slot.as_mut() else {
            return Err(EntityRegistryError::RemovedEntity(id));
        };
        *current = location;
        Ok(())
    }

    pub fn location(&self, id: EntityId) -> Option<&EntityLocation> {
        let index = usize::try_from(id.0).ok()?;
        self.locations.get(index)?.as_ref()
    }

    pub fn is_alive(&self, id: EntityId) -> bool {
        self.location(id).is_some()
    }

    pub const fn next_id(&self) -> EntityId {
        EntityId(self.next_id)
    }

    pub const fn allocated_count(&self) -> u64 {
        self.next_id - FIRST_ENTITY_ID.0
    }

    pub fn live_count(&self) -> usize {
        self.locations.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn iter_alive(&self) -> impl Iterator<Item = (EntityId, &EntityLocation)> {
        (FIRST_ENTITY_ID.0..)
            .zip(self.locations.iter().skip(1))
            .filter_map(|(id, location)| location.as_ref().map(|location| (EntityId(id), location)))
    }

    pub(crate) fn canonical_slots(
        &self,
    ) -> impl Iterator<Item = (EntityId, Option<EntityLocation>)> + '_ {
        (FIRST_ENTITY_ID.0..)
            .zip(self.locations.iter().skip(1))
            .map(|(id, location)| (EntityId(id), *location))
    }

    #[cfg(test)]
    pub(crate) fn force_next_id_for_test(&mut self, next_id: EntityId) {
        self.next_id = next_id.0;
    }

    #[cfg(test)]
    pub(crate) fn reserve_capacity_for_test(&mut self, additional: usize) {
        self.locations.reserve(additional);
    }

    fn slot_mut(
        &mut self,
        id: EntityId,
    ) -> Result<&mut Option<EntityLocation>, EntityRegistryError> {
        if id == RESERVED_ENTITY_ID {
            return Err(EntityRegistryError::ReservedEntityId);
        }
        let Ok(index) = usize::try_from(id.0) else {
            return Err(EntityRegistryError::UnknownEntity(id));
        };
        self.locations
            .get_mut(index)
            .ok_or(EntityRegistryError::UnknownEntity(id))
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionGeneration(pub u64);

impl ConnectionGeneration {
    pub const INITIAL: Self = Self(0);

    pub fn checked_advance(self) -> Result<Self, ConnectionGenerationError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(ConnectionGenerationError::Overflow)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ConnectionGenerationError {
    #[error("canonical connection generation overflow")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn zero_is_reserved_and_first_allocation_is_one() {
        let mut registry = EntityRegistry::new();

        assert_eq!(registry.next_id(), FIRST_ENTITY_ID);
        assert_eq!(registry.location(RESERVED_ENTITY_ID), None);
        assert_eq!(registry.allocated_count(), 0);
        assert_eq!(registry.live_count(), 0);

        let id = registry
            .allocate(EntityLocation::Gate(GateIndex(0)))
            .expect("the first allocation succeeds");

        assert_eq!(id, EntityId(1));
        assert_eq!(registry.next_id(), EntityId(2));
        assert_eq!(registry.allocated_count(), 1);
        assert_eq!(registry.live_count(), 1);
    }

    #[test]
    fn removal_leaves_a_tombstone_and_ids_are_never_reused() {
        let mut registry = EntityRegistry::new();
        let removed_id = registry
            .allocate(EntityLocation::Wire(WireIndex(0)))
            .expect("wire allocation succeeds");
        let retained_id = registry
            .allocate(EntityLocation::Gate(GateIndex(0)))
            .expect("gate allocation succeeds");

        assert_eq!(
            registry.remove(removed_id),
            Ok(EntityLocation::Wire(WireIndex(0)))
        );
        assert_eq!(registry.location(removed_id), None);
        assert_eq!(registry.live_count(), 1);
        assert_eq!(
            registry.remove(removed_id),
            Err(EntityRegistryError::RemovedEntity(removed_id))
        );

        let replacement_id = registry
            .allocate(EntityLocation::Wire(WireIndex(0)))
            .expect("replacement allocation succeeds");
        assert_eq!(retained_id, EntityId(2));
        assert_eq!(replacement_id, EntityId(3));
        assert_ne!(replacement_id, removed_id);
    }

    #[test]
    fn lookup_and_location_update_track_dense_compaction() {
        let mut registry = EntityRegistry::new();
        let id = registry
            .allocate(EntityLocation::Junction(JunctionIndex(7)))
            .expect("junction allocation succeeds");

        assert_eq!(
            registry.location(id),
            Some(&EntityLocation::Junction(JunctionIndex(7)))
        );
        registry
            .update_location(id, EntityLocation::Junction(JunctionIndex(2)))
            .expect("an alive entity location can be updated");
        assert_eq!(
            registry.location(id),
            Some(&EntityLocation::Junction(JunctionIndex(2)))
        );
    }

    #[test]
    fn invalid_and_removed_entities_have_typed_mutation_errors() {
        let mut registry = EntityRegistry::new();
        assert_eq!(
            registry.remove(RESERVED_ENTITY_ID),
            Err(EntityRegistryError::ReservedEntityId)
        );
        assert_eq!(
            registry.update_location(EntityId(9), EntityLocation::MainCore),
            Err(EntityRegistryError::UnknownEntity(EntityId(9)))
        );

        let id = registry
            .allocate(EntityLocation::MainCore)
            .expect("main core allocation succeeds");
        registry.remove(id).expect("main core removal succeeds");
        assert_eq!(
            registry.update_location(id, EntityLocation::MainCore),
            Err(EntityRegistryError::RemovedEntity(id))
        );
    }

    #[test]
    fn alive_iteration_is_in_entity_id_order_and_skips_tombstones() {
        let mut registry = EntityRegistry::new();
        let first = registry
            .allocate(EntityLocation::Gate(GateIndex(4)))
            .expect("first allocation succeeds");
        let second = registry
            .allocate(EntityLocation::Wire(WireIndex(8)))
            .expect("second allocation succeeds");
        let third = registry
            .allocate(EntityLocation::Enemy(EnemyIndex(3)))
            .expect("third allocation succeeds");
        registry.remove(second).expect("middle removal succeeds");

        let alive: Vec<_> = registry
            .iter_alive()
            .map(|(id, location)| (id, *location))
            .collect();
        assert_eq!(
            alive,
            vec![
                (first, EntityLocation::Gate(GateIndex(4))),
                (third, EntityLocation::Enemy(EnemyIndex(3))),
            ]
        );
    }

    #[test]
    fn allocator_overflow_is_checked_before_mutation() {
        let mut registry = EntityRegistry {
            next_id: u64::MAX,
            locations: vec![None],
        };

        assert_eq!(
            registry.allocate(EntityLocation::MainCore),
            Err(EntityRegistryError::EntityIdExhausted)
        );
        assert_eq!(registry.next_id(), EntityId(u64::MAX));
        assert_eq!(registry.locations, vec![None]);
    }

    #[test]
    fn connection_generation_starts_at_zero_and_advances_without_wrapping() {
        assert_eq!(
            ConnectionGeneration::default(),
            ConnectionGeneration::INITIAL
        );
        assert_eq!(
            ConnectionGeneration::INITIAL.checked_advance(),
            Ok(ConnectionGeneration(1))
        );
        assert_eq!(
            ConnectionGeneration(u64::MAX).checked_advance(),
            Err(ConnectionGenerationError::Overflow)
        );
    }

    #[test]
    fn typed_ids_round_trip_their_canonical_entity_id() {
        let entity_id = EntityId(42);

        assert_eq!(GateId::from(entity_id).entity_id(), entity_id);
        assert_eq!(EntityId::from(WireId(entity_id)), entity_id);
        assert_eq!(JunctionId(entity_id).entity_id(), entity_id);
        assert_eq!(DriverId(entity_id).entity_id(), entity_id);
        assert_eq!(SinkId(entity_id).entity_id(), entity_id);
        assert_eq!(RelaySiteId(entity_id).entity_id(), entity_id);
        assert_eq!(MobileId(entity_id).entity_id(), entity_id);
    }

    #[test]
    fn seeded_allocate_remove_sequences_keep_ids_monotonic_and_never_reuse_them() {
        let mut registry = EntityRegistry::new();
        let mut seed = 0xd1b5_4a32_d192_ed03_u64;
        let mut live = Vec::<(EntityId, EntityLocation)>::new();
        let mut seen = BTreeSet::new();
        let mut retired = BTreeSet::new();
        let mut last_allocated = RESERVED_ENTITY_ID;
        let mut allocation_count = 0_u64;

        for step in 0_u32..4_096 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;

            if live.is_empty() || seed % 5 < 3 {
                let location = match step % 4 {
                    0 => EntityLocation::Gate(GateIndex(step)),
                    1 => EntityLocation::Wire(WireIndex(step)),
                    2 => EntityLocation::Junction(JunctionIndex(step)),
                    _ => EntityLocation::Enemy(EnemyIndex(step)),
                };
                let id = registry.allocate(location).expect("allocation succeeds");
                allocation_count += 1;

                assert_eq!(id, EntityId(allocation_count));
                assert!(
                    id > last_allocated,
                    "entity IDs must increase monotonically"
                );
                assert!(seen.insert(id), "allocated ID {id:?} was reused");
                assert!(
                    !retired.contains(&id),
                    "retired entity ID {id:?} was reused"
                );
                assert_eq!(registry.location(id), Some(&location));

                last_allocated = id;
                live.push((id, location));
            } else {
                let index = (seed as usize) % live.len();
                let (id, location) = live.swap_remove(index);
                assert_eq!(registry.remove(id), Ok(location));
                assert!(retired.insert(id));
                assert!(!registry.is_alive(id));
                assert_eq!(
                    registry.remove(id),
                    Err(EntityRegistryError::RemovedEntity(id))
                );
            }

            assert_eq!(registry.allocated_count(), allocation_count);
            assert_eq!(registry.next_id(), EntityId(allocation_count + 1));
            assert_eq!(registry.live_count(), live.len());
        }

        let mut expected_alive = live;
        expected_alive.sort_unstable_by_key(|(id, _)| *id);
        let actual_alive: Vec<_> = registry
            .iter_alive()
            .map(|(id, location)| (id, *location))
            .collect();
        assert_eq!(actual_alive, expected_alive);

        for _ in 0..128 {
            let id = registry
                .allocate(EntityLocation::MainCore)
                .expect("tail allocation succeeds");
            allocation_count += 1;
            assert_eq!(id, EntityId(allocation_count));
            assert!(id > last_allocated);
            assert!(seen.insert(id));
            assert!(!retired.contains(&id));
            last_allocated = id;
        }
    }
}
