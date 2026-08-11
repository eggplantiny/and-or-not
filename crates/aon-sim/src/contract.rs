use crate::profile::{ProfileBundle, ProfileValidationError};
use crate::{ProfileHash, ProfileKind};
use serde::{Deserialize, Deserializer};
use std::fmt;
use thiserror::Error;

pub const SEMANTICS_VERSION_V1: &str = "aon-semantics-v1";
pub const HASH_ALGORITHM_ID_BLAKE3_V1: &str = "blake3-v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticsVersion {
    #[default]
    AonV1,
}

impl SemanticsVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AonV1 => SEMANTICS_VERSION_V1,
        }
    }

    pub fn parse(value: &str) -> Result<Self, ContractValidationError> {
        match value {
            SEMANTICS_VERSION_V1 => Ok(Self::AonV1),
            actual => Err(ContractValidationError::UnsupportedSemanticsVersion {
                actual: actual.to_owned(),
            }),
        }
    }

    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::AonV1 => 0,
        }
    }
}

impl fmt::Display for SemanticsVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SemanticsVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HashAlgorithmId {
    #[default]
    Blake3V1,
}

impl HashAlgorithmId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blake3V1 => HASH_ALGORITHM_ID_BLAKE3_V1,
        }
    }

    pub fn parse(value: &str) -> Result<Self, ContractValidationError> {
        match value {
            HASH_ALGORITHM_ID_BLAKE3_V1 => Ok(Self::Blake3V1),
            actual => Err(ContractValidationError::UnsupportedHashAlgorithm {
                actual: actual.to_owned(),
            }),
        }
    }
}

impl fmt::Display for HashAlgorithmId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HashAlgorithmId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationContract {
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
    pub balance_profile_hash: ProfileHash,
}

impl SimulationContract {
    pub fn from_profiles(profiles: &ProfileBundle) -> Result<Self, ContractValidationError> {
        let hashes = profiles.canonical_hashes()?;
        Ok(Self {
            semantics_version: SemanticsVersion::AonV1,
            numeric_profile_hash: hashes.numeric,
            physical_scale_profile_hash: hashes.physical_scale,
            balance_profile_hash: hashes.balance,
        })
    }

    pub const fn hash_algorithm_id(&self) -> HashAlgorithmId {
        HashAlgorithmId::Blake3V1
    }

    pub fn validate_profiles(
        &self,
        profiles: &ProfileBundle,
    ) -> Result<(), ContractValidationError> {
        let hashes = profiles.canonical_hashes()?;
        compare_hash(
            ProfileKind::Numeric,
            self.numeric_profile_hash,
            hashes.numeric,
        )?;
        compare_hash(
            ProfileKind::PhysicalScale,
            self.physical_scale_profile_hash,
            hashes.physical_scale,
        )?;
        compare_hash(
            ProfileKind::Balance,
            self.balance_profile_hash,
            hashes.balance,
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ContractValidationError {
    #[error(transparent)]
    Profile(#[from] ProfileValidationError),

    #[error("unsupported semantics version `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },

    #[error("unsupported hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },

    #[error("{profile} profile hash mismatch: expected {expected}, got {actual}")]
    ProfileHashMismatch {
        profile: ProfileKind,
        expected: ProfileHash,
        actual: ProfileHash,
    },
}

fn compare_hash(
    profile: ProfileKind,
    expected: ProfileHash,
    actual: ProfileHash,
) -> Result<(), ContractValidationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ContractValidationError::ProfileHashMismatch {
            profile,
            expected,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{BalanceProfile, NumericProfile, PhysicalScaleProfile};

    fn bundle() -> ProfileBundle {
        ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical"),
            balance: BalanceProfile::stage0_alpha("balance"),
        }
    }

    #[test]
    fn version_and_algorithm_ids_accept_only_the_v1_contract() {
        assert_eq!(
            SemanticsVersion::parse("aon-semantics-v1"),
            Ok(SemanticsVersion::AonV1)
        );
        assert!(matches!(
            SemanticsVersion::parse("bootstrap-v0"),
            Err(ContractValidationError::UnsupportedSemanticsVersion { .. })
        ));
        assert_eq!(
            HashAlgorithmId::parse("blake3-v1"),
            Ok(HashAlgorithmId::Blake3V1)
        );
        assert!(matches!(
            HashAlgorithmId::parse("other"),
            Err(ContractValidationError::UnsupportedHashAlgorithm { .. })
        ));
    }

    #[test]
    fn contract_binds_all_three_canonical_profile_hashes() {
        let profiles = bundle();
        let contract = SimulationContract::from_profiles(&profiles).expect("valid bundle");
        assert_eq!(contract.validate_profiles(&profiles), Ok(()));
        assert_eq!(contract.hash_algorithm_id(), HashAlgorithmId::Blake3V1);

        let mut changed = profiles;
        changed.balance.quartz_period += 1;
        assert!(matches!(
            contract.validate_profiles(&changed),
            Err(ContractValidationError::ProfileHashMismatch {
                profile: ProfileKind::Balance,
                ..
            })
        ));
    }

    #[test]
    fn invalid_profile_is_rejected_before_contract_hash_comparison() {
        let mut profiles = bundle();
        let contract = SimulationContract::from_profiles(&profiles).expect("valid bundle");
        profiles.numeric.fixed_one = 1;
        assert!(matches!(
            contract.validate_profiles(&profiles),
            Err(ContractValidationError::Profile(
                ProfileValidationError::FixedOneMismatch { .. }
            ))
        ));
    }
}
