use crate::{FIXED_ONE, Fixed, ProfileHash, ProfileKind};
use serde::{Deserialize, Deserializer};
use std::fmt;
use thiserror::Error;

pub const PROFILE_SCHEMA_VERSION_V1: u32 = 1;
pub const PROFILE_SCHEMA_VERSION_V2: u32 = 2;

pub const REFERENCE_WIRE_GEOMETRY_QUANTUM: Fixed = Fixed(FIXED_ONE / 64);
pub const REFERENCE_CIRCUIT_ROUTING_PITCH: Fixed = Fixed(FIXED_ONE / 4);
pub const REFERENCE_WORLD_ROUTING_PITCH: Fixed = Fixed(FIXED_ONE);
pub const REFERENCE_WIRE_BODY_RADIUS: Fixed = Fixed(FIXED_ONE / 32);
pub const REFERENCE_GATE_MINIMUM_EXTENT: Fixed = Fixed(FIXED_ONE / 2);

const PROFILE_HASH_DOMAIN: &[u8] = b"AON\0PROFILE\0V1\0";
const PROFILE_ENCODER_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowPolicy {
    DeterministicError,
}

impl OverflowPolicy {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::DeterministicError => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DivisionProfile {
    FloorCeilNearestEven,
}

impl DivisionProfile {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::FloorCeilNearestEven => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GeometryLengthProfile {
    CeilIntegerEuclideanSqrt,
}

impl GeometryLengthProfile {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::CeilIntegerEuclideanSqrt => 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumericProfile {
    pub schema_version: u32,
    pub profile_id: String,
    pub kind: ProfileKind,
    pub fixed_one: i64,
    pub overflow: OverflowPolicy,
    pub division: DivisionProfile,
    pub geometry_length: GeometryLengthProfile,
}

impl NumericProfile {
    pub fn reference_v1(profile_id: impl Into<String>) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION_V1,
            profile_id: profile_id.into(),
            kind: ProfileKind::Numeric,
            fixed_one: FIXED_ONE,
            overflow: OverflowPolicy::DeterministicError,
            division: DivisionProfile::FloorCeilNearestEven,
            geometry_length: GeometryLengthProfile::CeilIntegerEuclideanSqrt,
        }
    }

    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        validate_header(
            self.schema_version,
            &self.profile_id,
            self.kind,
            ProfileKind::Numeric,
            PROFILE_SCHEMA_VERSION_V1,
        )?;
        if self.fixed_one != FIXED_ONE {
            return Err(ProfileValidationError::FixedOneMismatch {
                expected: FIXED_ONE,
                actual: self.fixed_one,
            });
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<ProfileHash, ProfileValidationError> {
        self.validate()?;
        Ok(streaming_hash(|write| self.encode_canonical(write)))
    }

    fn encode_canonical(&self, write: &mut dyn FnMut(&[u8])) {
        encode_header(ProfileKind::Numeric, self.schema_version, write);
        write_i64(self.fixed_one, write);
        write_u8(self.overflow.canonical_tag(), write);
        write_u8(self.division.canonical_tag(), write);
        write_u8(self.geometry_length.canonical_tag(), write);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GateFootprint {
    pub width: Fixed,
    pub height: Fixed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortAnchor {
    pub x: Fixed,
    pub y: Fixed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BinaryGatePortAnchors {
    pub input_a: PortAnchor,
    pub input_b: PortAnchor,
    pub output: PortAnchor,
    pub power: PortAnchor,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnaryGatePortAnchors {
    pub input: PortAnchor,
    pub output: PortAnchor,
    pub power: PortAnchor,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GateFootprintTable {
    #[serde(rename = "and")]
    pub and_gate: GateFootprint,
    #[serde(rename = "or")]
    pub or_gate: GateFootprint,
    #[serde(rename = "not")]
    pub not_gate: GateFootprint,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatePortTable {
    #[serde(rename = "and")]
    pub and_gate: BinaryGatePortAnchors,
    #[serde(rename = "or")]
    pub or_gate: BinaryGatePortAnchors,
    #[serde(rename = "not")]
    pub not_gate: UnaryGatePortAnchors,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysicalScaleProfile {
    pub schema_version: u32,
    pub profile_id: String,
    pub kind: ProfileKind,
    pub wire_geometry_quantum: Fixed,
    pub circuit_routing_pitch: Fixed,
    pub world_routing_pitch: Fixed,
    pub wire_body_radius: Fixed,
    pub gate_footprints: GateFootprintTable,
    pub gate_port_anchors: GatePortTable,
    pub substrate_clearance: Fixed,
}

impl PhysicalScaleProfile {
    pub fn stage0_alpha(profile_id: impl Into<String>) -> Self {
        let binary = BinaryGatePortAnchors {
            input_a: PortAnchor {
                x: Fixed(-16_384),
                y: Fixed(-8_192),
            },
            input_b: PortAnchor {
                x: Fixed(-16_384),
                y: Fixed(8_192),
            },
            output: PortAnchor {
                x: Fixed(16_384),
                y: Fixed(0),
            },
            power: PortAnchor {
                x: Fixed(0),
                y: Fixed(-16_384),
            },
        };
        let footprint = GateFootprint {
            width: REFERENCE_GATE_MINIMUM_EXTENT,
            height: REFERENCE_GATE_MINIMUM_EXTENT,
        };
        Self {
            schema_version: PROFILE_SCHEMA_VERSION_V1,
            profile_id: profile_id.into(),
            kind: ProfileKind::PhysicalScale,
            wire_geometry_quantum: REFERENCE_WIRE_GEOMETRY_QUANTUM,
            circuit_routing_pitch: REFERENCE_CIRCUIT_ROUTING_PITCH,
            world_routing_pitch: REFERENCE_WORLD_ROUTING_PITCH,
            wire_body_radius: REFERENCE_WIRE_BODY_RADIUS,
            gate_footprints: GateFootprintTable {
                and_gate: footprint,
                or_gate: footprint,
                not_gate: footprint,
            },
            gate_port_anchors: GatePortTable {
                and_gate: binary,
                or_gate: binary,
                not_gate: UnaryGatePortAnchors {
                    input: PortAnchor {
                        x: Fixed(-16_384),
                        y: Fixed(0),
                    },
                    output: PortAnchor {
                        x: Fixed(16_384),
                        y: Fixed(0),
                    },
                    power: PortAnchor {
                        x: Fixed(0),
                        y: Fixed(-16_384),
                    },
                },
            },
            substrate_clearance: REFERENCE_WIRE_BODY_RADIUS,
        }
    }

    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        validate_header(
            self.schema_version,
            &self.profile_id,
            self.kind,
            ProfileKind::PhysicalScale,
            PROFILE_SCHEMA_VERSION_V1,
        )?;

        let quantum = self.wire_geometry_quantum.0;
        require_positive(ProfileKind::PhysicalScale, "wireGeometryQuantum", quantum)?;
        for (field, value) in [
            ("circuitRoutingPitch", self.circuit_routing_pitch.0),
            ("worldRoutingPitch", self.world_routing_pitch.0),
            ("wireBodyRadius", self.wire_body_radius.0),
        ] {
            require_positive(ProfileKind::PhysicalScale, field, value)?;
            require_quantized(field, value, quantum)?;
        }
        if self.substrate_clearance.0 < 0 {
            return Err(ProfileValidationError::NegativeField {
                profile: ProfileKind::PhysicalScale,
                field: "substrateClearance",
            });
        }
        require_quantized("substrateClearance", self.substrate_clearance.0, quantum)?;

        validate_footprint("gateFootprints.and", self.gate_footprints.and_gate, quantum)?;
        validate_footprint("gateFootprints.or", self.gate_footprints.or_gate, quantum)?;
        validate_footprint("gateFootprints.not", self.gate_footprints.not_gate, quantum)?;

        validate_binary_anchors(
            "gatePortAnchors.and",
            self.gate_port_anchors.and_gate,
            self.gate_footprints.and_gate,
            quantum,
        )?;
        validate_binary_anchors(
            "gatePortAnchors.or",
            self.gate_port_anchors.or_gate,
            self.gate_footprints.or_gate,
            quantum,
        )?;
        validate_unary_anchors(
            "gatePortAnchors.not",
            self.gate_port_anchors.not_gate,
            self.gate_footprints.not_gate,
            quantum,
        )?;
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<ProfileHash, ProfileValidationError> {
        self.validate()?;
        Ok(streaming_hash(|write| self.encode_canonical(write)))
    }

    fn encode_canonical(&self, write: &mut dyn FnMut(&[u8])) {
        encode_header(ProfileKind::PhysicalScale, self.schema_version, write);
        for value in [
            self.wire_geometry_quantum,
            self.circuit_routing_pitch,
            self.world_routing_pitch,
            self.wire_body_radius,
        ] {
            write_i64(value.0, write);
        }
        encode_footprint(self.gate_footprints.and_gate, write);
        encode_footprint(self.gate_footprints.or_gate, write);
        encode_footprint(self.gate_footprints.not_gate, write);
        encode_binary_anchors(self.gate_port_anchors.and_gate, write);
        encode_binary_anchors(self.gate_port_anchors.or_gate, write);
        encode_unary_anchors(self.gate_port_anchors.not_gate, write);
        write_i64(self.substrate_clearance.0, write);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    numerator: i64,
    denominator: i64,
}

impl Rational {
    pub fn new(numerator: i64, denominator: i64) -> Result<Self, ProfileValidationError> {
        if denominator == 0 {
            return Err(ProfileValidationError::ZeroRationalDenominator);
        }

        let mut numerator = i128::from(numerator);
        let mut denominator = i128::from(denominator);
        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or(ProfileValidationError::RationalOutOfRange)?;
            denominator = -denominator;
        }

        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }

        let divisor = gcd(numerator.unsigned_abs(), denominator as u128);
        numerator /= divisor as i128;
        denominator /= divisor as i128;
        Ok(Self {
            numerator: i64::try_from(numerator)
                .map_err(|_| ProfileValidationError::RationalOutOfRange)?,
            denominator: i64::try_from(denominator)
                .map_err(|_| ProfileValidationError::RationalOutOfRange)?,
        })
    }

    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    pub const fn denominator(self) -> i64 {
        self.denominator
    }

    const fn is_positive(self) -> bool {
        self.numerator > 0
    }

    const fn is_nonnegative(self) -> bool {
        self.numerator >= 0
    }

    fn is_at_most_one(self) -> bool {
        self.numerator <= self.denominator
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct RationalWire {
            numerator: i64,
            denominator: i64,
        }

        let wire = RationalWire::deserialize(deserializer)?;
        Self::new(wire.numerator, wire.denominator).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityProbeProfile {
    pub main_core_capacity: u64,
    pub relay_capacity: u64,
    pub overcap_linear_k: Rational,
    pub overcap_quadratic_k: Rational,
    pub capacity_denominator_floor: u64,
    pub relay_offline_grace_ticks: u64,
    pub support_heat_fraction: Rational,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrientationWeightTable {
    pub broadside_near: u64,
    pub diagonal: u64,
    pub endfire_near: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrientationBoundaryMultipliers {
    pub broadside_abs_cross_multiplier: u64,
    pub endfire_abs_dot_multiplier: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadiationReferenceProfile {
    pub distance_weights: [u64; 5],
    pub delays: [u64; 5],
    pub orientation_weights: OrientationWeightTable,
    pub orientation_boundaries: OrientationBoundaryMultipliers,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalanceProfile {
    pub schema_version: u32,
    pub profile_id: String,
    pub kind: ProfileKind,
    pub simulation_hz: u32,
    pub gate_base_delay: u64,
    pub gate_switch_base_energy: u64,
    pub sense_delay: u64,
    pub logic_threshold: u64,
    pub nominal_gate_drive: u64,
    pub input_load: u64,
    #[serde(rename = "wireLoadPerWU")]
    pub wire_load_per_wu: Rational,
    pub fanout_free_load: u64,
    pub fanout_step: u64,
    pub wire_linear_k: Rational,
    pub wire_quadratic_k: Rational,
    pub logic_operate_threshold: Rational,
    pub brownout_delay_floor: Rational,
    pub sense_radius: Fixed,
    pub quartz_period: u64,
    pub radiation_cell_size: Fixed,
    #[serde(default)]
    pub capacity_probe: Option<CapacityProbeProfile>,
    #[serde(default)]
    pub radiation_reference: Option<RadiationReferenceProfile>,
}

impl BalanceProfile {
    pub fn stage0_alpha(profile_id: impl Into<String>) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION_V2,
            profile_id: profile_id.into(),
            kind: ProfileKind::Balance,
            simulation_hz: 20,
            gate_base_delay: 1,
            gate_switch_base_energy: 1,
            sense_delay: 1,
            logic_threshold: 100,
            nominal_gate_drive: 400,
            input_load: 1,
            wire_load_per_wu: ratio(1, 1),
            fanout_free_load: 4,
            fanout_step: 4,
            wire_linear_k: ratio(1, 10),
            wire_quadratic_k: ratio(1, 40),
            logic_operate_threshold: ratio(1, 5),
            brownout_delay_floor: ratio(1, 5),
            sense_radius: Fixed(FIXED_ONE + FIXED_ONE / 4),
            quartz_period: 8,
            radiation_cell_size: Fixed(FIXED_ONE),
            capacity_probe: None,
            radiation_reference: None,
        }
    }

    pub fn capacity_probe_alpha(profile_id: impl Into<String>) -> Self {
        let mut profile = Self::stage0_alpha(profile_id);
        profile.capacity_probe = Some(CapacityProbeProfile {
            main_core_capacity: 1_000,
            relay_capacity: 500,
            overcap_linear_k: ratio(1, 1),
            overcap_quadratic_k: ratio(2, 1),
            capacity_denominator_floor: 1,
            relay_offline_grace_ticks: 1,
            support_heat_fraction: ratio(1, 4),
        });
        profile
    }

    pub fn radiation_reference_alpha(profile_id: impl Into<String>) -> Self {
        let mut profile = Self::stage0_alpha(profile_id);
        profile.radiation_reference = Some(RadiationReferenceProfile {
            distance_weights: [16, 8, 4, 2, 1],
            delays: [1, 1, 2, 3, 4],
            orientation_weights: OrientationWeightTable {
                broadside_near: 4,
                diagonal: 2,
                endfire_near: 1,
            },
            orientation_boundaries: OrientationBoundaryMultipliers {
                broadside_abs_cross_multiplier: 2,
                endfire_abs_dot_multiplier: 2,
            },
        });
        profile
    }

    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        validate_header(
            self.schema_version,
            &self.profile_id,
            self.kind,
            ProfileKind::Balance,
            PROFILE_SCHEMA_VERSION_V2,
        )?;
        for (field, valid) in [
            ("simulationHz", self.simulation_hz > 0),
            ("gateBaseDelay", self.gate_base_delay > 0),
            ("gateSwitchBaseEnergy", self.gate_switch_base_energy > 0),
            ("senseDelay", self.sense_delay > 0),
            ("logicThreshold", self.logic_threshold > 0),
            ("nominalGateDrive", self.nominal_gate_drive > 0),
            ("inputLoad", self.input_load > 0),
            ("fanoutStep", self.fanout_step > 0),
            ("quartzPeriod", self.quartz_period > 0),
        ] {
            if !valid {
                return Err(ProfileValidationError::NonPositiveField {
                    profile: ProfileKind::Balance,
                    field,
                });
            }
        }
        if self.nominal_gate_drive < self.logic_threshold {
            return Err(ProfileValidationError::InvalidBalanceRelation {
                field: "nominalGateDrive",
                relation: "must be greater than or equal to logicThreshold",
            });
        }
        require_positive_rational("wireLoadPerWU", self.wire_load_per_wu)?;
        require_nonnegative_rational("wireLinearK", self.wire_linear_k)?;
        require_positive_rational("wireQuadraticK", self.wire_quadratic_k)?;
        require_unit_interval("logicOperateThreshold", self.logic_operate_threshold)?;
        require_unit_interval("brownoutDelayFloor", self.brownout_delay_floor)?;
        require_positive(ProfileKind::Balance, "senseRadius", self.sense_radius.0)?;
        require_positive(
            ProfileKind::Balance,
            "radiationCellSize",
            self.radiation_cell_size.0,
        )?;
        if let Some(capacity_probe) = self.capacity_probe {
            validate_capacity_probe(capacity_probe)?;
        }
        if let Some(radiation_reference) = self.radiation_reference {
            validate_radiation_reference(radiation_reference)?;
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<ProfileHash, ProfileValidationError> {
        self.validate()?;
        Ok(streaming_hash(|write| self.encode_canonical(write)))
    }

    fn encode_canonical(&self, write: &mut dyn FnMut(&[u8])) {
        encode_header(ProfileKind::Balance, self.schema_version, write);
        write_u32(self.simulation_hz, write);
        for value in [
            self.gate_base_delay,
            self.gate_switch_base_energy,
            self.sense_delay,
            self.logic_threshold,
            self.nominal_gate_drive,
            self.input_load,
        ] {
            write_u64(value, write);
        }
        encode_rational(self.wire_load_per_wu, write);
        write_u64(self.fanout_free_load, write);
        write_u64(self.fanout_step, write);
        encode_rational(self.wire_linear_k, write);
        encode_rational(self.wire_quadratic_k, write);
        encode_rational(self.logic_operate_threshold, write);
        encode_rational(self.brownout_delay_floor, write);
        write_i64(self.sense_radius.0, write);
        write_u64(self.quartz_period, write);
        write_i64(self.radiation_cell_size.0, write);
        match self.capacity_probe {
            None => write_u8(0, write),
            Some(capacity_probe) => {
                write_u8(1, write);
                encode_capacity_probe(capacity_probe, write);
            }
        }
        match self.radiation_reference {
            None => write_u8(0, write),
            Some(radiation_reference) => {
                write_u8(1, write);
                encode_radiation_reference(radiation_reference, write);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileBundle {
    pub numeric: NumericProfile,
    pub physical_scale: PhysicalScaleProfile,
    pub balance: BalanceProfile,
}

impl ProfileBundle {
    pub const fn numeric(&self) -> &NumericProfile {
        &self.numeric
    }

    pub const fn physical_scale(&self) -> &PhysicalScaleProfile {
        &self.physical_scale
    }

    pub const fn balance(&self) -> &BalanceProfile {
        &self.balance
    }

    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        self.numeric.validate()?;
        self.physical_scale.validate()?;
        self.balance.validate()?;
        Ok(())
    }

    pub fn canonical_hashes(&self) -> Result<ProfileHashes, ProfileValidationError> {
        Ok(ProfileHashes {
            numeric: self.numeric.canonical_hash()?,
            physical_scale: self.physical_scale.canonical_hash()?,
            balance: self.balance.canonical_hash()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileHashes {
    pub numeric: ProfileHash,
    pub physical_scale: ProfileHash,
    pub balance: ProfileHash,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProfileValidationError {
    #[error("unsupported profile schema: expected {expected}, got {actual}")]
    UnsupportedSchema { expected: u32, actual: u32 },

    #[error("profileId must not be empty")]
    EmptyProfileId,

    #[error("profile kind mismatch: expected {expected}, got {actual}")]
    ProfileKindMismatch {
        expected: ProfileKind,
        actual: ProfileKind,
    },

    #[error("numeric fixedOne mismatch: expected {expected}, got {actual}")]
    FixedOneMismatch { expected: i64, actual: i64 },

    #[error("{profile} profile field `{field}` must be positive")]
    NonPositiveField {
        profile: ProfileKind,
        field: &'static str,
    },

    #[error("{profile} profile field `{field}` must not be negative")]
    NegativeField {
        profile: ProfileKind,
        field: &'static str,
    },

    #[error("physical-scale profile field `{field}` is not aligned to wireGeometryQuantum")]
    NotQuantized { field: String },

    #[error("physical-scale profile anchor `{field}` lies outside its gate footprint")]
    AnchorOutsideFootprint { field: String },

    #[error("physical-scale profile anchor `{field}` does not lie on its gate footprint boundary")]
    AnchorNotOnFootprintBoundary { field: String },

    #[error(
        "physical-scale profile gate footprint `{field}` cannot be centered on the geometry quantum"
    )]
    GateFootprintNotCenterable { field: String },

    #[error("rational denominator must not be zero")]
    ZeroRationalDenominator,

    #[error("normalized rational does not fit signed 64-bit fields")]
    RationalOutOfRange,

    #[error("balance profile field `{field}` must be in the interval (0, 1]")]
    OutsideUnitInterval { field: &'static str },

    #[error("balance profile field `{field}` {relation}")]
    InvalidBalanceRelation {
        field: &'static str,
        relation: &'static str,
    },
}

fn validate_header(
    schema_version: u32,
    profile_id: &str,
    actual_kind: ProfileKind,
    expected_kind: ProfileKind,
    expected_schema_version: u32,
) -> Result<(), ProfileValidationError> {
    if schema_version != expected_schema_version {
        return Err(ProfileValidationError::UnsupportedSchema {
            expected: expected_schema_version,
            actual: schema_version,
        });
    }
    if profile_id.trim().is_empty() {
        return Err(ProfileValidationError::EmptyProfileId);
    }
    if actual_kind != expected_kind {
        return Err(ProfileValidationError::ProfileKindMismatch {
            expected: expected_kind,
            actual: actual_kind,
        });
    }
    Ok(())
}

fn require_positive(
    profile: ProfileKind,
    field: &'static str,
    value: i64,
) -> Result<(), ProfileValidationError> {
    if value > 0 {
        Ok(())
    } else {
        Err(ProfileValidationError::NonPositiveField { profile, field })
    }
}

fn require_quantized(
    field: impl Into<String>,
    value: i64,
    quantum: i64,
) -> Result<(), ProfileValidationError> {
    if value.rem_euclid(quantum) == 0 {
        Ok(())
    } else {
        Err(ProfileValidationError::NotQuantized {
            field: field.into(),
        })
    }
}

fn validate_footprint(
    field: &str,
    footprint: GateFootprint,
    quantum: i64,
) -> Result<(), ProfileValidationError> {
    if footprint.width.0 <= 0 || footprint.height.0 <= 0 {
        return Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::PhysicalScale,
            field: "gateFootprintExtent",
        });
    }
    require_quantized(format!("{field}.width"), footprint.width.0, quantum)?;
    require_quantized(format!("{field}.height"), footprint.height.0, quantum)?;
    let Some(centered_quantum) = quantum.checked_mul(2) else {
        return Err(ProfileValidationError::GateFootprintNotCenterable {
            field: field.to_owned(),
        });
    };
    if footprint.width.0.rem_euclid(centered_quantum) != 0
        || footprint.height.0.rem_euclid(centered_quantum) != 0
    {
        return Err(ProfileValidationError::GateFootprintNotCenterable {
            field: field.to_owned(),
        });
    }
    Ok(())
}

fn validate_anchor(
    field: impl Into<String>,
    anchor: PortAnchor,
    footprint: GateFootprint,
    quantum: i64,
) -> Result<(), ProfileValidationError> {
    let field = field.into();
    require_quantized(format!("{field}.x"), anchor.x.0, quantum)?;
    require_quantized(format!("{field}.y"), anchor.y.0, quantum)?;
    let x = i128::from(anchor.x.0).abs();
    let y = i128::from(anchor.y.0).abs();
    let half_width = i128::from(footprint.width.0) / 2;
    let half_height = i128::from(footprint.height.0) / 2;
    if x > half_width || y > half_height {
        return Err(ProfileValidationError::AnchorOutsideFootprint { field });
    }
    if x != half_width && y != half_height {
        return Err(ProfileValidationError::AnchorNotOnFootprintBoundary { field });
    }
    Ok(())
}

fn validate_binary_anchors(
    field: &str,
    anchors: BinaryGatePortAnchors,
    footprint: GateFootprint,
    quantum: i64,
) -> Result<(), ProfileValidationError> {
    validate_anchor(
        format!("{field}.inputA"),
        anchors.input_a,
        footprint,
        quantum,
    )?;
    validate_anchor(
        format!("{field}.inputB"),
        anchors.input_b,
        footprint,
        quantum,
    )?;
    validate_anchor(
        format!("{field}.output"),
        anchors.output,
        footprint,
        quantum,
    )?;
    validate_anchor(format!("{field}.power"), anchors.power, footprint, quantum)
}

fn validate_unary_anchors(
    field: &str,
    anchors: UnaryGatePortAnchors,
    footprint: GateFootprint,
    quantum: i64,
) -> Result<(), ProfileValidationError> {
    validate_anchor(format!("{field}.input"), anchors.input, footprint, quantum)?;
    validate_anchor(
        format!("{field}.output"),
        anchors.output,
        footprint,
        quantum,
    )?;
    validate_anchor(format!("{field}.power"), anchors.power, footprint, quantum)
}

fn require_positive_rational(
    field: &'static str,
    value: Rational,
) -> Result<(), ProfileValidationError> {
    if value.is_positive() {
        Ok(())
    } else {
        Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field,
        })
    }
}

fn require_nonnegative_rational(
    field: &'static str,
    value: Rational,
) -> Result<(), ProfileValidationError> {
    if value.is_nonnegative() {
        Ok(())
    } else {
        Err(ProfileValidationError::NegativeField {
            profile: ProfileKind::Balance,
            field,
        })
    }
}

fn require_unit_interval(
    field: &'static str,
    value: Rational,
) -> Result<(), ProfileValidationError> {
    if value.is_positive() && value.is_at_most_one() {
        Ok(())
    } else {
        Err(ProfileValidationError::OutsideUnitInterval { field })
    }
}

fn validate_capacity_probe(profile: CapacityProbeProfile) -> Result<(), ProfileValidationError> {
    for (field, valid) in [
        (
            "capacityProbe.mainCoreCapacity",
            profile.main_core_capacity > 0,
        ),
        ("capacityProbe.relayCapacity", profile.relay_capacity > 0),
        (
            "capacityProbe.capacityDenominatorFloor",
            profile.capacity_denominator_floor > 0,
        ),
        (
            "capacityProbe.relayOfflineGraceTicks",
            profile.relay_offline_grace_ticks > 0,
        ),
    ] {
        if !valid {
            return Err(ProfileValidationError::NonPositiveField {
                profile: ProfileKind::Balance,
                field,
            });
        }
    }
    require_nonnegative_rational("capacityProbe.overcapLinearK", profile.overcap_linear_k)?;
    require_nonnegative_rational(
        "capacityProbe.overcapQuadraticK",
        profile.overcap_quadratic_k,
    )?;
    require_unit_interval(
        "capacityProbe.supportHeatFraction",
        profile.support_heat_fraction,
    )
}

fn validate_radiation_reference(
    profile: RadiationReferenceProfile,
) -> Result<(), ProfileValidationError> {
    if profile.distance_weights.contains(&0) {
        return Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "radiationReference.distanceWeights",
        });
    }
    if !profile
        .distance_weights
        .windows(2)
        .all(|pair| pair[0] > pair[1])
    {
        return Err(ProfileValidationError::InvalidBalanceRelation {
            field: "radiationReference.distanceWeights",
            relation: "must be strictly decreasing by distance",
        });
    }
    if profile.delays.contains(&0) {
        return Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "radiationReference.delays",
        });
    }
    if !profile.delays.windows(2).all(|pair| pair[0] <= pair[1]) {
        return Err(ProfileValidationError::InvalidBalanceRelation {
            field: "radiationReference.delays",
            relation: "must be nondecreasing by distance",
        });
    }

    let weights = profile.orientation_weights;
    if weights.broadside_near == 0 || weights.diagonal == 0 || weights.endfire_near == 0 {
        return Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "radiationReference.orientationWeights",
        });
    }
    if !(weights.broadside_near > weights.diagonal && weights.diagonal > weights.endfire_near) {
        return Err(ProfileValidationError::InvalidBalanceRelation {
            field: "radiationReference.orientationWeights",
            relation: "must satisfy broadsideNear > diagonal > endfireNear",
        });
    }

    let boundaries = profile.orientation_boundaries;
    if boundaries.broadside_abs_cross_multiplier == 0 || boundaries.endfire_abs_dot_multiplier == 0
    {
        return Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "radiationReference.orientationBoundaries",
        });
    }
    Ok(())
}

fn ratio(numerator: i64, denominator: i64) -> Rational {
    Rational::new(numerator, denominator).expect("reference ratio is valid")
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn streaming_hash(encode: impl FnOnce(&mut dyn FnMut(&[u8]))) -> ProfileHash {
    let mut hasher = blake3::Hasher::new();
    encode(&mut |bytes| {
        hasher.update(bytes);
    });
    ProfileHash::from_bytes(*hasher.finalize().as_bytes())
}

fn encode_header(kind: ProfileKind, schema_version: u32, write: &mut dyn FnMut(&[u8])) {
    write(PROFILE_HASH_DOMAIN);
    write_u16(PROFILE_ENCODER_VERSION, write);
    write_u8(kind.canonical_tag(), write);
    write_u32(schema_version, write);
}

fn encode_footprint(footprint: GateFootprint, write: &mut dyn FnMut(&[u8])) {
    write_i64(footprint.width.0, write);
    write_i64(footprint.height.0, write);
}

fn encode_anchor(anchor: PortAnchor, write: &mut dyn FnMut(&[u8])) {
    write_i64(anchor.x.0, write);
    write_i64(anchor.y.0, write);
}

fn encode_binary_anchors(anchors: BinaryGatePortAnchors, write: &mut dyn FnMut(&[u8])) {
    encode_anchor(anchors.input_a, write);
    encode_anchor(anchors.input_b, write);
    encode_anchor(anchors.output, write);
    encode_anchor(anchors.power, write);
}

fn encode_unary_anchors(anchors: UnaryGatePortAnchors, write: &mut dyn FnMut(&[u8])) {
    encode_anchor(anchors.input, write);
    encode_anchor(anchors.output, write);
    encode_anchor(anchors.power, write);
}

fn encode_rational(value: Rational, write: &mut dyn FnMut(&[u8])) {
    write_i64(value.numerator, write);
    write_i64(value.denominator, write);
}

fn encode_capacity_probe(profile: CapacityProbeProfile, write: &mut dyn FnMut(&[u8])) {
    write_u64(profile.main_core_capacity, write);
    write_u64(profile.relay_capacity, write);
    encode_rational(profile.overcap_linear_k, write);
    encode_rational(profile.overcap_quadratic_k, write);
    write_u64(profile.capacity_denominator_floor, write);
    write_u64(profile.relay_offline_grace_ticks, write);
    encode_rational(profile.support_heat_fraction, write);
}

fn encode_radiation_reference(profile: RadiationReferenceProfile, write: &mut dyn FnMut(&[u8])) {
    for value in profile.distance_weights {
        write_u64(value, write);
    }
    for value in profile.delays {
        write_u64(value, write);
    }
    for value in [
        profile.orientation_weights.broadside_near,
        profile.orientation_weights.diagonal,
        profile.orientation_weights.endfire_near,
        profile
            .orientation_boundaries
            .broadside_abs_cross_multiplier,
        profile.orientation_boundaries.endfire_abs_dot_multiplier,
    ] {
        write_u64(value, write);
    }
}

fn write_u8(value: u8, write: &mut dyn FnMut(&[u8])) {
    write(&[value]);
}

fn write_u16(value: u16, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_u32(value: u32, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_u64(value: u64, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_i64(value: i64, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

impl<'de> Deserialize<'de> for Fixed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        i64::deserialize(deserializer).map(Self)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physical(profile_id: &str) -> PhysicalScaleProfile {
        PhysicalScaleProfile::stage0_alpha(profile_id)
    }

    #[test]
    fn profile_id_is_validated_but_excluded_from_all_profile_hashes() {
        let numeric_a = NumericProfile::reference_v1("numeric-a");
        let numeric_b = NumericProfile::reference_v1("numeric-b");
        assert_eq!(numeric_a.canonical_hash(), numeric_b.canonical_hash());

        let physical_a = physical("physical-a");
        let physical_b = physical("physical-b");
        assert_eq!(physical_a.canonical_hash(), physical_b.canonical_hash());

        let balance_a = BalanceProfile::stage0_alpha("balance-a");
        let balance_b = BalanceProfile::stage0_alpha("balance-b");
        assert_eq!(balance_a.canonical_hash(), balance_b.canonical_hash());
    }

    #[test]
    fn numeric_hash_uses_fixed_domain_version_kind_schema_and_little_endian_fields() {
        let profile = NumericProfile::reference_v1("not-hashed");
        let mut actual = Vec::new();
        profile.encode_canonical(&mut |bytes| actual.extend_from_slice(bytes));

        let mut expected = Vec::new();
        expected.extend_from_slice(PROFILE_HASH_DOMAIN);
        expected.extend_from_slice(&PROFILE_ENCODER_VERSION.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&PROFILE_SCHEMA_VERSION_V1.to_le_bytes());
        expected.extend_from_slice(&FIXED_ONE.to_le_bytes());
        expected.extend_from_slice(&[0, 0, 0]);
        assert_eq!(actual, expected);
        assert!(
            !actual
                .windows(b"not-hashed".len())
                .any(|w| w == b"not-hashed")
        );
    }

    #[test]
    fn balance_v2_hashes_schema_and_switch_energy_in_fixed_field_order() {
        let profile = BalanceProfile::stage0_alpha("not-hashed");
        let mut actual = Vec::new();
        profile.encode_canonical(&mut |bytes| actual.extend_from_slice(bytes));

        let mut expected_prefix = Vec::new();
        expected_prefix.extend_from_slice(PROFILE_HASH_DOMAIN);
        expected_prefix.extend_from_slice(&PROFILE_ENCODER_VERSION.to_le_bytes());
        expected_prefix.push(ProfileKind::Balance.canonical_tag());
        expected_prefix.extend_from_slice(&PROFILE_SCHEMA_VERSION_V2.to_le_bytes());
        expected_prefix.extend_from_slice(&profile.simulation_hz.to_le_bytes());
        expected_prefix.extend_from_slice(&profile.gate_base_delay.to_le_bytes());
        expected_prefix.extend_from_slice(&profile.gate_switch_base_energy.to_le_bytes());
        assert!(actual.starts_with(&expected_prefix));

        let mut changed = profile.clone();
        changed.gate_switch_base_energy = 2;
        assert_ne!(profile.canonical_hash(), changed.canonical_hash());
    }

    #[test]
    fn rational_is_normalized_before_hashing() {
        assert_eq!(Rational::new(2, 20), Ok(ratio(1, 10)));
        assert_eq!(Rational::new(-2, -20), Ok(ratio(1, 10)));
        assert_eq!(Rational::new(0, -20), Ok(ratio(0, 1)));

        let mut left = BalanceProfile::stage0_alpha("left");
        let mut right = BalanceProfile::stage0_alpha("right");
        left.wire_linear_k = Rational::new(1, 10).expect("valid ratio");
        right.wire_linear_k = Rational::new(2, 20).expect("valid ratio");
        assert_eq!(left.canonical_hash(), right.canonical_hash());
    }

    #[test]
    fn rational_json_rejects_zero_and_unknown_fields() {
        assert!(serde_json::from_str::<Rational>(r#"{"numerator":1,"denominator":0}"#).is_err());
        assert!(
            serde_json::from_str::<Rational>(r#"{"numerator":1,"denominator":2,"extra":3}"#)
                .is_err()
        );
    }

    #[test]
    fn typed_profiles_reject_wrong_schema_kind_and_invariants() {
        let mut numeric = NumericProfile::reference_v1("numeric");
        numeric.schema_version = 2;
        assert_eq!(
            numeric.validate(),
            Err(ProfileValidationError::UnsupportedSchema {
                expected: 1,
                actual: 2,
            })
        );

        let mut wrong_kind = physical("physical");
        wrong_kind.kind = ProfileKind::Balance;
        assert!(matches!(
            wrong_kind.validate(),
            Err(ProfileValidationError::ProfileKindMismatch { .. })
        ));

        let mut physical = physical("physical");
        physical.gate_port_anchors.and_gate.input_a.x = Fixed(1);
        assert!(matches!(
            physical.validate(),
            Err(ProfileValidationError::NotQuantized { .. })
        ));

        let mut uncentered = PhysicalScaleProfile::stage0_alpha("physical");
        uncentered.gate_footprints.and_gate.width = uncentered
            .gate_footprints
            .and_gate
            .width
            .checked_add(uncentered.wire_geometry_quantum)
            .expect("test extent remains in range");
        assert!(matches!(
            uncentered.validate(),
            Err(ProfileValidationError::GateFootprintNotCenterable { .. })
        ));

        let mut out_of_bounds = PhysicalScaleProfile::stage0_alpha("physical");
        out_of_bounds.gate_port_anchors.and_gate.input_a.x = Fixed(32_768);
        assert!(matches!(
            out_of_bounds.validate(),
            Err(ProfileValidationError::AnchorOutsideFootprint { .. })
        ));

        let mut inside_footprint = PhysicalScaleProfile::stage0_alpha("physical");
        inside_footprint.gate_port_anchors.and_gate.input_a = PortAnchor {
            x: Fixed(0),
            y: Fixed(0),
        };
        assert!(matches!(
            inside_footprint.validate(),
            Err(ProfileValidationError::AnchorNotOnFootprintBoundary { .. })
        ));

        let mut balance = BalanceProfile::stage0_alpha("balance");
        balance.gate_base_delay = 0;
        assert_eq!(
            balance.validate(),
            Err(ProfileValidationError::NonPositiveField {
                profile: ProfileKind::Balance,
                field: "gateBaseDelay",
            })
        );

        let mut old_balance = BalanceProfile::stage0_alpha("balance");
        old_balance.schema_version = PROFILE_SCHEMA_VERSION_V1;
        assert_eq!(
            old_balance.validate(),
            Err(ProfileValidationError::UnsupportedSchema {
                expected: PROFILE_SCHEMA_VERSION_V2,
                actual: PROFILE_SCHEMA_VERSION_V1,
            })
        );

        let mut zero_switch_energy = BalanceProfile::stage0_alpha("balance");
        zero_switch_energy.gate_switch_base_energy = 0;
        assert_eq!(
            zero_switch_energy.validate(),
            Err(ProfileValidationError::NonPositiveField {
                profile: ProfileKind::Balance,
                field: "gateSwitchBaseEnergy",
            })
        );

        let mut zero_quadratic_delay = BalanceProfile::stage0_alpha("balance");
        zero_quadratic_delay.wire_quadratic_k = ratio(0, 1);
        assert_eq!(
            zero_quadratic_delay.validate(),
            Err(ProfileValidationError::NonPositiveField {
                profile: ProfileKind::Balance,
                field: "wireQuadraticK",
            })
        );
    }

    #[test]
    fn json_order_and_whitespace_do_not_affect_semantic_hash() {
        let first: NumericProfile = serde_json::from_str(
            r#"{
                "schemaVersion": 1,
                "profileId": "first",
                "kind": "numeric",
                "fixedOne": 65536,
                "overflow": "deterministic-error",
                "division": "floor-ceil-nearest-even",
                "geometryLength": "ceil-integer-euclidean-sqrt"
            }"#,
        )
        .expect("valid numeric profile");
        let second: NumericProfile = serde_json::from_str(
            r#"{"geometryLength":"ceil-integer-euclidean-sqrt","division":"floor-ceil-nearest-even","overflow":"deterministic-error","fixedOne":65536,"kind":"numeric","profileId":"second","schemaVersion":1}"#,
        )
        .expect("valid numeric profile");

        assert_eq!(first.canonical_hash(), second.canonical_hash());
    }

    #[test]
    fn profile_json_rejects_duplicate_unknown_and_float_fields() {
        let duplicate = r#"{
            "schemaVersion":1,"schemaVersion":1,"profileId":"n","kind":"numeric",
            "fixedOne":65536,"overflow":"deterministic-error",
            "division":"floor-ceil-nearest-even",
            "geometryLength":"ceil-integer-euclidean-sqrt"
        }"#;
        let unknown = r#"{
            "schemaVersion":1,"profileId":"n","kind":"numeric","extra":0,
            "fixedOne":65536,"overflow":"deterministic-error",
            "division":"floor-ceil-nearest-even",
            "geometryLength":"ceil-integer-euclidean-sqrt"
        }"#;
        let float = r#"{
            "schemaVersion":1,"profileId":"n","kind":"numeric",
            "fixedOne":65536.0,"overflow":"deterministic-error",
            "division":"floor-ceil-nearest-even",
            "geometryLength":"ceil-integer-euclidean-sqrt"
        }"#;
        assert!(serde_json::from_str::<NumericProfile>(duplicate).is_err());
        assert!(serde_json::from_str::<NumericProfile>(unknown).is_err());
        assert!(serde_json::from_str::<NumericProfile>(float).is_err());
    }

    #[test]
    fn stage0_balance_constructor_contains_all_reference_values() {
        let profile = BalanceProfile::stage0_alpha("stage0-alpha");
        assert_eq!(profile.schema_version, PROFILE_SCHEMA_VERSION_V2);
        assert_eq!(profile.simulation_hz, 20);
        assert_eq!(profile.gate_base_delay, 1);
        assert_eq!(profile.gate_switch_base_energy, 1);
        assert_eq!(profile.sense_delay, 1);
        assert_eq!(profile.logic_threshold, 100);
        assert_eq!(profile.nominal_gate_drive, 400);
        assert_eq!(profile.input_load, 1);
        assert_eq!(profile.wire_load_per_wu, ratio(1, 1));
        assert_eq!(profile.fanout_free_load, 4);
        assert_eq!(profile.fanout_step, 4);
        assert_eq!(profile.wire_linear_k, ratio(1, 10));
        assert_eq!(profile.wire_quadratic_k, ratio(1, 40));
        assert_eq!(profile.logic_operate_threshold, ratio(1, 5));
        assert_eq!(profile.brownout_delay_floor, ratio(1, 5));
        assert_eq!(profile.sense_radius, Fixed(81_920));
        assert_eq!(profile.quartz_period, 8);
        assert_eq!(profile.radiation_cell_size, Fixed(65_536));
        assert_eq!(profile.capacity_probe, None);
        assert_eq!(profile.radiation_reference, None);
        assert_eq!(profile.validate(), Ok(()));
    }

    #[test]
    fn optional_probe_constructors_contain_the_reference_tables() {
        let capacity = BalanceProfile::capacity_probe_alpha("capacity-probe")
            .capacity_probe
            .expect("capacity probe is present");
        assert_eq!(capacity.main_core_capacity, 1_000);
        assert_eq!(capacity.relay_capacity, 500);
        assert_eq!(capacity.overcap_linear_k, ratio(1, 1));
        assert_eq!(capacity.overcap_quadratic_k, ratio(2, 1));
        assert_eq!(capacity.capacity_denominator_floor, 1);
        assert_eq!(capacity.relay_offline_grace_ticks, 1);
        assert_eq!(capacity.support_heat_fraction, ratio(1, 4));

        let radiation = BalanceProfile::radiation_reference_alpha("radiation-reference")
            .radiation_reference
            .expect("radiation reference is present");
        assert_eq!(radiation.distance_weights, [16, 8, 4, 2, 1]);
        assert_eq!(radiation.delays, [1, 1, 2, 3, 4]);
        assert_eq!(
            radiation.orientation_weights,
            OrientationWeightTable {
                broadside_near: 4,
                diagonal: 2,
                endfire_near: 1,
            }
        );
        assert_eq!(
            radiation.orientation_boundaries,
            OrientationBoundaryMultipliers {
                broadside_abs_cross_multiplier: 2,
                endfire_abs_dot_multiplier: 2,
            }
        );
    }

    #[test]
    fn optional_probe_validation_rejects_invalid_coefficients_and_tables() {
        let mut capacity = BalanceProfile::capacity_probe_alpha("capacity-probe");
        capacity
            .capacity_probe
            .as_mut()
            .expect("capacity probe is present")
            .support_heat_fraction = ratio(5, 4);
        assert_eq!(
            capacity.validate(),
            Err(ProfileValidationError::OutsideUnitInterval {
                field: "capacityProbe.supportHeatFraction",
            })
        );

        let mut radiation = BalanceProfile::radiation_reference_alpha("radiation-reference");
        radiation
            .radiation_reference
            .as_mut()
            .expect("radiation reference is present")
            .delays = [1, 2, 1, 3, 4];
        assert_eq!(
            radiation.validate(),
            Err(ProfileValidationError::InvalidBalanceRelation {
                field: "radiationReference.delays",
                relation: "must be nondecreasing by distance",
            })
        );
    }

    #[test]
    fn optional_probe_wire_types_are_strict() {
        let capacity_unknown = r#"{
            "mainCoreCapacity":1000,
            "relayCapacity":500,
            "overcapLinearK":{"numerator":1,"denominator":1},
            "overcapQuadraticK":{"numerator":2,"denominator":1},
            "capacityDenominatorFloor":1,
            "relayOfflineGraceTicks":1,
            "supportHeatFraction":{"numerator":1,"denominator":4},
            "extra":0
        }"#;
        assert!(serde_json::from_str::<CapacityProbeProfile>(capacity_unknown).is_err());

        let radiation_wrong_length = r#"{
            "distanceWeights":[16,8,4,2],
            "delays":[1,1,2,3,4],
            "orientationWeights":{"broadsideNear":4,"diagonal":2,"endfireNear":1},
            "orientationBoundaries":{
                "broadsideAbsCrossMultiplier":2,
                "endfireAbsDotMultiplier":2
            }
        }"#;
        assert!(serde_json::from_str::<RadiationReferenceProfile>(radiation_wrong_length).is_err());
    }

    #[test]
    fn optional_probe_canonical_encoding_uses_presence_tags_and_little_endian_fields() {
        let base = BalanceProfile::stage0_alpha("base");
        let mut base_bytes = Vec::new();
        base.encode_canonical(&mut |bytes| base_bytes.extend_from_slice(bytes));
        assert!(base_bytes.ends_with(&[0, 0]));

        let capacity = BalanceProfile::capacity_probe_alpha("capacity");
        let mut capacity_bytes = Vec::new();
        capacity.encode_canonical(&mut |bytes| capacity_bytes.extend_from_slice(bytes));
        let mut expected_capacity_tail = vec![1];
        expected_capacity_tail.extend_from_slice(&1_000_u64.to_le_bytes());
        expected_capacity_tail.extend_from_slice(&500_u64.to_le_bytes());
        for value in [1_i64, 1, 2, 1] {
            expected_capacity_tail.extend_from_slice(&value.to_le_bytes());
        }
        expected_capacity_tail.extend_from_slice(&1_u64.to_le_bytes());
        expected_capacity_tail.extend_from_slice(&1_u64.to_le_bytes());
        expected_capacity_tail.extend_from_slice(&1_i64.to_le_bytes());
        expected_capacity_tail.extend_from_slice(&4_i64.to_le_bytes());
        expected_capacity_tail.push(0);
        assert!(capacity_bytes.ends_with(&expected_capacity_tail));

        let radiation = BalanceProfile::radiation_reference_alpha("radiation");
        let mut radiation_bytes = Vec::new();
        radiation.encode_canonical(&mut |bytes| radiation_bytes.extend_from_slice(bytes));
        let mut expected_radiation_tail = vec![0, 1];
        for value in [16_u64, 8, 4, 2, 1, 1, 1, 2, 3, 4, 4, 2, 1, 2, 2] {
            expected_radiation_tail.extend_from_slice(&value.to_le_bytes());
        }
        assert!(radiation_bytes.ends_with(&expected_radiation_tail));

        assert_ne!(base.canonical_hash(), capacity.canonical_hash());
        assert_ne!(base.canonical_hash(), radiation.canonical_hash());
        assert_ne!(capacity.canonical_hash(), radiation.canonical_hash());
    }

    #[test]
    fn reference_profile_hashes_are_golden_contract_values() {
        assert_eq!(
            NumericProfile::reference_v1("ignored")
                .canonical_hash()
                .expect("reference numeric profile is valid")
                .to_string(),
            "fe92f0c723660040a3200254890c8a34ec3ed9e65fc242de1c0951e4ecd00469"
        );
        assert_eq!(
            PhysicalScaleProfile::stage0_alpha("ignored")
                .canonical_hash()
                .expect("reference physical profile is valid")
                .to_string(),
            "0e0f7fe8c9ccbf0b159d44e4e53d05417cf558c37e796e5f8bccd8221aec6490"
        );
        assert_eq!(
            BalanceProfile::stage0_alpha("ignored")
                .canonical_hash()
                .expect("reference balance profile is valid")
                .to_string(),
            "b1540d6ad19c616ce60e96523108264355311168c51a0b92de2fdf596e2646fd"
        );
        assert_eq!(
            BalanceProfile::capacity_probe_alpha("ignored")
                .canonical_hash()
                .expect("reference capacity probe profile is valid")
                .to_string(),
            "3fb2f3470804e9e95bde625ff615fc74ecff39fe0e8654371cd461178e1f3d8c"
        );
        assert_eq!(
            BalanceProfile::radiation_reference_alpha("ignored")
                .canonical_hash()
                .expect("reference radiation profile is valid")
                .to_string(),
            "86d135f608076ec8c8c1f2702d28cc7c3c4792c4311c503ffa1532239d4589c9"
        );
    }
}
