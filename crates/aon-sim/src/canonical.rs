use crate::{EntityLocation, EntityRegistry, Revision, SimulationContract, StateHash, Tick};

const STATE_DOMAIN: &[u8] = b"AON\0STATE\0V1\0";
const STATE_ENCODER_VERSION: u16 = 1;
const EMPTY_STORE_COUNT: usize = 8;

pub(crate) fn state_hash(
    contract: &SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    entities: &EntityRegistry,
) -> StateHash {
    let mut hasher = blake3::Hasher::new();
    encode_state(
        contract,
        next_tick,
        topology_revision,
        entities,
        &mut |bytes| {
            hasher.update(bytes);
        },
    );
    StateHash::from_bytes(*hasher.finalize().as_bytes())
}

fn encode_state(
    contract: &SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    entities: &EntityRegistry,
    write: &mut dyn FnMut(&[u8]),
) {
    write(STATE_DOMAIN);
    write_u16(STATE_ENCODER_VERSION, write);
    write_u8(contract.semantics_version.canonical_tag(), write);
    write(contract.numeric_profile_hash.as_bytes());
    write(contract.physical_scale_profile_hash.as_bytes());
    write(contract.balance_profile_hash.as_bytes());
    write_u64(next_tick.0, write);
    write_u64(topology_revision.0, write);
    write_u64(entities.next_id().0, write);
    write_u64(entities.allocated_count(), write);
    for (entity_id, location) in entities.canonical_slots() {
        write_u64(entity_id.0, write);
        match location {
            Some(location) => {
                write_u8(1, write);
                write_u8(entity_kind_tag(location), write);
            }
            None => write_u8(0, write),
        }
    }

    // Gate, wire, junction, fixed substrate, mobile substrate, scheduled event,
    // pending destruction, and path-certificate stores are introduced by S0-M2+.
    for _ in 0..EMPTY_STORE_COUNT {
        write_u64(0, write);
    }
}

const fn entity_kind_tag(location: EntityLocation) -> u8 {
    match location {
        EntityLocation::MainCore => 0,
        EntityLocation::RelaySite(_) => 1,
        EntityLocation::Gate(_) => 2,
        EntityLocation::Wire(_) => 3,
        EntityLocation::Junction(_) => 4,
        EntityLocation::FixedSubstrate(_) => 5,
        EntityLocation::MobileSubstrate(_) => 6,
        EntityLocation::PowerSource(_) => 7,
        EntityLocation::Quartz(_) => 8,
        EntityLocation::Deposit(_) => 9,
        EntityLocation::Enemy(_) => 10,
        EntityLocation::ConstructionSite(_) => 11,
    }
}

fn write_u8(value: u8, write: &mut dyn FnMut(&[u8])) {
    write(&[value]);
}

fn write_u16(value: u16, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_u64(value: u64, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityLocation, GateIndex, ProfileHash, WireIndex};

    fn contract() -> SimulationContract {
        SimulationContract {
            semantics_version: crate::SemanticsVersion::AonV1,
            numeric_profile_hash: ProfileHash::from_bytes([0x11; 32]),
            physical_scale_profile_hash: ProfileHash::from_bytes([0x22; 32]),
            balance_profile_hash: ProfileHash::from_bytes([0x33; 32]),
        }
    }

    #[test]
    fn state_encoding_has_exact_contract_tick_revision_and_identity_order() {
        let mut entities = EntityRegistry::new();
        entities
            .allocate(EntityLocation::Gate(GateIndex(7)))
            .expect("gate allocation succeeds");
        let removed = entities
            .allocate(EntityLocation::Wire(WireIndex(9)))
            .expect("wire allocation succeeds");
        entities.remove(removed).expect("wire removal succeeds");

        let mut actual = Vec::new();
        encode_state(&contract(), Tick(5), Revision(3), &entities, &mut |bytes| {
            actual.extend_from_slice(bytes)
        });

        let mut expected = Vec::new();
        expected.extend_from_slice(STATE_DOMAIN);
        expected.extend_from_slice(&STATE_ENCODER_VERSION.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&[0x11; 32]);
        expected.extend_from_slice(&[0x22; 32]);
        expected.extend_from_slice(&[0x33; 32]);
        expected.extend_from_slice(&5_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.push(1);
        expected.push(2);
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&[0_u8; EMPTY_STORE_COUNT * 8]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn tombstones_and_allocation_frontier_change_state_hash() {
        let empty = EntityRegistry::new();
        let mut tombstone = EntityRegistry::new();
        let id = tombstone
            .allocate(EntityLocation::MainCore)
            .expect("allocation succeeds");
        tombstone.remove(id).expect("removal succeeds");

        assert_ne!(
            state_hash(&contract(), Tick(0), Revision(0), &empty),
            state_hash(&contract(), Tick(0), Revision(0), &tombstone)
        );
    }

    #[test]
    fn dense_storage_compaction_does_not_change_state_hash() {
        let mut first = EntityRegistry::new();
        let id = first
            .allocate(EntityLocation::Gate(GateIndex(7)))
            .expect("allocation succeeds");
        let mut compacted = first.clone();
        compacted
            .update_location(id, EntityLocation::Gate(GateIndex(0)))
            .expect("live entity location updates");

        assert_eq!(
            state_hash(&contract(), Tick(0), Revision(0), &first),
            state_hash(&contract(), Tick(0), Revision(0), &compacted)
        );
    }
}
