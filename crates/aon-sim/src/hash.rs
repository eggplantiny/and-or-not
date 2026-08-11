use std::fmt;
use thiserror::Error;

const HASH_BYTE_LENGTH: usize = 32;
const HASH_HEX_LENGTH: usize = HASH_BYTE_LENGTH * 2;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum HashParseError {
    #[error("canonical hash must contain exactly 64 lowercase hexadecimal characters")]
    InvalidLength,

    #[error("canonical hash contains a non-lowercase-hexadecimal character at byte {index}")]
    InvalidCharacter { index: usize },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileHash([u8; 32]);

impl ProfileHash {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Result<Self, HashParseError> {
        parse_hash(value).map(Self)
    }
}

impl fmt::Display for ProfileHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(&self.0, formatter)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateHash([u8; 32]);

impl StateHash {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Result<Self, HashParseError> {
        parse_hash(value).map(Self)
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(&self.0, formatter)
    }
}

fn write_lower_hex(bytes: &[u8], formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}

fn parse_hash(value: &str) -> Result<[u8; HASH_BYTE_LENGTH], HashParseError> {
    let encoded = value.as_bytes();
    if encoded.len() != HASH_HEX_LENGTH {
        return Err(HashParseError::InvalidLength);
    }

    let mut decoded = [0_u8; HASH_BYTE_LENGTH];
    for (index, pair) in encoded.chunks_exact(2).enumerate() {
        let high = decode_lower_hex(pair[0], index * 2)?;
        let low = decode_lower_hex(pair[1], index * 2 + 1)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

fn decode_lower_hex(value: u8, index: usize) -> Result<u8, HashParseError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(HashParseError::InvalidCharacter { index }),
    }
}

#[cfg(test)]
mod tests {
    use super::{HashParseError, ProfileHash, StateHash, write_lower_hex};
    use std::fmt;

    #[test]
    fn state_hash_display_is_fixed_lowercase_hex() {
        let mut bytes = [0_u8; 32];
        bytes[0] = 0x0a;
        bytes[31] = 0xff;
        let hash = StateHash::from_bytes(bytes);

        assert_eq!(hash.to_string().len(), 64);
        assert_eq!(
            hash.to_string(),
            "0a000000000000000000000000000000000000000000000000000000000000ff"
        );
    }

    #[test]
    fn lower_hex_writer_propagates_formatter_result() {
        struct DisplayBytes;
        impl fmt::Display for DisplayBytes {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_lower_hex(&[0xab, 0xcd], formatter)
            }
        }

        assert_eq!(DisplayBytes.to_string(), "abcd");
    }

    #[test]
    fn canonical_hash_hex_round_trips() {
        let encoded = "0a000000000000000000000000000000000000000000000000000000000000ff";
        let hash = ProfileHash::from_hex(encoded).expect("golden hash is valid");

        assert_eq!(hash.to_string(), encoded);
    }

    #[test]
    fn hash_parser_rejects_wrong_length_and_uppercase() {
        assert_eq!(
            ProfileHash::from_hex("00"),
            Err(HashParseError::InvalidLength)
        );
        let mut uppercase = "0".repeat(64);
        uppercase.replace_range(0..1, "A");
        assert_eq!(
            ProfileHash::from_hex(&uppercase),
            Err(HashParseError::InvalidCharacter { index: 0 })
        );
    }
}
