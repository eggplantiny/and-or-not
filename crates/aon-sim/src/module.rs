use crate::structural_geometry::{
    parallel_segments_are_too_close, point_is_strict_segment_interior,
    segment_intersects_aabb_interior, segment_overlaps_aabb_boundary,
    segment_touches_aabb_boundary, segments_have_positive_collinear_overlap,
};
use crate::{
    ArtifactHash, Fixed, FixedAabb, FixedVec2, GatePort, GateType, HASH_ALGORITHM_ID_BLAKE3_V1,
    HashAlgorithmId, HashParseError, JsonErrorCategory, NumericError, PhysicalScaleProfile,
    PortAnchor, ProfileHash, SEMANTICS_VERSION_V1, SemanticsVersion, SimulationContract,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;

pub const MODULE_FORMAT_VERSION_V1: u32 = 1;
const MODULE_HASH_DOMAIN: &[u8] = b"AON\0MODULE\0V1\0";
const MODULE_CANONICAL_ENCODER_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleFormatVersion {
    #[default]
    V1,
}

impl ModuleFormatVersion {
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::V1 => MODULE_FORMAT_VERSION_V1,
        }
    }

    fn parse(value: u32) -> Result<Self, ModuleError> {
        match value {
            MODULE_FORMAT_VERSION_V1 => Ok(Self::V1),
            actual => Err(ModuleError::UnsupportedFormatVersion {
                expected: MODULE_FORMAT_VERSION_V1,
                actual,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleLocalId(u32);

impl ModuleLocalId {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ModuleLocalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleContract {
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleRoutingDomain {
    OpenWorld,
    Substrate(ModuleLocalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleEndpoint {
    Free,
    Junction(ModuleLocalId),
    GatePort { gate: ModuleLocalId, port: GatePort },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubstrateBlueprint {
    pub id: ModuleLocalId,
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateBlueprint {
    pub id: ModuleLocalId,
    pub substrate: ModuleLocalId,
    pub gate_type: GateType,
    pub origin: FixedVec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JunctionBlueprint {
    pub id: ModuleLocalId,
    pub routing_domain: ModuleRoutingDomain,
    pub position: FixedVec2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireBlueprint {
    pub id: ModuleLocalId,
    pub routing_domain: ModuleRoutingDomain,
    pub points: Vec<FixedVec2>,
    pub endpoint_a: ModuleEndpoint,
    pub endpoint_b: ModuleEndpoint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbsoluteModuleGeometry {
    pub substrates: Vec<SubstrateBlueprint>,
    pub gates: Vec<GateBlueprint>,
    pub junctions: Vec<JunctionBlueprint>,
    pub wires: Vec<WireBlueprint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleIoBinding {
    pub name: String,
    pub endpoint: ModuleEndpoint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleProvenance {
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleBlueprint {
    pub format_version: ModuleFormatVersion,
    pub hash_algorithm_id: HashAlgorithmId,
    pub name: String,
    pub contract: ModuleContract,
    pub balance_profile_hash: Option<ProfileHash>,
    pub geometry: AbsoluteModuleGeometry,
    pub io_bindings: Vec<ModuleIoBinding>,
    pub provenance: ModuleProvenance,
}

impl ModuleBlueprint {
    pub fn semantic_hash(&self) -> Result<ArtifactHash, ModuleError> {
        let canonical = CanonicalModule::new(self)?;
        let mut encoder = CanonicalEncoder::new();
        encoder.bytes(MODULE_HASH_DOMAIN);
        encoder.u16(MODULE_CANONICAL_ENCODER_VERSION);
        encoder.u32(self.format_version.as_u32());
        encoder.text(self.hash_algorithm_id.as_str())?;
        encoder.text(self.contract.semantics_version.as_str())?;
        encoder.bytes(self.contract.numeric_profile_hash.as_bytes());
        encoder.bytes(self.contract.physical_scale_profile_hash.as_bytes());
        canonical.encode(&mut encoder)?;
        Ok(ArtifactHash::from_bytes(
            *blake3::hash(&encoder.finish()).as_bytes(),
        ))
    }
}

pub fn validate_module_against(
    module: &ModuleBlueprint,
    target: &SimulationContract,
    physical: &PhysicalScaleProfile,
) -> Result<(), ModuleError> {
    let canonical = CanonicalModule::new(module)?;
    if module.contract.semantics_version != target.semantics_version {
        return Err(ModuleError::SemanticsMismatch {
            expected: module.contract.semantics_version,
            actual: target.semantics_version,
        });
    }
    if module.contract.numeric_profile_hash != target.numeric_profile_hash {
        return Err(ModuleError::NumericProfileMismatch {
            expected: module.contract.numeric_profile_hash,
            actual: target.numeric_profile_hash,
        });
    }
    if module.contract.physical_scale_profile_hash != target.physical_scale_profile_hash {
        return Err(ModuleError::PhysicalScaleProfileMismatch {
            expected: module.contract.physical_scale_profile_hash,
            actual: target.physical_scale_profile_hash,
        });
    }
    physical.validate()?;
    let actual = physical.canonical_hash()?;
    if actual != target.physical_scale_profile_hash {
        return Err(ModuleError::TargetPhysicalProfileMismatch {
            expected: target.physical_scale_profile_hash,
            actual,
        });
    }
    canonical.validate_geometry(physical)
}

pub fn decode_module_artifact(source: &str) -> Result<ModuleBlueprint, ModuleError> {
    let wire: ModuleBlueprintWire =
        serde_json::from_str(source).map_err(|error| ModuleError::InvalidJson {
            category: JsonErrorCategory::from(error.classify()),
            line: error.line(),
            column: error.column(),
        })?;
    wire.try_into()
}

pub fn encode_module_artifact(module: &ModuleBlueprint) -> Result<String, ModuleError> {
    let canonical = CanonicalModule::new(module)?;
    let wire = ModuleBlueprintWire::from_canonical(&canonical);
    let mut encoded = serde_json::to_string_pretty(&wire).map_err(|_| ModuleError::JsonEncoding)?;
    encoded.push('\n');
    Ok(encoded)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleBlueprintWire {
    format_version: u32,
    hash_algorithm_id: String,
    name: String,
    contract: ModuleContractWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    balance_profile_hash: Option<String>,
    geometry: AbsoluteModuleGeometryWire,
    io_bindings: Vec<ModuleIoBindingWire>,
    provenance: ModuleProvenanceWire,
}

impl ModuleBlueprintWire {
    fn from_canonical(canonical: &CanonicalModule<'_>) -> Self {
        let module = canonical.module;
        let substrates = canonical
            .substrates
            .values()
            .map(|substrate| SubstrateBlueprintWire::from(**substrate))
            .collect();
        let gates = canonical
            .gates
            .values()
            .map(|gate| GateBlueprintWire::from(**gate))
            .collect();
        let junctions = canonical
            .junctions
            .values()
            .map(|junction| JunctionBlueprintWire::from(**junction))
            .collect();
        let wires = canonical
            .wires
            .values()
            .map(|wire| WireBlueprintWire::from(*wire))
            .collect();
        let mut io_bindings: Vec<_> = module
            .io_bindings
            .iter()
            .map(ModuleIoBindingWire::from)
            .collect();
        io_bindings.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| left.endpoint.sort_key().cmp(&right.endpoint.sort_key()))
        });
        Self {
            format_version: module.format_version.as_u32(),
            hash_algorithm_id: module.hash_algorithm_id.as_str().to_owned(),
            name: module.name.clone(),
            contract: ModuleContractWire::from(module.contract),
            balance_profile_hash: module.balance_profile_hash.map(|hash| hash.to_string()),
            geometry: AbsoluteModuleGeometryWire {
                substrates,
                gates,
                junctions,
                wires,
            },
            io_bindings,
            provenance: ModuleProvenanceWire::from(&module.provenance),
        }
    }
}

impl TryFrom<ModuleBlueprintWire> for ModuleBlueprint {
    type Error = ModuleError;

    fn try_from(wire: ModuleBlueprintWire) -> Result<Self, Self::Error> {
        let format_version = ModuleFormatVersion::parse(wire.format_version)?;
        let hash_algorithm_id = match wire.hash_algorithm_id.as_str() {
            HASH_ALGORITHM_ID_BLAKE3_V1 => HashAlgorithmId::Blake3V1,
            _ => {
                return Err(ModuleError::UnsupportedHashAlgorithm {
                    actual: wire.hash_algorithm_id,
                });
            }
        };
        let contract = wire.contract.try_into()?;
        let balance_profile_hash = wire
            .balance_profile_hash
            .map(|value| parse_profile_hash("balanceProfileHash", &value))
            .transpose()?;
        let module = Self {
            format_version,
            hash_algorithm_id,
            name: wire.name,
            contract,
            balance_profile_hash,
            geometry: wire.geometry.try_into()?,
            io_bindings: wire
                .io_bindings
                .into_iter()
                .map(ModuleIoBinding::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            provenance: wire.provenance.into(),
        };
        CanonicalModule::new(&module)?;
        Ok(module)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleContractWire {
    semantics_version: String,
    numeric_profile_hash: String,
    physical_scale_profile_hash: String,
}

impl From<ModuleContract> for ModuleContractWire {
    fn from(contract: ModuleContract) -> Self {
        Self {
            semantics_version: contract.semantics_version.as_str().to_owned(),
            numeric_profile_hash: contract.numeric_profile_hash.to_string(),
            physical_scale_profile_hash: contract.physical_scale_profile_hash.to_string(),
        }
    }
}

impl TryFrom<ModuleContractWire> for ModuleContract {
    type Error = ModuleError;

    fn try_from(wire: ModuleContractWire) -> Result<Self, Self::Error> {
        let semantics_version = match wire.semantics_version.as_str() {
            SEMANTICS_VERSION_V1 => SemanticsVersion::AonV1,
            _ => {
                return Err(ModuleError::UnsupportedSemanticsVersion {
                    actual: wire.semantics_version,
                });
            }
        };
        Ok(Self {
            semantics_version,
            numeric_profile_hash: parse_profile_hash(
                "contract.numericProfileHash",
                &wire.numeric_profile_hash,
            )?,
            physical_scale_profile_hash: parse_profile_hash(
                "contract.physicalScaleProfileHash",
                &wire.physical_scale_profile_hash,
            )?,
        })
    }
}

fn parse_profile_hash(field: &'static str, value: &str) -> Result<ProfileHash, ModuleError> {
    ProfileHash::from_hex(value).map_err(|error| ModuleError::InvalidHash { field, error })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AbsoluteModuleGeometryWire {
    substrates: Vec<SubstrateBlueprintWire>,
    gates: Vec<GateBlueprintWire>,
    junctions: Vec<JunctionBlueprintWire>,
    wires: Vec<WireBlueprintWire>,
}

impl TryFrom<AbsoluteModuleGeometryWire> for AbsoluteModuleGeometry {
    type Error = ModuleError;

    fn try_from(wire: AbsoluteModuleGeometryWire) -> Result<Self, Self::Error> {
        Ok(Self {
            substrates: wire
                .substrates
                .into_iter()
                .map(SubstrateBlueprint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            gates: wire
                .gates
                .into_iter()
                .map(GateBlueprint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            junctions: wire
                .junctions
                .into_iter()
                .map(JunctionBlueprint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            wires: wire
                .wires
                .into_iter()
                .map(WireBlueprint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubstrateBlueprintWire {
    id: u32,
    origin: FixedVec2Wire,
    routing_area: FixedAabbWire,
    footprint: FixedAabbWire,
}

impl From<SubstrateBlueprint> for SubstrateBlueprintWire {
    fn from(substrate: SubstrateBlueprint) -> Self {
        Self {
            id: substrate.id.get(),
            origin: substrate.origin.into(),
            routing_area: substrate.routing_area.into(),
            footprint: substrate.footprint.into(),
        }
    }
}

impl TryFrom<SubstrateBlueprintWire> for SubstrateBlueprint {
    type Error = ModuleError;

    fn try_from(wire: SubstrateBlueprintWire) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_local_id(wire.id)?,
            origin: wire.origin.into(),
            routing_area: wire.routing_area.into(),
            footprint: wire.footprint.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GateBlueprintWire {
    id: u32,
    substrate: u32,
    gate_type: GateTypeWire,
    origin: FixedVec2Wire,
}

impl From<GateBlueprint> for GateBlueprintWire {
    fn from(gate: GateBlueprint) -> Self {
        Self {
            id: gate.id.get(),
            substrate: gate.substrate.get(),
            gate_type: gate.gate_type.into(),
            origin: gate.origin.into(),
        }
    }
}

impl TryFrom<GateBlueprintWire> for GateBlueprint {
    type Error = ModuleError;

    fn try_from(wire: GateBlueprintWire) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_local_id(wire.id)?,
            substrate: parse_local_id(wire.substrate)?,
            gate_type: wire.gate_type.into(),
            origin: wire.origin.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JunctionBlueprintWire {
    id: u32,
    routing_domain: ModuleRoutingDomainWire,
    position: FixedVec2Wire,
}

impl From<JunctionBlueprint> for JunctionBlueprintWire {
    fn from(junction: JunctionBlueprint) -> Self {
        Self {
            id: junction.id.get(),
            routing_domain: junction.routing_domain.into(),
            position: junction.position.into(),
        }
    }
}

impl TryFrom<JunctionBlueprintWire> for JunctionBlueprint {
    type Error = ModuleError;

    fn try_from(wire: JunctionBlueprintWire) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_local_id(wire.id)?,
            routing_domain: wire.routing_domain.try_into()?,
            position: wire.position.into(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireBlueprintWire {
    id: u32,
    routing_domain: ModuleRoutingDomainWire,
    points: Vec<FixedVec2Wire>,
    endpoint_a: ModuleEndpointWire,
    endpoint_b: ModuleEndpointWire,
}

impl From<&WireBlueprint> for WireBlueprintWire {
    fn from(wire: &WireBlueprint) -> Self {
        Self {
            id: wire.id.get(),
            routing_domain: wire.routing_domain.into(),
            points: wire.points.iter().copied().map(Into::into).collect(),
            endpoint_a: wire.endpoint_a.into(),
            endpoint_b: wire.endpoint_b.into(),
        }
    }
}

impl TryFrom<WireBlueprintWire> for WireBlueprint {
    type Error = ModuleError;

    fn try_from(wire: WireBlueprintWire) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_local_id(wire.id)?,
            routing_domain: wire.routing_domain.try_into()?,
            points: wire.points.into_iter().map(Into::into).collect(),
            endpoint_a: wire.endpoint_a.try_into()?,
            endpoint_b: wire.endpoint_b.try_into()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleIoBindingWire {
    name: String,
    endpoint: ModuleEndpointWire,
}

impl From<&ModuleIoBinding> for ModuleIoBindingWire {
    fn from(binding: &ModuleIoBinding) -> Self {
        Self {
            name: binding.name.clone(),
            endpoint: binding.endpoint.into(),
        }
    }
}

impl TryFrom<ModuleIoBindingWire> for ModuleIoBinding {
    type Error = ModuleError;

    fn try_from(wire: ModuleIoBindingWire) -> Result<Self, Self::Error> {
        Ok(Self {
            name: wire.name,
            endpoint: wire.endpoint.try_into()?,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleProvenanceWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

impl From<&ModuleProvenance> for ModuleProvenanceWire {
    fn from(provenance: &ModuleProvenance) -> Self {
        Self {
            source: provenance.source.clone(),
        }
    }
}

impl From<ModuleProvenanceWire> for ModuleProvenance {
    fn from(provenance: ModuleProvenanceWire) -> Self {
        Self {
            source: provenance.source,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ModuleRoutingDomainWire {
    OpenWorld,
    Substrate { substrate: u32 },
}

impl From<ModuleRoutingDomain> for ModuleRoutingDomainWire {
    fn from(domain: ModuleRoutingDomain) -> Self {
        match domain {
            ModuleRoutingDomain::OpenWorld => Self::OpenWorld,
            ModuleRoutingDomain::Substrate(substrate) => Self::Substrate {
                substrate: substrate.get(),
            },
        }
    }
}

impl TryFrom<ModuleRoutingDomainWire> for ModuleRoutingDomain {
    type Error = ModuleError;

    fn try_from(domain: ModuleRoutingDomainWire) -> Result<Self, Self::Error> {
        match domain {
            ModuleRoutingDomainWire::OpenWorld => Ok(Self::OpenWorld),
            ModuleRoutingDomainWire::Substrate { substrate } => {
                Ok(Self::Substrate(parse_local_id(substrate)?))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ModuleEndpointWire {
    Free,
    Junction { junction: u32 },
    GatePort { gate: u32, port: GatePortWire },
}

impl ModuleEndpointWire {
    fn sort_key(self) -> (u8, u32, u8) {
        match self {
            Self::Free => (0, 0, 0),
            Self::Junction { junction } => (1, junction, 0),
            Self::GatePort { gate, port } => (2, gate, port.tag()),
        }
    }
}

impl From<ModuleEndpoint> for ModuleEndpointWire {
    fn from(endpoint: ModuleEndpoint) -> Self {
        match endpoint {
            ModuleEndpoint::Free => Self::Free,
            ModuleEndpoint::Junction(junction) => Self::Junction {
                junction: junction.get(),
            },
            ModuleEndpoint::GatePort { gate, port } => Self::GatePort {
                gate: gate.get(),
                port: port.into(),
            },
        }
    }
}

impl TryFrom<ModuleEndpointWire> for ModuleEndpoint {
    type Error = ModuleError;

    fn try_from(endpoint: ModuleEndpointWire) -> Result<Self, Self::Error> {
        match endpoint {
            ModuleEndpointWire::Free => Ok(Self::Free),
            ModuleEndpointWire::Junction { junction } => {
                Ok(Self::Junction(parse_local_id(junction)?))
            }
            ModuleEndpointWire::GatePort { gate, port } => Ok(Self::GatePort {
                gate: parse_local_id(gate)?,
                port: port.into(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum GateTypeWire {
    And,
    Or,
    Not,
}

impl From<GateType> for GateTypeWire {
    fn from(gate_type: GateType) -> Self {
        match gate_type {
            GateType::And => Self::And,
            GateType::Or => Self::Or,
            GateType::Not => Self::Not,
        }
    }
}

impl From<GateTypeWire> for GateType {
    fn from(gate_type: GateTypeWire) -> Self {
        match gate_type {
            GateTypeWire::And => Self::And,
            GateTypeWire::Or => Self::Or,
            GateTypeWire::Not => Self::Not,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum GatePortWire {
    InputA,
    InputB,
    Output,
    Power,
}

impl GatePortWire {
    fn tag(self) -> u8 {
        match self {
            Self::InputA => 0,
            Self::InputB => 1,
            Self::Output => 2,
            Self::Power => 3,
        }
    }
}

impl From<GatePort> for GatePortWire {
    fn from(port: GatePort) -> Self {
        match port {
            GatePort::InputA => Self::InputA,
            GatePort::InputB => Self::InputB,
            GatePort::Output => Self::Output,
            GatePort::Power => Self::Power,
        }
    }
}

impl From<GatePortWire> for GatePort {
    fn from(port: GatePortWire) -> Self {
        match port {
            GatePortWire::InputA => Self::InputA,
            GatePortWire::InputB => Self::InputB,
            GatePortWire::Output => Self::Output,
            GatePortWire::Power => Self::Power,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixedVec2Wire {
    x: i64,
    y: i64,
}

impl From<FixedVec2> for FixedVec2Wire {
    fn from(point: FixedVec2) -> Self {
        Self {
            x: point.x.0,
            y: point.y.0,
        }
    }
}

impl From<FixedVec2Wire> for FixedVec2 {
    fn from(point: FixedVec2Wire) -> Self {
        Self::new(Fixed(point.x), Fixed(point.y))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixedAabbWire {
    min: FixedVec2Wire,
    max: FixedVec2Wire,
}

impl From<FixedAabb> for FixedAabbWire {
    fn from(aabb: FixedAabb) -> Self {
        Self {
            min: aabb.min.into(),
            max: aabb.max.into(),
        }
    }
}

impl From<FixedAabbWire> for FixedAabb {
    fn from(aabb: FixedAabbWire) -> Self {
        Self::new(aabb.min.into(), aabb.max.into())
    }
}

fn parse_local_id(value: u32) -> Result<ModuleLocalId, ModuleError> {
    ModuleLocalId::new(value).ok_or(ModuleError::InvalidLocalId { actual: value })
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ModuleError {
    #[error("invalid Module JSON: category={category:?}, line={line}, column={column}")]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },
    #[error("unable to encode canonical Module JSON")]
    JsonEncoding,
    #[error("unsupported Module format: expected {expected}, got {actual}")]
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    #[error("unsupported Module hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },
    #[error("unsupported Module semantics version `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },
    #[error("invalid Module hash field `{field}`: {error}")]
    InvalidHash {
        field: &'static str,
        error: HashParseError,
    },
    #[error("module-local id must be nonzero, got {actual}")]
    InvalidLocalId { actual: u32 },
    #[error("Module collection `{collection}` exceeds the canonical u32 limit")]
    CollectionTooLong { collection: &'static str },
    #[error("module semantics mismatch: expected {expected}, got {actual}")]
    SemanticsMismatch {
        expected: SemanticsVersion,
        actual: SemanticsVersion,
    },
    #[error("module numeric profile mismatch: expected {expected}, got {actual}")]
    NumericProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error("module physical-scale profile mismatch: expected {expected}, got {actual}")]
    PhysicalScaleProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error(
        "physical profile bytes do not match target contract: expected {expected}, got {actual}"
    )]
    TargetPhysicalProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error("duplicate module-local id {id}")]
    DuplicateLocalId { id: ModuleLocalId },
    #[error("dangling module-local reference {id}")]
    DanglingReference { id: ModuleLocalId },
    #[error("module-local reference {id} does not refer to a {expected}")]
    WrongKindReference {
        id: ModuleLocalId,
        expected: &'static str,
    },
    #[error("gate {gate} does not expose port {port:?}")]
    InvalidGatePort { gate: ModuleLocalId, port: GatePort },
    #[error("duplicate module I/O name {name:?}")]
    DuplicateIoName { name: String },
    #[error("module I/O name must not be empty")]
    EmptyIoName,
    #[error("substrate {id:?} has an empty AABB")]
    EmptySubstrateAabb { id: ModuleLocalId },
    #[error("substrate {id} routing area is outside its footprint")]
    InvalidSubstrateBounds { id: ModuleLocalId },
    #[error("geometry for module-local id {id:?} is not quantized")]
    NotQuantized { id: ModuleLocalId },
    #[error("geometry for module-local id {id} is not aligned to its routing pitch")]
    InvalidRoutingPitch { id: ModuleLocalId },
    #[error("geometry for module-local id {id} is outside its routing domain")]
    RoutingBoundsViolation { id: ModuleLocalId },
    #[error("wire {id:?} must contain at least two distinct points")]
    InvalidPolyline { id: ModuleLocalId },
    #[error("wire {wire} endpoint {end} uses a different routing domain than its target")]
    EndpointDomainMismatch {
        wire: ModuleLocalId,
        end: &'static str,
    },
    #[error("wire {wire} endpoint {end} does not equal its target position")]
    EndpointPositionMismatch {
        wire: ModuleLocalId,
        end: &'static str,
    },
    #[error("Module I/O binding must refer to a gate port or junction")]
    InvalidIoEndpoint,
    #[error("module geometry for {first} overlaps {second}")]
    GeometryOverlap {
        first: ModuleLocalId,
        second: ModuleLocalId,
    },
    #[error("module wires {first} and {second} violate routing-pitch spacing")]
    InsufficientWireSpacing {
        first: ModuleLocalId,
        second: ModuleLocalId,
    },
    #[error("junction {junction} lies in the strict interior of wire {wire}")]
    JunctionOnWireInterior {
        junction: ModuleLocalId,
        wire: ModuleLocalId,
    },
    #[error("wire {wire} has a non-anchor boundary contact with gate {gate}")]
    InvalidGateContact {
        wire: ModuleLocalId,
        gate: ModuleLocalId,
    },
    #[error("open-world wire {wire} binds both endpoints to junction {junction}")]
    SameJunctionEndpoints {
        wire: ModuleLocalId,
        junction: ModuleLocalId,
    },
    #[error("module artifact text field is too long for canonical encoding")]
    TextTooLong,
    #[error(transparent)]
    Numeric(#[from] NumericError),
    #[error(transparent)]
    Profile(#[from] crate::ProfileValidationError),
}

struct CanonicalModule<'a> {
    module: &'a ModuleBlueprint,
    ids: BTreeSet<ModuleLocalId>,
    substrates: BTreeMap<ModuleLocalId, &'a SubstrateBlueprint>,
    gates: BTreeMap<ModuleLocalId, &'a GateBlueprint>,
    junctions: BTreeMap<ModuleLocalId, &'a JunctionBlueprint>,
    wires: BTreeMap<ModuleLocalId, &'a WireBlueprint>,
}

impl<'a> CanonicalModule<'a> {
    fn new(module: &'a ModuleBlueprint) -> Result<Self, ModuleError> {
        canonical_count("substrates", module.geometry.substrates.len())?;
        canonical_count("gates", module.geometry.gates.len())?;
        canonical_count("junctions", module.geometry.junctions.len())?;
        canonical_count("wires", module.geometry.wires.len())?;
        canonical_count("ioBindings", module.io_bindings.len())?;

        let mut ids = BTreeSet::new();
        let mut substrates = BTreeMap::new();
        let mut gates = BTreeMap::new();
        let mut junctions = BTreeMap::new();
        let mut wires = BTreeMap::new();
        for substrate in &module.geometry.substrates {
            insert_local_id(&mut ids, substrate.id)?;
            substrates.insert(substrate.id, substrate);
        }
        for gate in &module.geometry.gates {
            insert_local_id(&mut ids, gate.id)?;
            gates.insert(gate.id, gate);
        }
        for junction in &module.geometry.junctions {
            insert_local_id(&mut ids, junction.id)?;
            junctions.insert(junction.id, junction);
        }
        for wire in &module.geometry.wires {
            canonical_count("wire.points", wire.points.len())?;
            insert_local_id(&mut ids, wire.id)?;
            wires.insert(wire.id, wire);
        }
        for wire in wires.values() {
            if wire.points.len() < 2 {
                return Err(ModuleError::InvalidPolyline { id: wire.id });
            }
        }
        let canonical = Self {
            module,
            ids,
            substrates,
            gates,
            junctions,
            wires,
        };
        canonical.validate_references()?;
        Ok(canonical)
    }

    fn validate_references(&self) -> Result<(), ModuleError> {
        for gate in self.gates.values() {
            self.require_kind(gate.substrate, &self.substrates, "substrate")?;
        }
        for domain in self
            .junctions
            .values()
            .map(|x| x.routing_domain)
            .chain(self.wires.values().map(|x| x.routing_domain))
        {
            if let ModuleRoutingDomain::Substrate(id) = domain {
                self.require_kind(id, &self.substrates, "substrate")?;
            }
        }
        for wire in self.wires.values() {
            self.validate_endpoint_reference(wire.endpoint_a)?;
            self.validate_endpoint_reference(wire.endpoint_b)?;
        }

        let mut names = BTreeSet::new();
        for io in &self.module.io_bindings {
            canonical_text(&io.name)?;
            if io.name.is_empty() {
                return Err(ModuleError::EmptyIoName);
            }
            if io.endpoint == ModuleEndpoint::Free {
                return Err(ModuleError::InvalidIoEndpoint);
            }
            self.validate_endpoint_reference(io.endpoint)?;
            if !names.insert(io.name.as_str()) {
                return Err(ModuleError::DuplicateIoName {
                    name: io.name.clone(),
                });
            }
        }
        Ok(())
    }

    fn require_kind<T>(
        &self,
        id: ModuleLocalId,
        collection: &BTreeMap<ModuleLocalId, T>,
        expected: &'static str,
    ) -> Result<(), ModuleError> {
        if collection.contains_key(&id) {
            Ok(())
        } else if self.ids.contains(&id) {
            Err(ModuleError::WrongKindReference { id, expected })
        } else {
            Err(ModuleError::DanglingReference { id })
        }
    }

    fn validate_endpoint_reference(&self, endpoint: ModuleEndpoint) -> Result<(), ModuleError> {
        match endpoint {
            ModuleEndpoint::Free => Ok(()),
            ModuleEndpoint::Junction(id) => self.require_kind(id, &self.junctions, "junction"),
            ModuleEndpoint::GatePort { gate, port } => {
                self.require_kind(gate, &self.gates, "gate")?;
                let gate_type = self.gates[&gate].gate_type;
                if gate_type == GateType::Not && port == GatePort::InputB {
                    return Err(ModuleError::InvalidGatePort { gate, port });
                }
                Ok(())
            }
        }
    }

    fn validate_geometry(&self, physical: &PhysicalScaleProfile) -> Result<(), ModuleError> {
        for wire in self.wires.values() {
            if wire.points.len() < 2 || wire.points.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(ModuleError::InvalidPolyline { id: wire.id });
            }
        }

        self.preflight_checked_arithmetic(physical)?;

        for substrate in self.substrates.values() {
            if !substrate.footprint.is_nonempty() || !substrate.routing_area.is_nonempty() {
                return Err(ModuleError::EmptySubstrateAabb { id: substrate.id });
            }
        }

        let quantum = physical.wire_geometry_quantum;
        let mut world_substrate_footprints = BTreeMap::new();
        for substrate in self.substrates.values() {
            if !point_is_quantized(substrate.origin, quantum)
                || !aabb_is_quantized(substrate.routing_area, quantum)
                || !aabb_is_quantized(substrate.footprint, quantum)
            {
                return Err(ModuleError::NotQuantized { id: substrate.id });
            }
            if !point_is_quantized(substrate.origin, physical.world_routing_pitch)
                || !aabb_is_quantized(substrate.routing_area, physical.circuit_routing_pitch)
            {
                return Err(ModuleError::InvalidRoutingPitch { id: substrate.id });
            }
            if !substrate.footprint.contains_aabb(substrate.routing_area) {
                return Err(ModuleError::InvalidSubstrateBounds { id: substrate.id });
            }
            world_substrate_footprints.insert(
                substrate.id,
                substrate.footprint.translated(substrate.origin)?,
            );
        }
        reject_aabb_overlaps(&world_substrate_footprints)?;

        let mut gate_footprints: BTreeMap<ModuleLocalId, (ModuleLocalId, FixedAabb)> =
            BTreeMap::new();
        for gate in self.gates.values() {
            if !point_is_quantized(gate.origin, quantum) {
                return Err(ModuleError::NotQuantized { id: gate.id });
            }
            let substrate = self.substrates[&gate.substrate];
            let local_origin = checked_sub_point(gate.origin, substrate.origin)?;
            if !point_is_quantized(local_origin, physical.circuit_routing_pitch) {
                return Err(ModuleError::InvalidRoutingPitch { id: gate.id });
            }
            let local_footprint = gate_aabb(local_origin, gate.gate_type, physical)?;
            if !substrate.routing_area.contains_aabb(local_footprint) {
                return Err(ModuleError::RoutingBoundsViolation { id: gate.id });
            }
            gate_footprints.insert(
                gate.id,
                (
                    gate.substrate,
                    gate_aabb(gate.origin, gate.gate_type, physical)?,
                ),
            );
        }
        reject_gate_overlaps(&gate_footprints)?;

        for junction in self.junctions.values() {
            self.validate_routed_point(
                junction.id,
                junction.routing_domain,
                junction.position,
                physical,
            )?;
        }

        for wire in self.wires.values() {
            for &point in &wire.points {
                if !point_is_quantized(point, physical.wire_geometry_quantum) {
                    return Err(ModuleError::NotQuantized { id: wire.id });
                }
            }
            for &point in &wire.points[1..wire.points.len() - 1] {
                self.validate_routed_point(wire.id, wire.routing_domain, point, physical)?;
            }
            self.validate_wire_domain_endpoint(wire.id, wire.routing_domain, wire.points[0])?;
            self.validate_wire_domain_endpoint(
                wire.id,
                wire.routing_domain,
                wire.points[wire.points.len() - 1],
            )?;
            crate::polyline_length(&wire.points)?;
        }

        self.validate_structural_geometry_laws(physical)?;

        for wire in self.wires.values() {
            self.validate_wire_endpoint(wire, "A", wire.points[0], wire.endpoint_a, physical)?;
            self.validate_wire_endpoint(
                wire,
                "B",
                wire.points[wire.points.len() - 1],
                wire.endpoint_b,
                physical,
            )?;
            if let (
                ModuleRoutingDomain::OpenWorld,
                ModuleEndpoint::Junction(first),
                ModuleEndpoint::Junction(second),
            ) = (wire.routing_domain, wire.endpoint_a, wire.endpoint_b)
                && first == second
            {
                return Err(ModuleError::SameJunctionEndpoints {
                    wire: wire.id,
                    junction: first,
                });
            }
        }
        Ok(())
    }

    fn preflight_checked_arithmetic(
        &self,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ModuleError> {
        for substrate in self.substrates.values() {
            substrate.footprint.translated(substrate.origin)?;
        }

        for gate in self.gates.values() {
            let substrate = self.substrates[&gate.substrate];
            let local_origin = checked_sub_point(gate.origin, substrate.origin)?;
            gate_aabb(local_origin, gate.gate_type, physical)?;
            gate_aabb(gate.origin, gate.gate_type, physical)?;
        }

        for junction in self.junctions.values() {
            if let ModuleRoutingDomain::Substrate(substrate_id) = junction.routing_domain {
                checked_sub_point(junction.position, self.substrates[&substrate_id].origin)?;
            }
        }

        for wire in self.wires.values() {
            crate::polyline_length(&wire.points)?;
            if let ModuleRoutingDomain::Substrate(substrate_id) = wire.routing_domain {
                let origin = self.substrates[&substrate_id].origin;
                for &point in &wire.points {
                    checked_sub_point(point, origin)?;
                }
            }
            self.endpoint_target(wire.endpoint_a, physical)?;
            self.endpoint_target(wire.endpoint_b, physical)?;

            for first in 0..wire.points.len() - 1 {
                for second in first + 1..wire.points.len() - 1 {
                    segments_have_positive_collinear_overlap(
                        wire.points[first],
                        wire.points[first + 1],
                        wire.points[second],
                        wire.points[second + 1],
                    )?;
                    if second > first + 1 {
                        parallel_segments_are_too_close(
                            wire.points[first],
                            wire.points[first + 1],
                            wire.points[second],
                            wire.points[second + 1],
                            module_routing_pitch(wire.routing_domain, physical),
                        )?;
                    }
                }
            }
        }

        let wires: Vec<_> = self.wires.values().copied().collect();
        for (first_index, first) in wires.iter().copied().enumerate() {
            for second in wires[first_index + 1..].iter().copied() {
                if first.routing_domain != second.routing_domain {
                    continue;
                }
                for first_segment in first.points.windows(2) {
                    for second_segment in second.points.windows(2) {
                        segments_have_positive_collinear_overlap(
                            first_segment[0],
                            first_segment[1],
                            second_segment[0],
                            second_segment[1],
                        )?;
                        parallel_segments_are_too_close(
                            first_segment[0],
                            first_segment[1],
                            second_segment[0],
                            second_segment[1],
                            module_routing_pitch(first.routing_domain, physical),
                        )?;
                    }
                }
            }
        }

        for wire in self.wires.values() {
            for junction in self.junctions.values() {
                if junction.routing_domain == wire.routing_domain {
                    for segment in wire.points.windows(2) {
                        point_is_strict_segment_interior(
                            junction.position,
                            segment[0],
                            segment[1],
                        )?;
                    }
                }
            }
            for gate in self.gates.values() {
                let aabb = gate_aabb(gate.origin, gate.gate_type, physical)?;
                for (segment_index, segment) in wire.points.windows(2).enumerate() {
                    segment_intersects_aabb_interior(segment[0], segment[1], aabb)?;
                    segment_overlaps_aabb_boundary(segment[0], segment[1], aabb)?;
                    segment_touches_aabb_boundary(segment[0], segment[1], aabb)?;
                    gate_boundary_contact_is_profile_anchor(
                        &wire.points,
                        segment_index,
                        wire.routing_domain,
                        gate.gate_type,
                        gate.origin,
                        ModuleRoutingDomain::Substrate(gate.substrate),
                        physical,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn validate_structural_geometry_laws(
        &self,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ModuleError> {
        for wire in self.wires.values() {
            for first in 0..wire.points.len() - 1 {
                for second in first + 1..wire.points.len() - 1 {
                    if segments_have_positive_collinear_overlap(
                        wire.points[first],
                        wire.points[first + 1],
                        wire.points[second],
                        wire.points[second + 1],
                    )? {
                        return Err(ModuleError::GeometryOverlap {
                            first: wire.id,
                            second: wire.id,
                        });
                    }
                    if second > first + 1
                        && parallel_segments_are_too_close(
                            wire.points[first],
                            wire.points[first + 1],
                            wire.points[second],
                            wire.points[second + 1],
                            module_routing_pitch(wire.routing_domain, physical),
                        )?
                        && !segments_share_physical_endpoint(
                            &wire.points,
                            first,
                            &wire.points,
                            second,
                        )
                    {
                        return Err(ModuleError::InsufficientWireSpacing {
                            first: wire.id,
                            second: wire.id,
                        });
                    }
                }
            }
        }

        let wires: Vec<_> = self.wires.values().copied().collect();
        for (first_index, first) in wires.iter().copied().enumerate() {
            for second in wires[first_index + 1..].iter().copied() {
                if first.routing_domain != second.routing_domain {
                    continue;
                }
                for (first_segment_index, first_segment) in first.points.windows(2).enumerate() {
                    for (second_segment_index, second_segment) in
                        second.points.windows(2).enumerate()
                    {
                        if segments_have_positive_collinear_overlap(
                            first_segment[0],
                            first_segment[1],
                            second_segment[0],
                            second_segment[1],
                        )? {
                            return Err(ModuleError::GeometryOverlap {
                                first: first.id,
                                second: second.id,
                            });
                        }
                        if parallel_segments_are_too_close(
                            first_segment[0],
                            first_segment[1],
                            second_segment[0],
                            second_segment[1],
                            module_routing_pitch(first.routing_domain, physical),
                        )? && !segments_share_physical_endpoint(
                            &first.points,
                            first_segment_index,
                            &second.points,
                            second_segment_index,
                        ) {
                            return Err(ModuleError::InsufficientWireSpacing {
                                first: first.id,
                                second: second.id,
                            });
                        }
                    }
                }
            }
        }

        for wire in self.wires.values() {
            for junction in self.junctions.values() {
                if junction.routing_domain != wire.routing_domain {
                    continue;
                }
                for segment in wire.points.windows(2) {
                    if point_is_strict_segment_interior(junction.position, segment[0], segment[1])?
                    {
                        return Err(ModuleError::JunctionOnWireInterior {
                            junction: junction.id,
                            wire: wire.id,
                        });
                    }
                }
            }
        }

        for wire in self.wires.values() {
            for gate in self.gates.values() {
                self.validate_wire_gate_contact(wire, gate, physical)?;
            }
        }
        Ok(())
    }

    fn validate_wire_gate_contact(
        &self,
        wire: &WireBlueprint,
        gate: &GateBlueprint,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ModuleError> {
        let gate_domain = ModuleRoutingDomain::Substrate(gate.substrate);
        let aabb = gate_aabb(gate.origin, gate.gate_type, physical)?;
        for (segment_index, segment) in wire.points.windows(2).enumerate() {
            if segment_intersects_aabb_interior(segment[0], segment[1], aabb)?
                || segment_overlaps_aabb_boundary(segment[0], segment[1], aabb)?
            {
                return Err(ModuleError::GeometryOverlap {
                    first: wire.id,
                    second: gate.id,
                });
            }
            if segment_touches_aabb_boundary(segment[0], segment[1], aabb)?
                && !gate_boundary_contact_is_profile_anchor(
                    &wire.points,
                    segment_index,
                    wire.routing_domain,
                    gate.gate_type,
                    gate.origin,
                    gate_domain,
                    physical,
                )?
            {
                return Err(ModuleError::InvalidGateContact {
                    wire: wire.id,
                    gate: gate.id,
                });
            }
        }
        Ok(())
    }

    fn validate_wire_domain_endpoint(
        &self,
        id: ModuleLocalId,
        domain: ModuleRoutingDomain,
        point: FixedVec2,
    ) -> Result<(), ModuleError> {
        if let ModuleRoutingDomain::Substrate(substrate_id) = domain {
            let substrate = self.substrates[&substrate_id];
            let local = checked_sub_point(point, substrate.origin)?;
            if !substrate.routing_area.contains_point(local) {
                return Err(ModuleError::RoutingBoundsViolation { id });
            }
        }
        Ok(())
    }

    fn validate_routed_point(
        &self,
        id: ModuleLocalId,
        domain: ModuleRoutingDomain,
        point: FixedVec2,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ModuleError> {
        if !point_is_quantized(point, physical.wire_geometry_quantum) {
            return Err(ModuleError::NotQuantized { id });
        }
        match domain {
            ModuleRoutingDomain::OpenWorld => {
                if !point_is_quantized(point, physical.world_routing_pitch) {
                    return Err(ModuleError::InvalidRoutingPitch { id });
                }
            }
            ModuleRoutingDomain::Substrate(substrate_id) => {
                let substrate = self.substrates[&substrate_id];
                let local = checked_sub_point(point, substrate.origin)?;
                if !point_is_quantized(local, physical.circuit_routing_pitch) {
                    return Err(ModuleError::InvalidRoutingPitch { id });
                }
                if !substrate.routing_area.contains_point(local) {
                    return Err(ModuleError::RoutingBoundsViolation { id });
                }
            }
        }
        Ok(())
    }

    fn validate_wire_endpoint(
        &self,
        wire: &WireBlueprint,
        end: &'static str,
        actual_position: FixedVec2,
        endpoint: ModuleEndpoint,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ModuleError> {
        let Some((target_domain, target_position)) = self.endpoint_target(endpoint, physical)?
        else {
            return Ok(());
        };
        if target_domain != wire.routing_domain {
            return Err(ModuleError::EndpointDomainMismatch { wire: wire.id, end });
        }
        if target_position != actual_position {
            return Err(ModuleError::EndpointPositionMismatch { wire: wire.id, end });
        }
        Ok(())
    }

    fn endpoint_target(
        &self,
        endpoint: ModuleEndpoint,
        physical: &PhysicalScaleProfile,
    ) -> Result<Option<(ModuleRoutingDomain, FixedVec2)>, ModuleError> {
        match endpoint {
            ModuleEndpoint::Free => Ok(None),
            ModuleEndpoint::Junction(id) => {
                let junction = self.junctions[&id];
                Ok(Some((junction.routing_domain, junction.position)))
            }
            ModuleEndpoint::GatePort { gate, port } => {
                let gate = self.gates[&gate];
                let anchor = gate_port_anchor(gate.gate_type, port, physical).ok_or(
                    ModuleError::InvalidGatePort {
                        gate: gate.id,
                        port,
                    },
                )?;
                Ok(Some((
                    ModuleRoutingDomain::Substrate(gate.substrate),
                    checked_add_point(gate.origin, FixedVec2::new(anchor.x, anchor.y))?,
                )))
            }
        }
    }

    fn encode(&self, encoder: &mut CanonicalEncoder) -> Result<(), ModuleError> {
        encoder.count("substrates", self.substrates.len())?;
        for substrate in self.substrates.values() {
            encoder.local_id(substrate.id);
            encoder.point(substrate.origin);
            encoder.aabb(substrate.routing_area);
            encoder.aabb(substrate.footprint);
        }

        encoder.count("gates", self.gates.len())?;
        for gate in self.gates.values() {
            encoder.local_id(gate.id);
            encoder.local_id(gate.substrate);
            encoder.u8(gate_type_tag(gate.gate_type));
            encoder.point(gate.origin);
        }

        encoder.count("junctions", self.junctions.len())?;
        for junction in self.junctions.values() {
            encoder.local_id(junction.id);
            encoder.routing_domain(junction.routing_domain);
            encoder.point(junction.position);
        }

        encoder.count("wires", self.wires.len())?;
        for wire in self.wires.values() {
            encoder.local_id(wire.id);
            encoder.routing_domain(wire.routing_domain);
            encoder.count("wire.points", wire.points.len())?;
            for &point in &wire.points {
                encoder.point(point);
            }
            encoder.endpoint(wire.endpoint_a);
            encoder.endpoint(wire.endpoint_b);
        }

        let mut io_bindings: Vec<_> = self.module.io_bindings.iter().collect();
        io_bindings.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| {
                    module_endpoint_sort_key(left.endpoint)
                        .cmp(&module_endpoint_sort_key(right.endpoint))
                })
        });
        encoder.count("ioBindings", io_bindings.len())?;
        for binding in io_bindings {
            encoder.text(&binding.name)?;
            encoder.endpoint(binding.endpoint);
        }
        Ok(())
    }
}

fn insert_local_id(
    ids: &mut BTreeSet<ModuleLocalId>,
    id: ModuleLocalId,
) -> Result<(), ModuleError> {
    if ids.insert(id) {
        Ok(())
    } else {
        Err(ModuleError::DuplicateLocalId { id })
    }
}

fn canonical_count(collection: &'static str, count: usize) -> Result<u32, ModuleError> {
    u32::try_from(count).map_err(|_| ModuleError::CollectionTooLong { collection })
}

fn canonical_text(text: &str) -> Result<(), ModuleError> {
    u32::try_from(text.len())
        .map(|_| ())
        .map_err(|_| ModuleError::TextTooLong)
}

fn point_is_quantized(point: FixedVec2, quantum: Fixed) -> bool {
    quantum.0 > 0 && point.x.0.rem_euclid(quantum.0) == 0 && point.y.0.rem_euclid(quantum.0) == 0
}

fn aabb_is_quantized(aabb: FixedAabb, quantum: Fixed) -> bool {
    point_is_quantized(aabb.min, quantum) && point_is_quantized(aabb.max, quantum)
}

fn checked_sub_point(left: FixedVec2, right: FixedVec2) -> Result<FixedVec2, ModuleError> {
    Ok(FixedVec2::new(
        left.x.checked_sub(right.x)?,
        left.y.checked_sub(right.y)?,
    ))
}

fn checked_add_point(left: FixedVec2, right: FixedVec2) -> Result<FixedVec2, ModuleError> {
    Ok(FixedVec2::new(
        left.x.checked_add(right.x)?,
        left.y.checked_add(right.y)?,
    ))
}

fn gate_aabb(
    origin: FixedVec2,
    gate_type: GateType,
    physical: &PhysicalScaleProfile,
) -> Result<FixedAabb, ModuleError> {
    let footprint = match gate_type {
        GateType::And => physical.gate_footprints.and_gate,
        GateType::Or => physical.gate_footprints.or_gate,
        GateType::Not => physical.gate_footprints.not_gate,
    };
    let half = FixedVec2::new(Fixed(footprint.width.0 / 2), Fixed(footprint.height.0 / 2));
    Ok(FixedAabb::new(
        checked_sub_point(origin, half)?,
        checked_add_point(origin, half)?,
    ))
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

fn module_routing_pitch(domain: ModuleRoutingDomain, physical: &PhysicalScaleProfile) -> Fixed {
    match domain {
        ModuleRoutingDomain::OpenWorld => physical.world_routing_pitch,
        ModuleRoutingDomain::Substrate(_) => physical.circuit_routing_pitch,
    }
}

fn physical_segment_endpoints(
    points: &[FixedVec2],
    segment_index: usize,
) -> [Option<FixedVec2>; 2] {
    [
        (segment_index == 0).then_some(points[0]),
        (segment_index + 2 == points.len()).then(|| points[points.len() - 1]),
    ]
}

fn segments_share_physical_endpoint(
    first_points: &[FixedVec2],
    first_segment: usize,
    second_points: &[FixedVec2],
    second_segment: usize,
) -> bool {
    physical_segment_endpoints(first_points, first_segment)
        .into_iter()
        .flatten()
        .any(|first| {
            physical_segment_endpoints(second_points, second_segment)
                .into_iter()
                .flatten()
                .any(|second| first == second)
        })
}

fn gate_boundary_contact_is_profile_anchor(
    points: &[FixedVec2],
    segment_index: usize,
    wire_domain: ModuleRoutingDomain,
    gate_type: GateType,
    gate_origin: FixedVec2,
    gate_domain: ModuleRoutingDomain,
    physical: &PhysicalScaleProfile,
) -> Result<bool, ModuleError> {
    if wire_domain != gate_domain {
        return Ok(false);
    }
    if segment_index == 0 && point_is_gate_anchor(points[0], gate_type, gate_origin, physical)? {
        return Ok(true);
    }
    if segment_index + 2 == points.len()
        && point_is_gate_anchor(points[points.len() - 1], gate_type, gate_origin, physical)?
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
) -> Result<bool, ModuleError> {
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

fn reject_aabb_overlaps(aabbs: &BTreeMap<ModuleLocalId, FixedAabb>) -> Result<(), ModuleError> {
    let values: Vec<_> = aabbs.iter().collect();
    for (index, (&first_id, &first)) in values.iter().copied().enumerate() {
        for (&second_id, &second) in values[index + 1..].iter().copied() {
            if first.interior_overlaps(second) {
                return Err(ModuleError::GeometryOverlap {
                    first: first_id,
                    second: second_id,
                });
            }
        }
    }
    Ok(())
}

fn reject_gate_overlaps(
    gates: &BTreeMap<ModuleLocalId, (ModuleLocalId, FixedAabb)>,
) -> Result<(), ModuleError> {
    let values: Vec<_> = gates.iter().collect();
    for (index, (&first_id, &(first_domain, first))) in values.iter().copied().enumerate() {
        for (&second_id, &(second_domain, second)) in values[index + 1..].iter().copied() {
            if first_domain == second_domain && first.interior_overlaps(second) {
                return Err(ModuleError::GeometryOverlap {
                    first: first_id,
                    second: second_id,
                });
            }
        }
    }
    Ok(())
}

fn gate_type_tag(gate_type: GateType) -> u8 {
    match gate_type {
        GateType::And => 0,
        GateType::Or => 1,
        GateType::Not => 2,
    }
}

fn gate_port_tag(port: GatePort) -> u8 {
    match port {
        GatePort::InputA => 0,
        GatePort::InputB => 1,
        GatePort::Output => 2,
        GatePort::Power => 3,
    }
}

fn module_endpoint_sort_key(endpoint: ModuleEndpoint) -> (u8, u32, u8) {
    match endpoint {
        ModuleEndpoint::Free => (0, 0, 0),
        ModuleEndpoint::Junction(junction) => (1, junction.get(), 0),
        ModuleEndpoint::GatePort { gate, port } => (2, gate.get(), gate_port_tag(port)),
    }
}

struct CanonicalEncoder(Vec<u8>);

impl CanonicalEncoder {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn finish(self) -> Vec<u8> {
        self.0
    }
    fn bytes(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }
    fn count(&mut self, collection: &'static str, count: usize) -> Result<(), ModuleError> {
        self.u32(canonical_count(collection, count)?);
        Ok(())
    }
    fn text(&mut self, value: &str) -> Result<(), ModuleError> {
        self.u32(u32::try_from(value.len()).map_err(|_| ModuleError::TextTooLong)?);
        self.bytes(value.as_bytes());
        Ok(())
    }
    fn local_id(&mut self, id: ModuleLocalId) {
        self.u32(id.get());
    }
    fn point(&mut self, point: FixedVec2) {
        self.i64(point.x.0);
        self.i64(point.y.0);
    }
    fn aabb(&mut self, aabb: FixedAabb) {
        self.point(aabb.min);
        self.point(aabb.max);
    }
    fn routing_domain(&mut self, domain: ModuleRoutingDomain) {
        match domain {
            ModuleRoutingDomain::OpenWorld => self.u8(0),
            ModuleRoutingDomain::Substrate(id) => {
                self.u8(1);
                self.local_id(id);
            }
        }
    }
    fn endpoint(&mut self, endpoint: ModuleEndpoint) {
        match endpoint {
            ModuleEndpoint::Free => self.u8(0),
            ModuleEndpoint::Junction(id) => {
                self.u8(1);
                self.local_id(id);
            }
            ModuleEndpoint::GatePort { gate, port } => {
                self.u8(2);
                self.local_id(gate);
                self.u8(gate_port_tag(port));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_collection_count_over_u32_is_typed_without_allocation() {
        if usize::BITS > u32::BITS {
            assert_eq!(
                canonical_count("wires", u32::MAX as usize + 1),
                Err(ModuleError::CollectionTooLong {
                    collection: "wires"
                })
            );
        }
    }
}
