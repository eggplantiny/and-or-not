use crate::{InitialWorld, ProfileArtifact, ProfileHash, StateHash};

const PROFILE_DOMAIN: &[u8] = b"AON\0PROFILE\0BOOTSTRAP\0";
const STATE_DOMAIN: &[u8] = b"AON\0STATE\0BOOTSTRAP\0";
const PROFILE_ENCODER_VERSION: u16 = 1;
const STATE_ENCODER_VERSION: u16 = 1;
const EMPTY_STORE_COUNT: usize = 8;

pub(crate) fn profile_hash(profile: &ProfileArtifact) -> ProfileHash {
    let mut encoder = CanonicalEncoder::new();
    encoder.push_bytes(PROFILE_DOMAIN);
    encoder.push_u16(PROFILE_ENCODER_VERSION);
    encoder.push_u32(profile.schema_version());
    encoder.push_u8(profile.kind().canonical_tag());
    encoder.push_string(profile.profile_id());
    ProfileHash::from_bytes(*blake3::hash(encoder.as_slice()).as_bytes())
}

pub(crate) fn state_hash(
    semantics_version: &str,
    numeric_profile: ProfileHash,
    physical_scale_profile: ProfileHash,
    balance_profile: ProfileHash,
    initial_world: &InitialWorld,
    next_tick: u64,
) -> StateHash {
    let bytes = encode_state(
        semantics_version,
        numeric_profile,
        physical_scale_profile,
        balance_profile,
        initial_world,
        next_tick,
    );
    StateHash::from_bytes(*blake3::hash(&bytes).as_bytes())
}

fn encode_state(
    semantics_version: &str,
    numeric_profile: ProfileHash,
    physical_scale_profile: ProfileHash,
    balance_profile: ProfileHash,
    initial_world: &InitialWorld,
    next_tick: u64,
) -> Vec<u8> {
    let mut encoder = CanonicalEncoder::new();
    encoder.push_bytes(STATE_DOMAIN);
    encoder.push_u16(STATE_ENCODER_VERSION);
    encoder.push_string(semantics_version);
    encoder.push_bytes(numeric_profile.as_bytes());
    encoder.push_bytes(physical_scale_profile.as_bytes());
    encoder.push_bytes(balance_profile.as_bytes());
    encoder.push_u8(initial_world.canonical_tag());
    encoder.push_u64(next_tick);

    // Entity registry, gates, wires, junctions, fixed substrates, mobile
    // substrates, scheduled events, and pending destructions are all empty.
    for _ in 0..EMPTY_STORE_COUNT {
        encoder.push_u64(0);
    }

    encoder.finish()
}

struct CanonicalEncoder {
    bytes: Vec<u8>,
}

impl CanonicalEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn push_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn push_u16(&mut self, value: u16) {
        self.push_bytes(&value.to_le_bytes());
    }

    fn push_u32(&mut self, value: u32) {
        self.push_bytes(&value.to_le_bytes());
    }

    fn push_u64(&mut self, value: u64) {
        self.push_bytes(&value.to_le_bytes());
    }

    fn push_string(&mut self, value: &str) {
        let length = u32::try_from(value.len()).expect("bootstrap identifier length fits in u32");
        self.push_u32(length);
        self.push_bytes(value.as_bytes());
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{STATE_DOMAIN, STATE_ENCODER_VERSION, encode_state};
    use crate::{InitialWorld, ProfileHash};

    #[test]
    fn empty_encoding_has_exact_field_order_and_widths() {
        let actual = encode_state(
            "x",
            ProfileHash::from_bytes([0x11; 32]),
            ProfileHash::from_bytes([0x22; 32]),
            ProfileHash::from_bytes([0x33; 32]),
            &InitialWorld::Empty,
            5,
        );

        let mut expected = Vec::new();
        expected.extend_from_slice(STATE_DOMAIN);
        expected.extend_from_slice(&STATE_ENCODER_VERSION.to_le_bytes());
        expected.extend_from_slice(&1_u32.to_le_bytes());
        expected.extend_from_slice(b"x");
        expected.extend_from_slice(&[0x11; 32]);
        expected.extend_from_slice(&[0x22; 32]);
        expected.extend_from_slice(&[0x33; 32]);
        expected.push(0);
        expected.extend_from_slice(&5_u64.to_le_bytes());
        expected.extend_from_slice(&[0_u8; 8 * 8]);

        assert_eq!(actual, expected);
    }
}
