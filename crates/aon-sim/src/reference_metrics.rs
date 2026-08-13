use crate::{
    ArtifactHash, Capacity, ConstructionProbeProfile, ConstructionTarget, EndpointTarget, EnemyId,
    ExperimentRunId, GateType, HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, Integrity,
    JsonErrorCategory, MaterializedReferenceArchitecture,
    REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2, ReferenceArchitectureArtifact,
    ReferenceArchitectureFormatVersion, ReferenceArchitectureLocalId,
    ReferenceArchitectureMaterializationBatchKind, ReferenceArchitectureMaterializationStep,
    ReferenceArchitectureOperation, ReferenceArchitecturePairManifest, ReferenceArchitectureRole,
    ReferenceArchitectureRoutingDomain, ReferenceArchitectureScenarioResolution,
    ReferenceArchitectureSemanticTarget, RenderSnapshot, RoutingDomain, RunEndCause, RunStatus,
    StateHash, StepReport, Tick, WireEnd, WireId, reference_architecture_command_log_hash,
    required_construction_work,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const REFERENCE_METRIC_SET_FORMAT_V1: u32 = 1;
pub const REFERENCE_METRIC_SET_ID_V1: &str = "s1-m5-reference-baseline-v1";
pub const REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1: u32 = 1;
const REFERENCE_METRIC_SET_HASH_DOMAIN: &[u8] = b"AON\0REFERENCE-METRIC-SET\0V1\0";
const REFERENCE_METRIC_SET_CANONICAL_ENCODER_VERSION: u16 = 1;
const REFERENCE_METRIC_ARTIFACT_HASH_DOMAIN: &[u8] = b"AON\0REFERENCE-METRICS\0V1\0";
const REFERENCE_METRIC_ARTIFACT_CANONICAL_ENCODER_VERSION: u16 = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceMetric {
    TotalWireLengthRaw = 0,
    TotalWireNcu = 1,
    SharedWireLengthRaw = 2,
    SensorWireLengthRaw = 3,
    TrunkWireLengthRaw = 4,
    DefenseWireLengthRaw = 5,
    OtherWireLengthRaw = 6,
    GateCount = 7,
    AndCount = 8,
    OrCount = 9,
    NotCount = 10,
    PlannedConstructionWork = 11,
    BuildCommandCount = 12,
    CommandLogHash = 13,
    SurvivedBoundary = 14,
    CompletedTicks = 15,
    TerminalStatus = 16,
    MeasurementStartCoreIntegrity = 17,
    FinalCoreIntegrity = 18,
    CoreDamage = 19,
    PowerGeneration = 20,
    PowerNominalDemand = 21,
    PowerGranted = 22,
    PowerSourceCost = 23,
    PowerTransmissionLoss = 24,
    BrownoutTicks = 25,
    ConstructionRequested = 26,
    ConstructionNominalPower = 27,
    ConstructionGrantedWork = 28,
    ConstructionAppliedWork = 29,
    HeatGenerated = 30,
    NetworkPeakUsedNcu = 31,
    NetworkFinalUsedNcu = 32,
    NetworkIntegralUsedNcu = 33,
    SupportDemandIntegral = 34,
    EnemyKills = 35,
    ResponseLatencyTicks = 36,
}

pub const REFERENCE_METRICS_V1: [ReferenceMetric; 37] = [
    ReferenceMetric::TotalWireLengthRaw,
    ReferenceMetric::TotalWireNcu,
    ReferenceMetric::SharedWireLengthRaw,
    ReferenceMetric::SensorWireLengthRaw,
    ReferenceMetric::TrunkWireLengthRaw,
    ReferenceMetric::DefenseWireLengthRaw,
    ReferenceMetric::OtherWireLengthRaw,
    ReferenceMetric::GateCount,
    ReferenceMetric::AndCount,
    ReferenceMetric::OrCount,
    ReferenceMetric::NotCount,
    ReferenceMetric::PlannedConstructionWork,
    ReferenceMetric::BuildCommandCount,
    ReferenceMetric::CommandLogHash,
    ReferenceMetric::SurvivedBoundary,
    ReferenceMetric::CompletedTicks,
    ReferenceMetric::TerminalStatus,
    ReferenceMetric::MeasurementStartCoreIntegrity,
    ReferenceMetric::FinalCoreIntegrity,
    ReferenceMetric::CoreDamage,
    ReferenceMetric::PowerGeneration,
    ReferenceMetric::PowerNominalDemand,
    ReferenceMetric::PowerGranted,
    ReferenceMetric::PowerSourceCost,
    ReferenceMetric::PowerTransmissionLoss,
    ReferenceMetric::BrownoutTicks,
    ReferenceMetric::ConstructionRequested,
    ReferenceMetric::ConstructionNominalPower,
    ReferenceMetric::ConstructionGrantedWork,
    ReferenceMetric::ConstructionAppliedWork,
    ReferenceMetric::HeatGenerated,
    ReferenceMetric::NetworkPeakUsedNcu,
    ReferenceMetric::NetworkFinalUsedNcu,
    ReferenceMetric::NetworkIntegralUsedNcu,
    ReferenceMetric::SupportDemandIntegral,
    ReferenceMetric::EnemyKills,
    ReferenceMetric::ResponseLatencyTicks,
];

impl ReferenceMetric {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TotalWireLengthRaw => "totalWireLengthRaw",
            Self::TotalWireNcu => "totalWireNcu",
            Self::SharedWireLengthRaw => "sharedWireLengthRaw",
            Self::SensorWireLengthRaw => "sensorWireLengthRaw",
            Self::TrunkWireLengthRaw => "trunkWireLengthRaw",
            Self::DefenseWireLengthRaw => "defenseWireLengthRaw",
            Self::OtherWireLengthRaw => "otherWireLengthRaw",
            Self::GateCount => "gateCount",
            Self::AndCount => "andCount",
            Self::OrCount => "orCount",
            Self::NotCount => "notCount",
            Self::PlannedConstructionWork => "plannedConstructionWork",
            Self::BuildCommandCount => "buildCommandCount",
            Self::CommandLogHash => "commandLogHash",
            Self::SurvivedBoundary => "survivedBoundary",
            Self::CompletedTicks => "completedTicks",
            Self::TerminalStatus => "terminalStatus",
            Self::MeasurementStartCoreIntegrity => "measurementStartCoreIntegrity",
            Self::FinalCoreIntegrity => "finalCoreIntegrity",
            Self::CoreDamage => "coreDamage",
            Self::PowerGeneration => "powerGeneration",
            Self::PowerNominalDemand => "powerNominalDemand",
            Self::PowerGranted => "powerGranted",
            Self::PowerSourceCost => "powerSourceCost",
            Self::PowerTransmissionLoss => "powerTransmissionLoss",
            Self::BrownoutTicks => "brownoutTicks",
            Self::ConstructionRequested => "constructionRequested",
            Self::ConstructionNominalPower => "constructionNominalPower",
            Self::ConstructionGrantedWork => "constructionGrantedWork",
            Self::ConstructionAppliedWork => "constructionAppliedWork",
            Self::HeatGenerated => "heatGenerated",
            Self::NetworkPeakUsedNcu => "networkPeakUsedNcu",
            Self::NetworkFinalUsedNcu => "networkFinalUsedNcu",
            Self::NetworkIntegralUsedNcu => "networkIntegralUsedNcu",
            Self::SupportDemandIntegral => "supportDemandIntegral",
            Self::EnemyKills => "enemyKills",
            Self::ResponseLatencyTicks => "responseLatencyTicks",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        REFERENCE_METRICS_V1
            .into_iter()
            .find(|metric| metric.as_str() == value)
    }
}

/// One hash-significant pairing of semantic Architecture observation names.
/// Runtime Entity IDs are deliberately absent from this portable definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceResponseObservationSpec {
    pub name: String,
    pub hostile_entry_binding: String,
    pub defense_contact_binding: String,
    pub enemy_ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceMetricSetArtifact {
    format_version: u32,
    hash_algorithm_id: HashAlgorithmId,
    metric_set_id: String,
    metrics: Vec<ReferenceMetric>,
    response_observations: Vec<ReferenceResponseObservationSpec>,
}

impl ReferenceMetricSetArtifact {
    pub fn v1(
        mut response_observations: Vec<ReferenceResponseObservationSpec>,
    ) -> Result<Self, ReferenceMetricError> {
        response_observations.sort_unstable();
        let artifact = Self {
            format_version: REFERENCE_METRIC_SET_FORMAT_V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            metric_set_id: REFERENCE_METRIC_SET_ID_V1.to_owned(),
            metrics: REFERENCE_METRICS_V1.to_vec(),
            response_observations,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub const fn hash_algorithm_id(&self) -> HashAlgorithmId {
        self.hash_algorithm_id
    }

    pub fn metric_set_id(&self) -> &str {
        &self.metric_set_id
    }

    pub fn metrics(&self) -> &[ReferenceMetric] {
        &self.metrics
    }

    pub fn response_observations(&self) -> &[ReferenceResponseObservationSpec] {
        &self.response_observations
    }

    pub fn semantic_hash(&self) -> Result<ArtifactHash, ReferenceMetricError> {
        self.validate()?;
        let mut encoder = MetricEncoder::default();
        encoder.bytes(REFERENCE_METRIC_SET_HASH_DOMAIN);
        encoder.u16(REFERENCE_METRIC_SET_CANONICAL_ENCODER_VERSION);
        encoder.u32(self.format_version);
        encoder.text(self.hash_algorithm_id.as_str())?;
        encoder.text(&self.metric_set_id)?;
        encoder.count("metrics", self.metrics.len())?;
        for metric in &self.metrics {
            encoder.u8(*metric as u8);
        }
        encoder.count("responseObservations", self.response_observations.len())?;
        for observation in &self.response_observations {
            encoder.text(&observation.name)?;
            encoder.text(&observation.hostile_entry_binding)?;
            encoder.text(&observation.defense_contact_binding)?;
            encoder.u32(observation.enemy_ordinal);
        }
        Ok(ArtifactHash::from_bytes(
            *blake3::hash(&encoder.finish()).as_bytes(),
        ))
    }

    fn validate(&self) -> Result<(), ReferenceMetricError> {
        if self.format_version != REFERENCE_METRIC_SET_FORMAT_V1 {
            return Err(ReferenceMetricError::UnsupportedFormatVersion {
                expected: REFERENCE_METRIC_SET_FORMAT_V1,
                actual: self.format_version,
            });
        }
        if self.hash_algorithm_id != HashAlgorithmId::Blake3V1 {
            return Err(ReferenceMetricError::UnsupportedHashAlgorithm {
                expected: HASH_ALGORITHM_ID_BLAKE3_V1,
                actual: self.hash_algorithm_id.as_str().to_owned(),
            });
        }
        if self.metric_set_id != REFERENCE_METRIC_SET_ID_V1 {
            return Err(ReferenceMetricError::MetricSetIdMismatch {
                expected: REFERENCE_METRIC_SET_ID_V1,
                actual: self.metric_set_id.clone(),
            });
        }
        if self.metrics.as_slice() != REFERENCE_METRICS_V1 {
            return Err(ReferenceMetricError::MetricListMismatch);
        }
        validate_response_specs(&self.response_observations)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceMetricSetWire {
    format_version: u32,
    hash_algorithm_id: String,
    metric_set_id: String,
    metrics: Vec<String>,
    response_observations: Vec<ReferenceResponseObservationWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceMetricFormatEnvelope {
    format_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceMetricHashEnvelope {
    hash_algorithm_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceResponseObservationWire {
    name: String,
    hostile_entry_binding: String,
    defense_contact_binding: String,
    enemy_ordinal: u32,
}

pub fn decode_reference_metric_set_artifact(
    bytes: &[u8],
) -> Result<ReferenceMetricSetArtifact, ReferenceMetricError> {
    let envelope: ReferenceMetricFormatEnvelope = decode_metric_json(bytes)?;
    if envelope.format_version != REFERENCE_METRIC_SET_FORMAT_V1 {
        return Err(ReferenceMetricError::UnsupportedFormatVersion {
            expected: REFERENCE_METRIC_SET_FORMAT_V1,
            actual: envelope.format_version,
        });
    }
    let hash_envelope: ReferenceMetricHashEnvelope = decode_metric_json(bytes)?;
    if hash_envelope.hash_algorithm_id != HASH_ALGORITHM_ID_BLAKE3_V1 {
        return Err(ReferenceMetricError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: hash_envelope.hash_algorithm_id,
        });
    }
    let wire: ReferenceMetricSetWire = decode_metric_json(bytes)?;
    let hash_algorithm_id = HashAlgorithmId::parse(&wire.hash_algorithm_id).map_err(|_| {
        ReferenceMetricError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: wire.hash_algorithm_id.clone(),
        }
    })?;
    let metrics = wire
        .metrics
        .iter()
        .map(|metric| {
            ReferenceMetric::parse(metric)
                .ok_or_else(|| ReferenceMetricError::UnknownMetric(metric.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let artifact = ReferenceMetricSetArtifact {
        format_version: wire.format_version,
        hash_algorithm_id,
        metric_set_id: wire.metric_set_id,
        metrics,
        response_observations: wire
            .response_observations
            .into_iter()
            .map(|observation| {
                Ok(ReferenceResponseObservationSpec {
                    name: observation.name,
                    hostile_entry_binding: observation.hostile_entry_binding,
                    defense_contact_binding: observation.defense_contact_binding,
                    enemy_ordinal: observation.enemy_ordinal,
                })
            })
            .collect::<Result<Vec<_>, ReferenceMetricError>>()?,
    };
    artifact.validate()?;
    Ok(artifact)
}

pub fn encode_reference_metric_set_artifact(
    artifact: &ReferenceMetricSetArtifact,
) -> Result<Vec<u8>, ReferenceMetricError> {
    artifact.validate()?;
    let wire = ReferenceMetricSetWire {
        format_version: artifact.format_version,
        hash_algorithm_id: artifact.hash_algorithm_id.as_str().to_owned(),
        metric_set_id: artifact.metric_set_id.clone(),
        metrics: artifact
            .metrics
            .iter()
            .map(|metric| metric.as_str().to_owned())
            .collect(),
        response_observations: artifact
            .response_observations
            .iter()
            .map(|observation| ReferenceResponseObservationWire {
                name: observation.name.clone(),
                hostile_entry_binding: observation.hostile_entry_binding.clone(),
                defense_contact_binding: observation.defense_contact_binding.clone(),
                enemy_ordinal: observation.enemy_ordinal,
            })
            .collect(),
    };
    let mut encoded =
        serde_json::to_vec_pretty(&wire).map_err(|error| ReferenceMetricError::EncodeJson {
            message: error.to_string(),
        })?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn validate_response_specs(
    observations: &[ReferenceResponseObservationSpec],
) -> Result<(), ReferenceMetricError> {
    canonical_count("responseObservations", observations.len())?;
    if observations.is_empty() {
        return Err(ReferenceMetricError::EmptyResponseObservations);
    }
    let mut previous: Option<&ReferenceResponseObservationSpec> = None;
    let mut sensor_bindings = BTreeSet::new();
    let mut defense_bindings = BTreeSet::new();
    for observation in observations {
        if observation.name.is_empty() {
            return Err(ReferenceMetricError::EmptyText {
                field: "responseObservations[].name",
            });
        }
        canonical_text(&observation.name)?;
        for (field, value) in [
            (
                "responseObservations[].hostileEntryBinding",
                &observation.hostile_entry_binding,
            ),
            (
                "responseObservations[].defenseContactBinding",
                &observation.defense_contact_binding,
            ),
        ] {
            if value.is_empty() {
                return Err(ReferenceMetricError::EmptyText { field });
            }
            canonical_text(value)?;
        }
        if let Some(prior) = previous {
            if prior == observation {
                return Err(ReferenceMetricError::DuplicateResponseObservation {
                    name: observation.name.clone(),
                });
            }
            if prior >= observation {
                return Err(ReferenceMetricError::NonCanonicalResponseOrder);
            }
            if prior.name == observation.name {
                return Err(ReferenceMetricError::DuplicateResponseObservation {
                    name: observation.name.clone(),
                });
            }
        }
        if !sensor_bindings.insert((
            observation.hostile_entry_binding.as_str(),
            observation.enemy_ordinal,
        )) {
            return Err(ReferenceMetricError::DuplicateObservationBinding {
                name: observation.name.clone(),
            });
        }
        if !defense_bindings.insert((
            observation.defense_contact_binding.as_str(),
            observation.enemy_ordinal,
        )) {
            return Err(ReferenceMetricError::DuplicateObservationBinding {
                name: observation.name.clone(),
            });
        }
        previous = Some(observation);
    }
    Ok(())
}

/// Runtime resolution of one portable response-observation row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedReferenceResponseObservation {
    pub name: String,
    pub sensor_wire: WireId,
    pub sensor_end: WireEnd,
    pub defense_wire: WireId,
    pub enemy: EnemyId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceStaticInventory {
    pub total_wire_length_raw: i64,
    pub total_wire_ncu: Capacity,
    pub shared_wire_length_raw: i64,
    pub sensor_wire_length_raw: i64,
    pub trunk_wire_length_raw: i64,
    pub defense_wire_length_raw: i64,
    pub other_wire_length_raw: i64,
    pub gate_count: u64,
    pub and_count: u64,
    pub or_count: u64,
    pub not_count: u64,
    pub planned_construction_work: u128,
    pub build_command_count: u64,
    pub command_log_hash: ArtifactHash,
}

impl ReferenceStaticInventory {
    pub fn validate(&self) -> Result<(), ReferenceMetricError> {
        let rows = [
            ("totalWireLengthRaw", self.total_wire_length_raw),
            ("sharedWireLengthRaw", self.shared_wire_length_raw),
            ("sensorWireLengthRaw", self.sensor_wire_length_raw),
            ("trunkWireLengthRaw", self.trunk_wire_length_raw),
            ("defenseWireLengthRaw", self.defense_wire_length_raw),
            ("otherWireLengthRaw", self.other_wire_length_raw),
        ];
        for (field, value) in rows {
            if value < 0 {
                return Err(ReferenceMetricError::NegativeStaticLength { field, value });
            }
        }
        let expected_ncu = u64::try_from(self.total_wire_length_raw)
            .map(Capacity)
            .map_err(|_| ReferenceMetricError::ArithmeticOverflow)?;
        if self.total_wire_ncu != expected_ncu {
            return Err(ReferenceMetricError::WireNcuMismatch {
                expected: expected_ncu,
                actual: self.total_wire_ncu,
            });
        }
        let subtotal = [
            self.shared_wire_length_raw,
            self.sensor_wire_length_raw,
            self.trunk_wire_length_raw,
            self.defense_wire_length_raw,
            self.other_wire_length_raw,
        ]
        .into_iter()
        .try_fold(0_i64, |total, value| total.checked_add(value))
        .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
        if subtotal != self.total_wire_length_raw {
            return Err(ReferenceMetricError::WireLengthSubtotalMismatch {
                expected: self.total_wire_length_raw,
                actual: subtotal,
            });
        }
        let gates = self
            .and_count
            .checked_add(self.or_count)
            .and_then(|value| value.checked_add(self.not_count))
            .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
        if gates != self.gate_count {
            return Err(ReferenceMetricError::GateCountMismatch {
                expected: self.gate_count,
                actual: gates,
            });
        }
        Ok(())
    }

    /// Cross-checks only materialized facts visible in a render snapshot. Semantic role
    /// subtotals and planned Work remain design-artifact facts.
    pub fn validate_materialized_snapshot(
        &self,
        snapshot: &RenderSnapshot,
    ) -> Result<(), ReferenceMetricError> {
        self.validate()?;
        let measured = snapshot_wire_length_raw(snapshot)?;
        if measured != self.total_wire_length_raw {
            return Err(ReferenceMetricError::MaterializedWireLengthMismatch {
                expected: self.total_wire_length_raw,
                actual: measured,
            });
        }
        let (and_count, or_count, not_count) = gate_counts(snapshot)?;
        if (and_count, or_count, not_count) != (self.and_count, self.or_count, self.not_count) {
            return Err(ReferenceMetricError::MaterializedGateInventoryMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WireMetricClass {
    Shared,
    Sensor,
    Trunk,
    Defense,
    Other,
}

/// Recomputes the complete static inventory from validated Architecture operations and semantic
/// roles. The reserved role prefixes `shared.`, `sensor.`, `trunk.`, and `defense.` are mutually
/// exclusive; unclassified Wires contribute to `other`.
pub fn derive_reference_static_inventory(
    architecture: &ReferenceArchitectureArtifact,
    construction_probe: &ConstructionProbeProfile,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<ReferenceStaticInventory, ReferenceMetricError> {
    validate_materialization_evidence(architecture, materialization, scenario)?;
    if materialization.local_entities.len() != architecture.operations.len() {
        return Err(ReferenceMetricError::MaterializationInventoryMismatch {
            expected: architecture.operations.len(),
            actual: materialization.local_entities.len(),
        });
    }
    let build_command_count = u64::try_from(materialization.commands.len())
        .map_err(|_| ReferenceMetricError::ArithmeticOverflow)?;
    let command_log_hash = reference_architecture_command_log_hash(&materialization.commands)
        .map_err(ReferenceMetricError::Architecture)?;
    if command_log_hash != materialization.command_log_hash {
        return Err(ReferenceMetricError::CommandLogHashMismatch {
            expected: command_log_hash,
            actual: materialization.command_log_hash,
        });
    }
    let mut wire_classes = std::collections::BTreeMap::new();
    for (name, target) in architecture
        .role_bindings
        .iter()
        .map(|binding| (binding.name.as_str(), binding.target))
        .chain(
            architecture
                .observation_bindings
                .iter()
                .map(|binding| (binding.name.as_str(), binding.target)),
        )
    {
        let Some(class) = role_wire_class(name) else {
            continue;
        };
        let local_id = semantic_target_wire(target).ok_or_else(|| {
            ReferenceMetricError::ReservedWireRoleTargetsNonWire {
                name: name.to_owned(),
            }
        })?;
        if let Some(previous) = wire_classes.insert(local_id, class)
            && previous != class
        {
            return Err(ReferenceMetricError::ConflictingWireRoleClass { local_id });
        }
    }

    let mut inventory = ReferenceStaticInventory {
        total_wire_length_raw: 0,
        total_wire_ncu: Capacity(0),
        shared_wire_length_raw: 0,
        sensor_wire_length_raw: 0,
        trunk_wire_length_raw: 0,
        defense_wire_length_raw: 0,
        other_wire_length_raw: 0,
        gate_count: 0,
        and_count: 0,
        or_count: 0,
        not_count: 0,
        planned_construction_work: 0,
        build_command_count,
        command_log_hash,
    };
    let mut seen_ids = BTreeSet::new();
    for operation in &architecture.operations {
        if !seen_ids.insert(operation.local_id()) {
            return Err(ReferenceMetricError::DuplicateArchitectureLocalId {
                local_id: operation.local_id(),
            });
        }
        match operation {
            ReferenceArchitectureOperation::PlaceWire(wire) => {
                let length = crate::polyline_length(&wire.points)
                    .map_err(|_| ReferenceMetricError::WireGeometry)?;
                if length.0 < 0 {
                    return Err(ReferenceMetricError::WireGeometry);
                }
                inventory.total_wire_length_raw =
                    checked_add_i64(inventory.total_wire_length_raw, length.0)?;
                let class = wire_classes
                    .get(&wire.id)
                    .copied()
                    .unwrap_or(WireMetricClass::Other);
                let subtotal = match class {
                    WireMetricClass::Shared => &mut inventory.shared_wire_length_raw,
                    WireMetricClass::Sensor => &mut inventory.sensor_wire_length_raw,
                    WireMetricClass::Trunk => &mut inventory.trunk_wire_length_raw,
                    WireMetricClass::Defense => &mut inventory.defense_wire_length_raw,
                    WireMetricClass::Other => &mut inventory.other_wire_length_raw,
                };
                *subtotal = checked_add_i64(*subtotal, length.0)?;
                add_planned_work(
                    &mut inventory.planned_construction_work,
                    ConstructionTarget::Wire {
                        routing_domain: resolve_metric_routing_domain(
                            wire.routing_domain,
                            materialization,
                        )?,
                        points: wire.points.clone(),
                        endpoint_a: EndpointTarget::Free,
                        endpoint_b: EndpointTarget::Free,
                    },
                    construction_probe,
                )?;
            }
            ReferenceArchitectureOperation::PlaceGate(gate) => {
                inventory.gate_count = inventory
                    .gate_count
                    .checked_add(1)
                    .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
                let counter = match gate.gate_type {
                    GateType::And => &mut inventory.and_count,
                    GateType::Or => &mut inventory.or_count,
                    GateType::Not => &mut inventory.not_count,
                };
                *counter = counter
                    .checked_add(1)
                    .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
                add_planned_work(
                    &mut inventory.planned_construction_work,
                    ConstructionTarget::Gate {
                        gate_type: gate.gate_type,
                        origin: gate.origin,
                        routing_domain: resolve_metric_routing_domain(
                            gate.routing_domain,
                            materialization,
                        )?,
                    },
                    construction_probe,
                )?;
            }
            ReferenceArchitectureOperation::PlaceJunction(junction) => add_planned_work(
                &mut inventory.planned_construction_work,
                ConstructionTarget::Junction {
                    routing_domain: resolve_metric_routing_domain(
                        junction.routing_domain,
                        materialization,
                    )?,
                    position: junction.position,
                },
                construction_probe,
            )?,
            ReferenceArchitectureOperation::PlaceFixedSubstrate(substrate) => add_planned_work(
                &mut inventory.planned_construction_work,
                ConstructionTarget::FixedSubstrate {
                    origin: substrate.origin,
                    routing_area: substrate.routing_area,
                    footprint: substrate.footprint,
                },
                construction_probe,
            )?,
            ReferenceArchitectureOperation::PlaceMobileSubstrate(mobile) => {
                return Err(ReferenceMetricError::UnsupportedPlannedConstructionTarget {
                    local_id: mobile.id,
                });
            }
        }
    }
    inventory.total_wire_ncu = Capacity(
        u64::try_from(inventory.total_wire_length_raw)
            .map_err(|_| ReferenceMetricError::ArithmeticOverflow)?,
    );
    for local_id in wire_classes.keys() {
        if !architecture.operations.iter().any(|operation| {
            matches!(operation, ReferenceArchitectureOperation::PlaceWire(wire) if wire.id == *local_id)
        }) {
            return Err(ReferenceMetricError::WireRoleTargetsNonWire {
                local_id: *local_id,
            });
        }
    }
    inventory.validate()?;
    Ok(inventory)
}

fn resolve_metric_routing_domain(
    domain: ReferenceArchitectureRoutingDomain,
    materialization: &MaterializedReferenceArchitecture,
) -> Result<RoutingDomain, ReferenceMetricError> {
    Ok(match domain {
        ReferenceArchitectureRoutingDomain::OpenWorld => RoutingDomain::OpenWorld,
        ReferenceArchitectureRoutingDomain::FixedSubstrate(local_id) => {
            RoutingDomain::FixedSubstrate(
                *materialization
                    .local_entities
                    .get(&local_id)
                    .ok_or(ReferenceMetricError::MissingMaterializedLocalId { local_id })?,
            )
        }
        ReferenceArchitectureRoutingDomain::MobileSubstrate(local_id) => {
            RoutingDomain::MobileSubstrate(
                *materialization
                    .local_entities
                    .get(&local_id)
                    .ok_or(ReferenceMetricError::MissingMaterializedLocalId { local_id })?,
            )
        }
    })
}

fn validate_materialization_evidence(
    architecture: &ReferenceArchitectureArtifact,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<(), ReferenceMetricError> {
    let expected_ids = architecture
        .operations
        .iter()
        .map(ReferenceArchitectureOperation::local_id)
        .collect::<BTreeSet<_>>();
    let actual_ids = materialization
        .local_entities
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    if expected_ids != actual_ids {
        return Err(ReferenceMetricError::MaterializationLocalIdsMismatch);
    }
    let plan = architecture
        .materialization_plan()
        .map_err(ReferenceMetricError::Architecture)?;
    if materialization.commands.len() != plan.steps().len()
        || materialization.commands.len() != materialization.acceptances.len()
    {
        return Err(
            ReferenceMetricError::MaterializationAcceptanceCountMismatch {
                commands: materialization.commands.len(),
                acceptances: materialization.acceptances.len(),
            },
        );
    }
    match architecture.format_version {
        ReferenceArchitectureFormatVersion::V1 => {
            validate_v1_materialization_timeline(&plan, materialization, scenario)?
        }
        ReferenceArchitectureFormatVersion::V2 => {
            validate_v2_materialization_timeline(&plan, materialization, scenario)?
        }
    }
    let mut created_entities = BTreeSet::new();
    for (command, acceptance) in materialization
        .commands
        .iter()
        .zip(&materialization.acceptances)
    {
        if command.target_tick != acceptance.target_tick || command.ordinal != acceptance.ordinal {
            return Err(ReferenceMetricError::MaterializationAcceptanceMismatch);
        }
        if let Some(entity) = acceptance.created_entity
            && !created_entities.insert(entity)
        {
            return Err(ReferenceMetricError::DuplicateMaterializedEntity { entity });
        }
    }
    let mapped_entities = materialization
        .local_entities
        .values()
        .copied()
        .collect::<BTreeSet<_>>();
    if created_entities != mapped_entities {
        return Err(ReferenceMetricError::MaterializationCreatedEntitiesMismatch);
    }
    Ok(())
}

fn validate_v1_materialization_timeline(
    plan: &crate::ReferenceArchitectureMaterializationPlan,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<(), ReferenceMetricError> {
    if !materialization.executed_batch_evidence.is_empty()
        || !materialization.binding_stage_evidence.is_empty()
    {
        return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
    }
    let mut command_index = 0_usize;
    let mut previous_command_tick = None;
    for (_, batch) in plan.execution_batches() {
        let target_tick = materialization
            .commands
            .get(command_index)
            .map(|command| command.target_tick)
            .ok_or(ReferenceMetricError::MaterializationBuildBoundaryMismatch)?;
        if previous_command_tick.and_then(next_tick) != Some(target_tick)
            && previous_command_tick.is_some()
        {
            return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
        }
        validate_materialization_batch_commands(
            batch,
            target_tick,
            &mut command_index,
            materialization,
            scenario,
        )?;
        previous_command_tick = Some(target_tick);
    }
    if command_index != materialization.commands.len() {
        return Err(ReferenceMetricError::MaterializationCommandMismatch {
            index: command_index,
        });
    }
    if previous_command_tick.and_then(next_tick) != Some(materialization.build_end_tick) {
        return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
    }
    Ok(())
}

fn validate_v2_materialization_timeline(
    plan: &crate::ReferenceArchitectureMaterializationPlan,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<(), ReferenceMetricError> {
    if materialization.executed_batch_evidence.is_empty()
        || materialization.binding_stage_evidence.is_empty()
    {
        return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
    }
    let expected_kinds = plan
        .execution_batches()
        .map(|(kind, _)| kind)
        .collect::<Vec<_>>();
    let mut observed_expected_kinds = vec![false; expected_kinds.len()];
    let mut command_index = 0_usize;
    let mut binding_evidence_index = 0_usize;
    let mut prior_batch_order = None;
    let mut required_command_tick = None;

    for executed in &materialization.executed_batch_evidence {
        let batch_order = v2_batch_order(executed.kind)
            .ok_or(ReferenceMetricError::MaterializationBuildBoundaryMismatch)?;
        if prior_batch_order.is_some_and(|prior| prior >= batch_order)
            || required_command_tick.is_some_and(|required| required != executed.command_tick)
        {
            return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
        }
        prior_batch_order = Some(batch_order);

        let batch = if let Some(expected_index) = expected_kinds
            .iter()
            .position(|&expected| expected == executed.kind)
        {
            observed_expected_kinds[expected_index] = true;
            plan.batch(executed.kind)
                .expect("kind was collected from the exact plan")
        } else if matches!(
            executed.kind,
            ReferenceArchitectureMaterializationBatchKind::Placement { phase: 0..=5 }
        ) {
            // Pair materialization executes the union of both placement phases. The side that has
            // no local commands for a shared phase still records the empty batch it actually ran.
            &[]
        } else {
            return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
        };
        validate_materialization_batch_commands(
            batch,
            executed.command_tick,
            &mut command_index,
            materialization,
            scenario,
        )?;

        let boundary_after_batch =
            next_tick(executed.command_tick).ok_or(ReferenceMetricError::ArithmeticOverflow)?;
        required_command_tick = Some(match executed.kind {
            ReferenceArchitectureMaterializationBatchKind::Placement { .. } => boundary_after_batch,
            ReferenceArchitectureMaterializationBatchKind::Binding { stage } => {
                let evidence = materialization
                    .binding_stage_evidence
                    .get(binding_evidence_index)
                    .ok_or(ReferenceMetricError::MaterializationBuildBoundaryMismatch)?;
                binding_evidence_index += 1;
                if evidence.stage != stage
                    || evidence.command_tick != executed.command_tick
                    || evidence.barrier_ticks.len()
                        > usize::from(REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2)
                {
                    return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
                }
                let mut expected_tick = boundary_after_batch;
                for &completed_tick in &evidence.barrier_ticks {
                    if completed_tick != expected_tick {
                        return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
                    }
                    expected_tick =
                        next_tick(expected_tick).ok_or(ReferenceMetricError::ArithmeticOverflow)?;
                }
                if evidence.quiescent_tick != expected_tick {
                    return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
                }
                evidence.quiescent_tick
            }
        });
    }

    if observed_expected_kinds.iter().any(|&observed| !observed)
        || command_index != materialization.commands.len()
        || binding_evidence_index != materialization.binding_stage_evidence.len()
        || required_command_tick != Some(materialization.build_end_tick)
    {
        return Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch);
    }
    Ok(())
}

fn validate_materialization_batch_commands(
    batch: &[ReferenceArchitectureMaterializationStep],
    target_tick: Tick,
    command_index: &mut usize,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<(), ReferenceMetricError> {
    for (ordinal, step) in batch.iter().enumerate() {
        let ordinal =
            u64::try_from(ordinal).map_err(|_| ReferenceMetricError::ArithmeticOverflow)?;
        let expected = step
            .resolve_command(
                target_tick,
                ordinal,
                &materialization.local_entities,
                scenario,
            )
            .map_err(ReferenceMetricError::Architecture)?;
        if materialization.commands.get(*command_index) != Some(&expected) {
            return Err(ReferenceMetricError::MaterializationCommandMismatch {
                index: *command_index,
            });
        }
        let acceptance = materialization.acceptances[*command_index];
        let expected_created = step
            .created_local_id()
            .map(|local_id| materialization.local_entities[&local_id]);
        if acceptance.target_tick != target_tick
            || acceptance.ordinal != ordinal
            || acceptance.created_entity != expected_created
        {
            return Err(ReferenceMetricError::MaterializationAcceptanceMismatch);
        }
        *command_index += 1;
    }
    Ok(())
}

fn v2_batch_order(kind: ReferenceArchitectureMaterializationBatchKind) -> Option<u16> {
    match kind {
        ReferenceArchitectureMaterializationBatchKind::Placement { phase } if phase < 6 => {
            Some(u16::from(phase))
        }
        ReferenceArchitectureMaterializationBatchKind::Placement { .. } => None,
        ReferenceArchitectureMaterializationBatchKind::Binding { stage } => {
            Some(6 + u16::from(stage))
        }
    }
}

fn next_tick(tick: Tick) -> Option<Tick> {
    tick.0.checked_add(1).map(Tick)
}

/// Resolves every portable Metric Set row against one exact materialized Architecture and the
/// Scenario-v4 canonical identity tables. Both local IDs must denote Wires, and every Enemy
/// ordinal is checked before a collector can observe any Tick.
pub fn resolve_reference_response_observations(
    metric_set: &ReferenceMetricSetArtifact,
    architecture: &ReferenceArchitectureArtifact,
    materialization: &MaterializedReferenceArchitecture,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<Vec<ResolvedReferenceResponseObservation>, ReferenceMetricError> {
    metric_set.validate()?;
    validate_materialization_evidence(architecture, materialization, scenario)?;
    let wire_ids = architecture
        .operations
        .iter()
        .filter_map(|operation| match operation {
            ReferenceArchitectureOperation::PlaceWire(wire) => Some(wire.id),
            ReferenceArchitectureOperation::PlaceGate(_)
            | ReferenceArchitectureOperation::PlaceJunction(_)
            | ReferenceArchitectureOperation::PlaceFixedSubstrate(_)
            | ReferenceArchitectureOperation::PlaceMobileSubstrate(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let mut resolved = Vec::with_capacity(metric_set.response_observations.len());
    for row in &metric_set.response_observations {
        let sensor_binding = architecture
            .observation_bindings
            .iter()
            .find(|binding| binding.name == row.hostile_entry_binding)
            .ok_or_else(|| ReferenceMetricError::MissingArchitectureBinding {
                role: "materialized",
                name: row.hostile_entry_binding.clone(),
            })?;
        let (sensor_local_id, sensor_end) = match sensor_binding.target {
            ReferenceArchitectureSemanticTarget::WireSensePort { wire, end } => (wire, end),
            _ => {
                return Err(ReferenceMetricError::ArchitectureBindingTargetMismatch {
                    role: "materialized",
                    name: row.hostile_entry_binding.clone(),
                });
            }
        };
        let defense_binding = architecture
            .role_bindings
            .iter()
            .find(|binding| binding.name == row.defense_contact_binding)
            .ok_or_else(|| ReferenceMetricError::MissingArchitectureBinding {
                role: "materialized",
                name: row.defense_contact_binding.clone(),
            })?;
        let defense_local_id = match defense_binding.target {
            ReferenceArchitectureSemanticTarget::LocalEntity(local_id) => local_id,
            _ => {
                return Err(ReferenceMetricError::ArchitectureBindingTargetMismatch {
                    role: "materialized",
                    name: row.defense_contact_binding.clone(),
                });
            }
        };
        for local_id in [sensor_local_id, defense_local_id] {
            if !wire_ids.contains(&local_id) {
                return Err(ReferenceMetricError::ObservationTargetsNonWire {
                    name: row.name.clone(),
                    local_id,
                });
            }
        }
        let sensor = materialization
            .local_entities
            .get(&sensor_local_id)
            .copied()
            .ok_or(ReferenceMetricError::MissingMaterializedLocalId {
                local_id: sensor_local_id,
            })?;
        let defense = materialization
            .local_entities
            .get(&defense_local_id)
            .copied()
            .ok_or(ReferenceMetricError::MissingMaterializedLocalId {
                local_id: defense_local_id,
            })?;
        let enemy = *scenario.enemies.get(row.enemy_ordinal as usize).ok_or(
            ReferenceMetricError::MissingScenarioEnemy {
                ordinal: row.enemy_ordinal,
            },
        )?;
        resolved.push(ResolvedReferenceResponseObservation {
            name: row.name.clone(),
            sensor_wire: WireId(sensor),
            sensor_end,
            defense_wire: WireId(defense),
            enemy,
        });
    }
    Ok(resolved)
}

/// Proves that the portable pair names and the single Metric Set select the same exact sensor and
/// defense Wires in both architecture artifacts. This is a static fairness check; runtime Entity
/// IDs are resolved separately after each private materialization.
pub fn validate_reference_metric_bindings(
    pair: &ReferenceArchitecturePairManifest,
    metric_set: &ReferenceMetricSetArtifact,
    brute: &ReferenceArchitectureArtifact,
    computed: &ReferenceArchitectureArtifact,
) -> Result<(), ReferenceMetricError> {
    metric_set.validate()?;
    if pair.metric_set_id() != metric_set.metric_set_id()
        || pair.metric_set_hash() != metric_set.semantic_hash()?
    {
        return Err(ReferenceMetricError::PairMetricSetMismatch);
    }
    for (role, architecture) in [
        (ReferenceArchitectureRole::Brute, brute),
        (ReferenceArchitectureRole::Computed, computed),
    ] {
        let pair_design = pair
            .designs()
            .iter()
            .find(|binding| binding.role == role)
            .ok_or(ReferenceMetricError::PairDesignMismatch { role })?;
        let actual_hash = architecture
            .semantic_hash()
            .map_err(ReferenceMetricError::Architecture)?;
        if pair_design.design.artifact_hash() != actual_hash
            || architecture.contract != pair.contract()
        {
            return Err(ReferenceMetricError::PairDesignMismatch { role });
        }
    }
    if pair.response_bindings().len() != metric_set.response_observations.len() {
        return Err(ReferenceMetricError::PairResponseBindingMismatch);
    }
    for (pair_row, metric_row) in pair
        .response_bindings()
        .iter()
        .zip(&metric_set.response_observations)
    {
        if pair_row.name != metric_row.name
            || pair_row.hostile_entry_binding != metric_row.hostile_entry_binding
            || pair_row.defense_contact_binding != metric_row.defense_contact_binding
        {
            return Err(ReferenceMetricError::PairResponseBindingMismatch);
        }
        for (role, architecture) in [("brute", brute), ("computed", computed)] {
            let sensor = architecture
                .observation_bindings
                .iter()
                .find(|binding| binding.name == metric_row.hostile_entry_binding)
                .ok_or_else(|| ReferenceMetricError::MissingArchitectureBinding {
                    role,
                    name: pair_row.hostile_entry_binding.clone(),
                })?;
            if !matches!(
                sensor.target,
                ReferenceArchitectureSemanticTarget::WireSensePort { .. }
            ) {
                return Err(ReferenceMetricError::ArchitectureBindingTargetMismatch {
                    role,
                    name: pair_row.hostile_entry_binding.clone(),
                });
            }
            let defense = architecture
                .role_bindings
                .iter()
                .find(|binding| binding.name == metric_row.defense_contact_binding)
                .ok_or_else(|| ReferenceMetricError::MissingArchitectureBinding {
                    role,
                    name: pair_row.defense_contact_binding.clone(),
                })?;
            let defense_local_id = match defense.target {
                ReferenceArchitectureSemanticTarget::LocalEntity(local_id) => local_id,
                _ => {
                    return Err(ReferenceMetricError::ArchitectureBindingTargetMismatch {
                        role,
                        name: pair_row.defense_contact_binding.clone(),
                    });
                }
            };
            if !architecture.operations.iter().any(
                |operation| matches!(operation, ReferenceArchitectureOperation::PlaceWire(wire) if wire.id == defense_local_id),
            ) {
                return Err(ReferenceMetricError::ArchitectureBindingTargetMismatch {
                    role,
                    name: pair_row.defense_contact_binding.clone(),
                });
            }
        }
    }
    Ok(())
}

fn add_planned_work(
    total: &mut u128,
    target: ConstructionTarget,
    probe: &ConstructionProbeProfile,
) -> Result<(), ReferenceMetricError> {
    let work = required_construction_work(&target, probe)
        .map_err(ReferenceMetricError::ConstructionWork)?;
    *total = total
        .checked_add(u128::from(work.0))
        .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
    Ok(())
}

fn role_wire_class(name: &str) -> Option<WireMetricClass> {
    match name.split('.').next().unwrap_or_default() {
        "shared" => Some(WireMetricClass::Shared),
        "sensor" => Some(WireMetricClass::Sensor),
        "trunk" => Some(WireMetricClass::Trunk),
        "defense" => Some(WireMetricClass::Defense),
        _ => None,
    }
}

fn semantic_target_wire(
    target: ReferenceArchitectureSemanticTarget,
) -> Option<ReferenceArchitectureLocalId> {
    match target {
        ReferenceArchitectureSemanticTarget::LocalEntity(id)
        | ReferenceArchitectureSemanticTarget::WireSensePort { wire: id, .. } => Some(id),
        ReferenceArchitectureSemanticTarget::GatePort { .. }
        | ReferenceArchitectureSemanticTarget::MobilePort { .. }
        | ReferenceArchitectureSemanticTarget::MainCore
        | ReferenceArchitectureSemanticTarget::PowerSource { .. }
        | ReferenceArchitectureSemanticTarget::Enemy { .. } => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceMetricBoundaries {
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
}

impl ReferenceMetricBoundaries {
    pub fn validate(self) -> Result<(), ReferenceMetricError> {
        if self.build_end_tick > self.measurement_start_tick
            || self.measurement_start_tick >= self.max_ticks
        {
            return Err(ReferenceMetricError::InvalidBoundaries {
                build_end_tick: self.build_end_tick,
                measurement_start_tick: self.measurement_start_tick,
                max_ticks: self.max_ticks,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceMetricResultBoundaries {
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub final_next_tick: Tick,
    pub max_ticks: Tick,
}

impl ReferenceMetricResultBoundaries {
    fn validate(self) -> Result<(), ReferenceMetricError> {
        ReferenceMetricBoundaries {
            build_end_tick: self.build_end_tick,
            measurement_start_tick: self.measurement_start_tick,
            max_ticks: self.max_ticks,
        }
        .validate()?;
        if self.final_next_tick <= self.measurement_start_tick
            || self.final_next_tick > self.max_ticks
        {
            return Err(ReferenceMetricError::InvalidFinalBoundary {
                measurement_start_tick: self.measurement_start_tick,
                final_next_tick: self.final_next_tick,
                max_ticks: self.max_ticks,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceMetricTickSample {
    pub next_tick: Tick,
    pub state_hash: StateHash,
    pub run_status: RunStatus,
    pub core_integrity: Integrity,
}

impl ReferenceMetricTickSample {
    pub fn from_snapshot(snapshot: &RenderSnapshot) -> Result<Self, ReferenceMetricError> {
        let core = snapshot
            .main_core()
            .ok_or(ReferenceMetricError::MissingMainCore)?;
        Ok(Self {
            next_tick: snapshot.next_tick(),
            state_hash: snapshot.state_hash(),
            run_status: snapshot.run_status(),
            core_integrity: core.integrity,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceTerminalStatus {
    Running,
    Ended {
        completed_tick: Tick,
        cause: RunEndCause,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceResponseLatency {
    pub name: String,
    pub stimulus_tick: Tick,
    pub response_tick: Tick,
    pub latency_ticks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRuntimeMetrics {
    pub survived_boundary: bool,
    pub completed_ticks: u64,
    pub terminal_status: ReferenceTerminalStatus,
    pub measurement_start_core_integrity: Integrity,
    pub final_core_integrity: Integrity,
    pub core_damage: Integrity,
    pub power_generation: u128,
    pub power_nominal_demand: u128,
    pub power_granted: u128,
    pub power_source_cost: u128,
    pub power_transmission_loss: u128,
    pub brownout_ticks: u64,
    pub construction_requested: u128,
    pub construction_nominal_power: u128,
    pub construction_granted_work: u128,
    pub construction_applied_work: u128,
    pub heat_generated: u128,
    pub network_peak_used_ncu: Capacity,
    pub network_final_used_ncu: Capacity,
    pub network_integral_used_ncu: u128,
    pub support_demand_integral: u128,
    pub enemy_kills: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceMetricResult {
    pub boundaries: ReferenceMetricResultBoundaries,
    pub static_inventory: ReferenceStaticInventory,
    pub runtime_metrics: ReferenceRuntimeMetrics,
    pub response_latency_ticks: Vec<ReferenceResponseLatency>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceMetricArtifact {
    pub format_version: u32,
    pub hash_algorithm_id: HashAlgorithmId,
    pub metric_set_id: String,
    pub metric_set_hash: ArtifactHash,
    pub run_id: ExperimentRunId,
    pub result: ReferenceMetricResult,
}

impl ReferenceMetricArtifact {
    pub fn v1(
        definition: &ReferenceMetricSetArtifact,
        run_id: ExperimentRunId,
        result: ReferenceMetricResult,
    ) -> Result<Self, ReferenceMetricError> {
        validate_metric_result(&result)?;
        Ok(Self {
            format_version: REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            metric_set_id: definition.metric_set_id.clone(),
            metric_set_hash: definition.semantic_hash()?,
            run_id,
            result,
        })
    }

    pub fn validate_against(
        &self,
        definition: &ReferenceMetricSetArtifact,
    ) -> Result<(), ReferenceMetricError> {
        if self.format_version != REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1 {
            return Err(ReferenceMetricError::UnsupportedFormatVersion {
                expected: REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1,
                actual: self.format_version,
            });
        }
        if self.hash_algorithm_id != HashAlgorithmId::Blake3V1 {
            return Err(ReferenceMetricError::UnsupportedHashAlgorithm {
                expected: HASH_ALGORITHM_ID_BLAKE3_V1,
                actual: self.hash_algorithm_id.as_str().to_owned(),
            });
        }
        definition.validate()?;
        if self.metric_set_id != definition.metric_set_id {
            return Err(ReferenceMetricError::MetricSetIdMismatch {
                expected: REFERENCE_METRIC_SET_ID_V1,
                actual: self.metric_set_id.clone(),
            });
        }
        let expected = definition.semantic_hash()?;
        if self.metric_set_hash != expected {
            return Err(ReferenceMetricError::MetricSetHashMismatch {
                expected,
                actual: self.metric_set_hash,
            });
        }
        validate_metric_result(&self.result)?;
        let result_names = self
            .result
            .response_latency_ticks
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>();
        let definition_names = definition
            .response_observations
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>();
        if result_names != definition_names {
            return Err(ReferenceMetricError::ResponseDefinitionMismatch);
        }
        Ok(())
    }

    pub fn semantic_hash(
        &self,
        definition: &ReferenceMetricSetArtifact,
    ) -> Result<ArtifactHash, ReferenceMetricError> {
        self.validate_against(definition)?;
        let mut encoder = MetricEncoder::default();
        encoder.bytes(REFERENCE_METRIC_ARTIFACT_HASH_DOMAIN);
        encoder.u16(REFERENCE_METRIC_ARTIFACT_CANONICAL_ENCODER_VERSION);
        encoder.u32(self.format_version);
        encoder.text(&self.metric_set_id)?;
        encoder.bytes(self.run_id.as_bytes());
        encode_result(&self.result, &mut encoder)?;
        Ok(ArtifactHash::from_bytes(
            *blake3::hash(&encoder.finish()).as_bytes(),
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceMetricArtifactWire {
    format_version: u32,
    hash_algorithm_id: String,
    metric_set_id: String,
    metric_set_hash: String,
    run_id: String,
    boundaries: ReferenceMetricResultBoundariesWire,
    static_inventory: ReferenceStaticInventoryWire,
    runtime_metrics: ReferenceRuntimeMetricsWire,
    response_latency_ticks: Vec<ReferenceResponseLatencyWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceMetricResultBoundariesWire {
    build_end_tick: u64,
    measurement_start_tick: u64,
    final_next_tick: u64,
    max_ticks: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceStaticInventoryWire {
    total_wire_length_raw: i64,
    total_wire_ncu: u64,
    shared_wire_length_raw: i64,
    sensor_wire_length_raw: i64,
    trunk_wire_length_raw: i64,
    defense_wire_length_raw: i64,
    other_wire_length_raw: i64,
    gate_count: u64,
    and_count: u64,
    or_count: u64,
    not_count: u64,
    planned_construction_work: String,
    build_command_count: u64,
    command_log_hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceRuntimeMetricsWire {
    survived_boundary: bool,
    completed_ticks: u64,
    terminal_status: ReferenceTerminalStatusWire,
    measurement_start_core_integrity: u64,
    final_core_integrity: u64,
    core_damage: u64,
    power_generation: String,
    power_nominal_demand: String,
    power_granted: String,
    power_source_cost: String,
    power_transmission_loss: String,
    brownout_ticks: u64,
    construction_requested: String,
    construction_nominal_power: String,
    construction_granted_work: String,
    construction_applied_work: String,
    heat_generated: String,
    network_peak_used_ncu: u64,
    network_final_used_ncu: u64,
    network_integral_used_ncu: String,
    support_demand_integral: String,
    enemy_kills: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum ReferenceTerminalStatusWire {
    Running,
    Ended {
        #[serde(rename = "completedTick")]
        completed_tick: u64,
        cause: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceResponseLatencyWire {
    name: String,
    stimulus_tick: u64,
    response_tick: u64,
    latency_ticks: u64,
}

pub fn decode_reference_metric_artifact(
    bytes: &[u8],
    definition: &ReferenceMetricSetArtifact,
) -> Result<ReferenceMetricArtifact, ReferenceMetricError> {
    let envelope: ReferenceMetricFormatEnvelope = decode_metric_json(bytes)?;
    if envelope.format_version != REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1 {
        return Err(ReferenceMetricError::UnsupportedFormatVersion {
            expected: REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1,
            actual: envelope.format_version,
        });
    }
    let hash_envelope: ReferenceMetricHashEnvelope = decode_metric_json(bytes)?;
    if hash_envelope.hash_algorithm_id != HASH_ALGORITHM_ID_BLAKE3_V1 {
        return Err(ReferenceMetricError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: hash_envelope.hash_algorithm_id,
        });
    }
    let wire: ReferenceMetricArtifactWire = decode_metric_json(bytes)?;
    let hash_algorithm_id = HashAlgorithmId::parse(&wire.hash_algorithm_id).map_err(|_| {
        ReferenceMetricError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: wire.hash_algorithm_id.clone(),
        }
    })?;
    let metric_set_hash = ArtifactHash::from_hex(&wire.metric_set_hash).map_err(|error| {
        ReferenceMetricError::InvalidHash {
            field: "metricSetHash",
            error,
        }
    })?;
    let run_id = ExperimentRunId::from_hex(&wire.run_id).map_err(|error| {
        ReferenceMetricError::InvalidHash {
            field: "runId",
            error,
        }
    })?;
    let metric_set_id = wire.metric_set_id.clone();
    let result = decode_result(&wire)?;
    let artifact = ReferenceMetricArtifact {
        format_version: REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1,
        hash_algorithm_id,
        metric_set_id,
        metric_set_hash,
        run_id,
        result,
    };
    artifact.validate_against(definition)?;
    Ok(artifact)
}

pub fn encode_reference_metric_artifact(
    artifact: &ReferenceMetricArtifact,
    definition: &ReferenceMetricSetArtifact,
) -> Result<Vec<u8>, ReferenceMetricError> {
    artifact.validate_against(definition)?;
    let wire = encode_artifact_wire(artifact);
    let mut encoded =
        serde_json::to_vec_pretty(&wire).map_err(|error| ReferenceMetricError::EncodeJson {
            message: error.to_string(),
        })?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn decode_result(
    wire: &ReferenceMetricArtifactWire,
) -> Result<ReferenceMetricResult, ReferenceMetricError> {
    let terminal_status = match &wire.runtime_metrics.terminal_status {
        ReferenceTerminalStatusWire::Running => ReferenceTerminalStatus::Running,
        ReferenceTerminalStatusWire::Ended {
            completed_tick,
            cause,
        } => {
            if cause != "main-core-destroyed" {
                return Err(ReferenceMetricError::InvalidTerminalCause {
                    actual: cause.clone(),
                });
            }
            ReferenceTerminalStatus::Ended {
                completed_tick: Tick(*completed_tick),
                cause: RunEndCause::MainCoreDestroyed,
            }
        }
    };
    let static_inventory = ReferenceStaticInventory {
        total_wire_length_raw: wire.static_inventory.total_wire_length_raw,
        total_wire_ncu: Capacity(wire.static_inventory.total_wire_ncu),
        shared_wire_length_raw: wire.static_inventory.shared_wire_length_raw,
        sensor_wire_length_raw: wire.static_inventory.sensor_wire_length_raw,
        trunk_wire_length_raw: wire.static_inventory.trunk_wire_length_raw,
        defense_wire_length_raw: wire.static_inventory.defense_wire_length_raw,
        other_wire_length_raw: wire.static_inventory.other_wire_length_raw,
        gate_count: wire.static_inventory.gate_count,
        and_count: wire.static_inventory.and_count,
        or_count: wire.static_inventory.or_count,
        not_count: wire.static_inventory.not_count,
        planned_construction_work: parse_decimal(
            "staticInventory.plannedConstructionWork",
            &wire.static_inventory.planned_construction_work,
        )?,
        build_command_count: wire.static_inventory.build_command_count,
        command_log_hash: ArtifactHash::from_hex(&wire.static_inventory.command_log_hash).map_err(
            |error| ReferenceMetricError::InvalidHash {
                field: "staticInventory.commandLogHash",
                error,
            },
        )?,
    };
    let runtime_metrics = ReferenceRuntimeMetrics {
        survived_boundary: wire.runtime_metrics.survived_boundary,
        completed_ticks: wire.runtime_metrics.completed_ticks,
        terminal_status,
        measurement_start_core_integrity: Integrity(
            wire.runtime_metrics.measurement_start_core_integrity,
        ),
        final_core_integrity: Integrity(wire.runtime_metrics.final_core_integrity),
        core_damage: Integrity(wire.runtime_metrics.core_damage),
        power_generation: parse_decimal(
            "runtimeMetrics.powerGeneration",
            &wire.runtime_metrics.power_generation,
        )?,
        power_nominal_demand: parse_decimal(
            "runtimeMetrics.powerNominalDemand",
            &wire.runtime_metrics.power_nominal_demand,
        )?,
        power_granted: parse_decimal(
            "runtimeMetrics.powerGranted",
            &wire.runtime_metrics.power_granted,
        )?,
        power_source_cost: parse_decimal(
            "runtimeMetrics.powerSourceCost",
            &wire.runtime_metrics.power_source_cost,
        )?,
        power_transmission_loss: parse_decimal(
            "runtimeMetrics.powerTransmissionLoss",
            &wire.runtime_metrics.power_transmission_loss,
        )?,
        brownout_ticks: wire.runtime_metrics.brownout_ticks,
        construction_requested: parse_decimal(
            "runtimeMetrics.constructionRequested",
            &wire.runtime_metrics.construction_requested,
        )?,
        construction_nominal_power: parse_decimal(
            "runtimeMetrics.constructionNominalPower",
            &wire.runtime_metrics.construction_nominal_power,
        )?,
        construction_granted_work: parse_decimal(
            "runtimeMetrics.constructionGrantedWork",
            &wire.runtime_metrics.construction_granted_work,
        )?,
        construction_applied_work: parse_decimal(
            "runtimeMetrics.constructionAppliedWork",
            &wire.runtime_metrics.construction_applied_work,
        )?,
        heat_generated: parse_decimal(
            "runtimeMetrics.heatGenerated",
            &wire.runtime_metrics.heat_generated,
        )?,
        network_peak_used_ncu: Capacity(wire.runtime_metrics.network_peak_used_ncu),
        network_final_used_ncu: Capacity(wire.runtime_metrics.network_final_used_ncu),
        network_integral_used_ncu: parse_decimal(
            "runtimeMetrics.networkIntegralUsedNcu",
            &wire.runtime_metrics.network_integral_used_ncu,
        )?,
        support_demand_integral: parse_decimal(
            "runtimeMetrics.supportDemandIntegral",
            &wire.runtime_metrics.support_demand_integral,
        )?,
        enemy_kills: wire.runtime_metrics.enemy_kills,
    };
    let result = ReferenceMetricResult {
        boundaries: ReferenceMetricResultBoundaries {
            build_end_tick: Tick(wire.boundaries.build_end_tick),
            measurement_start_tick: Tick(wire.boundaries.measurement_start_tick),
            final_next_tick: Tick(wire.boundaries.final_next_tick),
            max_ticks: Tick(wire.boundaries.max_ticks),
        },
        static_inventory,
        runtime_metrics,
        response_latency_ticks: wire
            .response_latency_ticks
            .iter()
            .map(|row| ReferenceResponseLatency {
                name: row.name.clone(),
                stimulus_tick: Tick(row.stimulus_tick),
                response_tick: Tick(row.response_tick),
                latency_ticks: row.latency_ticks,
            })
            .collect(),
    };
    validate_metric_result(&result)?;
    Ok(result)
}

fn encode_artifact_wire(artifact: &ReferenceMetricArtifact) -> ReferenceMetricArtifactWire {
    let result = &artifact.result;
    let terminal_status = match result.runtime_metrics.terminal_status {
        ReferenceTerminalStatus::Running => ReferenceTerminalStatusWire::Running,
        ReferenceTerminalStatus::Ended {
            completed_tick,
            cause: RunEndCause::MainCoreDestroyed,
        } => ReferenceTerminalStatusWire::Ended {
            completed_tick: completed_tick.0,
            cause: "main-core-destroyed".to_owned(),
        },
    };
    ReferenceMetricArtifactWire {
        format_version: artifact.format_version,
        hash_algorithm_id: artifact.hash_algorithm_id.as_str().to_owned(),
        metric_set_id: artifact.metric_set_id.clone(),
        metric_set_hash: artifact.metric_set_hash.to_string(),
        run_id: artifact.run_id.to_string(),
        boundaries: ReferenceMetricResultBoundariesWire {
            build_end_tick: result.boundaries.build_end_tick.0,
            measurement_start_tick: result.boundaries.measurement_start_tick.0,
            final_next_tick: result.boundaries.final_next_tick.0,
            max_ticks: result.boundaries.max_ticks.0,
        },
        static_inventory: ReferenceStaticInventoryWire {
            total_wire_length_raw: result.static_inventory.total_wire_length_raw,
            total_wire_ncu: result.static_inventory.total_wire_ncu.0,
            shared_wire_length_raw: result.static_inventory.shared_wire_length_raw,
            sensor_wire_length_raw: result.static_inventory.sensor_wire_length_raw,
            trunk_wire_length_raw: result.static_inventory.trunk_wire_length_raw,
            defense_wire_length_raw: result.static_inventory.defense_wire_length_raw,
            other_wire_length_raw: result.static_inventory.other_wire_length_raw,
            gate_count: result.static_inventory.gate_count,
            and_count: result.static_inventory.and_count,
            or_count: result.static_inventory.or_count,
            not_count: result.static_inventory.not_count,
            planned_construction_work: result
                .static_inventory
                .planned_construction_work
                .to_string(),
            build_command_count: result.static_inventory.build_command_count,
            command_log_hash: result.static_inventory.command_log_hash.to_string(),
        },
        runtime_metrics: ReferenceRuntimeMetricsWire {
            survived_boundary: result.runtime_metrics.survived_boundary,
            completed_ticks: result.runtime_metrics.completed_ticks,
            terminal_status,
            measurement_start_core_integrity: result
                .runtime_metrics
                .measurement_start_core_integrity
                .0,
            final_core_integrity: result.runtime_metrics.final_core_integrity.0,
            core_damage: result.runtime_metrics.core_damage.0,
            power_generation: result.runtime_metrics.power_generation.to_string(),
            power_nominal_demand: result.runtime_metrics.power_nominal_demand.to_string(),
            power_granted: result.runtime_metrics.power_granted.to_string(),
            power_source_cost: result.runtime_metrics.power_source_cost.to_string(),
            power_transmission_loss: result.runtime_metrics.power_transmission_loss.to_string(),
            brownout_ticks: result.runtime_metrics.brownout_ticks,
            construction_requested: result.runtime_metrics.construction_requested.to_string(),
            construction_nominal_power: result
                .runtime_metrics
                .construction_nominal_power
                .to_string(),
            construction_granted_work: result.runtime_metrics.construction_granted_work.to_string(),
            construction_applied_work: result.runtime_metrics.construction_applied_work.to_string(),
            heat_generated: result.runtime_metrics.heat_generated.to_string(),
            network_peak_used_ncu: result.runtime_metrics.network_peak_used_ncu.0,
            network_final_used_ncu: result.runtime_metrics.network_final_used_ncu.0,
            network_integral_used_ncu: result.runtime_metrics.network_integral_used_ncu.to_string(),
            support_demand_integral: result.runtime_metrics.support_demand_integral.to_string(),
            enemy_kills: result.runtime_metrics.enemy_kills,
        },
        response_latency_ticks: result
            .response_latency_ticks
            .iter()
            .map(|row| ReferenceResponseLatencyWire {
                name: row.name.clone(),
                stimulus_tick: row.stimulus_tick.0,
                response_tick: row.response_tick.0,
                latency_ticks: row.latency_ticks,
            })
            .collect(),
    }
}

fn validate_metric_result(result: &ReferenceMetricResult) -> Result<(), ReferenceMetricError> {
    result.boundaries.validate()?;
    result.static_inventory.validate()?;
    let runtime = &result.runtime_metrics;
    if runtime.completed_ticks != result.boundaries.final_next_tick.0
        || runtime.network_peak_used_ncu < runtime.network_final_used_ncu
    {
        return Err(ReferenceMetricError::TerminalBoundaryMismatch);
    }
    match runtime.terminal_status {
        ReferenceTerminalStatus::Running => {
            if !runtime.survived_boundary
                || result.boundaries.final_next_tick != result.boundaries.max_ticks
            {
                return Err(ReferenceMetricError::TerminalBoundaryMismatch);
            }
        }
        ReferenceTerminalStatus::Ended {
            completed_tick,
            cause: RunEndCause::MainCoreDestroyed,
        } => {
            if runtime.survived_boundary
                || completed_tick.0.checked_add(1).map(Tick)
                    != Some(result.boundaries.final_next_tick)
                || runtime.final_core_integrity != Integrity(0)
            {
                return Err(ReferenceMetricError::TerminalBoundaryMismatch);
            }
        }
    }
    let expected_damage = runtime
        .measurement_start_core_integrity
        .0
        .checked_sub(runtime.final_core_integrity.0)
        .map(Integrity)
        .ok_or(ReferenceMetricError::CoreIntegrityIncreased {
            initial: runtime.measurement_start_core_integrity,
            final_integrity: runtime.final_core_integrity,
        })?;
    if runtime.core_damage != expected_damage {
        return Err(ReferenceMetricError::CoreDamageMismatch {
            measurement_start: runtime.measurement_start_core_integrity,
            final_integrity: runtime.final_core_integrity,
            expected: expected_damage,
            actual: runtime.core_damage,
        });
    }
    if result.response_latency_ticks.is_empty() {
        return Err(ReferenceMetricError::EmptyResponseObservations);
    }
    let mut previous_name: Option<&str> = None;
    for row in &result.response_latency_ticks {
        if row.name.is_empty() {
            return Err(ReferenceMetricError::EmptyText {
                field: "responseLatencyTicks[].name",
            });
        }
        canonical_text(&row.name)?;
        if previous_name.is_some_and(|name| name.as_bytes() >= row.name.as_bytes()) {
            return Err(ReferenceMetricError::NonCanonicalResponseOrder);
        }
        if row.stimulus_tick < result.boundaries.measurement_start_tick
            || row.response_tick < row.stimulus_tick
            || row.response_tick >= result.boundaries.final_next_tick
            || row.response_tick.0.checked_sub(row.stimulus_tick.0) != Some(row.latency_ticks)
        {
            return Err(ReferenceMetricError::ObservationOrderViolation {
                name: row.name.clone(),
            });
        }
        previous_name = Some(&row.name);
    }
    Ok(())
}

fn encode_result(
    result: &ReferenceMetricResult,
    encoder: &mut MetricEncoder,
) -> Result<(), ReferenceMetricError> {
    validate_metric_result(result)?;
    let boundaries = result.boundaries;
    encoder.u64(boundaries.build_end_tick.0);
    encoder.u64(boundaries.measurement_start_tick.0);
    encoder.u64(boundaries.final_next_tick.0);
    encoder.u64(boundaries.max_ticks.0);
    let inventory = &result.static_inventory;
    for value in [
        inventory.total_wire_length_raw,
        inventory.shared_wire_length_raw,
        inventory.sensor_wire_length_raw,
        inventory.trunk_wire_length_raw,
        inventory.defense_wire_length_raw,
        inventory.other_wire_length_raw,
    ] {
        encoder.i64(value);
    }
    encoder.u64(inventory.total_wire_ncu.0);
    for value in [
        inventory.gate_count,
        inventory.and_count,
        inventory.or_count,
        inventory.not_count,
    ] {
        encoder.u64(value);
    }
    encoder.u128(inventory.planned_construction_work);
    encoder.u64(inventory.build_command_count);
    encoder.bytes(inventory.command_log_hash.as_bytes());
    let runtime = &result.runtime_metrics;
    encoder.u8(u8::from(runtime.survived_boundary));
    encoder.u64(runtime.completed_ticks);
    match runtime.terminal_status {
        ReferenceTerminalStatus::Running => encoder.u8(0),
        ReferenceTerminalStatus::Ended {
            completed_tick,
            cause: RunEndCause::MainCoreDestroyed,
        } => {
            encoder.u8(1);
            encoder.u64(completed_tick.0);
            encoder.u8(0);
        }
    }
    encoder.u64(runtime.measurement_start_core_integrity.0);
    encoder.u64(runtime.final_core_integrity.0);
    encoder.u64(runtime.core_damage.0);
    for value in [
        runtime.power_generation,
        runtime.power_nominal_demand,
        runtime.power_granted,
        runtime.power_source_cost,
        runtime.power_transmission_loss,
    ] {
        encoder.u128(value);
    }
    encoder.u64(runtime.brownout_ticks);
    for value in [
        runtime.construction_requested,
        runtime.construction_nominal_power,
        runtime.construction_granted_work,
        runtime.construction_applied_work,
        runtime.heat_generated,
    ] {
        encoder.u128(value);
    }
    encoder.u64(runtime.network_peak_used_ncu.0);
    encoder.u64(runtime.network_final_used_ncu.0);
    encoder.u128(runtime.network_integral_used_ncu);
    encoder.u128(runtime.support_demand_integral);
    encoder.u64(runtime.enemy_kills);
    encoder.count("responseLatencyTicks", result.response_latency_ticks.len())?;
    for row in &result.response_latency_ticks {
        encoder.text(&row.name)?;
        encoder.u64(row.stimulus_tick.0);
        encoder.u64(row.response_tick.0);
        encoder.u64(row.latency_ticks);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct ResponseProgress {
    binding: ResolvedReferenceResponseObservation,
    stimulus_tick: Option<Tick>,
    response_tick: Option<Tick>,
}

/// Pure, derived, probe-neutral reduction of complete, contiguous StepReports.
#[derive(Clone, Debug)]
pub struct ReferenceMetricCollector {
    boundaries: ReferenceMetricBoundaries,
    static_inventory: ReferenceStaticInventory,
    bound_enemies: BTreeSet<EnemyId>,
    expected_next_tick: Tick,
    final_sample: ReferenceMetricTickSample,
    measurement_start_core_integrity: Option<Integrity>,
    power_generation: u128,
    power_nominal_demand: u128,
    power_granted: u128,
    power_source_cost: u128,
    power_transmission_loss: u128,
    brownout_ticks: u64,
    construction_requested: u128,
    construction_nominal_power: u128,
    construction_granted_work: u128,
    construction_applied_work: u128,
    heat_generated: u128,
    network_peak_used_ncu: Option<Capacity>,
    network_final_used_ncu: Option<Capacity>,
    network_integral_used_ncu: u128,
    support_demand_integral: u128,
    killed_enemies: BTreeSet<EnemyId>,
    response: Vec<ResponseProgress>,
}

impl ReferenceMetricCollector {
    pub fn new(
        boundaries: ReferenceMetricBoundaries,
        static_inventory: ReferenceStaticInventory,
        initial_sample: ReferenceMetricTickSample,
        bound_enemies: Vec<EnemyId>,
        mut response: Vec<ResolvedReferenceResponseObservation>,
    ) -> Result<Self, ReferenceMetricError> {
        boundaries.validate()?;
        static_inventory.validate()?;
        if initial_sample.next_tick != Tick(0) {
            return Err(ReferenceMetricError::InvalidInitialTick {
                actual: initial_sample.next_tick,
            });
        }
        if initial_sample.run_status != RunStatus::Running {
            return Err(ReferenceMetricError::InvalidInitialRunStatus);
        }
        let bound_enemies = unique_set("boundEnemies", bound_enemies)?;
        if bound_enemies.is_empty() {
            return Err(ReferenceMetricError::EmptyBoundEnemies);
        }
        response.sort_unstable_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        validate_resolved_responses(&response, &bound_enemies)?;
        let measurement_start_core_integrity =
            (boundaries.measurement_start_tick == Tick(0)).then_some(initial_sample.core_integrity);
        Ok(Self {
            boundaries,
            static_inventory,
            bound_enemies,
            expected_next_tick: Tick(0),
            final_sample: initial_sample,
            measurement_start_core_integrity,
            power_generation: 0,
            power_nominal_demand: 0,
            power_granted: 0,
            power_source_cost: 0,
            power_transmission_loss: 0,
            brownout_ticks: 0,
            construction_requested: 0,
            construction_nominal_power: 0,
            construction_granted_work: 0,
            construction_applied_work: 0,
            heat_generated: 0,
            network_peak_used_ncu: None,
            network_final_used_ncu: None,
            network_integral_used_ncu: 0,
            support_demand_integral: 0,
            killed_enemies: BTreeSet::new(),
            response: response
                .into_iter()
                .map(|binding| ResponseProgress {
                    binding,
                    stimulus_tick: None,
                    response_tick: None,
                })
                .collect(),
        })
    }

    pub fn observe_completed_tick(
        &mut self,
        report: &StepReport,
        snapshot: &RenderSnapshot,
    ) -> Result<(), ReferenceMetricError> {
        self.observe_sample(report, ReferenceMetricTickSample::from_snapshot(snapshot)?)
    }

    pub fn observe_sample(
        &mut self,
        report: &StepReport,
        sample: ReferenceMetricTickSample,
    ) -> Result<(), ReferenceMetricError> {
        self.validate_tick(report, sample)?;
        let mut candidate = self.clone();
        candidate.observe_validated_sample(report, sample)?;
        *self = candidate;
        Ok(())
    }

    fn observe_validated_sample(
        &mut self,
        report: &StepReport,
        sample: ReferenceMetricTickSample,
    ) -> Result<(), ReferenceMetricError> {
        if sample.next_tick == self.boundaries.measurement_start_tick {
            self.measurement_start_core_integrity = Some(sample.core_integrity);
        }
        if report.completed_tick >= self.boundaries.measurement_start_tick {
            self.reduce_window_tick(report)?;
        }
        self.expected_next_tick = report.next_tick;
        self.final_sample = sample;
        Ok(())
    }

    pub fn finish(self) -> Result<ReferenceMetricResult, ReferenceMetricError> {
        let terminal_status = match self.final_sample.run_status {
            RunStatus::Running if self.expected_next_tick == self.boundaries.max_ticks => {
                ReferenceTerminalStatus::Running
            }
            RunStatus::Running => {
                return Err(ReferenceMetricError::IncompleteRun {
                    completed_ticks: self.expected_next_tick.0,
                    max_ticks: self.boundaries.max_ticks.0,
                });
            }
            RunStatus::Ended {
                completed_tick,
                cause,
            } => ReferenceTerminalStatus::Ended {
                completed_tick,
                cause,
            },
        };
        let result_boundaries = ReferenceMetricResultBoundaries {
            build_end_tick: self.boundaries.build_end_tick,
            measurement_start_tick: self.boundaries.measurement_start_tick,
            final_next_tick: self.expected_next_tick,
            max_ticks: self.boundaries.max_ticks,
        };
        result_boundaries.validate()?;
        let initial_core_integrity = self
            .measurement_start_core_integrity
            .ok_or(ReferenceMetricError::MissingMeasurementStartSample)?;
        let core_damage = initial_core_integrity
            .0
            .checked_sub(self.final_sample.core_integrity.0)
            .ok_or(ReferenceMetricError::CoreIntegrityIncreased {
                initial: initial_core_integrity,
                final_integrity: self.final_sample.core_integrity,
            })?;
        let network_peak_used_ncu = self
            .network_peak_used_ncu
            .ok_or(ReferenceMetricError::EmptyMeasurementWindow)?;
        let network_final_used_ncu = self
            .network_final_used_ncu
            .ok_or(ReferenceMetricError::EmptyMeasurementWindow)?;
        let mut response_latency_ticks = Vec::with_capacity(self.response.len());
        for progress in self.response {
            let stimulus_tick = progress.stimulus_tick.ok_or_else(|| {
                ReferenceMetricError::ObservationNotReached {
                    name: progress.binding.name.clone(),
                    phase: ReferenceObservationPhase::Stimulus,
                }
            })?;
            let response_tick = progress.response_tick.ok_or_else(|| {
                ReferenceMetricError::ObservationNotReached {
                    name: progress.binding.name.clone(),
                    phase: ReferenceObservationPhase::Response,
                }
            })?;
            let latency_ticks = response_tick
                .0
                .checked_sub(stimulus_tick.0)
                .ok_or_else(|| ReferenceMetricError::ObservationOrderViolation {
                    name: progress.binding.name.clone(),
                })?;
            response_latency_ticks.push(ReferenceResponseLatency {
                name: progress.binding.name,
                stimulus_tick,
                response_tick,
                latency_ticks,
            });
        }
        let result = ReferenceMetricResult {
            boundaries: result_boundaries,
            static_inventory: self.static_inventory,
            runtime_metrics: ReferenceRuntimeMetrics {
                survived_boundary: matches!(terminal_status, ReferenceTerminalStatus::Running),
                completed_ticks: self.expected_next_tick.0,
                terminal_status,
                measurement_start_core_integrity: initial_core_integrity,
                final_core_integrity: self.final_sample.core_integrity,
                core_damage: Integrity(core_damage),
                power_generation: self.power_generation,
                power_nominal_demand: self.power_nominal_demand,
                power_granted: self.power_granted,
                power_source_cost: self.power_source_cost,
                power_transmission_loss: self.power_transmission_loss,
                brownout_ticks: self.brownout_ticks,
                construction_requested: self.construction_requested,
                construction_nominal_power: self.construction_nominal_power,
                construction_granted_work: self.construction_granted_work,
                construction_applied_work: self.construction_applied_work,
                heat_generated: self.heat_generated,
                network_peak_used_ncu,
                network_final_used_ncu,
                network_integral_used_ncu: self.network_integral_used_ncu,
                support_demand_integral: self.support_demand_integral,
                enemy_kills: u64::try_from(self.killed_enemies.len()).map_err(|_| {
                    ReferenceMetricError::ResultOutOfRange {
                        field: "enemyKills",
                    }
                })?,
            },
            response_latency_ticks,
        };
        validate_metric_result(&result)?;
        Ok(result)
    }

    fn validate_tick(
        &self,
        report: &StepReport,
        sample: ReferenceMetricTickSample,
    ) -> Result<(), ReferenceMetricError> {
        if matches!(self.final_sample.run_status, RunStatus::Ended { .. })
            || self.expected_next_tick >= self.boundaries.max_ticks
        {
            return Err(ReferenceMetricError::RunAlreadyComplete);
        }
        if report.completed_tick != self.expected_next_tick {
            return Err(ReferenceMetricError::NonContiguousTick {
                expected: self.expected_next_tick,
                actual: report.completed_tick,
            });
        }
        let expected_next = self
            .expected_next_tick
            .0
            .checked_add(1)
            .map(Tick)
            .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
        if report.next_tick != expected_next {
            return Err(ReferenceMetricError::ReportNextTickMismatch {
                expected: expected_next,
                actual: report.next_tick,
            });
        }
        if report.next_tick > self.boundaries.max_ticks {
            return Err(ReferenceMetricError::RunPastMaxTicks {
                max_ticks: self.boundaries.max_ticks,
                actual: report.next_tick,
            });
        }
        if sample.next_tick != report.next_tick
            || sample.state_hash != report.state_hash
            || sample.run_status != report.run_status
        {
            return Err(ReferenceMetricError::SnapshotReportMismatch);
        }
        Ok(())
    }

    fn reduce_window_tick(&mut self, report: &StepReport) -> Result<(), ReferenceMetricError> {
        let power = report
            .power
            .as_ref()
            .ok_or(ReferenceMetricError::MissingPowerReport {
                tick: report.completed_tick,
            })?;
        validate_power_report_order(power, report.completed_tick)?;
        validate_step_row_order(report)?;
        let accounting =
            report
                .network_accounting
                .ok_or(ReferenceMetricError::MissingNetworkAccounting {
                    tick: report.completed_tick,
                })?;
        let support = accounting.total_support_demand().ok_or(
            ReferenceMetricError::MissingSupportDemand {
                tick: report.completed_tick,
            },
        )?;
        let mut observations = Vec::with_capacity(self.response.len());
        for progress in &self.response {
            let sense = power
                .sense
                .binary_search_by_key(
                    &(progress.binding.sensor_wire, progress.binding.sensor_end),
                    |row| (row.wire, row.end),
                )
                .ok()
                .map(|index| power.sense[index])
                .ok_or_else(|| ReferenceMetricError::MissingSensorObservation {
                    name: progress.binding.name.clone(),
                    tick: report.completed_tick,
                })?;
            let contact = report.contacts.iter().any(|row| {
                row.wire == progress.binding.defense_wire
                    && row.target == progress.binding.enemy
                    && row.absorbed.0 > 0
            });
            if progress.stimulus_tick.is_none() && !sense.sampled_presence && contact {
                return Err(ReferenceMetricError::ObservationOrderViolation {
                    name: progress.binding.name.clone(),
                });
            }
            observations.push((sense.sampled_presence, contact));
        }
        for region in &power.regions {
            add_u64(&mut self.power_generation, region.generation.0)?;
        }
        let mut brownout = false;
        for load in &power.loads {
            add_u64(&mut self.power_nominal_demand, load.nominal.0)?;
            add_u64(&mut self.power_granted, load.granted.0)?;
            add_u64(&mut self.power_source_cost, load.source_cost.0)?;
            add_u64(&mut self.power_transmission_loss, load.transmission_loss.0)?;
            brownout |= load.nominal.0 > 0 && load.ratio < crate::PowerRatio::ONE;
        }
        if brownout {
            self.brownout_ticks = self
                .brownout_ticks
                .checked_add(1)
                .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
        }
        for heat in &power.heat_contributions {
            add_u64(&mut self.heat_generated, heat.energy.0)?;
        }
        for work in &report.construction_work {
            add_u64(&mut self.construction_requested, work.requested.0)?;
            add_u64(&mut self.construction_nominal_power, work.nominal_power.0)?;
            add_u64(&mut self.construction_granted_work, work.granted_work.0)?;
            add_u64(&mut self.construction_applied_work, work.applied_work.0)?;
        }
        for heat in &report.interaction_heat {
            add_u64(&mut self.heat_generated, heat.energy.0)?;
        }
        let used = accounting.used();
        self.network_peak_used_ncu = Some(
            self.network_peak_used_ncu
                .map_or(used, |previous| previous.max(used)),
        );
        self.network_final_used_ncu = Some(used);
        add_u64(&mut self.network_integral_used_ncu, used.0)?;
        add_u64(&mut self.support_demand_integral, support.0)?;
        for destruction in &report.destructions {
            let enemy = EnemyId(destruction.target);
            if destruction.kind == crate::DestructionKind::Damage
                && self.bound_enemies.contains(&enemy)
            {
                self.killed_enemies.insert(enemy);
            }
        }
        for (progress, (sampled_presence, contact)) in self.response.iter_mut().zip(observations) {
            if progress.stimulus_tick.is_none() && sampled_presence {
                progress.stimulus_tick = Some(report.completed_tick);
            }
            if progress.stimulus_tick.is_some() && progress.response_tick.is_none() && contact {
                progress.response_tick = Some(report.completed_tick);
            }
        }
        Ok(())
    }
}

pub fn reduce_reference_metrics<'a>(
    collector: ReferenceMetricCollector,
    reports: impl IntoIterator<Item = (&'a StepReport, ReferenceMetricTickSample)>,
) -> Result<ReferenceMetricResult, ReferenceMetricError> {
    let mut collector = collector;
    for (report, sample) in reports {
        collector.observe_sample(report, sample)?;
    }
    collector.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceObservationPhase {
    Stimulus,
    Response,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReferenceMetricError {
    #[error("invalid Reference Metric JSON: category={category:?}, line={line}, column={column}")]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },
    #[error("unsupported Reference Metric format version: expected {expected}, got {actual}")]
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    #[error("unsupported Reference Metric hash algorithm: expected {expected}, got {actual}")]
    UnsupportedHashAlgorithm {
        expected: &'static str,
        actual: String,
    },
    #[error("Reference Metric Set ID mismatch: expected {expected}, got {actual}")]
    MetricSetIdMismatch {
        expected: &'static str,
        actual: String,
    },
    #[error("Reference Metric Set semantic hash mismatch: expected {expected}, got {actual}")]
    MetricSetHashMismatch {
        expected: ArtifactHash,
        actual: ArtifactHash,
    },
    #[error("Reference Metric Set metrics do not equal the exact ordered v1 list")]
    MetricListMismatch,
    #[error("unknown Reference Metric {0:?}")]
    UnknownMetric(String),
    #[error("Reference Metric Set must contain at least one response observation")]
    EmptyResponseObservations,
    #[error("Reference response observations are not in canonical UTF-8 name order")]
    NonCanonicalResponseOrder,
    #[error("duplicate Reference response observation {name:?}")]
    DuplicateResponseObservation { name: String },
    #[error("duplicate Reference response binding in observation {name:?}")]
    DuplicateObservationBinding { name: String },
    #[error("Reference Metric field {field} must not be empty")]
    EmptyText { field: &'static str },
    #[error("Reference Metric local ID in {field} must be nonzero")]
    ReservedLocalId { field: &'static str },
    #[error("Architecture Wire {local_id} has conflicting metric role classes")]
    ConflictingWireRoleClass {
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("Architecture local ID {local_id} appears more than once")]
    DuplicateArchitectureLocalId {
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("metric Wire role targets non-Wire local ID {local_id}")]
    WireRoleTargetsNonWire {
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("reserved metric Wire role {name:?} targets a non-Wire semantic surface")]
    ReservedWireRoleTargetsNonWire { name: String },
    #[error("Reference total Wire NCU {actual:?} does not equal raw length {expected:?}")]
    WireNcuMismatch {
        expected: Capacity,
        actual: Capacity,
    },
    #[error("materialized local-ID inventory count mismatch: expected {expected}, got {actual}")]
    MaterializationInventoryMismatch { expected: usize, actual: usize },
    #[error("materialized local-ID keys do not equal the Architecture operation IDs")]
    MaterializationLocalIdsMismatch,
    #[error(
        "materialized command/acceptance count mismatch: commands {commands}, acceptances {acceptances}"
    )]
    MaterializationAcceptanceCountMismatch { commands: usize, acceptances: usize },
    #[error("materialized build boundary does not equal its exact dependency-batch schedule")]
    MaterializationBuildBoundaryMismatch,
    #[error("materialized command {index} does not equal the exact resolved plan command")]
    MaterializationCommandMismatch { index: usize },
    #[error("materialized acceptance target Tick or ordinal does not match its command")]
    MaterializationAcceptanceMismatch,
    #[error("materialized Architecture reuses runtime Entity {entity:?}")]
    DuplicateMaterializedEntity { entity: crate::EntityId },
    #[error("materialized created identities do not equal the complete local-ID map")]
    MaterializationCreatedEntitiesMismatch,
    #[error("materialized Command Log hash mismatch: expected {expected}, got {actual}")]
    CommandLogHashMismatch {
        expected: ArtifactHash,
        actual: ArtifactHash,
    },
    #[error("Metric observation {name:?} local ID {local_id} does not denote a Wire")]
    ObservationTargetsNonWire {
        name: String,
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("Metric observation references missing materialized local ID {local_id}")]
    MissingMaterializedLocalId {
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("Metric observation references missing Scenario Enemy ordinal {ordinal}")]
    MissingScenarioEnemy { ordinal: u32 },
    #[error("planned Construction Work does not support local primitive {local_id}")]
    UnsupportedPlannedConstructionTarget {
        local_id: ReferenceArchitectureLocalId,
    },
    #[error("unable to derive exact planned Construction Work: {0}")]
    ConstructionWork(crate::ConstructionError),
    #[error("unable to verify materialized Reference Architecture evidence: {0}")]
    Architecture(crate::ReferenceArchitectureError),
    #[error("invalid Wire end {actual:?}: expected `a` or `b`")]
    InvalidWireEnd { actual: String },
    #[error("invalid terminal cause {actual:?}: expected `main-core-destroyed`")]
    InvalidTerminalCause { actual: String },
    #[error("Reference Metric text exceeds the canonical u32 byte length")]
    TextTooLong,
    #[error("Reference Metric collection {collection} exceeds the canonical u32 length")]
    CollectionTooLong { collection: &'static str },
    #[error("unable to encode canonical Reference Metric JSON: {message}")]
    EncodeJson { message: String },
    #[error("invalid canonical hash in {field}: {error}")]
    InvalidHash {
        field: &'static str,
        error: crate::HashParseError,
    },
    #[error("Reference Metric decimal field {field} is not canonical unsigned base-10")]
    NonCanonicalDecimal { field: &'static str },
    #[error("Reference Metric decimal field {field} exceeds u128")]
    DecimalOverflow { field: &'static str },
    #[error(
        "Reference Metric boundaries are invalid: build={build_end_tick:?}, measurement={measurement_start_tick:?}, max={max_ticks:?}"
    )]
    InvalidBoundaries {
        build_end_tick: Tick,
        measurement_start_tick: Tick,
        max_ticks: Tick,
    },
    #[error(
        "Reference Metric final boundary is invalid: measurement={measurement_start_tick:?}, final={final_next_tick:?}, max={max_ticks:?}"
    )]
    InvalidFinalBoundary {
        measurement_start_tick: Tick,
        final_next_tick: Tick,
        max_ticks: Tick,
    },
    #[error("Reference Metric static length {field} must be nonnegative, got {value}")]
    NegativeStaticLength { field: &'static str, value: i64 },
    #[error("Reference Wire length subtotals {actual} do not equal total {expected}")]
    WireLengthSubtotalMismatch { expected: i64, actual: i64 },
    #[error("Reference Gate type counts {actual} do not equal total {expected}")]
    GateCountMismatch { expected: u64, actual: u64 },
    #[error("materialized Wire length {actual} does not equal planned {expected}")]
    MaterializedWireLengthMismatch { expected: i64, actual: i64 },
    #[error("materialized Gate inventory does not equal planned inventory")]
    MaterializedGateInventoryMismatch,
    #[error("unable to measure materialized Reference Wire geometry")]
    WireGeometry,
    #[error("Reference Metric baseline requires Main Core")]
    MissingMainCore,
    #[error("Reference Metric initial sample must be Tick 0, got {actual:?}")]
    InvalidInitialTick { actual: Tick },
    #[error("Reference Metric initial sample must have Running status")]
    InvalidInitialRunStatus,
    #[error("Reference Metric bound Enemy set must not be empty")]
    EmptyBoundEnemies,
    #[error("duplicate identity in Reference Metric collection {collection}")]
    DuplicateIdentity { collection: &'static str },
    #[error("resolved response {name:?} references unbound Enemy {enemy:?}")]
    UnknownResponseEnemy { name: String, enemy: EnemyId },
    #[error("resolved response {name:?} contains a reserved runtime identity")]
    ReservedResponseIdentity { name: String },
    #[error("Reference Metric run has already reached its evaluation boundary")]
    RunAlreadyComplete,
    #[error("noncontiguous Reference Metric Tick: expected {expected:?}, got {actual:?}")]
    NonContiguousTick { expected: Tick, actual: Tick },
    #[error("Reference Metric report next Tick mismatch: expected {expected:?}, got {actual:?}")]
    ReportNextTickMismatch { expected: Tick, actual: Tick },
    #[error("Reference Metric run exceeds maxTicks {max_ticks:?}: got {actual:?}")]
    RunPastMaxTicks { max_ticks: Tick, actual: Tick },
    #[error("Reference Metric snapshot does not match its completed StepReport")]
    SnapshotReportMismatch,
    #[error("Reference Metric checked u128 arithmetic overflow")]
    ArithmeticOverflow,
    #[error("Reference Metric result field {field} does not fit its exact result type")]
    ResultOutOfRange { field: &'static str },
    #[error("Reference Metric run is incomplete: completed {completed_ticks}, max {max_ticks}")]
    IncompleteRun {
        completed_ticks: u64,
        max_ticks: u64,
    },
    #[error("Reference Metric trace has no completed Tick in its measurement window")]
    EmptyMeasurementWindow,
    #[error("Reference Metric trace does not contain the measurement-start sample")]
    MissingMeasurementStartSample,
    #[error("Main Core Integrity increased from {initial:?} to {final_integrity:?}")]
    CoreIntegrityIncreased {
        initial: Integrity,
        final_integrity: Integrity,
    },
    #[error(
        "Main Core damage mismatch: measurement start {measurement_start:?}, final {final_integrity:?}, expected {expected:?}, got {actual:?}"
    )]
    CoreDamageMismatch {
        measurement_start: Integrity,
        final_integrity: Integrity,
        expected: Integrity,
        actual: Integrity,
    },
    #[error("Tick {tick:?} has no Power StepReport")]
    MissingPowerReport { tick: Tick },
    #[error("Tick {tick:?} has no Network Accounting")]
    MissingNetworkAccounting { tick: Tick },
    #[error("Tick {tick:?} has no v5 Capacity Support demand")]
    MissingSupportDemand { tick: Tick },
    #[error("Tick {tick:?} has noncanonical or duplicate report rows in {collection}")]
    NonCanonicalReportRows {
        tick: Tick,
        collection: &'static str,
    },
    #[error("Tick {tick:?} is missing sensor observation {name:?}")]
    MissingSensorObservation { name: String, tick: Tick },
    #[error("response observation {name:?} did not reach {phase:?}")]
    ObservationNotReached {
        name: String,
        phase: ReferenceObservationPhase,
    },
    #[error("response observation {name:?} recorded response before stimulus")]
    ObservationOrderViolation { name: String },
    #[error("Reference Metric terminal status is incoherent with its boundaries")]
    TerminalBoundaryMismatch,
    #[error("Reference response latency rows do not match the Metric Set definition")]
    ResponseDefinitionMismatch,
    #[error("Pair Metric Set ID/hash does not match the supplied Metric Set artifact")]
    PairMetricSetMismatch,
    #[error("Pair design reference for role {role:?} does not match the supplied Architecture")]
    PairDesignMismatch { role: ReferenceArchitectureRole },
    #[error("Pair response bindings do not exactly match the Metric Set rows")]
    PairResponseBindingMismatch,
    #[error("{role} Architecture is missing semantic binding {name:?}")]
    MissingArchitectureBinding { role: &'static str, name: String },
    #[error("{role} Architecture semantic binding {name:?} targets the wrong surface")]
    ArchitectureBindingTargetMismatch { role: &'static str, name: String },
}

fn decode_metric_json<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, ReferenceMetricError> {
    serde_json::from_slice(bytes).map_err(|error| ReferenceMetricError::InvalidJson {
        category: JsonErrorCategory::from(error.classify()),
        line: error.line(),
        column: error.column(),
    })
}

fn snapshot_wire_length_raw(snapshot: &RenderSnapshot) -> Result<i64, ReferenceMetricError> {
    let mut total = 0_i64;
    for wire in snapshot.wires() {
        let length =
            crate::polyline_length(&wire.points).map_err(|_| ReferenceMetricError::WireGeometry)?;
        if length.0 < 0 {
            return Err(ReferenceMetricError::WireGeometry);
        }
        total = total
            .checked_add(length.0)
            .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
    }
    Ok(total)
}

fn gate_counts(snapshot: &RenderSnapshot) -> Result<(u64, u64, u64), ReferenceMetricError> {
    let mut and_count = 0_u64;
    let mut or_count = 0_u64;
    let mut not_count = 0_u64;
    for gate in snapshot.gates() {
        let counter = match gate.gate_type {
            GateType::And => &mut and_count,
            GateType::Or => &mut or_count,
            GateType::Not => &mut not_count,
        };
        *counter = counter
            .checked_add(1)
            .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
    }
    Ok((and_count, or_count, not_count))
}

fn validate_resolved_responses(
    rows: &[ResolvedReferenceResponseObservation],
    bound_enemies: &BTreeSet<EnemyId>,
) -> Result<(), ReferenceMetricError> {
    if rows.is_empty() {
        return Err(ReferenceMetricError::EmptyResponseObservations);
    }
    let mut prior_name: Option<&str> = None;
    let mut targets = BTreeSet::new();
    for row in rows {
        if row.name.is_empty() {
            return Err(ReferenceMetricError::EmptyText {
                field: "resolvedResponse[].name",
            });
        }
        canonical_text(&row.name)?;
        if prior_name.is_some_and(|prior| prior.as_bytes() >= row.name.as_bytes()) {
            return if prior_name == Some(row.name.as_str()) {
                Err(ReferenceMetricError::DuplicateResponseObservation {
                    name: row.name.clone(),
                })
            } else {
                Err(ReferenceMetricError::NonCanonicalResponseOrder)
            };
        }
        if row.sensor_wire.entity_id().0 == 0
            || row.defense_wire.entity_id().0 == 0
            || row.enemy.entity_id().0 == 0
        {
            return Err(ReferenceMetricError::ReservedResponseIdentity {
                name: row.name.clone(),
            });
        }
        if !bound_enemies.contains(&row.enemy) {
            return Err(ReferenceMetricError::UnknownResponseEnemy {
                name: row.name.clone(),
                enemy: row.enemy,
            });
        }
        if !targets.insert((row.sensor_wire, row.sensor_end, row.defense_wire, row.enemy)) {
            return Err(ReferenceMetricError::DuplicateObservationBinding {
                name: row.name.clone(),
            });
        }
        prior_name = Some(&row.name);
    }
    Ok(())
}

fn validate_power_report_order(
    power: &crate::PowerStepReport,
    tick: Tick,
) -> Result<(), ReferenceMetricError> {
    strictly_ordered_by(&power.regions, |row| row.region)
        .then_some(())
        .ok_or(ReferenceMetricError::NonCanonicalReportRows {
            tick,
            collection: "power.regions",
        })?;
    strictly_ordered_by(&power.loads, |row| row.demand)
        .then_some(())
        .ok_or(ReferenceMetricError::NonCanonicalReportRows {
            tick,
            collection: "power.loads",
        })?;
    strictly_ordered_by(&power.sense, |row| (row.wire, row.end))
        .then_some(())
        .ok_or(ReferenceMetricError::NonCanonicalReportRows {
            tick,
            collection: "power.sense",
        })?;
    strictly_ordered_by(&power.heat_contributions, |row| {
        (row.owner, row.kind, row.demand)
    })
    .then_some(())
    .ok_or(ReferenceMetricError::NonCanonicalReportRows {
        tick,
        collection: "power.heatContributions",
    })?;
    Ok(())
}

fn validate_step_row_order(report: &StepReport) -> Result<(), ReferenceMetricError> {
    let tick = report.completed_tick;
    for (valid, collection) in [
        (
            strictly_ordered_by(&report.construction_work, |row| (row.site, row.builder)),
            "constructionWork",
        ),
        (
            strictly_ordered_by(&report.contacts, |row| (row.wire, row.target)),
            "contacts",
        ),
        (
            strictly_ordered_by(&report.interaction_heat, |row| {
                (row.owner, row.kind, row.demand)
            }),
            "interactionHeat",
        ),
        (
            strictly_ordered_by(&report.destructions, |row| row.target),
            "destructions",
        ),
    ] {
        if !valid {
            return Err(ReferenceMetricError::NonCanonicalReportRows { tick, collection });
        }
    }
    Ok(())
}

fn strictly_ordered_by<T, K: Ord>(rows: &[T], key: impl Fn(&T) -> K) -> bool {
    rows.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn unique_set<T: Ord>(
    collection: &'static str,
    values: Vec<T>,
) -> Result<BTreeSet<T>, ReferenceMetricError> {
    let expected = values.len();
    let set = values.into_iter().collect::<BTreeSet<_>>();
    if set.len() != expected {
        return Err(ReferenceMetricError::DuplicateIdentity { collection });
    }
    Ok(set)
}

fn add_u64(total: &mut u128, value: u64) -> Result<(), ReferenceMetricError> {
    *total = total
        .checked_add(u128::from(value))
        .ok_or(ReferenceMetricError::ArithmeticOverflow)?;
    Ok(())
}

fn checked_add_i64(left: i64, right: i64) -> Result<i64, ReferenceMetricError> {
    left.checked_add(right)
        .ok_or(ReferenceMetricError::ArithmeticOverflow)
}

fn parse_decimal(field: &'static str, value: &str) -> Result<u128, ReferenceMetricError> {
    let bytes = value.as_bytes();
    let canonical = match bytes {
        [b'0'] => true,
        [b'1'..=b'9', tail @ ..] => tail.iter().all(u8::is_ascii_digit),
        _ => false,
    };
    if !canonical {
        return Err(ReferenceMetricError::NonCanonicalDecimal { field });
    }
    value
        .parse::<u128>()
        .map_err(|_| ReferenceMetricError::DecimalOverflow { field })
}

fn canonical_count(collection: &'static str, count: usize) -> Result<u32, ReferenceMetricError> {
    u32::try_from(count).map_err(|_| ReferenceMetricError::CollectionTooLong { collection })
}

fn canonical_text(value: &str) -> Result<(), ReferenceMetricError> {
    u32::try_from(value.len())
        .map(|_| ())
        .map_err(|_| ReferenceMetricError::TextTooLong)
}

#[derive(Default)]
struct MetricEncoder(Vec<u8>);

impl MetricEncoder {
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

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn u128(&mut self, value: u128) {
        self.bytes(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }

    fn count(
        &mut self,
        collection: &'static str,
        count: usize,
    ) -> Result<(), ReferenceMetricError> {
        self.u32(canonical_count(collection, count)?);
        Ok(())
    }

    fn text(&mut self, value: &str) -> Result<(), ReferenceMetricError> {
        self.u32(u32::try_from(value.len()).map_err(|_| ReferenceMetricError::TextTooLong)?);
        self.bytes(value.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn local(value: u32) -> ReferenceArchitectureLocalId {
        ReferenceArchitectureLocalId::new(value).expect("test local ID is nonzero")
    }

    fn definition() -> ReferenceMetricSetArtifact {
        ReferenceMetricSetArtifact::v1(vec![ReferenceResponseObservationSpec {
            name: "north.0".to_owned(),
            hostile_entry_binding: "sensor.north.0".to_owned(),
            defense_contact_binding: "defense.north.0".to_owned(),
            enemy_ordinal: 0,
        }])
        .expect("valid test Metric Set")
    }

    #[test]
    fn metric_set_v1_declares_every_hashed_static_and_runtime_field_in_exact_tag_order() {
        assert_eq!(REFERENCE_METRICS_V1.len(), 37);
        for (tag, metric) in REFERENCE_METRICS_V1.into_iter().enumerate() {
            assert_eq!(usize::from(metric as u8), tag);
            assert_eq!(ReferenceMetric::parse(metric.as_str()), Some(metric));
        }
        assert_eq!(
            ReferenceMetric::MeasurementStartCoreIntegrity.as_str(),
            "measurementStartCoreIntegrity"
        );
        assert_eq!(ReferenceMetric::FinalCoreIntegrity as u8, 18);
        assert_eq!(ReferenceMetric::ResponseLatencyTicks as u8, 36);

        let definition = definition();
        assert_eq!(definition.metrics(), REFERENCE_METRICS_V1);
    }

    fn result() -> ReferenceMetricResult {
        ReferenceMetricResult {
            boundaries: ReferenceMetricResultBoundaries {
                build_end_tick: Tick(1),
                measurement_start_tick: Tick(2),
                final_next_tick: Tick(5),
                max_ticks: Tick(5),
            },
            static_inventory: ReferenceStaticInventory {
                total_wire_length_raw: 10,
                total_wire_ncu: Capacity(10),
                shared_wire_length_raw: 0,
                sensor_wire_length_raw: 4,
                trunk_wire_length_raw: 0,
                defense_wire_length_raw: 6,
                other_wire_length_raw: 0,
                gate_count: 0,
                and_count: 0,
                or_count: 0,
                not_count: 0,
                planned_construction_work: u128::from(u64::MAX) + 1,
                build_command_count: 2,
                command_log_hash: ArtifactHash::from_bytes([0x11; 32]),
            },
            runtime_metrics: ReferenceRuntimeMetrics {
                survived_boundary: true,
                completed_ticks: 5,
                terminal_status: ReferenceTerminalStatus::Running,
                measurement_start_core_integrity: Integrity(100),
                final_core_integrity: Integrity(90),
                core_damage: Integrity(10),
                power_generation: 300,
                power_nominal_demand: 200,
                power_granted: 180,
                power_source_cost: 190,
                power_transmission_loss: 10,
                brownout_ticks: 1,
                construction_requested: 0,
                construction_nominal_power: 0,
                construction_granted_work: 0,
                construction_applied_work: 0,
                heat_generated: 7,
                network_peak_used_ncu: Capacity(10),
                network_final_used_ncu: Capacity(9),
                network_integral_used_ncu: 28,
                support_demand_integral: 4,
                enemy_kills: 1,
            },
            response_latency_ticks: vec![ReferenceResponseLatency {
                name: "north.0".to_owned(),
                stimulus_tick: Tick(2),
                response_tick: Tick(4),
                latency_ticks: 2,
            }],
        }
    }

    fn static_fixture() -> (
        ReferenceArchitectureArtifact,
        MaterializedReferenceArchitecture,
        ConstructionProbeProfile,
        ReferenceArchitectureScenarioResolution,
    ) {
        let profiles = crate::ProfileBundle {
            numeric: crate::NumericProfile::reference_v1("metric-static-numeric"),
            physical_scale: crate::PhysicalScaleProfile::stage0_alpha("metric-static-physical"),
            balance: crate::BalanceProfile::construction_contact_damage_alpha(
                "metric-static-balance",
            ),
        };
        let contract = crate::SimulationContract::from_profiles(&profiles).expect("valid profiles");
        let construction_probe = profiles
            .balance
            .construction_probe
            .expect("v5 construction probe");
        let sensor_id = local(1);
        let defense_id = local(2);
        let substrate_id = local(3);
        let substrate_entity = crate::EntityId(10);
        let sensor_entity = crate::EntityId(11);
        let defense_entity = crate::EntityId(12);
        let zero = crate::FixedVec2::new(crate::Fixed::ZERO, crate::Fixed::ZERO);
        let east = crate::FixedVec2::new(crate::Fixed(crate::FIXED_ONE), crate::Fixed::ZERO);
        let footprint = crate::FixedAabb::new(
            zero,
            crate::FixedVec2::new(
                crate::Fixed(crate::FIXED_ONE),
                crate::Fixed(crate::FIXED_ONE),
            ),
        );
        let operations = vec![
            ReferenceArchitectureOperation::PlaceFixedSubstrate(crate::ReferenceFixedSubstrate {
                id: substrate_id,
                origin: zero,
                routing_area: footprint,
                footprint,
            }),
            ReferenceArchitectureOperation::PlaceWire(crate::ReferenceWire {
                id: sensor_id,
                routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate_id),
                points: vec![zero, east],
                endpoint_a: crate::ReferenceArchitectureEndpoint::Free,
                endpoint_b: crate::ReferenceArchitectureEndpoint::Free,
            }),
            ReferenceArchitectureOperation::PlaceWire(crate::ReferenceWire {
                id: defense_id,
                routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate_id),
                points: vec![zero, east],
                endpoint_a: crate::ReferenceArchitectureEndpoint::Free,
                endpoint_b: crate::ReferenceArchitectureEndpoint::Free,
            }),
        ];
        let architecture = ReferenceArchitectureArtifact {
            format_version: crate::ReferenceArchitectureFormatVersion::V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "metric static fixture".to_owned(),
            contract,
            operations,
            role_bindings: vec![crate::ReferenceArchitectureRoleBinding {
                name: "defense.north.0".to_owned(),
                target: ReferenceArchitectureSemanticTarget::LocalEntity(defense_id),
            }],
            observation_bindings: vec![crate::ReferenceArchitectureObservationBinding {
                name: "sensor.north.0".to_owned(),
                target: ReferenceArchitectureSemanticTarget::WireSensePort {
                    wire: sensor_id,
                    end: WireEnd::A,
                },
            }],
            materialization_schedule: None,
        };
        let commands = vec![
            crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceFixedSubstrate(crate::PlaceFixedSubstrateCommand {
                    origin: zero,
                    routing_area: footprint,
                    footprint,
                }),
            },
            crate::CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: RoutingDomain::FixedSubstrate(substrate_entity),
                    points: vec![zero, east],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            },
            crate::CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 1,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: RoutingDomain::FixedSubstrate(substrate_entity),
                    points: vec![zero, east],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            },
        ];
        let acceptances = commands
            .iter()
            .zip([substrate_entity, sensor_entity, defense_entity])
            .map(|(command, created_entity)| crate::CommandAcceptance {
                target_tick: command.target_tick,
                ordinal: command.ordinal,
                created_entity: Some(created_entity),
            })
            .collect::<Vec<_>>();
        let command_log_hash =
            reference_architecture_command_log_hash(&commands).expect("command log hash");
        let materialization = MaterializedReferenceArchitecture {
            local_entities: BTreeMap::from([
                (substrate_id, substrate_entity),
                (sensor_id, sensor_entity),
                (defense_id, defense_entity),
            ]),
            commands,
            acceptances,
            command_log_hash,
            build_end_tick: Tick(2),
            executed_batch_evidence: Vec::new(),
            binding_stage_evidence: Vec::new(),
        };
        let scenario = ReferenceArchitectureScenarioResolution {
            main_core: crate::MainCoreId(crate::EntityId(1)),
            power_sources: Vec::new(),
            enemies: vec![EnemyId(crate::EntityId(20))],
        };
        (architecture, materialization, construction_probe, scenario)
    }

    fn v2_static_fixture_with_empty_pair_placement() -> (
        ReferenceArchitectureArtifact,
        MaterializedReferenceArchitecture,
        ConstructionProbeProfile,
        ReferenceArchitectureScenarioResolution,
    ) {
        let (mut architecture, mut materialization, construction_probe, scenario) =
            static_fixture();
        architecture.format_version = ReferenceArchitectureFormatVersion::V2;
        let sensor_id = local(1);
        let ReferenceArchitectureOperation::PlaceWire(sensor) = architecture
            .operations
            .iter_mut()
            .find(|operation| operation.local_id() == sensor_id)
            .expect("sensor operation")
        else {
            panic!("sensor local ID denotes a Wire")
        };
        sensor.endpoint_a = crate::ReferenceArchitectureEndpoint::MainCore;
        architecture.materialization_schedule =
            Some(crate::ReferenceArchitectureMaterializationSchedule {
                binding_batches: vec![vec![crate::ReferenceArchitectureBindingEndpoint {
                    wire: sensor_id,
                    end: WireEnd::A,
                }]],
            });

        for command in &mut materialization.commands[1..] {
            command.target_tick = Tick(2);
        }
        for acceptance in &mut materialization.acceptances[1..] {
            acceptance.target_tick = Tick(2);
        }
        materialization.commands.push(crate::CommandEnvelope {
            target_tick: Tick(3),
            ordinal: 0,
            command: crate::Command::BindPort(crate::BindPortCommand {
                wire: WireId(materialization.local_entities[&sensor_id]),
                end: WireEnd::A,
                target: EndpointTarget::MainCoreAnchor(scenario.main_core),
            }),
        });
        materialization.acceptances.push(crate::CommandAcceptance {
            target_tick: Tick(3),
            ordinal: 0,
            created_entity: None,
        });
        materialization.command_log_hash =
            reference_architecture_command_log_hash(&materialization.commands)
                .expect("v2 command log hash");
        materialization.build_end_tick = Tick(4);
        materialization.executed_batch_evidence = vec![
            crate::ReferenceArchitectureExecutedBatchEvidence {
                kind: ReferenceArchitectureMaterializationBatchKind::Placement { phase: 0 },
                command_tick: Tick(0),
            },
            crate::ReferenceArchitectureExecutedBatchEvidence {
                kind: ReferenceArchitectureMaterializationBatchKind::Placement { phase: 1 },
                command_tick: Tick(1),
            },
            crate::ReferenceArchitectureExecutedBatchEvidence {
                kind: ReferenceArchitectureMaterializationBatchKind::Placement { phase: 2 },
                command_tick: Tick(2),
            },
            crate::ReferenceArchitectureExecutedBatchEvidence {
                kind: ReferenceArchitectureMaterializationBatchKind::Binding { stage: 0 },
                command_tick: Tick(3),
            },
        ];
        materialization.binding_stage_evidence =
            vec![crate::ReferenceArchitectureBindingStageEvidence {
                stage: 0,
                command_tick: Tick(3),
                barrier_ticks: Vec::new(),
                quiescent_tick: Tick(4),
            }];
        (architecture, materialization, construction_probe, scenario)
    }

    fn collector_trace_fixture() -> (
        ReferenceMetricCollector,
        Vec<StepReport>,
        crate::PowerSourceId,
        WireId,
        WireId,
        EnemyId,
    ) {
        let mut balance =
            crate::BalanceProfile::construction_contact_damage_alpha("metric-reducer-balance");
        balance
            .capacity_probe
            .as_mut()
            .expect("v5 capacity probe")
            .support_heat_fraction =
            crate::Rational::new(1, i64::MAX).expect("minimal positive support heat");
        let profiles = crate::ProfileBundle {
            numeric: crate::NumericProfile::reference_v1("metric-reducer-numeric"),
            physical_scale: crate::PhysicalScaleProfile::stage0_alpha("metric-reducer-physical"),
            balance,
        };
        let contract = crate::SimulationContract::from_profiles(&profiles).expect("valid profiles");
        let wu = |x: i64, y: i64| {
            crate::FixedVec2::new(
                crate::Fixed(x * crate::FIXED_ONE),
                crate::Fixed(y * crate::FIXED_ONE),
            )
        };
        let package = crate::SimulationPackage::new(
            "metric-reducer-trace",
            crate::InitialWorld::MainCorePowerEnemyV1 {
                main_core_position: wu(-100, -100),
                main_core_integrity: Integrity(100),
                main_core_heat_energy: crate::HeatEnergy(0),
                power_sources: vec![crate::PowerSourceInitialState::new(
                    wu(-50, -50),
                    crate::Energy(100),
                )],
                enemies: vec![crate::EnemyInitialState::new(
                    wu(100, 100),
                    crate::FixedVec2::new(crate::Fixed::ZERO, crate::Fixed::ZERO),
                    crate::Fixed(1_024),
                    Integrity(10),
                    crate::HeatEnergy(0),
                )],
            },
            crate::StageFeatureSet {
                signal: true,
                mobility: true,
                capacity: true,
                sensing: true,
                power: true,
                construction: true,
                contact: true,
                damage: true,
                ..crate::StageFeatureSet::none()
            },
            contract,
            profiles,
        );
        let mut simulation = crate::Simulation::new(package).expect("metric trace starts");
        let initial_sample = ReferenceMetricTickSample {
            next_tick: Tick(0),
            state_hash: simulation.state_hash(),
            run_status: RunStatus::Running,
            core_integrity: Integrity(100),
        };
        let source = simulation
            .power_sources()
            .next()
            .expect("fixture source")
            .id();
        let enemy = simulation
            .enemies()
            .iter()
            .next()
            .expect("fixture enemy")
            .id();
        let sensor = WireId(crate::EntityId(80));
        let defense = WireId(crate::EntityId(81));
        let builds = (0..2)
            .map(|ordinal| crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![wu(0, ordinal as i64 * 2), wu(200, ordinal as i64 * 2)],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            })
            .collect::<Vec<_>>();
        let mut reports = vec![simulation.step(&builds).expect("build Tick")];
        assert_eq!(
            reports[0].command_acceptances.len(),
            2,
            "build rejection: {:?}",
            reports[0].command_rejections
        );
        reports.push(simulation.step(&[]).expect("first measurement Tick"));
        reports.push(simulation.step(&[]).expect("second measurement Tick"));
        assert!(
            reports
                .iter()
                .all(|report| report.run_status == RunStatus::Running)
        );
        assert!(
            reports[1..].iter().all(|report| {
                report
                    .network_accounting
                    .and_then(|accounting| accounting.total_support_demand())
                    .is_some_and(|demand| demand.0 > 0)
            }),
            "fixture must exercise positive v5 support demand"
        );
        let collector = ReferenceMetricCollector::new(
            ReferenceMetricBoundaries {
                build_end_tick: Tick(1),
                measurement_start_tick: Tick(1),
                max_ticks: Tick(3),
            },
            result().static_inventory,
            initial_sample,
            vec![enemy],
            vec![ResolvedReferenceResponseObservation {
                name: "north.0".to_owned(),
                sensor_wire: sensor,
                sensor_end: WireEnd::A,
                defense_wire: defense,
                enemy,
            }],
        )
        .expect("collector fixture");
        (collector, reports, source, sensor, defense, enemy)
    }

    #[allow(clippy::too_many_arguments)]
    fn set_exact_metric_rows(
        report: &mut StepReport,
        source: crate::PowerSourceId,
        sensor: WireId,
        defense: WireId,
        enemy: EnemyId,
        generation: u64,
        nominal: u64,
        granted: u64,
        source_cost: u64,
        loss: u64,
        power_heat: u64,
        interaction_heat: u64,
        construction: (u64, u64, u64, u64),
        contact: bool,
    ) {
        let ratio = if nominal == granted {
            crate::PowerRatio::ONE
        } else {
            crate::PowerRatio::new(crate::Fixed(crate::FIXED_ONE / 2)).expect("half ratio")
        };
        let demand = crate::DemandId::new(crate::EntityId(90), crate::DemandKind::LiveWire);
        report.power = Some(crate::PowerStepReport {
            regions: vec![crate::PowerRegionReport {
                region: crate::PowerRegionId(1),
                first_node: crate::PowerNodeKey::SourceAnchor(source),
                sources: vec![source],
                generation: crate::Energy(generation),
                total_nominal_demand: crate::Energy(nominal),
                ratio,
            }],
            loads: vec![crate::PowerLoadReport {
                demand,
                region: crate::PowerRegionId(1),
                nominal: crate::Energy(nominal),
                granted: crate::Energy(granted),
                ratio,
                source_route: None,
                transmission_loss: crate::Energy(loss),
                source_cost: crate::Energy(source_cost),
            }],
            sense: vec![crate::PowerSenseReport {
                wire: sensor,
                end: WireEnd::A,
                sampled_presence: true,
                intended_level: crate::LogicLevel::High,
                intended_strength: crate::DriveStrength(1),
                current_driver: crate::DriverSample::s0m3(
                    crate::DriverId(crate::EntityId(91)),
                    crate::LogicLevel::Low,
                    crate::DriveStrength(0),
                    report.completed_tick,
                ),
            }],
            heat_contributions: vec![crate::PowerHeatReport {
                owner: sensor,
                kind: crate::PowerHeatKind::TransmissionLoss,
                demand,
                energy: crate::HeatEnergy(power_heat),
            }],
            ..crate::PowerStepReport::default()
        });
        report.construction_work = vec![crate::ConstructionWorkReport {
            site: crate::ConstructionSiteId(crate::EntityId(92)),
            builder: crate::MobileId(crate::EntityId(93)),
            requested: crate::Energy(construction.0),
            nominal_power: crate::Energy(construction.1),
            granted_work: crate::Energy(construction.2),
            applied_work: crate::Energy(construction.3),
            completed_work: crate::Energy(0),
        }];
        report.interaction_heat = vec![crate::InteractionHeatReport {
            owner: crate::EntityId(94),
            kind: crate::InteractionHeatKind::Movement,
            demand: None,
            energy: crate::HeatEnergy(interaction_heat),
        }];
        report.contacts = contact
            .then_some(crate::ContactEnergyReport {
                wire: defense,
                target: enemy,
                weight: 1,
                absorbed: crate::Energy(1),
            })
            .into_iter()
            .collect();
        report.destructions = vec![crate::DestructionReport {
            target: enemy.entity_id(),
            kind: crate::DestructionKind::Damage,
        }];
    }

    fn sample_for(report: &StepReport, core_integrity: u64) -> ReferenceMetricTickSample {
        ReferenceMetricTickSample {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
            run_status: report.run_status,
            core_integrity: Integrity(core_integrity),
        }
    }

    #[test]
    fn metric_set_and_result_round_trip_with_stable_semantic_hashes() {
        let definition = definition();
        let definition_bytes =
            encode_reference_metric_set_artifact(&definition).expect("Metric Set encodes");
        let decoded_definition =
            decode_reference_metric_set_artifact(&definition_bytes).expect("Metric Set decodes");
        assert_eq!(decoded_definition, definition);
        assert_eq!(
            decoded_definition.semantic_hash(),
            definition.semantic_hash()
        );

        let artifact = ReferenceMetricArtifact::v1(
            &definition,
            ExperimentRunId::from_bytes([0x22; 32]),
            result(),
        )
        .expect("result artifact is valid");
        let bytes = encode_reference_metric_artifact(&artifact, &definition)
            .expect("result artifact encodes");
        let decoded =
            decode_reference_metric_artifact(&bytes, &definition).expect("result artifact decodes");
        assert_eq!(decoded, artifact);
        assert_eq!(
            decoded.semantic_hash(&definition),
            artifact.semantic_hash(&definition)
        );
        assert!(
            String::from_utf8(bytes)
                .expect("JSON is UTF-8")
                .contains("\"18446744073709551616\"")
        );
    }

    #[test]
    fn metric_set_hash_binds_portable_observation_names_and_enemy_ordinal() {
        let baseline = definition();
        let baseline_hash = baseline.semantic_hash().expect("baseline Metric Set hash");

        let mut changed = baseline.clone();
        changed.response_observations[0].name = "north.1".to_owned();
        assert_ne!(
            changed.semantic_hash().expect("name-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline.clone();
        changed.response_observations[0].hostile_entry_binding = "sensor.north.1".to_owned();
        assert_ne!(
            changed
                .semantic_hash()
                .expect("hostile-binding-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline.clone();
        changed.response_observations[0].defense_contact_binding = "defense.north.1".to_owned();
        assert_ne!(
            changed
                .semantic_hash()
                .expect("defense-binding-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline;
        changed.response_observations[0].enemy_ordinal = 1;
        assert_ne!(
            changed
                .semantic_hash()
                .expect("enemy-ordinal-sensitive hash"),
            baseline_hash
        );
    }

    #[test]
    fn strict_json_and_canonical_decimal_fail_closed() {
        let definition = definition();
        let artifact = ReferenceMetricArtifact::v1(
            &definition,
            ExperimentRunId::from_bytes([0x22; 32]),
            result(),
        )
        .expect("valid artifact");
        let encoded =
            encode_reference_metric_artifact(&artifact, &definition).expect("artifact encodes");
        let source = String::from_utf8(encoded).expect("JSON is UTF-8");
        let leading_zero = source.replace(
            "\"plannedConstructionWork\": \"18446744073709551616\"",
            "\"plannedConstructionWork\": \"018446744073709551616\"",
        );
        assert_eq!(
            decode_reference_metric_artifact(leading_zero.as_bytes(), &definition),
            Err(ReferenceMetricError::NonCanonicalDecimal {
                field: "staticInventory.plannedConstructionWork"
            })
        );
        let unknown = source.replacen('{', "{\n  \"unknown\": 0,", 1);
        assert!(matches!(
            decode_reference_metric_artifact(unknown.as_bytes(), &definition),
            Err(ReferenceMetricError::InvalidJson { .. })
        ));
        let mut trailing = source.into_bytes();
        trailing.extend_from_slice(b"{}\n");
        assert!(matches!(
            decode_reference_metric_artifact(&trailing, &definition),
            Err(ReferenceMetricError::InvalidJson { .. })
        ));
    }

    #[test]
    fn inventory_and_latency_invariants_reject_inconsistent_results() {
        let mut invalid = result();
        invalid.static_inventory.other_wire_length_raw = 1;
        assert_eq!(
            validate_metric_result(&invalid),
            Err(ReferenceMetricError::WireLengthSubtotalMismatch {
                expected: 10,
                actual: 11,
            })
        );

        let mut invalid = result();
        invalid.response_latency_ticks[0].latency_ticks = 1;
        assert_eq!(
            validate_metric_result(&invalid),
            Err(ReferenceMetricError::ObservationOrderViolation {
                name: "north.0".to_owned(),
            })
        );

        let mut invalid = result();
        invalid.runtime_metrics.core_damage = Integrity(9);
        assert_eq!(
            validate_metric_result(&invalid),
            Err(ReferenceMetricError::CoreDamageMismatch {
                measurement_start: Integrity(100),
                final_integrity: Integrity(90),
                expected: Integrity(10),
                actual: Integrity(9),
            })
        );

        let mut invalid = result();
        invalid.runtime_metrics.survived_boundary = false;
        invalid.runtime_metrics.terminal_status = ReferenceTerminalStatus::Ended {
            completed_tick: Tick(4),
            cause: RunEndCause::MainCoreDestroyed,
        };
        assert_eq!(
            validate_metric_result(&invalid),
            Err(ReferenceMetricError::TerminalBoundaryMismatch)
        );
    }

    #[test]
    fn format_and_hash_envelopes_precede_unknown_body_fields() {
        let definition = definition();
        let definition_bytes =
            encode_reference_metric_set_artifact(&definition).expect("definition encodes");
        let definition_json = String::from_utf8(definition_bytes).expect("UTF-8");
        let unknown_definition = definition_json.replacen('{', "{\n  \"unknown\": 0,", 1);
        let unsupported_definition =
            unknown_definition.replacen("\"formatVersion\": 1", "\"formatVersion\": 9", 1);
        assert_eq!(
            decode_reference_metric_set_artifact(unsupported_definition.as_bytes()),
            Err(ReferenceMetricError::UnsupportedFormatVersion {
                expected: REFERENCE_METRIC_SET_FORMAT_V1,
                actual: 9,
            })
        );
        let wrong_hash_definition = unknown_definition.replacen(
            "\"hashAlgorithmId\": \"blake3-v1\"",
            "\"hashAlgorithmId\": \"future-hash\"",
            1,
        );
        assert_eq!(
            decode_reference_metric_set_artifact(wrong_hash_definition.as_bytes()),
            Err(ReferenceMetricError::UnsupportedHashAlgorithm {
                expected: HASH_ALGORITHM_ID_BLAKE3_V1,
                actual: "future-hash".to_owned(),
            })
        );

        let artifact = ReferenceMetricArtifact::v1(
            &definition,
            ExperimentRunId::from_bytes([0x22; 32]),
            result(),
        )
        .expect("artifact is valid");
        let artifact_json = String::from_utf8(
            encode_reference_metric_artifact(&artifact, &definition).expect("artifact encodes"),
        )
        .expect("UTF-8");
        let unknown_artifact = artifact_json.replacen('{', "{\n  \"unknown\": 0,", 1);
        let unsupported_artifact =
            unknown_artifact.replacen("\"formatVersion\": 1", "\"formatVersion\": 9", 1);
        assert_eq!(
            decode_reference_metric_artifact(unsupported_artifact.as_bytes(), &definition),
            Err(ReferenceMetricError::UnsupportedFormatVersion {
                expected: REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1,
                actual: 9,
            })
        );
        let wrong_hash_artifact = unknown_artifact.replacen(
            "\"hashAlgorithmId\": \"blake3-v1\"",
            "\"hashAlgorithmId\": \"future-hash\"",
            1,
        );
        assert_eq!(
            decode_reference_metric_artifact(wrong_hash_artifact.as_bytes(), &definition),
            Err(ReferenceMetricError::UnsupportedHashAlgorithm {
                expected: HASH_ALGORITHM_ID_BLAKE3_V1,
                actual: "future-hash".to_owned(),
            })
        );
    }

    #[test]
    fn static_inventory_and_response_resolution_recompute_materialized_facts() {
        let (architecture, materialization, construction_probe, scenario) = static_fixture();
        let inventory = derive_reference_static_inventory(
            &architecture,
            &construction_probe,
            &materialization,
            &scenario,
        )
        .expect("static inventory derives");
        assert_eq!(inventory.total_wire_length_raw, 2 * crate::FIXED_ONE);
        assert_eq!(
            inventory.total_wire_ncu,
            Capacity(2 * crate::FIXED_ONE as u64)
        );
        assert_eq!(inventory.sensor_wire_length_raw, crate::FIXED_ONE);
        assert_eq!(inventory.defense_wire_length_raw, crate::FIXED_ONE);
        assert_eq!(inventory.planned_construction_work, 7);
        assert_eq!(inventory.build_command_count, 3);
        assert_eq!(inventory.command_log_hash, materialization.command_log_hash);

        let resolved = resolve_reference_response_observations(
            &definition(),
            &architecture,
            &materialization,
            &scenario,
        )
        .expect("response resolves");
        assert_eq!(
            resolved,
            vec![ResolvedReferenceResponseObservation {
                name: "north.0".to_owned(),
                sensor_wire: WireId(crate::EntityId(11)),
                sensor_end: WireEnd::A,
                defense_wire: WireId(crate::EntityId(12)),
                enemy: EnemyId(crate::EntityId(20)),
            }]
        );

        let mut wrong = materialization.clone();
        wrong.command_log_hash = ArtifactHash::from_bytes([0x99; 32]);
        assert!(matches!(
            derive_reference_static_inventory(
                &architecture,
                &construction_probe,
                &wrong,
                &scenario,
            ),
            Err(ReferenceMetricError::CommandLogHashMismatch { .. })
        ));
    }

    #[test]
    fn v2_metric_evidence_closes_empty_pair_batches_and_rejects_timeline_tampering() {
        let (architecture, materialization, construction_probe, scenario) =
            v2_static_fixture_with_empty_pair_placement();
        derive_reference_static_inventory(
            &architecture,
            &construction_probe,
            &materialization,
            &scenario,
        )
        .expect("an explicitly evidenced empty pair-side placement is valid");

        let assert_boundary_rejected = |tampered: &MaterializedReferenceArchitecture| {
            assert!(matches!(
                derive_reference_static_inventory(
                    &architecture,
                    &construction_probe,
                    tampered,
                    &scenario,
                ),
                Err(ReferenceMetricError::MaterializationBuildBoundaryMismatch)
            ));
        };

        let mut missing_empty_batch = materialization.clone();
        missing_empty_batch.executed_batch_evidence.remove(1);
        assert_boundary_rejected(&missing_empty_batch);

        let mut wrong_empty_batch_tick = materialization.clone();
        wrong_empty_batch_tick.executed_batch_evidence[1].command_tick = Tick(2);
        assert_boundary_rejected(&wrong_empty_batch_tick);

        let mut wrong_binding_kind = materialization.clone();
        wrong_binding_kind.executed_batch_evidence[3].kind =
            ReferenceArchitectureMaterializationBatchKind::Binding { stage: 1 };
        assert_boundary_rejected(&wrong_binding_kind);

        let mut skipped_barrier_tick = materialization.clone();
        skipped_barrier_tick.binding_stage_evidence[0].barrier_ticks = vec![Tick(5)];
        skipped_barrier_tick.binding_stage_evidence[0].quiescent_tick = Tick(6);
        skipped_barrier_tick.build_end_tick = Tick(6);
        assert_boundary_rejected(&skipped_barrier_tick);

        let mut wrong_build_end = materialization;
        wrong_build_end.build_end_tick = Tick(5);
        assert_boundary_rejected(&wrong_build_end);
    }

    #[test]
    fn portable_response_names_resolve_across_independent_local_namespaces() {
        let (brute, brute_materialization, _, scenario) = static_fixture();
        let brute_response = resolve_reference_response_observations(
            &definition(),
            &brute,
            &brute_materialization,
            &scenario,
        )
        .expect("Brute names resolve");

        let (mut computed, mut computed_materialization, _, computed_scenario) = static_fixture();
        let computed_sensor_local = local(101);
        let computed_defense_local = local(102);
        let computed_substrate_local = local(103);
        let computed_substrate_entity = crate::EntityId(110);
        let computed_sensor_entity = crate::EntityId(111);
        let computed_defense_entity = crate::EntityId(112);
        for operation in &mut computed.operations {
            match operation {
                ReferenceArchitectureOperation::PlaceFixedSubstrate(substrate) => {
                    substrate.id = computed_substrate_local;
                }
                ReferenceArchitectureOperation::PlaceWire(wire) => {
                    wire.id = if wire.id == local(1) {
                        computed_sensor_local
                    } else {
                        computed_defense_local
                    };
                    wire.routing_domain = ReferenceArchitectureRoutingDomain::FixedSubstrate(
                        computed_substrate_local,
                    );
                }
                ReferenceArchitectureOperation::PlaceGate(_)
                | ReferenceArchitectureOperation::PlaceJunction(_)
                | ReferenceArchitectureOperation::PlaceMobileSubstrate(_) => {
                    panic!("static fixture contains only a fixed substrate and Wires")
                }
            }
        }
        computed.role_bindings[0].target =
            ReferenceArchitectureSemanticTarget::LocalEntity(computed_defense_local);
        computed.observation_bindings[0].target =
            ReferenceArchitectureSemanticTarget::WireSensePort {
                wire: computed_sensor_local,
                end: WireEnd::A,
            };
        computed_materialization.local_entities = BTreeMap::from([
            (computed_substrate_local, computed_substrate_entity),
            (computed_sensor_local, computed_sensor_entity),
            (computed_defense_local, computed_defense_entity),
        ]);
        for command in &mut computed_materialization.commands {
            if let crate::Command::PlaceWire(wire) = &mut command.command {
                wire.routing_domain = RoutingDomain::FixedSubstrate(computed_substrate_entity);
            }
        }
        for (acceptance, entity) in computed_materialization.acceptances.iter_mut().zip([
            computed_substrate_entity,
            computed_sensor_entity,
            computed_defense_entity,
        ]) {
            acceptance.created_entity = Some(entity);
        }
        computed_materialization.command_log_hash =
            reference_architecture_command_log_hash(&computed_materialization.commands)
                .expect("Computed command log hash");

        let computed_response = resolve_reference_response_observations(
            &definition(),
            &computed,
            &computed_materialization,
            &computed_scenario,
        )
        .expect("the same portable names resolve through the Computed local namespace");
        assert_eq!(brute_response[0].name, computed_response[0].name);
        assert_eq!(brute_response[0].enemy, computed_response[0].enemy);
        assert_ne!(
            brute_response[0].sensor_wire,
            computed_response[0].sensor_wire
        );
        assert_ne!(
            brute_response[0].defense_wire,
            computed_response[0].defense_wire
        );
        assert_eq!(
            computed_response[0].sensor_wire,
            WireId(computed_sensor_entity)
        );
        assert_eq!(
            computed_response[0].defense_wire,
            WireId(computed_defense_entity)
        );

        computed.observation_bindings[0].name = "sensor.north.missing".to_owned();
        assert_eq!(
            resolve_reference_response_observations(
                &definition(),
                &computed,
                &computed_materialization,
                &computed_scenario,
            ),
            Err(ReferenceMetricError::MissingArchitectureBinding {
                role: "materialized",
                name: "sensor.north.0".to_owned(),
            })
        );
    }

    #[test]
    fn reducer_exactly_applies_window_power_heat_support_kills_and_latency_once() {
        let (mut collector, mut reports, source, sensor, defense, enemy) =
            collector_trace_fixture();
        collector
            .observe_sample(&reports[0], sample_for(&reports[0], 100))
            .expect("pre-window build Tick is retained but not reduced");
        set_exact_metric_rows(
            &mut reports[1],
            source,
            sensor,
            defense,
            enemy,
            10,
            10,
            5,
            6,
            1,
            3,
            5,
            (7, 8, 4, 2),
            false,
        );
        set_exact_metric_rows(
            &mut reports[2],
            source,
            sensor,
            defense,
            enemy,
            20,
            5,
            5,
            5,
            0,
            4,
            6,
            (3, 4, 3, 3),
            true,
        );
        let expected_used_integral = reports[1..]
            .iter()
            .map(|report| u128::from(report.network_accounting.expect("accounting").used().0))
            .sum::<u128>();
        let expected_support_integral = reports[1..]
            .iter()
            .map(|report| {
                u128::from(
                    report
                        .network_accounting
                        .expect("accounting")
                        .total_support_demand()
                        .expect("v5 support")
                        .0,
                )
            })
            .sum::<u128>();
        let expected_peak = reports[1..]
            .iter()
            .map(|report| report.network_accounting.expect("accounting").used())
            .max()
            .expect("measurement rows");
        let expected_final = reports[2].network_accounting.expect("accounting").used();
        collector
            .observe_sample(&reports[1], sample_for(&reports[1], 100))
            .expect("first measurement Tick");
        collector
            .observe_sample(&reports[2], sample_for(&reports[2], 90))
            .expect("second measurement Tick");
        let reduced = collector.finish().expect("complete exact reduction");

        assert_eq!(reduced.boundaries.final_next_tick, Tick(3));
        assert_eq!(
            reduced.runtime_metrics.measurement_start_core_integrity,
            Integrity(100)
        );
        assert_eq!(reduced.runtime_metrics.final_core_integrity, Integrity(90));
        assert_eq!(reduced.runtime_metrics.core_damage, Integrity(10));
        assert_eq!(reduced.runtime_metrics.power_generation, 30);
        assert_eq!(reduced.runtime_metrics.power_nominal_demand, 15);
        assert_eq!(reduced.runtime_metrics.power_granted, 10);
        assert_eq!(reduced.runtime_metrics.power_source_cost, 11);
        assert_eq!(reduced.runtime_metrics.power_transmission_loss, 1);
        assert_eq!(reduced.runtime_metrics.brownout_ticks, 1);
        assert_eq!(reduced.runtime_metrics.construction_requested, 10);
        assert_eq!(reduced.runtime_metrics.construction_nominal_power, 12);
        assert_eq!(reduced.runtime_metrics.construction_granted_work, 7);
        assert_eq!(reduced.runtime_metrics.construction_applied_work, 5);
        assert_eq!(reduced.runtime_metrics.heat_generated, 18);
        assert_eq!(reduced.runtime_metrics.network_peak_used_ncu, expected_peak);
        assert_eq!(
            reduced.runtime_metrics.network_final_used_ncu,
            expected_final
        );
        assert_eq!(
            reduced.runtime_metrics.network_integral_used_ncu,
            expected_used_integral
        );
        assert_eq!(
            reduced.runtime_metrics.support_demand_integral,
            expected_support_integral
        );
        assert!(expected_support_integral > 0);
        assert_eq!(reduced.runtime_metrics.enemy_kills, 1);
        assert_eq!(
            reduced.response_latency_ticks,
            vec![ReferenceResponseLatency {
                name: "north.0".to_owned(),
                stimulus_tick: Tick(1),
                response_tick: Tick(2),
                latency_ticks: 1,
            }]
        );
    }

    #[test]
    fn reducer_failure_is_atomic_and_order_faults_precede_numeric_overflow() {
        let (mut collector, mut reports, source, sensor, defense, enemy) =
            collector_trace_fixture();
        collector
            .observe_sample(&reports[0], sample_for(&reports[0], 100))
            .expect("pre-window Tick");
        set_exact_metric_rows(
            &mut reports[1],
            source,
            sensor,
            defense,
            enemy,
            1,
            1,
            1,
            1,
            0,
            1,
            1,
            (1, 1, 1, 1),
            false,
        );

        let mut missing_sensor = reports[1].clone();
        missing_sensor.power.as_mut().expect("power").sense.clear();
        assert_eq!(
            collector.observe_sample(&missing_sensor, sample_for(&missing_sensor, 100)),
            Err(ReferenceMetricError::MissingSensorObservation {
                name: "north.0".to_owned(),
                tick: Tick(1),
            })
        );
        assert_eq!(collector.expected_next_tick, Tick(1));
        assert_eq!(collector.power_generation, 0);
        assert_eq!(collector.heat_generated, 0);
        assert_eq!(collector.network_integral_used_ncu, 0);

        collector.power_generation = u128::MAX;
        let mut unordered = reports[1].clone();
        unordered
            .construction_work
            .push(unordered.construction_work[0]);
        assert_eq!(
            collector.observe_sample(&unordered, sample_for(&unordered, 100)),
            Err(ReferenceMetricError::NonCanonicalReportRows {
                tick: Tick(1),
                collection: "constructionWork",
            })
        );
        assert_eq!(collector.expected_next_tick, Tick(1));
        assert_eq!(collector.power_generation, u128::MAX);

        assert_eq!(
            collector.observe_sample(&reports[1], sample_for(&reports[1], 100)),
            Err(ReferenceMetricError::ArithmeticOverflow)
        );
        assert_eq!(collector.expected_next_tick, Tick(1));
        assert_eq!(collector.power_generation, u128::MAX);
        assert_eq!(collector.heat_generated, 0);
    }

    #[test]
    fn metric_artifact_hash_binds_new_ncu_core_baseline_and_runtime_fields() {
        let definition = definition();
        let baseline = ReferenceMetricArtifact::v1(
            &definition,
            ExperimentRunId::from_bytes([0x22; 32]),
            result(),
        )
        .expect("baseline artifact");
        let baseline_hash = baseline.semantic_hash(&definition).expect("baseline hash");

        let mut changed = baseline.clone();
        changed.result.static_inventory.total_wire_length_raw += 1;
        changed.result.static_inventory.total_wire_ncu.0 += 1;
        changed.result.static_inventory.sensor_wire_length_raw += 1;
        assert_ne!(
            changed
                .semantic_hash(&definition)
                .expect("length-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline.clone();
        changed
            .result
            .runtime_metrics
            .measurement_start_core_integrity
            .0 += 1;
        changed.result.runtime_metrics.core_damage.0 += 1;
        assert_ne!(
            changed
                .semantic_hash(&definition)
                .expect("core-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline.clone();
        changed.result.runtime_metrics.heat_generated += 1;
        assert_ne!(
            changed
                .semantic_hash(&definition)
                .expect("runtime-sensitive hash"),
            baseline_hash
        );

        let mut changed = baseline;
        changed.run_id = ExperimentRunId::from_bytes([0x23; 32]);
        assert_ne!(
            changed
                .semantic_hash(&definition)
                .expect("Run-ID-sensitive hash"),
            baseline_hash
        );
    }
}
