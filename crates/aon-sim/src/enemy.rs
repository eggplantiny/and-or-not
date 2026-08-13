use crate::{
    EnemyId, EnemyIndex, Fixed, FixedVec2, HeatEnergy, Integrity, NumericError, RESERVED_ENTITY_ID,
};
use thiserror::Error;

/// Canonical state for one Scenario-owned Enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnemyState {
    id: EnemyId,
    position: FixedVec2,
    velocity_per_tick: FixedVec2,
    radius: Fixed,
    integrity: Integrity,
    heat_energy: HeatEnergy,
}

impl EnemyState {
    pub const fn new(
        id: EnemyId,
        position: FixedVec2,
        velocity_per_tick: FixedVec2,
        radius: Fixed,
        integrity: Integrity,
        heat_energy: HeatEnergy,
    ) -> Self {
        Self {
            id,
            position,
            velocity_per_tick,
            radius,
            integrity,
            heat_energy,
        }
    }

    pub const fn id(self) -> EnemyId {
        self.id
    }

    pub const fn position(self) -> FixedVec2 {
        self.position
    }

    pub const fn velocity_per_tick(self) -> FixedVec2 {
        self.velocity_per_tick
    }

    pub const fn radius(self) -> Fixed {
        self.radius
    }

    pub const fn integrity(self) -> Integrity {
        self.integrity
    }

    pub const fn heat_energy(self) -> HeatEnergy {
        self.heat_energy
    }

    pub fn staged_endpoint(self) -> Result<FixedVec2, NumericError> {
        Ok(FixedVec2::new(
            self.position.x.checked_add(self.velocity_per_tick.x)?,
            self.position.y.checked_add(self.velocity_per_tick.y)?,
        ))
    }

    pub(crate) fn set_position(&mut self, position: FixedVec2) {
        self.position = position;
    }

    pub(crate) fn set_integrity(&mut self, integrity: Integrity) {
        self.integrity = integrity;
    }

    pub(crate) fn set_heat_energy(&mut self, heat_energy: HeatEnergy) {
        self.heat_energy = heat_energy;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnemySlot {
    id: EnemyId,
    state: Option<EnemyState>,
}

/// Entity-ID ordered canonical slots for Scenario-owned Enemies.
///
/// Removal leaves a tombstone so the `EnemyIndex` stored in the global registry never shifts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnemyStore {
    slots: Vec<EnemySlot>,
    live_count: usize,
}

impl EnemyStore {
    pub fn new(mut states: Vec<EnemyState>) -> Result<Self, EnemyStoreError> {
        for state in &states {
            let enemy = state.id();
            if enemy.entity_id() == RESERVED_ENTITY_ID {
                return Err(EnemyStoreError::ReservedEnemyId);
            }
            if state.radius().0 <= 0 {
                return Err(EnemyStoreError::NonPositiveRadius { enemy });
            }
            if state.integrity().0 == 0 {
                return Err(EnemyStoreError::NonPositiveIntegrity { enemy });
            }
            state
                .staged_endpoint()
                .map_err(|_| EnemyStoreError::TrajectoryOverflow { enemy })?;
        }

        states.sort_unstable_by_key(|state| state.id());
        if let Some(duplicate) = states.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
            return Err(EnemyStoreError::DuplicateEnemyId {
                enemy: duplicate[0].id(),
            });
        }
        let live_count = states.len();
        Ok(Self {
            slots: states
                .into_iter()
                .map(|state| EnemySlot {
                    id: state.id(),
                    state: Some(state),
                })
                .collect(),
            live_count,
        })
    }

    pub fn get(&self, id: EnemyId) -> Option<&EnemyState> {
        self.slots
            .binary_search_by_key(&id, |slot| slot.id)
            .ok()
            .and_then(|index| self.slots[index].state.as_ref())
    }

    pub(crate) fn get_mut(&mut self, id: EnemyId) -> Option<&mut EnemyState> {
        let index = self.slots.binary_search_by_key(&id, |slot| slot.id).ok()?;
        self.slots[index].state.as_mut()
    }

    pub fn get_by_index(&self, index: EnemyIndex) -> Option<&EnemyState> {
        let index = usize::try_from(index.0).ok()?;
        self.slots.get(index)?.state.as_ref()
    }

    pub(crate) fn remove_by_index(
        &mut self,
        index: EnemyIndex,
    ) -> Result<EnemyState, EnemyStoreError> {
        let index =
            usize::try_from(index.0).map_err(|_| EnemyStoreError::UnknownEnemyIndex { index })?;
        let slot = self
            .slots
            .get_mut(index)
            .ok_or(EnemyStoreError::UnknownEnemyIndex {
                index: EnemyIndex(index as u32),
            })?;
        let state = slot
            .state
            .take()
            .ok_or(EnemyStoreError::RemovedEnemy { enemy: slot.id })?;
        self.live_count = self
            .live_count
            .checked_sub(1)
            .ok_or(EnemyStoreError::InvalidCanonicalState)?;
        Ok(state)
    }

    pub fn iter(&self) -> impl Iterator<Item = &EnemyState> {
        self.slots.iter().filter_map(|slot| slot.state.as_ref())
    }

    pub(crate) fn iter_alive(&self) -> impl Iterator<Item = (EnemyIndex, &EnemyState)> + '_ {
        self.slots.iter().enumerate().filter_map(|(raw, slot)| {
            let index = u32::try_from(raw).ok().map(EnemyIndex)?;
            slot.state.as_ref().map(|state| (index, state))
        })
    }

    pub const fn len(&self) -> usize {
        self.live_count
    }

    pub const fn is_empty(&self) -> bool {
        self.live_count == 0
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum EnemyStoreError {
    #[error("Enemy entity ID 0 is reserved")]
    ReservedEnemyId,

    #[error("Enemy {enemy:?} radius must be positive")]
    NonPositiveRadius { enemy: EnemyId },

    #[error("Enemy {enemy:?} integrity must be positive")]
    NonPositiveIntegrity { enemy: EnemyId },

    #[error("Enemy {enemy:?} trajectory endpoint overflows canonical Fixed coordinates")]
    TrajectoryOverflow { enemy: EnemyId },

    #[error("duplicate Enemy entity ID {enemy:?}")]
    DuplicateEnemyId { enemy: EnemyId },

    #[error("unknown Enemy dense index {index:?}")]
    UnknownEnemyIndex { index: EnemyIndex },

    #[error("Enemy {enemy:?} has already been removed")]
    RemovedEnemy { enemy: EnemyId },

    #[error("canonical Enemy store invariant violated")]
    InvalidCanonicalState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;

    fn enemy(id: u64, x: i64, velocity_x: i64) -> EnemyState {
        EnemyState::new(
            EnemyId(EntityId(id)),
            FixedVec2::new(Fixed(x), Fixed::ZERO),
            FixedVec2::new(Fixed(velocity_x), Fixed::ZERO),
            Fixed(1),
            Integrity(10),
            HeatEnergy(0),
        )
    }

    #[test]
    fn store_orders_by_entity_id_and_preserves_id_independent_state() {
        let store = EnemyStore::new(vec![enemy(9, 900, 1), enemy(3, 300, -1)])
            .expect("valid Enemy states construct");
        assert_eq!(
            store.iter().map(|state| state.id()).collect::<Vec<_>>(),
            vec![EnemyId(EntityId(3)), EnemyId(EntityId(9))]
        );
        assert_eq!(
            store.get(EnemyId(EntityId(9))).map(|state| (
                state.position(),
                state.velocity_per_tick(),
                state.radius(),
                state.integrity(),
                state.heat_energy(),
            )),
            Some((
                FixedVec2::new(Fixed(900), Fixed::ZERO),
                FixedVec2::new(Fixed(1), Fixed::ZERO),
                Fixed(1),
                Integrity(10),
                HeatEnergy(0),
            ))
        );
    }

    #[test]
    fn store_rejects_reserved_duplicate_and_invalid_states() {
        assert_eq!(
            EnemyStore::new(vec![enemy(0, 0, 0)]),
            Err(EnemyStoreError::ReservedEnemyId)
        );
        assert_eq!(
            EnemyStore::new(vec![enemy(1, 0, 0), enemy(1, 1, 0)]),
            Err(EnemyStoreError::DuplicateEnemyId {
                enemy: EnemyId(EntityId(1))
            })
        );

        let mut bad_radius = enemy(1, 0, 0);
        bad_radius.radius = Fixed::ZERO;
        assert_eq!(
            EnemyStore::new(vec![bad_radius]),
            Err(EnemyStoreError::NonPositiveRadius {
                enemy: EnemyId(EntityId(1))
            })
        );

        let mut bad_integrity = enemy(1, 0, 0);
        bad_integrity.integrity = Integrity(0);
        assert_eq!(
            EnemyStore::new(vec![bad_integrity]),
            Err(EnemyStoreError::NonPositiveIntegrity {
                enemy: EnemyId(EntityId(1))
            })
        );

        assert_eq!(
            EnemyStore::new(vec![enemy(1, i64::MAX, 1)]),
            Err(EnemyStoreError::TrajectoryOverflow {
                enemy: EnemyId(EntityId(1))
            })
        );
    }

    #[test]
    fn removal_tombstones_the_stable_dense_index_without_shifting_later_enemies() {
        let mut store = EnemyStore::new(vec![enemy(2, 20, 0), enemy(5, 50, 0), enemy(9, 90, 0)])
            .expect("valid Enemy states construct");
        assert_eq!(
            store
                .iter_alive()
                .map(|(index, state)| (index, state.id()))
                .collect::<Vec<_>>(),
            vec![
                (EnemyIndex(0), EnemyId(EntityId(2))),
                (EnemyIndex(1), EnemyId(EntityId(5))),
                (EnemyIndex(2), EnemyId(EntityId(9))),
            ]
        );

        assert_eq!(
            store.remove_by_index(EnemyIndex(1)).map(|state| state.id()),
            Ok(EnemyId(EntityId(5)))
        );
        assert_eq!(store.len(), 2);
        assert_eq!(store.get_by_index(EnemyIndex(1)), None);
        assert_eq!(
            store.get_by_index(EnemyIndex(2)).map(|state| state.id()),
            Some(EnemyId(EntityId(9)))
        );
        assert_eq!(
            store
                .iter_alive()
                .map(|(index, state)| (index, state.id()))
                .collect::<Vec<_>>(),
            vec![
                (EnemyIndex(0), EnemyId(EntityId(2))),
                (EnemyIndex(2), EnemyId(EntityId(9))),
            ]
        );
        assert_eq!(
            store.remove_by_index(EnemyIndex(1)),
            Err(EnemyStoreError::RemovedEnemy {
                enemy: EnemyId(EntityId(5)),
            })
        );
    }
}
