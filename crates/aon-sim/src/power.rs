use crate::{
    DriveStrength, Energy, EntityId, FIXED_ONE, Fixed, FixedVec2, HeatEnergy, PowerSourceId, Tick,
    TopologyNodeId, WireId,
};
use std::collections::BTreeSet;
use thiserror::Error;

const POWER_RATIO_ONE_RAW: u64 = FIXED_ONE as u64;

/// The fixed, profile-independent number of upper-mid comparisons needed to search every
/// representable Power ratio in the inclusive `0..=FIXED_ONE` range.
pub const POWER_RATIO_SOLVER_COMPARISONS: usize = {
    let mut covered_values = 1_u64;
    let mut comparisons = 0_usize;
    while covered_values < POWER_RATIO_ONE_RAW + 1 {
        covered_values *= 2;
        comparisons += 1;
    }
    comparisons
};

/// A canonical fixed-point Power grant ratio constrained to the closed interval `[0, 1]`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerRatio(Fixed);

impl PowerRatio {
    pub const ZERO: Self = Self(Fixed::ZERO);
    pub const ONE: Self = Self(Fixed::ONE);

    pub fn new(value: Fixed) -> Result<Self, PowerError> {
        if (0..=FIXED_ONE).contains(&value.0) {
            Ok(Self(value))
        } else {
            Err(PowerError::RatioOutOfRange { raw: value.0 })
        }
    }

    pub const fn as_fixed(self) -> Fixed {
        self.0
    }

    pub const fn raw(self) -> i64 {
        self.0.0
    }

    const fn from_valid_raw(raw: u64) -> Self {
        Self(Fixed(raw as i64))
    }
}

impl TryFrom<Fixed> for PowerRatio {
    type Error = PowerError;

    fn try_from(value: Fixed) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Stable, derived identity for one kind of load owned by one Entity.
///
/// The field order is semantic: derived demands sort first by owner and then by the frozen
/// `DemandKind` tag. This ID is never allocated from canonical Entity state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemandId {
    owner: EntityId,
    kind: DemandKind,
}

impl DemandId {
    pub const fn new(owner: EntityId, kind: DemandKind) -> Self {
        Self { owner, kind }
    }

    pub const fn owner(self) -> EntityId {
        self.owner
    }

    pub const fn kind(self) -> DemandKind {
        self.kind
    }
}

/// Frozen TRD order for Power demand kinds. New kinds must only be appended.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandKind {
    GateIdle = 0,
    GateSwitch = 1,
    GateDrive = 2,
    WireLeakage = 3,
    WireSensing = 4,
    LiveWire = 5,
    OvercapacitySupport = 6,
    RelayActivation = 7,
    RelayUpkeep = 8,
    Movement = 9,
    Extraction = 10,
    Transfer = 11,
    Construction = 12,
    RadiationEmission = 13,
}

impl DemandKind {
    pub const fn canonical_tag(self) -> u8 {
        self as u8
    }
}

/// A derived Power-region identifier. Region membership is compiled scratch, not canonical state.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerRegionId(pub u64);

/// One explicit semantic path token used by the canonical Power-route tie break.
///
/// The numeric kind and local subtag are frozen by the graph adapter rather than inferred from
/// Rust enum layout. This keeps otherwise equal routes independent of adjacency insertion order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerPathToken {
    entity_kind_tag: u8,
    entity: EntityId,
    local_subtag: u8,
}

impl PowerPathToken {
    pub const fn new(entity_kind_tag: u8, entity: EntityId, local_subtag: u8) -> Self {
        Self {
            entity_kind_tag,
            entity,
            local_subtag,
        }
    }

    pub const fn entity_kind_tag(self) -> u8 {
        self.entity_kind_tag
    }

    pub const fn entity(self) -> EntityId {
        self.entity
    }

    pub const fn local_subtag(self) -> u8 {
        self.local_subtag
    }
}

/// The coefficient `numerator / denominator` used by the transmission-loss kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerLossCoefficient {
    numerator: u64,
    denominator: u64,
}

impl PowerLossCoefficient {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, PowerError> {
        if denominator == 0 {
            return Err(PowerError::ZeroLossCoefficientDenominator);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    pub const fn denominator(self) -> u64 {
        self.denominator
    }
}

/// Canonical route priority: length, straight-segment count, then the complete EntityId path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerRouteKey {
    total_length: Fixed,
    segment_count: u32,
    path_tokens: Vec<PowerPathToken>,
}

impl PowerRouteKey {
    pub fn new(
        total_length: Fixed,
        segment_count: u32,
        path_tokens: Vec<PowerPathToken>,
    ) -> Result<Self, PowerError> {
        if total_length.0 < 0 {
            return Err(PowerError::NonPositiveRouteLength {
                raw: total_length.0,
            });
        }
        if total_length.0 == 0 && segment_count != 0 || total_length.0 > 0 && segment_count == 0 {
            return Err(PowerError::EmptyRouteSegments);
        }
        if path_tokens.is_empty() {
            return Err(PowerError::EmptyRoutePathTokens);
        }
        Ok(Self {
            total_length,
            segment_count,
            path_tokens,
        })
    }

    pub const fn total_length(&self) -> Fixed {
        self.total_length
    }

    pub const fn segment_count(&self) -> u32 {
        self.segment_count
    }

    pub fn path_tokens(&self) -> &[PowerPathToken] {
        &self.path_tokens
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerRouteWire {
    wire: WireId,
    length: Fixed,
    segment_count: u32,
}

impl PowerRouteWire {
    pub fn new(wire: WireId, length: Fixed, segment_count: u32) -> Result<Self, PowerError> {
        if length.0 <= 0 {
            return Err(PowerError::NonPositiveRouteWireLength {
                wire,
                raw: length.0,
            });
        }
        if segment_count == 0 {
            return Err(PowerError::EmptyRouteWireSegments { wire });
        }
        Ok(Self {
            wire,
            length,
            segment_count,
        })
    }

    pub const fn wire(self) -> WireId {
        self.wire
    }

    pub const fn length(self) -> Fixed {
        self.length
    }

    pub const fn segment_count(self) -> u32 {
        self.segment_count
    }
}

/// A compiler-owned route from one load attachment to one Power Source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalPowerRoute {
    source: PowerSourceId,
    key: PowerRouteKey,
    wires: Vec<PowerRouteWire>,
}

impl CanonicalPowerRoute {
    pub fn new(
        source: PowerSourceId,
        key: PowerRouteKey,
        wires: Vec<PowerRouteWire>,
    ) -> Result<Self, PowerError> {
        let mut seen = BTreeSet::new();
        let mut summed_length = 0_i64;
        let mut summed_segments = 0_u32;
        for route_wire in &wires {
            if route_wire.length.0 <= 0 {
                return Err(PowerError::NonPositiveRouteWireLength {
                    wire: route_wire.wire,
                    raw: route_wire.length.0,
                });
            }
            if !seen.insert(route_wire.wire) {
                return Err(PowerError::DuplicateRouteWire {
                    wire: route_wire.wire,
                });
            }
            summed_length = summed_length
                .checked_add(route_wire.length.0)
                .ok_or(PowerError::NumericOverflow)?;
            summed_segments = summed_segments
                .checked_add(route_wire.segment_count)
                .ok_or(PowerError::NumericOverflow)?;
        }
        if summed_length != key.total_length.0 {
            return Err(PowerError::RouteLengthMismatch {
                declared_raw: key.total_length.0,
                summed_raw: summed_length,
            });
        }
        if summed_segments != key.segment_count {
            return Err(PowerError::RouteSegmentCountMismatch {
                declared: key.segment_count,
                summed: summed_segments,
            });
        }
        Ok(Self { source, key, wires })
    }

    pub const fn source(&self) -> PowerSourceId {
        self.source
    }

    pub const fn key(&self) -> &PowerRouteKey {
        &self.key
    }

    pub fn wires(&self) -> &[PowerRouteWire] {
        &self.wires
    }
}

/// Selects the unique minimum route without depending on input or adjacency iteration order.
pub fn select_canonical_source_route(
    routes: &[CanonicalPowerRoute],
) -> Result<Option<&CanonicalPowerRoute>, PowerError> {
    let mut ordered: Vec<_> = routes.iter().collect();
    ordered.sort_unstable_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then_with(|| left.source.cmp(&right.source))
    });
    if ordered
        .windows(2)
        .any(|pair| pair[0].key == pair[1].key && pair[0].source == pair[1].source)
    {
        return Err(PowerError::DuplicateRoutePriority);
    }
    Ok(ordered.first().copied())
}

/// Canonical world state for a world-generator-owned Power Source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerSourceState {
    id: PowerSourceId,
    position: FixedVec2,
    generation_per_tick: Energy,
}

impl PowerSourceState {
    pub const fn new(id: PowerSourceId, position: FixedVec2, generation_per_tick: Energy) -> Self {
        Self {
            id,
            position,
            generation_per_tick,
        }
    }

    pub const fn id(self) -> PowerSourceId {
        self.id
    }

    pub const fn position(self) -> FixedVec2 {
        self.position
    }

    pub const fn power_attachment(self) -> TopologyNodeId {
        TopologyNodeId::PowerSourceAnchor(self.id)
    }

    pub const fn generation_per_tick(self) -> Energy {
        self.generation_per_tick
    }
}

/// A fully collected nominal demand. The optional route is absent only in a source-less region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerDemand {
    id: DemandId,
    region: PowerRegionId,
    nominal: Energy,
    source_route: Option<CanonicalPowerRoute>,
}

impl PowerDemand {
    pub fn new(
        owner: EntityId,
        kind: DemandKind,
        region: PowerRegionId,
        nominal: Energy,
        source_route: Option<CanonicalPowerRoute>,
    ) -> Self {
        Self {
            id: DemandId::new(owner, kind),
            region,
            nominal,
            source_route,
        }
    }

    pub const fn id(&self) -> DemandId {
        self.id
    }

    pub const fn owner(&self) -> EntityId {
        self.id.owner
    }

    pub const fn kind(&self) -> DemandKind {
        self.id.kind
    }

    pub const fn region(&self) -> PowerRegionId {
        self.region
    }

    pub const fn nominal(&self) -> Energy {
        self.nominal
    }

    pub const fn source_route(&self) -> Option<&CanonicalPowerRoute> {
        self.source_route.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerGrant {
    demand_id: DemandId,
    granted: Energy,
    ratio: PowerRatio,
    transmission_loss: Energy,
    source_cost: Energy,
}

impl PowerGrant {
    pub const fn demand_id(self) -> DemandId {
        self.demand_id
    }

    pub const fn granted(self) -> Energy {
        self.granted
    }

    pub const fn ratio(self) -> PowerRatio {
        self.ratio
    }

    pub const fn transmission_loss(self) -> Energy {
        self.transmission_loss
    }

    pub const fn source_cost(self) -> Energy {
        self.source_cost
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerHeatContribution {
    demand_id: DemandId,
    wire: WireId,
    heat_energy: HeatEnergy,
}

impl PowerHeatContribution {
    pub const fn demand_id(self) -> DemandId {
        self.demand_id
    }

    pub const fn wire(self) -> WireId {
        self.wire
    }

    pub const fn heat_energy(self) -> HeatEnergy {
        self.heat_energy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerRegionSolution {
    region: PowerRegionId,
    generation: Energy,
    ratio: PowerRatio,
    grants: Vec<PowerGrant>,
    transmission_heat: Vec<PowerHeatContribution>,
}

impl PowerRegionSolution {
    pub const fn region(&self) -> PowerRegionId {
        self.region
    }

    pub const fn generation(&self) -> Energy {
        self.generation
    }

    pub const fn ratio(&self) -> PowerRatio {
        self.ratio
    }

    pub fn grants(&self) -> &[PowerGrant] {
        &self.grants
    }

    pub fn transmission_heat(&self) -> &[PowerHeatContribution] {
        &self.transmission_heat
    }
}

/// Solves one derived region using exactly one common ratio for every pre-collected demand.
///
/// `sources` and `demands` may be presented in any order. The returned grants and heat rows are
/// stable by `DemandId`, with per-route heat rows additionally stable by `WireId`.
pub fn solve_power_region(
    region: PowerRegionId,
    sources: &[PowerSourceState],
    demands: &[PowerDemand],
    loss_coefficient: PowerLossCoefficient,
) -> Result<PowerRegionSolution, PowerError> {
    let mut source_ids = BTreeSet::new();
    let mut generation = Energy(0);
    for source in sources {
        if !source_ids.insert(source.id) {
            return Err(PowerError::DuplicatePowerSource {
                power_source: source.id,
            });
        }
        generation = generation
            .checked_add(source.generation_per_tick)
            .map_err(|_| PowerError::NumericOverflow)?;
    }

    let mut ordered_demands: Vec<_> = demands.iter().collect();
    ordered_demands.sort_unstable_by_key(|demand| demand.id);
    for pair in ordered_demands.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(PowerError::DuplicateDemandId { demand: pair[0].id });
        }
    }
    for demand in &ordered_demands {
        if demand.region != region {
            return Err(PowerError::DemandRegionMismatch {
                demand: demand.id,
                expected: region,
                actual: demand.region,
            });
        }
        match (sources.is_empty(), demand.source_route.as_ref()) {
            (true, None) => {}
            (true, Some(_)) => {
                return Err(PowerError::UnexpectedSourceRoute { demand: demand.id });
            }
            (false, None) => {
                return Err(PowerError::MissingSourceRoute { demand: demand.id });
            }
            (false, Some(route)) if !source_ids.contains(&route.source) => {
                return Err(PowerError::UnknownRouteSource {
                    demand: demand.id,
                    power_source: route.source,
                });
            }
            (false, Some(_)) => {}
        }
    }

    let ratio = if sources.is_empty() || generation.0 == 0 {
        PowerRatio::ZERO
    } else {
        solve_ratio(&ordered_demands, generation, loss_coefficient)?
    };

    let mut grants = Vec::with_capacity(ordered_demands.len());
    let mut transmission_heat = Vec::new();
    for demand in ordered_demands {
        let granted = scale_energy(demand.nominal, ratio)?;
        let transmission_loss = match demand.source_route.as_ref() {
            Some(route) => transmission_loss(route.key.total_length, granted, loss_coefficient)?,
            None => Energy(0),
        };
        let source_cost = granted
            .checked_add(transmission_loss)
            .map_err(|_| PowerError::NumericOverflow)?;
        if let Some(route) = demand.source_route.as_ref() {
            transmission_heat.extend(distribute_transmission_heat(
                demand.id,
                transmission_loss,
                route,
            )?);
        }
        grants.push(PowerGrant {
            demand_id: demand.id,
            granted,
            ratio,
            transmission_loss,
            source_cost,
        });
    }

    Ok(PowerRegionSolution {
        region,
        generation,
        ratio,
        grants,
        transmission_heat,
    })
}

fn solve_ratio(
    demands: &[&PowerDemand],
    generation: Energy,
    coefficient: PowerLossCoefficient,
) -> Result<PowerRatio, PowerError> {
    let mut lower = 0_u64;
    let mut upper = POWER_RATIO_ONE_RAW;
    for _ in 0..POWER_RATIO_SOLVER_COMPARISONS {
        let midpoint = lower + (upper - lower).div_ceil(2);
        let ratio = PowerRatio::from_valid_raw(midpoint);
        if source_cost_for_ratio(demands, ratio, coefficient)?.0 <= generation.0 {
            lower = midpoint;
        } else {
            upper = midpoint
                .checked_sub(1)
                .ok_or(PowerError::SolverInvariantViolation)?;
        }
    }
    if lower != upper {
        return Err(PowerError::SolverInvariantViolation);
    }
    Ok(PowerRatio::from_valid_raw(lower))
}

fn source_cost_for_ratio(
    demands: &[&PowerDemand],
    ratio: PowerRatio,
    coefficient: PowerLossCoefficient,
) -> Result<Energy, PowerError> {
    let mut total = Energy(0);
    for demand in demands {
        let granted = scale_energy(demand.nominal, ratio)?;
        let route = demand
            .source_route
            .as_ref()
            .ok_or(PowerError::MissingSourceRoute { demand: demand.id })?;
        let loss = transmission_loss(route.key.total_length, granted, coefficient)?;
        total = total
            .checked_add(granted)
            .and_then(|value| value.checked_add(loss))
            .map_err(|_| PowerError::NumericOverflow)?;
    }
    Ok(total)
}

/// `RNE(nominal * rho / FIXED_ONE)` for a logic or Sense Driver strength.
pub fn scale_drive(nominal: DriveStrength, ratio: PowerRatio) -> Result<DriveStrength, PowerError> {
    scale_u64_rne(nominal.0, ratio).map(DriveStrength)
}

/// `RNE(nominal * rho / FIXED_ONE)` for nonnegative movement distance.
pub fn scale_movement(nominal: Fixed, ratio: PowerRatio) -> Result<Fixed, PowerError> {
    let nominal_raw = u64::try_from(nominal.0)
        .map_err(|_| PowerError::NegativeMovementBudget { raw: nominal.0 })?;
    let scaled = scale_u64_rne(nominal_raw, ratio)?;
    i64::try_from(scaled)
        .map(Fixed)
        .map_err(|_| PowerError::NumericOverflow)
}

/// `RNE(nominal * rho / FIXED_ONE)` for granted work.
pub fn scale_work(nominal: Energy, ratio: PowerRatio) -> Result<Energy, PowerError> {
    scale_energy(nominal, ratio)
}

/// Applies only the frozen brownout factor to an already-computed nominal gate delay.
pub fn brownout_gate_delay(
    nominal_delay: Tick,
    ratio: PowerRatio,
    brownout_delay_floor: PowerRatio,
) -> Result<Tick, PowerError> {
    let floor_raw = u64::try_from(brownout_delay_floor.raw())
        .map_err(|_| PowerError::InvalidBrownoutDelayFloor)?;
    if floor_raw == 0 {
        return Err(PowerError::InvalidBrownoutDelayFloor);
    }
    let ratio_raw =
        u64::try_from(ratio.raw()).map_err(|_| PowerError::RatioOutOfRange { raw: ratio.raw() })?;
    let denominator = u128::from(ratio_raw.max(floor_raw));
    let numerator = u128::from(nominal_delay.0)
        .checked_mul(u128::from(POWER_RATIO_ONE_RAW))
        .ok_or(PowerError::NumericOverflow)?;
    let quotient = ceil_div_u128(numerator, denominator)?;
    u64::try_from(quotient.max(1))
        .map(Tick)
        .map_err(|_| PowerError::NumericOverflow)
}

fn scale_energy(nominal: Energy, ratio: PowerRatio) -> Result<Energy, PowerError> {
    scale_u64_rne(nominal.0, ratio).map(Energy)
}

fn scale_u64_rne(nominal: u64, ratio: PowerRatio) -> Result<u64, PowerError> {
    let ratio_raw =
        u64::try_from(ratio.raw()).map_err(|_| PowerError::RatioOutOfRange { raw: ratio.raw() })?;
    let numerator = u128::from(nominal)
        .checked_mul(u128::from(ratio_raw))
        .ok_or(PowerError::NumericOverflow)?;
    let rounded = round_div_nearest_even_u128(numerator, u128::from(POWER_RATIO_ONE_RAW))?;
    u64::try_from(rounded).map_err(|_| PowerError::NumericOverflow)
}

/// Computes `ceil(K_num * distance_raw * delivered^2 / (K_den * FIXED_ONE))`.
pub fn transmission_loss(
    distance: Fixed,
    delivered: Energy,
    coefficient: PowerLossCoefficient,
) -> Result<Energy, PowerError> {
    let distance_raw = u64::try_from(distance.0)
        .map_err(|_| PowerError::NegativeTransmissionDistance { raw: distance.0 })?;
    if coefficient.numerator == 0 || distance_raw == 0 || delivered.0 == 0 {
        return Ok(Energy(0));
    }

    let numerator = U256::from_u64(coefficient.numerator)
        .checked_mul_u64(distance_raw)?
        .checked_mul_u64(delivered.0)?
        .checked_mul_u64(delivered.0)?;
    let denominator =
        U256::from_u64(coefficient.denominator).checked_mul_u64(POWER_RATIO_ONE_RAW)?;
    numerator.ceil_div_to_u64(denominator).map(Energy)
}

/// Distributes a route's transmission loss by wire length, then assigns each remaining raw unit
/// once in ascending WireId order.
pub fn distribute_transmission_heat(
    demand_id: DemandId,
    loss: Energy,
    route: &CanonicalPowerRoute,
) -> Result<Vec<PowerHeatContribution>, PowerError> {
    if loss.0 == 0 {
        return Ok(Vec::new());
    }
    let route_length = u128::try_from(route.key.total_length.0).map_err(|_| {
        PowerError::NonPositiveRouteLength {
            raw: route.key.total_length.0,
        }
    })?;
    if route_length == 0 {
        return Err(PowerError::NonPositiveRouteLength { raw: 0 });
    }

    let mut ordered_wires = route.wires.clone();
    ordered_wires.sort_unstable_by_key(|wire| wire.wire);
    let mut rows = Vec::with_capacity(ordered_wires.len());
    let mut distributed = 0_u64;
    for route_wire in ordered_wires {
        let wire_length = u128::try_from(route_wire.length.0).map_err(|_| {
            PowerError::NonPositiveRouteWireLength {
                wire: route_wire.wire,
                raw: route_wire.length.0,
            }
        })?;
        let numerator = u128::from(loss.0)
            .checked_mul(wire_length)
            .ok_or(PowerError::NumericOverflow)?;
        let share =
            u64::try_from(numerator / route_length).map_err(|_| PowerError::NumericOverflow)?;
        distributed = distributed
            .checked_add(share)
            .ok_or(PowerError::NumericOverflow)?;
        rows.push(PowerHeatContribution {
            demand_id,
            wire: route_wire.wire,
            heat_energy: HeatEnergy(share),
        });
    }
    let remainder = loss
        .0
        .checked_sub(distributed)
        .ok_or(PowerError::SolverInvariantViolation)?;
    let remainder = usize::try_from(remainder).map_err(|_| PowerError::NumericOverflow)?;
    if remainder > rows.len() {
        return Err(PowerError::SolverInvariantViolation);
    }
    for row in rows.iter_mut().take(remainder) {
        row.heat_energy = row
            .heat_energy
            .checked_add(HeatEnergy(1))
            .map_err(|_| PowerError::NumericOverflow)?;
    }
    Ok(rows)
}

fn round_div_nearest_even_u128(numerator: u128, denominator: u128) -> Result<u128, PowerError> {
    if denominator == 0 {
        return Err(PowerError::InvalidNumericDivisor);
    }
    let floor = numerator / denominator;
    let remainder = numerator % denominator;
    match remainder.cmp(&(denominator - remainder)) {
        std::cmp::Ordering::Less => Ok(floor),
        std::cmp::Ordering::Greater => floor.checked_add(1).ok_or(PowerError::NumericOverflow),
        std::cmp::Ordering::Equal if floor.is_multiple_of(2) => Ok(floor),
        std::cmp::Ordering::Equal => floor.checked_add(1).ok_or(PowerError::NumericOverflow),
    }
}

fn ceil_div_u128(numerator: u128, denominator: u128) -> Result<u128, PowerError> {
    if denominator == 0 {
        return Err(PowerError::InvalidNumericDivisor);
    }
    let quotient = numerator / denominator;
    if numerator.is_multiple_of(denominator) {
        Ok(quotient)
    } else {
        quotient.checked_add(1).ok_or(PowerError::NumericOverflow)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct U256([u64; 4]);

impl U256 {
    const fn from_u64(value: u64) -> Self {
        Self([value, 0, 0, 0])
    }

    fn checked_mul_u64(self, multiplier: u64) -> Result<Self, PowerError> {
        let mut output = [0_u64; 4];
        let mut carry = 0_u128;
        for (index, slot) in output.iter_mut().enumerate() {
            let product = u128::from(self.0[index])
                .checked_mul(u128::from(multiplier))
                .and_then(|value| value.checked_add(carry))
                .ok_or(PowerError::NumericOverflow)?;
            *slot = product as u64;
            carry = product >> 64;
        }
        if carry == 0 {
            Ok(Self(output))
        } else {
            Err(PowerError::NumericOverflow)
        }
    }

    fn ceil_div_to_u64(self, denominator: Self) -> Result<u64, PowerError> {
        if denominator == Self::default() {
            return Err(PowerError::InvalidNumericDivisor);
        }
        if self == Self::default() {
            return Ok(0);
        }
        let maximum = denominator.checked_mul_u64(u64::MAX)?;
        if self > maximum {
            return Err(PowerError::NumericOverflow);
        }

        let mut lower = 0_u64;
        let mut upper = u64::MAX;
        while lower < upper {
            let midpoint = lower + (upper - lower).div_ceil(2);
            if denominator.checked_mul_u64(midpoint)? <= self {
                lower = midpoint;
            } else {
                upper = midpoint - 1;
            }
        }
        if denominator.checked_mul_u64(lower)? == self {
            Ok(lower)
        } else {
            lower.checked_add(1).ok_or(PowerError::NumericOverflow)
        }
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PowerError {
    #[error("Power ratio raw value {raw} is outside 0..=FIXED_ONE")]
    RatioOutOfRange { raw: i64 },

    #[error("Power transmission-loss coefficient denominator must be positive")]
    ZeroLossCoefficientDenominator,

    #[error("Power route length must be nonnegative, got raw {raw}")]
    NonPositiveRouteLength { raw: i64 },

    #[error("Power route must contain at least one straight segment")]
    EmptyRouteSegments,

    #[error("Power route must contain a complete semantic path-token sequence")]
    EmptyRoutePathTokens,

    #[error("Power route Wire {wire:?} length must be positive, got raw {raw}")]
    NonPositiveRouteWireLength { wire: WireId, raw: i64 },

    #[error("Power route Wire {wire:?} must overlap at least one original straight segment")]
    EmptyRouteWireSegments { wire: WireId },

    #[error("Power route contains Wire {wire:?} more than once")]
    DuplicateRouteWire { wire: WireId },

    #[error("Power route length mismatch: declared raw {declared_raw}, summed raw {summed_raw}")]
    RouteLengthMismatch { declared_raw: i64, summed_raw: i64 },

    #[error("Power route segment-count mismatch: declared {declared}, summed {summed}")]
    RouteSegmentCountMismatch { declared: u32, summed: u32 },

    #[error("two candidate Power routes have the same complete canonical priority")]
    DuplicateRoutePriority,

    #[error("duplicate Power Source {power_source:?} in one region solve")]
    DuplicatePowerSource { power_source: PowerSourceId },

    #[error("duplicate derived Power DemandId {demand:?}")]
    DuplicateDemandId { demand: DemandId },

    #[error("Power demand {demand:?} belongs to region {actual:?}, expected region {expected:?}")]
    DemandRegionMismatch {
        demand: DemandId,
        expected: PowerRegionId,
        actual: PowerRegionId,
    },

    #[error("Power demand {demand:?} has a source route in a source-less region")]
    UnexpectedSourceRoute { demand: DemandId },

    #[error("Power demand {demand:?} is missing its canonical source route")]
    MissingSourceRoute { demand: DemandId },

    #[error("Power demand {demand:?} route names unknown source {power_source:?}")]
    UnknownRouteSource {
        demand: DemandId,
        power_source: PowerSourceId,
    },

    #[error("movement budget must be nonnegative, got raw {raw}")]
    NegativeMovementBudget { raw: i64 },

    #[error("transmission distance must be nonnegative, got raw {raw}")]
    NegativeTransmissionDistance { raw: i64 },

    #[error("brownout delay floor must be greater than zero")]
    InvalidBrownoutDelayFloor,

    #[error("canonical numeric divisor must be positive")]
    InvalidNumericDivisor,

    #[error("canonical numeric overflow")]
    NumericOverflow,

    #[error("deterministic Power solver invariant violated")]
    SolverInvariantViolation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PowerSourceId;

    fn source(id: u64, generation: u64) -> PowerSourceState {
        PowerSourceState::new(
            PowerSourceId(EntityId(id)),
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            Energy(generation),
        )
    }

    fn route(source_id: u64, wires: &[(u64, i64)]) -> CanonicalPowerRoute {
        let total_length = wires.iter().map(|(_, length)| *length).sum();
        CanonicalPowerRoute::new(
            PowerSourceId(EntityId(source_id)),
            PowerRouteKey::new(
                Fixed(total_length),
                u32::try_from(wires.len()).expect("fixture segment count fits u32"),
                wires
                    .iter()
                    .map(|(id, _)| PowerPathToken::new(3, EntityId(*id), 0))
                    .collect(),
            )
            .expect("fixture key is valid"),
            wires
                .iter()
                .map(|(id, length)| {
                    PowerRouteWire::new(WireId(EntityId(*id)), Fixed(*length), 1)
                        .expect("fixture route Wire is valid")
                })
                .collect(),
        )
        .expect("fixture route is valid")
    }

    fn demand(
        owner: u64,
        kind: DemandKind,
        nominal: u64,
        route: Option<CanonicalPowerRoute>,
    ) -> PowerDemand {
        PowerDemand::new(
            EntityId(owner),
            kind,
            PowerRegionId(7),
            Energy(nominal),
            route,
        )
    }

    #[test]
    fn power_ratio_is_closed_and_solver_width_is_profile_independent() {
        assert_eq!(
            PowerRatio::new(Fixed(-1)),
            Err(PowerError::RatioOutOfRange { raw: -1 })
        );
        assert_eq!(PowerRatio::new(Fixed::ZERO), Ok(PowerRatio::ZERO));
        assert_eq!(PowerRatio::new(Fixed::ONE), Ok(PowerRatio::ONE));
        assert_eq!(
            PowerRatio::new(Fixed(FIXED_ONE + 1)),
            Err(PowerError::RatioOutOfRange { raw: FIXED_ONE + 1 })
        );
        assert_eq!(POWER_RATIO_SOLVER_COMPARISONS, 17);
    }

    #[test]
    fn demand_kind_tags_and_ids_preserve_the_frozen_order() {
        let kinds = [
            DemandKind::GateIdle,
            DemandKind::GateSwitch,
            DemandKind::GateDrive,
            DemandKind::WireLeakage,
            DemandKind::WireSensing,
            DemandKind::LiveWire,
            DemandKind::OvercapacitySupport,
            DemandKind::RelayActivation,
            DemandKind::RelayUpkeep,
            DemandKind::Movement,
            DemandKind::Extraction,
            DemandKind::Transfer,
            DemandKind::Construction,
            DemandKind::RadiationEmission,
        ];
        assert_eq!(
            kinds.map(DemandKind::canonical_tag),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
        );

        let mut ids = vec![
            DemandId::new(EntityId(2), DemandKind::GateIdle),
            DemandId::new(EntityId(1), DemandKind::Movement),
            DemandId::new(EntityId(1), DemandKind::GateIdle),
        ];
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec![
                DemandId::new(EntityId(1), DemandKind::GateIdle),
                DemandId::new(EntityId(1), DemandKind::Movement),
                DemandId::new(EntityId(2), DemandKind::GateIdle),
            ]
        );
    }

    #[test]
    fn canonical_source_route_uses_length_segments_then_entity_path() {
        let long = route(50, &[(8, 4), (9, 3)]);
        let many_segments = route(50, &[(4, 3), (5, 3)]);
        let lexicographically_late = route(50, &[(3, 6)]);
        let winner = route(50, &[(2, 6)]);

        let candidates = vec![long, many_segments, lexicographically_late, winner.clone()];
        assert_eq!(
            select_canonical_source_route(&candidates)
                .expect("priorities are unique")
                .expect("a route exists"),
            &winner
        );
        let reversed: Vec<_> = candidates.into_iter().rev().collect();
        assert_eq!(
            select_canonical_source_route(&reversed)
                .expect("priorities are unique")
                .expect("a route exists"),
            &winner
        );
    }

    #[test]
    fn identical_path_to_distinct_sources_uses_source_id_as_the_final_tie_break() {
        let higher_source = route(51, &[(2, 6)]);
        let lower_source = route(50, &[(2, 6)]);
        let candidates = [higher_source, lower_source.clone()];
        assert_eq!(
            select_canonical_source_route(&candidates),
            Ok(Some(&lower_source))
        );
    }

    #[test]
    fn duplicate_complete_route_and_source_is_rejected_instead_of_using_input_order() {
        let first = route(50, &[(2, 6)]);
        let duplicate = route(50, &[(2, 6)]);
        assert_eq!(
            select_canonical_source_route(&[first, duplicate]),
            Err(PowerError::DuplicateRoutePriority)
        );
    }

    #[test]
    fn grant_kernels_use_nearest_even_and_gate_delay_uses_ceil() {
        let half = PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("half is in range");
        assert_eq!(scale_drive(DriveStrength(1), half), Ok(DriveStrength(0)));
        assert_eq!(scale_drive(DriveStrength(3), half), Ok(DriveStrength(2)));
        assert_eq!(scale_work(Energy(5), half), Ok(Energy(2)));
        assert_eq!(scale_work(Energy(7), half), Ok(Energy(4)));
        assert_eq!(scale_movement(Fixed(5), half), Ok(Fixed(2)));
        assert_eq!(scale_movement(Fixed(7), half), Ok(Fixed(4)));
        assert_eq!(
            scale_movement(Fixed(-1), half),
            Err(PowerError::NegativeMovementBudget { raw: -1 })
        );

        let quarter = PowerRatio::new(Fixed(FIXED_ONE / 4)).expect("quarter is in range");
        assert_eq!(
            brownout_gate_delay(Tick(3), PowerRatio::ONE, quarter),
            Ok(Tick(3))
        );
        assert_eq!(brownout_gate_delay(Tick(3), half, quarter), Ok(Tick(6)));
        assert_eq!(
            brownout_gate_delay(Tick(3), PowerRatio::ZERO, quarter),
            Ok(Tick(12))
        );
        assert_eq!(
            brownout_gate_delay(Tick(0), PowerRatio::ONE, quarter),
            Ok(Tick(1))
        );
        assert_eq!(
            brownout_gate_delay(Tick(1), PowerRatio::ZERO, PowerRatio::ZERO),
            Err(PowerError::InvalidBrownoutDelayFloor)
        );
    }

    #[test]
    fn transmission_loss_uses_the_frozen_units_and_ceil_rounding() {
        let one_tenth = PowerLossCoefficient::new(1, 10).expect("positive denominator");
        assert_eq!(
            transmission_loss(Fixed(2 * FIXED_ONE), Energy(5), one_tenth),
            Ok(Energy(5))
        );
        let one_third = PowerLossCoefficient::new(1, 3).expect("positive denominator");
        assert_eq!(
            transmission_loss(Fixed(FIXED_ONE), Energy(2), one_third),
            Ok(Energy(2))
        );
        assert_eq!(
            transmission_loss(Fixed(FIXED_ONE), Energy(0), one_third),
            Ok(Energy(0))
        );
        assert_eq!(
            transmission_loss(
                Fixed(FIXED_ONE),
                Energy(u64::MAX),
                PowerLossCoefficient::new(u64::MAX, 1).expect("positive denominator"),
            ),
            Err(PowerError::NumericOverflow)
        );
    }

    #[test]
    fn solver_finds_exact_full_half_and_source_less_ratios() {
        let coefficient = PowerLossCoefficient::new(0, 1).expect("positive denominator");
        let connected = demand(
            1,
            DemandKind::GateDrive,
            POWER_RATIO_ONE_RAW,
            Some(route(10, &[(20, FIXED_ONE)])),
        );

        let full = solve_power_region(
            PowerRegionId(7),
            &[source(10, POWER_RATIO_ONE_RAW)],
            std::slice::from_ref(&connected),
            coefficient,
        )
        .expect("full-power solve succeeds");
        assert_eq!(full.ratio(), PowerRatio::ONE);
        assert_eq!(full.grants()[0].granted(), Energy(POWER_RATIO_ONE_RAW));

        let half = solve_power_region(
            PowerRegionId(7),
            &[source(10, POWER_RATIO_ONE_RAW / 2)],
            std::slice::from_ref(&connected),
            coefficient,
        )
        .expect("half-power solve succeeds");
        assert_eq!(half.ratio().raw(), FIXED_ONE / 2);
        assert_eq!(half.grants()[0].granted(), Energy(POWER_RATIO_ONE_RAW / 2));

        let disconnected = demand(1, DemandKind::GateDrive, 1, None);
        let no_source = solve_power_region(PowerRegionId(7), &[], &[disconnected], coefficient)
            .expect("source-less region is a valid zero grant");
        assert_eq!(no_source.ratio(), PowerRatio::ZERO);
        assert_eq!(no_source.grants()[0].granted(), Energy(0));
    }

    #[test]
    fn solver_is_input_order_independent_and_returns_demand_id_order() {
        let coefficient = PowerLossCoefficient::new(0, 1).expect("positive denominator");
        let first = demand(
            2,
            DemandKind::Movement,
            POWER_RATIO_ONE_RAW,
            Some(route(10, &[(22, FIXED_ONE)])),
        );
        let second = demand(
            1,
            DemandKind::WireSensing,
            POWER_RATIO_ONE_RAW,
            Some(route(11, &[(21, FIXED_ONE)])),
        );
        let sources = [
            source(11, POWER_RATIO_ONE_RAW / 2),
            source(10, POWER_RATIO_ONE_RAW / 2),
        ];
        let forward = solve_power_region(
            PowerRegionId(7),
            &sources,
            &[first.clone(), second.clone()],
            coefficient,
        )
        .expect("forward solve succeeds");
        let reverse_sources = [sources[1], sources[0]];
        let reverse = solve_power_region(
            PowerRegionId(7),
            &reverse_sources,
            &[second, first],
            coefficient,
        )
        .expect("reverse solve succeeds");
        assert_eq!(forward, reverse);
        assert_eq!(
            forward
                .grants()
                .iter()
                .map(|grant| grant.demand_id())
                .collect::<Vec<_>>(),
            vec![
                DemandId::new(EntityId(1), DemandKind::WireSensing),
                DemandId::new(EntityId(2), DemandKind::Movement),
            ]
        );
    }

    #[test]
    fn transmission_heat_uses_floor_then_wire_id_remainder() {
        let demand_id = DemandId::new(EntityId(9), DemandKind::GateDrive);
        let route = route(
            50,
            &[(30, FIXED_ONE), (10, 2 * FIXED_ONE), (20, 3 * FIXED_ONE)],
        );
        let rows = distribute_transmission_heat(demand_id, Energy(10), &route)
            .expect("valid route distributes exactly");
        assert_eq!(
            rows.iter()
                .map(|row| (row.wire().entity_id().0, row.heat_energy().0))
                .collect::<Vec<_>>(),
            vec![(10, 4), (20, 5), (30, 1)]
        );
        assert_eq!(rows.iter().map(|row| row.heat_energy().0).sum::<u64>(), 10);
    }

    #[test]
    fn route_and_region_invariants_fail_closed() {
        let key = PowerRouteKey::new(Fixed(2), 1, vec![PowerPathToken::new(3, EntityId(1), 0)])
            .expect("key itself is valid");
        assert_eq!(
            CanonicalPowerRoute::new(
                PowerSourceId(EntityId(10)),
                key,
                vec![
                    PowerRouteWire::new(WireId(EntityId(1)), Fixed(1), 1)
                        .expect("positive Wire length")
                ],
            ),
            Err(PowerError::RouteLengthMismatch {
                declared_raw: 2,
                summed_raw: 1,
            })
        );

        let coefficient = PowerLossCoefficient::new(0, 1).expect("positive denominator");
        let wrong_region = PowerDemand::new(
            EntityId(1),
            DemandKind::GateIdle,
            PowerRegionId(8),
            Energy(1),
            Some(route(10, &[(20, FIXED_ONE)])),
        );
        assert!(matches!(
            solve_power_region(
                PowerRegionId(7),
                &[source(10, 1)],
                &[wrong_region],
                coefficient,
            ),
            Err(PowerError::DemandRegionMismatch { .. })
        ));

        let duplicate = demand(
            1,
            DemandKind::GateIdle,
            1,
            Some(route(10, &[(20, FIXED_ONE)])),
        );
        assert!(matches!(
            solve_power_region(
                PowerRegionId(7),
                &[source(10, 2)],
                &[duplicate.clone(), duplicate],
                coefficient,
            ),
            Err(PowerError::DuplicateDemandId { .. })
        ));
    }
}
