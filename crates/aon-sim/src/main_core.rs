use crate::{Capacity, FixedVec2, HeatEnergy, Integrity, MainCoreId};

/// Canonical state for the single Main Core created by world generation.
///
/// The Main Core's position is also its implicit Network Anchor position. The anchor does not
/// consume a second Entity ID: `MainCoreId` is the stable identity for both the body and its
/// singleton anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MainCoreState {
    id: MainCoreId,
    position: FixedVec2,
    capacity: Capacity,
    integrity: Integrity,
    heat_energy: HeatEnergy,
}

/// Stable topology identity for world-generator-owned anchor nodes.
///
/// S1-M1 has exactly one kind. The enum preserves the TRD's topology-node boundary without
/// introducing a second allocator or a redundant canonical ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopologyNodeId {
    MainCoreAnchor(MainCoreId),
}

impl MainCoreState {
    pub(crate) const fn new(
        id: MainCoreId,
        position: FixedVec2,
        capacity: Capacity,
        integrity: Integrity,
        heat_energy: HeatEnergy,
    ) -> Self {
        Self {
            id,
            position,
            capacity,
            integrity,
            heat_energy,
        }
    }

    #[cfg(test)]
    pub(crate) const fn with_id_for_test(self, id: MainCoreId) -> Self {
        Self { id, ..self }
    }

    pub const fn id(self) -> MainCoreId {
        self.id
    }

    pub const fn position(self) -> FixedVec2 {
        self.position
    }

    pub const fn anchor_node(self) -> TopologyNodeId {
        TopologyNodeId::MainCoreAnchor(self.id)
    }

    pub const fn capacity(self) -> Capacity {
        self.capacity
    }

    pub const fn integrity(self) -> Integrity {
        self.integrity
    }

    pub const fn heat_energy(self) -> HeatEnergy {
        self.heat_energy
    }

    pub(crate) const fn anchor_view(self) -> MainCoreAnchorView {
        MainCoreAnchorView {
            id: self.id,
            position: self.position,
        }
    }
}

/// Copy-only Phase 0 view used to validate structural endpoint commands without moving Main Core
/// ownership into `StructuralWorld`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MainCoreAnchorView {
    pub id: MainCoreId,
    pub position: FixedVec2,
}
