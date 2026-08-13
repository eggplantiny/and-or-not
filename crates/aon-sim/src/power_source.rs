use crate::power::PowerSourceState;
use crate::{FixedVec2, PowerSourceId, RESERVED_ENTITY_ID};
use thiserror::Error;

/// Immutable, canonical collection of world-generator-owned Power Sources.
///
/// World generation supplies stable Entity IDs after sorting Scenario inputs by semantic value.
/// The store validates the resulting records and retains them in Entity-ID order so iteration is
/// independent of artifact array order and allocation capacity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerSourceStore {
    states: Vec<PowerSourceState>,
}

impl PowerSourceStore {
    pub fn new(mut states: Vec<PowerSourceState>) -> Result<Self, PowerSourceStoreError> {
        for state in &states {
            let source = state.id();
            if source.entity_id() == RESERVED_ENTITY_ID {
                return Err(PowerSourceStoreError::ReservedSourceId);
            }
            if state.generation_per_tick().0 == 0 {
                return Err(PowerSourceStoreError::NonPositiveGeneration {
                    power_source: source,
                });
            }
        }

        states.sort_unstable_by_key(|state| state.id());
        if let Some(duplicate) = states.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
            return Err(PowerSourceStoreError::DuplicateSourceId {
                power_source: duplicate[0].id(),
            });
        }

        let mut positions: Vec<_> = states
            .iter()
            .map(|state| (state.position().x.0, state.position().y.0, state.id()))
            .collect();
        positions.sort_unstable();
        if let Some(duplicate) = positions
            .windows(2)
            .find(|pair| pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1)
        {
            return Err(PowerSourceStoreError::DuplicatePosition {
                position: FixedVec2::new(
                    crate::Fixed(duplicate[0].0),
                    crate::Fixed(duplicate[0].1),
                ),
            });
        }

        Ok(Self { states })
    }

    pub fn get(&self, id: PowerSourceId) -> Option<&PowerSourceState> {
        self.states
            .binary_search_by_key(&id, |state| state.id())
            .ok()
            .map(|index| &self.states[index])
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &PowerSourceState> {
        self.states.iter()
    }

    pub const fn len(&self) -> usize {
        self.states.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PowerSourceStoreError {
    #[error("Power Source entity ID 0 is reserved")]
    ReservedSourceId,

    #[error("Power Source {power_source:?} generationPerTick must be positive")]
    NonPositiveGeneration { power_source: PowerSourceId },

    #[error("duplicate Power Source entity ID {power_source:?}")]
    DuplicateSourceId { power_source: PowerSourceId },

    #[error("duplicate Power Source position {position:?}")]
    DuplicatePosition { position: FixedVec2 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Energy, EntityId, Fixed, TopologyNodeId};

    fn source(id: u64, x: i64, generation: u64) -> PowerSourceState {
        let id = PowerSourceId(EntityId(id));
        PowerSourceState::new(
            id,
            FixedVec2::new(Fixed(x), Fixed::ZERO),
            Energy(generation),
        )
    }

    #[test]
    fn construction_sorts_by_entity_id_and_lookup_is_typed() {
        assert_eq!(
            PowerSourceStore::new(Vec::new()),
            Ok(PowerSourceStore::default())
        );

        let store = PowerSourceStore::new(vec![source(9, 900, 2), source(3, 300, 1)])
            .expect("valid sources construct");

        assert_eq!(
            store.iter().map(|state| state.id()).collect::<Vec<_>>(),
            vec![PowerSourceId(EntityId(3)), PowerSourceId(EntityId(9))]
        );
        assert_eq!(
            store
                .get(PowerSourceId(EntityId(9)))
                .map(|state| state.generation_per_tick()),
            Some(Energy(2))
        );
        assert_eq!(
            store
                .get(PowerSourceId(EntityId(9)))
                .map(|state| state.power_attachment()),
            Some(TopologyNodeId::PowerSourceAnchor(PowerSourceId(EntityId(
                9
            ))))
        );
        assert_eq!(store.get(PowerSourceId(EntityId(8))), None);
    }

    #[test]
    fn construction_rejects_invalid_or_ambiguous_sources() {
        assert_eq!(
            PowerSourceStore::new(vec![source(0, 0, 1)]),
            Err(PowerSourceStoreError::ReservedSourceId)
        );
        assert_eq!(
            PowerSourceStore::new(vec![source(1, 0, 0)]),
            Err(PowerSourceStoreError::NonPositiveGeneration {
                power_source: PowerSourceId(EntityId(1))
            })
        );
        assert_eq!(
            PowerSourceStore::new(vec![source(1, 0, 1), source(1, 100, 1)]),
            Err(PowerSourceStoreError::DuplicateSourceId {
                power_source: PowerSourceId(EntityId(1))
            })
        );
        assert_eq!(
            PowerSourceStore::new(vec![source(1, 0, 1), source(2, 0, 1)]),
            Err(PowerSourceStoreError::DuplicatePosition {
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO)
            })
        );
    }
}
