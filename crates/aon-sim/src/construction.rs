use crate::profile::{ConstructionProbeProfile, Rational};
use crate::{
    ConstructionSiteId, ConstructionSiteIndex, DemandKind, EndpointTarget, Energy, FIXED_ONE,
    FixedAabb, FixedVec2, GateType, MobileId, NominalPowerDemand, PowerError, PowerNodeKey,
    PowerRatio, RESERVED_ENTITY_ID, RoutingDomain, ceil_div_nonnegative, polyline_length,
    scale_work,
};
use thiserror::Error;

/// The exact primitive reserved by a measured Construction Site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstructionTarget {
    Gate {
        gate_type: GateType,
        origin: FixedVec2,
        routing_domain: RoutingDomain,
    },
    Wire {
        routing_domain: RoutingDomain,
        points: Vec<FixedVec2>,
        endpoint_a: EndpointTarget,
        endpoint_b: EndpointTarget,
    },
    Junction {
        routing_domain: RoutingDomain,
        position: FixedVec2,
    },
    FixedSubstrate {
        origin: FixedVec2,
        routing_area: FixedAabb,
        footprint: FixedAabb,
    },
}

/// Canonical progress owned by one Site entity. The activated primitive receives a fresh ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionSite {
    pub id: ConstructionSiteId,
    pub target: ConstructionTarget,
    pub required_work: Energy,
    pub completed_work: Energy,
    pub activation_ready: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConstructionSiteSlot {
    id: ConstructionSiteId,
    site: Option<ConstructionSite>,
}

/// Entity-ID ordered canonical Site slots. Removal leaves a tombstone and never reuses an ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstructionSiteStore {
    slots: Vec<ConstructionSiteSlot>,
}

impl ConstructionSiteStore {
    pub fn new(mut sites: Vec<ConstructionSite>) -> Result<Self, ConstructionError> {
        sites.sort_unstable_by_key(|site| site.id);
        if let Some(pair) = sites.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(ConstructionError::DuplicateSite { site: pair[0].id });
        }
        for site in &sites {
            validate_site(site)?;
        }
        Ok(Self {
            slots: sites
                .into_iter()
                .map(|site| ConstructionSiteSlot {
                    id: site.id,
                    site: Some(site),
                })
                .collect(),
        })
    }

    pub fn insert(&mut self, site: ConstructionSite) -> Result<(), ConstructionError> {
        self.insert_with_index(site).map(|_| ())
    }

    /// Appends one stable canonical slot and returns the dense registry location for it.
    ///
    /// Slots never move after insertion. Public iteration is independently sorted by Entity ID,
    /// so storage layout cannot affect canonical ordering.
    pub fn insert_with_index(
        &mut self,
        site: ConstructionSite,
    ) -> Result<ConstructionSiteIndex, ConstructionError> {
        validate_site(&site)?;
        if self.slots.iter().any(|slot| slot.id == site.id) {
            return Err(ConstructionError::DuplicateSite { site: site.id });
        }
        let index = u32::try_from(self.slots.len())
            .map(ConstructionSiteIndex)
            .map_err(|_| ConstructionError::StoreIndexExhausted)?;
        self.slots.push(ConstructionSiteSlot {
            id: site.id,
            site: Some(site),
        });
        Ok(index)
    }

    pub fn get(&self, id: ConstructionSiteId) -> Option<&ConstructionSite> {
        self.slots
            .iter()
            .find(|slot| slot.id == id)
            .and_then(|slot| slot.site.as_ref())
    }

    fn get_mut(&mut self, id: ConstructionSiteId) -> Option<&mut ConstructionSite> {
        self.slots
            .iter_mut()
            .find(|slot| slot.id == id)
            .and_then(|slot| slot.site.as_mut())
    }

    pub fn get_by_index(&self, index: ConstructionSiteIndex) -> Option<&ConstructionSite> {
        self.slots
            .get(index.0 as usize)
            .and_then(|slot| slot.site.as_ref())
    }

    pub fn remove(
        &mut self,
        id: ConstructionSiteId,
    ) -> Result<ConstructionSite, ConstructionError> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.id == id)
            .ok_or(ConstructionError::UnknownSite { site: id })?;
        self.slots[index]
            .site
            .take()
            .ok_or(ConstructionError::UnknownSite { site: id })
    }

    pub fn remove_by_index(
        &mut self,
        index: ConstructionSiteIndex,
    ) -> Result<ConstructionSite, ConstructionError> {
        let slot = self
            .slots
            .get_mut(index.0 as usize)
            .ok_or(ConstructionError::UnknownStoreIndex { index })?;
        slot.site
            .take()
            .ok_or(ConstructionError::UnknownSite { site: slot.id })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ConstructionSite> {
        let mut sites = self
            .slots
            .iter()
            .filter_map(|slot| slot.site.as_ref())
            .collect::<Vec<_>>();
        sites.sort_unstable_by_key(|site| site.id);
        sites.into_iter()
    }

    /// Visits every append-only slot, including tombstones, in stable dense-index order.
    pub fn iter_slots(
        &self,
    ) -> impl Iterator<Item = (ConstructionSiteIndex, Option<&ConstructionSite>)> {
        self.slots.iter().enumerate().filter_map(|(raw, slot)| {
            u32::try_from(raw)
                .ok()
                .map(|raw| (ConstructionSiteIndex(raw), slot.site.as_ref()))
        })
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.site.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

/// One Phase-8 builder contribution. Input order is deliberately non-semantic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionWorkContribution {
    pub site: ConstructionSiteId,
    pub builder: MobileId,
    pub granted_work: Energy,
}

/// Stable Phase-11 reduction result for one `(site, builder)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionProgressResult {
    pub site: ConstructionSiteId,
    pub builder: MobileId,
    pub granted_work: Energy,
    pub applied_work: Energy,
    pub completed_work: Energy,
    pub activation_ready: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ConstructionError {
    #[error("Construction target kind is unsupported")]
    UnsupportedTarget,

    #[error("Construction target {axis} extent must be positive, got raw {raw}")]
    NonPositiveExtent { axis: &'static str, raw: i64 },

    #[error("Construction Wire length must be positive, got raw {raw}")]
    NegativeLength { raw: i64 },

    #[error("canonical Construction arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Construction Work is outside the canonical u64 range: {value}")]
    WorkOutOfRange { value: u128 },

    #[error("duplicate Construction contribution for Site {site:?}, builder {builder:?}")]
    DuplicateContribution {
        site: ConstructionSiteId,
        builder: MobileId,
    },

    #[error("duplicate Construction Site {site:?}")]
    DuplicateSite { site: ConstructionSiteId },

    #[error("canonical Construction Site store index exhausted")]
    StoreIndexExhausted,

    #[error("unknown Construction Site store index {index:?}")]
    UnknownStoreIndex { index: ConstructionSiteIndex },

    #[error("unknown or removed Construction Site {site:?}")]
    UnknownSite { site: ConstructionSiteId },

    #[error("Construction Site {site:?} was already activation-ready before this Tick")]
    SiteAlreadyReady { site: ConstructionSiteId },

    #[error("builder {builder:?} has an invalid Construction Power attachment")]
    InvalidConstructionAttachment { builder: MobileId },

    #[error(transparent)]
    Power(#[from] PowerError),
}

/// Computes exact Work using one final ceiling after checked factor cancellation.
pub fn required_construction_work(
    target: &ConstructionTarget,
    probe: &ConstructionProbeProfile,
) -> Result<Energy, ConstructionError> {
    validate_target_shape(target)?;
    let work = match target {
        ConstructionTarget::Gate { gate_type, .. } => u128::from(match gate_type {
            GateType::And => probe.and_gate_work,
            GateType::Or => probe.or_gate_work,
            GateType::Not => probe.not_gate_work,
        }),
        ConstructionTarget::Junction { .. } => u128::from(probe.junction_base_work),
        ConstructionTarget::Wire { points, .. } => {
            let length =
                polyline_length(points).map_err(|_| ConstructionError::ArithmeticOverflow)?;
            if length.0 <= 0 {
                return Err(ConstructionError::NegativeLength { raw: length.0 });
            }
            let variable = ceil_rational_product(
                probe.wire_work_per_ncu,
                &[length.0 as u128],
                &[FIXED_ONE as u128],
            )?;
            u128::from(probe.wire_endpoint_work)
                .checked_add(variable)
                .ok_or(ConstructionError::ArithmeticOverflow)?
        }
        ConstructionTarget::FixedSubstrate { footprint, .. } => {
            let width = extent("width", footprint.min.x.0, footprint.max.x.0)?;
            let height = extent("height", footprint.min.y.0, footprint.max.y.0)?;
            ceil_rational_product(
                probe.substrate_work_per_square_wu,
                &[width, height],
                &[FIXED_ONE as u128, FIXED_ONE as u128],
            )?
        }
    };
    energy_from_positive_work(work)
}

/// Builds the full per-Tick Construction load for a builder.
pub fn construction_nominal_demand(
    site: ConstructionSiteId,
    builder: MobileId,
    attachment: PowerNodeKey,
    probe: &ConstructionProbeProfile,
) -> Result<NominalPowerDemand, ConstructionError> {
    construction_nominal_demand_for_work(
        site,
        builder,
        attachment,
        Energy(probe.builder_work_per_tick),
        probe,
    )
}

/// Variant used by Phase 4 after clamping requested Work to the Site's remaining Work.
pub fn construction_nominal_demand_for_work(
    site: ConstructionSiteId,
    builder: MobileId,
    attachment: PowerNodeKey,
    requested_work: Energy,
    probe: &ConstructionProbeProfile,
) -> Result<NominalPowerDemand, ConstructionError> {
    if site.entity_id() == RESERVED_ENTITY_ID || builder.entity_id() == RESERVED_ENTITY_ID {
        return Err(ConstructionError::InvalidConstructionAttachment { builder });
    }
    match attachment {
        PowerNodeKey::WireOffset(wire, offset)
            if wire.entity_id() != RESERVED_ENTITY_ID && offset.0 >= 0 => {}
        _ => return Err(ConstructionError::InvalidConstructionAttachment { builder }),
    }
    let nominal = ceil_rational_product(
        probe.construction_power_per_work,
        &[u128::from(requested_work.0)],
        &[],
    )?;
    let nominal = energy_from_positive_work(nominal)?;
    Ok(NominalPowerDemand::new(
        builder.entity_id(),
        DemandKind::Construction,
        nominal,
        attachment,
    ))
}

/// Delegates to the retained Power rounding kernel without introducing another rounding path.
pub fn grant_construction_work(
    nominal: Energy,
    ratio: PowerRatio,
) -> Result<Energy, ConstructionError> {
    scale_work(nominal, ratio).map_err(ConstructionError::Power)
}

/// Atomically applies contributions in canonical `(site, builder)` order.
pub fn apply_construction_work(
    sites: &mut ConstructionSiteStore,
    contributions: &[ConstructionWorkContribution],
) -> Result<Vec<ConstructionProgressResult>, ConstructionError> {
    let mut ordered = contributions.to_vec();
    ordered.sort_unstable_by_key(|row| (row.site, row.builder));
    if let Some(pair) = ordered
        .windows(2)
        .find(|pair| (pair[0].site, pair[0].builder) == (pair[1].site, pair[1].builder))
    {
        return Err(ConstructionError::DuplicateContribution {
            site: pair[0].site,
            builder: pair[0].builder,
        });
    }
    for row in &ordered {
        let site = sites
            .get(row.site)
            .ok_or(ConstructionError::UnknownSite { site: row.site })?;
        if site.activation_ready {
            return Err(ConstructionError::SiteAlreadyReady { site: row.site });
        }
    }

    let mut candidate = sites.clone();
    let mut results = Vec::with_capacity(ordered.len());
    for row in ordered {
        let site = candidate
            .get_mut(row.site)
            .ok_or(ConstructionError::UnknownSite { site: row.site })?;
        let remaining = site.required_work.0 - site.completed_work.0;
        let applied = row.granted_work.0.min(remaining);
        site.completed_work = site
            .completed_work
            .checked_add(Energy(applied))
            .map_err(|_| ConstructionError::ArithmeticOverflow)?;
        site.activation_ready = site.completed_work == site.required_work;
        results.push(ConstructionProgressResult {
            site: row.site,
            builder: row.builder,
            granted_work: row.granted_work,
            applied_work: Energy(applied),
            completed_work: site.completed_work,
            activation_ready: site.activation_ready,
        });
    }
    *sites = candidate;
    Ok(results)
}

fn validate_site(site: &ConstructionSite) -> Result<(), ConstructionError> {
    if site.id.entity_id() == RESERVED_ENTITY_ID {
        return Err(ConstructionError::UnknownSite { site: site.id });
    }
    validate_target_shape(&site.target)?;
    if site.required_work.0 == 0
        || site.completed_work.0 > site.required_work.0
        || site.activation_ready != (site.completed_work == site.required_work)
    {
        return Err(ConstructionError::WorkOutOfRange {
            value: u128::from(site.completed_work.0),
        });
    }
    Ok(())
}

fn validate_target_shape(target: &ConstructionTarget) -> Result<(), ConstructionError> {
    match target {
        ConstructionTarget::Wire { points, .. } => {
            if points.len() < 2 || points.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(ConstructionError::NegativeLength { raw: 0 });
            }
            let length =
                polyline_length(points).map_err(|_| ConstructionError::ArithmeticOverflow)?;
            if length.0 <= 0 {
                return Err(ConstructionError::NegativeLength { raw: length.0 });
            }
        }
        ConstructionTarget::FixedSubstrate {
            routing_area,
            footprint,
            ..
        } => {
            extent(
                "routingArea.width",
                routing_area.min.x.0,
                routing_area.max.x.0,
            )?;
            extent(
                "routingArea.height",
                routing_area.min.y.0,
                routing_area.max.y.0,
            )?;
            extent("footprint.width", footprint.min.x.0, footprint.max.x.0)?;
            extent("footprint.height", footprint.min.y.0, footprint.max.y.0)?;
        }
        ConstructionTarget::Gate { .. } | ConstructionTarget::Junction { .. } => {}
    }
    Ok(())
}

fn extent(axis: &'static str, min: i64, max: i64) -> Result<u128, ConstructionError> {
    let raw = i128::from(max) - i128::from(min);
    if raw <= 0 {
        return Err(ConstructionError::NonPositiveExtent {
            axis,
            raw: i64::try_from(raw).unwrap_or(i64::MIN),
        });
    }
    Ok(raw as u128)
}

fn energy_from_positive_work(value: u128) -> Result<Energy, ConstructionError> {
    if value == 0 || value > u128::from(u64::MAX) {
        return Err(ConstructionError::WorkOutOfRange { value });
    }
    Ok(Energy(value as u64))
}

fn ceil_rational_product(
    coefficient: Rational,
    numerator_factors: &[u128],
    extra_denominator_factors: &[u128],
) -> Result<u128, ConstructionError> {
    if coefficient.numerator() <= 0 || coefficient.denominator() <= 0 {
        return Err(ConstructionError::WorkOutOfRange { value: 0 });
    }
    let mut numerators = Vec::with_capacity(numerator_factors.len() + 1);
    numerators.push(coefficient.numerator() as u128);
    numerators.extend_from_slice(numerator_factors);
    let mut denominators = Vec::with_capacity(extra_denominator_factors.len() + 1);
    denominators.push(coefficient.denominator() as u128);
    denominators.extend_from_slice(extra_denominator_factors);
    if denominators.contains(&0) {
        return Err(ConstructionError::ArithmeticOverflow);
    }
    for numerator in &mut numerators {
        for denominator in &mut denominators {
            let divisor = gcd(*numerator, *denominator);
            *numerator /= divisor;
            *denominator /= divisor;
        }
    }
    let numerator = numerators.into_iter().try_fold(1_u128, |product, factor| {
        product
            .checked_mul(factor)
            .ok_or(ConstructionError::ArithmeticOverflow)
    })?;
    let denominator = denominators
        .into_iter()
        .try_fold(1_u128, |product, factor| {
            product
                .checked_mul(factor)
                .ok_or(ConstructionError::ArithmeticOverflow)
        })?;
    ceil_div_nonnegative(numerator, denominator).map_err(|_| ConstructionError::ArithmeticOverflow)
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::EntityId;

    fn site(raw: u64) -> ConstructionSite {
        ConstructionSite {
            id: ConstructionSiteId(EntityId(raw)),
            target: ConstructionTarget::Junction {
                routing_domain: RoutingDomain::OpenWorld,
                position: FixedVec2::new(crate::Fixed::ZERO, crate::Fixed::ZERO),
            },
            required_work: Energy(1),
            completed_work: Energy(0),
            activation_ready: false,
        }
    }

    #[test]
    fn append_slots_stay_stable_while_live_iteration_is_entity_ordered() {
        let mut store = ConstructionSiteStore::default();
        let high = store.insert_with_index(site(9)).unwrap();
        let low = store.insert_with_index(site(2)).unwrap();
        assert_eq!(
            (high, low),
            (ConstructionSiteIndex(0), ConstructionSiteIndex(1))
        );
        assert_eq!(
            store.iter().map(|site| site.id).collect::<Vec<_>>(),
            vec![
                ConstructionSiteId(EntityId(2)),
                ConstructionSiteId(EntityId(9))
            ]
        );
        assert_eq!(
            store.remove_by_index(high).unwrap().id,
            ConstructionSiteId(EntityId(9))
        );
        let later = store.insert_with_index(site(7)).unwrap();
        assert_eq!(later, ConstructionSiteIndex(2));
        assert_eq!(
            store.get_by_index(low).unwrap().id,
            ConstructionSiteId(EntityId(2))
        );
        assert_eq!(
            store
                .iter_slots()
                .map(|(index, site)| (index, site.map(|site| site.id)))
                .collect::<Vec<_>>(),
            vec![
                (ConstructionSiteIndex(0), None),
                (
                    ConstructionSiteIndex(1),
                    Some(ConstructionSiteId(EntityId(2)))
                ),
                (
                    ConstructionSiteIndex(2),
                    Some(ConstructionSiteId(EntityId(7)))
                ),
            ]
        );
    }
}
