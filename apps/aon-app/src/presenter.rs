use crate::cell_buffer::{
    CellBuffer, CellBufferError, CellLayer, CellPoint, CellTone, CellVisual, CellWrite,
    PresentationSource, WireConnections,
};
use aon_sim::{
    EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GatePort, GatePortRef, GateRenderRecord,
    GateType, Heading, LogicLevel, MobilePort, MobilePortRef, MobileRenderRecord,
    PhysicalScaleProfile, PortAnchor, RenderSnapshot, RoutingDomain, TrackPosition, WireEnd,
    WireId, WireRenderRecord, floor_div, polyline_length,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

const MAX_RASTER_POINTS_PER_SEGMENT: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Network,
    Circuit { substrate: EntityId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub origin: CellPoint,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    pub const fn new(origin: CellPoint, width: u32, height: u32) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    fn bounds(self) -> Result<GridBounds, PresenterError> {
        if self.width == 0 || self.height == 0 {
            return Err(PresenterError::CellBuffer(CellBufferError::ZeroDimension {
                width: self.width,
                height: self.height,
            }));
        }
        let right = i64::from(self.origin.x)
            .checked_add(i64::from(self.width) - 1)
            .ok_or(PresenterError::ViewportCoordinateOverflow)?;
        let top = i64::from(self.origin.y)
            .checked_add(i64::from(self.height) - 1)
            .ok_or(PresenterError::ViewportCoordinateOverflow)?;
        i32::try_from(right).map_err(|_| PresenterError::ViewportCoordinateOverflow)?;
        i32::try_from(top).map_err(|_| PresenterError::ViewportCoordinateOverflow)?;
        Ok(GridBounds {
            left: i64::from(self.origin.x),
            right,
            bottom: i64::from(self.origin.y),
            top,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PickTarget {
    Entity(EntityId),
    GatePort(GatePortRef),
    MobilePort(MobilePortRef),
    WireEnd { wire: WireId, end: WireEnd },
}

impl PickTarget {
    pub const fn parent_entity(self) -> EntityId {
        match self {
            Self::Entity(entity) => entity,
            Self::GatePort(port) => port.gate.entity_id(),
            Self::MobilePort(port) => port.mobile.entity_id(),
            Self::WireEnd { wire, .. } => wire.entity_id(),
        }
    }

    const fn subtarget_rank(self) -> u8 {
        match self {
            Self::GatePort(_) | Self::MobilePort(_) => 0,
            Self::WireEnd { .. } => 1,
            Self::Entity(_) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenterDiagnostic {
    CoordinateOutsideHostRange {
        entity: EntityId,
    },
    RasterSpanExceeded {
        wire: WireId,
        requested_points: u128,
        limit: u128,
    },
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PresenterError {
    #[error(transparent)]
    CellBuffer(#[from] CellBufferError),

    #[error("the active presentation pitch must be positive, got {pitch:?}")]
    NonPositivePitch { pitch: Fixed },

    #[error("Circuit View substrate {substrate:?} is not present in the snapshot")]
    MissingCircuitSubstrate { substrate: EntityId },

    #[error("the requested viewport extends beyond CellPoint's checked coordinate range")]
    ViewportCoordinateOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotPresentation {
    buffer: CellBuffer,
    picks: BTreeMap<CellPoint, Vec<PickTarget>>,
    diagnostics: Vec<PresenterDiagnostic>,
}

impl SnapshotPresentation {
    pub const fn buffer(&self) -> &CellBuffer {
        &self.buffer
    }

    pub fn pick_targets(&self, point: CellPoint) -> &[PickTarget] {
        self.picks.get(&point).map_or(&[], Vec::as_slice)
    }

    pub fn primary_pick(&self, point: CellPoint) -> Option<PickTarget> {
        self.pick_targets(point).first().copied()
    }

    pub fn pick_points(&self, target: PickTarget) -> impl Iterator<Item = CellPoint> + '_ {
        self.picks
            .iter()
            .filter_map(move |(point, targets)| targets.contains(&target).then_some(*point))
    }

    pub fn diagnostics(&self) -> &[PresenterDiagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GridPoint {
    x: i64,
    y: i64,
}

impl GridPoint {
    const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    fn to_cell(self) -> Option<CellPoint> {
        Some(CellPoint::new(
            i32::try_from(self.x).ok()?,
            i32::try_from(self.y).ok()?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridBounds {
    left: i64,
    right: i64,
    bottom: i64,
    top: i64,
}

impl GridBounds {
    const fn contains(self, point: GridPoint) -> bool {
        self.left <= point.x
            && point.x <= self.right
            && self.bottom <= point.y
            && point.y <= self.top
    }

    const fn intersects_segment_bounds(self, start: GridPoint, end: GridPoint) -> bool {
        let min_x = if start.x < end.x { start.x } else { end.x };
        let max_x = if start.x > end.x { start.x } else { end.x };
        let min_y = if start.y < end.y { start.y } else { end.y };
        let max_y = if start.y > end.y { start.y } else { end.y };
        min_x <= self.right && self.left <= max_x && min_y <= self.top && self.bottom <= max_y
    }

    const fn expanded(self, margin: i64) -> Self {
        Self {
            left: self.left.saturating_sub(margin),
            right: self.right.saturating_add(margin),
            bottom: self.bottom.saturating_sub(margin),
            top: self.top.saturating_add(margin),
        }
    }

    const fn passed(self, point: GridPoint, step_x: i64, step_y: i64) -> bool {
        (step_x > 0 && point.x > self.right)
            || (step_x < 0 && point.x < self.left)
            || (step_y > 0 && point.y > self.top)
            || (step_y < 0 && point.y < self.bottom)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StrokeCell {
    north: bool,
    east: bool,
    south: bool,
    west: bool,
    ambiguous: bool,
    end_a: bool,
    end_b: bool,
}

impl StrokeCell {
    fn connect(&mut self, dx: i64, dy: i64) {
        match (dx, dy) {
            (0, 1) => self.north = true,
            (1, 0) => self.east = true,
            (0, -1) => self.south = true,
            (-1, 0) => self.west = true,
            _ => self.ambiguous = true,
        }
    }

    const fn connections(self) -> WireConnections {
        WireConnections::new(self.north, self.east, self.south, self.west)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisibleWireStroke {
    wire: WireId,
    level: LogicLevel,
    stroke: StrokeCell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RankedPick {
    layer: CellLayer,
    target: PickTarget,
}

impl RankedPick {
    const fn new(layer: CellLayer, target: PickTarget) -> Self {
        Self { layer, target }
    }
}

pub fn project_snapshot(
    snapshot: &RenderSnapshot,
    physical: &PhysicalScaleProfile,
    view: ViewMode,
    viewport: Viewport,
) -> Result<SnapshotPresentation, PresenterError> {
    let bounds = viewport.bounds()?;
    let mut buffer = CellBuffer::new(viewport.origin, viewport.width, viewport.height)?;
    let (pitch, coordinate_origin, circuit_substrate) = match view {
        ViewMode::Network => (physical.world_routing_pitch, FixedVec2::default(), None),
        ViewMode::Circuit { substrate } => {
            if let Some(record) = snapshot
                .fixed_substrates()
                .iter()
                .find(|record| record.id == substrate)
                .copied()
            {
                (
                    physical.circuit_routing_pitch,
                    record.origin,
                    Some((record.routing_area, record.footprint)),
                )
            } else if let Some(record) = snapshot
                .mobiles()
                .iter()
                .find(|record| record.id.entity_id() == substrate)
                .copied()
            {
                (
                    physical.circuit_routing_pitch,
                    FixedVec2::default(),
                    Some((record.routing_area, record.footprint)),
                )
            } else {
                return Err(PresenterError::MissingCircuitSubstrate { substrate });
            }
        }
    };
    if pitch.0 <= 0 {
        return Err(PresenterError::NonPositivePitch { pitch });
    }

    let mut diagnostics = Vec::new();
    let mut ranked_picks = BTreeMap::<CellPoint, Vec<RankedPick>>::new();

    match circuit_substrate {
        None => draw_network_substrates(
            snapshot,
            pitch,
            bounds,
            &mut buffer,
            &mut ranked_picks,
            &mut diagnostics,
        ),
        Some((routing_area, footprint)) => {
            draw_circuit_background(routing_area, footprint, pitch, bounds, &mut buffer)
        }
    }

    let mut wire_cells = BTreeMap::<CellPoint, BTreeMap<EntityId, VisibleWireStroke>>::new();
    for wire in snapshot
        .wires()
        .iter()
        .filter(|wire| domain_visible(wire.routing_domain, view))
    {
        project_wire(
            wire,
            coordinate_origin,
            pitch,
            bounds,
            &mut wire_cells,
            &mut diagnostics,
        );
    }
    draw_wire_cells(&mut buffer, &mut ranked_picks, wire_cells);

    for junction in snapshot
        .junctions()
        .iter()
        .filter(|junction| domain_visible(junction.routing_domain, view))
    {
        let entity = junction.id.entity_id();
        let Some(at) = project_fixed_point(junction.position, coordinate_origin, pitch)
            .and_then(GridPoint::to_cell)
        else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        if !bounds.contains(GridPoint::new(i64::from(at.x), i64::from(at.y))) {
            continue;
        }
        buffer.write(CellWrite::new(
            at,
            CellLayer::Junction,
            CellVisual::new(
                '●',
                CellTone::Neutral,
                Some(PresentationSource::Canonical(entity)),
            ),
        ));
        push_pick(
            &mut ranked_picks,
            at,
            CellLayer::Junction,
            PickTarget::Entity(entity),
        );
    }

    if matches!(view, ViewMode::Network) {
        draw_mobiles(
            snapshot,
            pitch,
            bounds,
            &mut buffer,
            &mut ranked_picks,
            &mut diagnostics,
        );
    }

    if let ViewMode::Circuit { substrate } = view
        && let Some(mobile) = snapshot
            .mobiles()
            .iter()
            .find(|mobile| mobile.id.entity_id() == substrate)
    {
        draw_mobile_ports(
            *mobile,
            pitch,
            bounds,
            &mut buffer,
            &mut ranked_picks,
            &mut diagnostics,
        );
    }

    let bound_ports = bound_gate_ports(snapshot);
    for gate in snapshot
        .gates()
        .iter()
        .filter(|gate| domain_visible(gate.routing_domain, view))
    {
        draw_gate(
            gate,
            physical,
            coordinate_origin,
            pitch,
            bounds,
            &bound_ports,
            &mut buffer,
            &mut ranked_picks,
            &mut diagnostics,
        );
    }

    let picks = finalize_picks(ranked_picks);
    Ok(SnapshotPresentation {
        buffer,
        picks,
        diagnostics,
    })
}

fn domain_visible(domain: RoutingDomain, view: ViewMode) -> bool {
    match (view, domain) {
        (ViewMode::Network, RoutingDomain::OpenWorld) => true,
        (
            ViewMode::Circuit {
                substrate: selected,
            },
            RoutingDomain::FixedSubstrate(actual),
        ) => selected == actual,
        (
            ViewMode::Circuit {
                substrate: selected,
            },
            RoutingDomain::MobileSubstrate(actual),
        ) => selected == actual,
        _ => false,
    }
}

fn draw_network_substrates(
    snapshot: &RenderSnapshot,
    pitch: Fixed,
    bounds: GridBounds,
    buffer: &mut CellBuffer,
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    diagnostics: &mut Vec<PresenterDiagnostic>,
) {
    let projected_origins = snapshot
        .fixed_substrates()
        .iter()
        .filter_map(|substrate| project_fixed_point(substrate.origin, FixedVec2::default(), pitch))
        .collect::<BTreeSet<_>>();

    for substrate in snapshot.fixed_substrates() {
        let entity = substrate.id;
        let footprint = translated_aabb_i128(substrate.footprint, substrate.origin);
        if !draw_aabb_background(
            footprint,
            FixedVec2::default(),
            pitch,
            bounds,
            buffer,
            &projected_origins,
        ) {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
        }
    }

    for substrate in snapshot.fixed_substrates() {
        let entity = substrate.id;
        let projected_origin = project_fixed_point(substrate.origin, FixedVec2::default(), pitch);
        let Some(origin) = projected_origin.and_then(GridPoint::to_cell) else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        if !bounds.contains(GridPoint::new(i64::from(origin.x), i64::from(origin.y))) {
            continue;
        }
        buffer.write(CellWrite::new(
            origin,
            CellLayer::Substrate,
            CellVisual::new(
                '■',
                CellTone::Neutral,
                Some(PresentationSource::Canonical(entity)),
            ),
        ));
        push_pick(
            picks,
            origin,
            CellLayer::Substrate,
            PickTarget::Entity(entity),
        );
    }
}

fn draw_mobiles(
    snapshot: &RenderSnapshot,
    pitch: Fixed,
    bounds: GridBounds,
    buffer: &mut CellBuffer,
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    diagnostics: &mut Vec<PresenterDiagnostic>,
) {
    for mobile in snapshot.mobiles() {
        let entity = mobile.id.entity_id();
        let Some(at) = project_fixed_point(mobile.world_position, FixedVec2::default(), pitch)
            .and_then(GridPoint::to_cell)
        else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        if !bounds.contains(GridPoint::new(i64::from(at.x), i64::from(at.y))) {
            continue;
        }
        let glyph = mobile_direction(snapshot, *mobile)
            .map(direction_glyph)
            .unwrap_or('>');
        let tone = match mobile.stop {
            LogicLevel::Low => CellTone::Neutral,
            LogicLevel::High => CellTone::High,
            LogicLevel::X => CellTone::Unknown,
        };
        buffer.write(CellWrite::new(
            at,
            CellLayer::Mobile,
            CellVisual::new(glyph, tone, Some(PresentationSource::Canonical(entity))),
        ));
        push_pick(picks, at, CellLayer::Mobile, PickTarget::Entity(entity));
    }
}

fn draw_mobile_ports(
    mobile: MobileRenderRecord,
    pitch: Fixed,
    bounds: GridBounds,
    buffer: &mut CellBuffer,
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    diagnostics: &mut Vec<PresenterDiagnostic>,
) {
    let entity = mobile.id.entity_id();
    let ports = [
        (MobilePort::Stop, mobile.routing_area.min, 'S', mobile.stop),
        (
            MobilePort::Left,
            FixedVec2::new(mobile.routing_area.max.x, mobile.routing_area.min.y),
            'L',
            mobile.left,
        ),
        (
            MobilePort::Right,
            mobile.routing_area.max,
            'R',
            mobile.right,
        ),
    ];
    for (port, point, glyph, level) in ports {
        let Some(at) =
            project_fixed_point(point, FixedVec2::default(), pitch).and_then(GridPoint::to_cell)
        else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        if !bounds.contains(GridPoint::new(i64::from(at.x), i64::from(at.y))) {
            continue;
        }
        buffer.write(CellWrite::new(
            at,
            CellLayer::GatePort,
            CellVisual::new(
                glyph,
                tone_for_logic(level),
                Some(PresentationSource::Canonical(entity)),
            ),
        ));
        push_pick(
            picks,
            at,
            CellLayer::GatePort,
            PickTarget::MobilePort(MobilePortRef {
                mobile: mobile.id,
                port,
            }),
        );
    }
}

fn mobile_direction(snapshot: &RenderSnapshot, mobile: MobileRenderRecord) -> Option<(i128, i128)> {
    match mobile.track_position {
        TrackPosition::Edge {
            edge,
            offset,
            heading,
        } => {
            let wire = snapshot.wires().iter().find(|wire| wire.id == edge)?;
            edge_direction_at_offset(&wire.points, offset, heading)
        }
        TrackPosition::Junction {
            junction,
            incoming_edge,
        } => {
            let wire = snapshot
                .wires()
                .iter()
                .find(|wire| wire.id == incoming_edge)?;
            match (wire.endpoint_a, wire.endpoint_b) {
                (EndpointTarget::Junction(bound), _) if bound == junction => {
                    vector(wire.points.get(1).copied()?, *wire.points.first()?)
                }
                (_, EndpointTarget::Junction(bound)) if bound == junction => {
                    let end = wire.points.len().checked_sub(1)?;
                    vector(
                        *wire.points.get(end.checked_sub(1)?)?,
                        *wire.points.get(end)?,
                    )
                }
                _ => None,
            }
        }
    }
}

fn edge_direction_at_offset(
    points: &[FixedVec2],
    offset: Fixed,
    heading: Heading,
) -> Option<(i128, i128)> {
    if offset.0 < 0 {
        return None;
    }
    let mut cumulative = Fixed::ZERO;
    let mut terminal = None;
    for (index, segment) in points.windows(2).enumerate() {
        let end = polyline_length(points.get(..index.checked_add(2)?)?).ok()?;
        let direction = match heading {
            Heading::Forward => vector(segment[0], segment[1]),
            Heading::Reverse => vector(segment[1], segment[0]),
        };
        terminal = direction;
        let inside = offset >= cumulative
            && match heading {
                // At an internal vertex Forward enters the following segment.
                Heading::Forward => offset < end,
                // At an internal vertex Reverse enters the preceding segment.
                Heading::Reverse => offset <= end,
            };
        if inside {
            return direction;
        }
        cumulative = end;
    }
    (offset == cumulative).then_some(terminal?)
}

fn vector(start: FixedVec2, end: FixedVec2) -> Option<(i128, i128)> {
    let vector = (
        i128::from(end.x.0) - i128::from(start.x.0),
        i128::from(end.y.0) - i128::from(start.y.0),
    );
    (vector != (0, 0)).then_some(vector)
}

fn direction_glyph((x, y): (i128, i128)) -> char {
    if x.unsigned_abs() >= y.unsigned_abs() {
        if x >= 0 { '>' } else { '<' }
    } else if y >= 0 {
        '^'
    } else {
        'v'
    }
}

fn draw_circuit_background(
    routing_area: FixedAabb,
    footprint: FixedAabb,
    pitch: Fixed,
    bounds: GridBounds,
    buffer: &mut CellBuffer,
) {
    let local_origin = FixedVec2::default();
    let excluded = BTreeSet::new();
    draw_aabb_background(
        aabb_i128(footprint),
        local_origin,
        pitch,
        bounds,
        buffer,
        &excluded,
    );
    draw_aabb_background(
        aabb_i128(routing_area),
        local_origin,
        pitch,
        bounds,
        buffer,
        &excluded,
    );
}

fn draw_aabb_background(
    aabb: RawAabb,
    coordinate_origin: FixedVec2,
    pitch: Fixed,
    bounds: GridBounds,
    buffer: &mut CellBuffer,
    excluded: &BTreeSet<GridPoint>,
) -> bool {
    let Some(min) = project_raw_point(aabb.min_x, aabb.min_y, coordinate_origin, pitch) else {
        return false;
    };
    let Some(max) = project_raw_point(aabb.max_x, aabb.max_y, coordinate_origin, pitch) else {
        return false;
    };
    let left = min.x.max(bounds.left);
    let right = max.x.min(bounds.right);
    let bottom = min.y.max(bounds.bottom);
    let top = max.y.min(bounds.top);
    if left > right || bottom > top {
        return true;
    }
    for y in bottom..=top {
        for x in left..=right {
            let point = GridPoint::new(x, y);
            if excluded.contains(&point) {
                continue;
            }
            let Some(point) = point.to_cell() else {
                continue;
            };
            buffer.write(CellWrite::new(
                point,
                CellLayer::Substrate,
                CellVisual::new('·', CellTone::Neutral, None),
            ));
        }
    }
    true
}

fn project_wire(
    wire: &WireRenderRecord,
    coordinate_origin: FixedVec2,
    pitch: Fixed,
    bounds: GridBounds,
    visible: &mut BTreeMap<CellPoint, BTreeMap<EntityId, VisibleWireStroke>>,
    diagnostics: &mut Vec<PresenterDiagnostic>,
) {
    let entity = wire.id.entity_id();
    let Some(points) = wire
        .points
        .iter()
        .copied()
        .map(|point| project_fixed_point(point, coordinate_origin, pitch))
        .collect::<Option<Vec<_>>>()
    else {
        diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
        return;
    };
    let Some(&first) = points.first() else {
        return;
    };
    let last = *points
        .last()
        .expect("a nonempty point list has a last point");
    let mut cells = BTreeMap::<GridPoint, StrokeCell>::new();
    for segment in points.windows(2) {
        if !bounds.intersects_segment_bounds(segment[0], segment[1]) {
            continue;
        }
        let raster = match rasterize_grid_segment_clipped(segment[0], segment[1], bounds) {
            Ok(path) => path,
            Err(requested_points) => {
                diagnostics.push(PresenterDiagnostic::RasterSpanExceeded {
                    wire: wire.id,
                    requested_points,
                    limit: MAX_RASTER_POINTS_PER_SEGMENT,
                });
                return;
            }
        };
        for &point in &raster.points {
            cells.entry(point).or_default();
        }
        for &(start, end) in &raster.edges {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            cells.entry(start).or_default().connect(dx, dy);
            cells.entry(end).or_default().connect(-dx, -dy);
        }
    }
    cells.entry(first).or_default().end_a = true;
    cells.entry(last).or_default().end_b = true;

    for (point, stroke) in cells {
        if !bounds.contains(point) {
            continue;
        }
        let Some(point) = point.to_cell() else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        visible.entry(point).or_default().insert(
            entity,
            VisibleWireStroke {
                wire: wire.id,
                level: wire.active_level,
                stroke,
            },
        );
    }
}

fn draw_wire_cells(
    buffer: &mut CellBuffer,
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    cells: BTreeMap<CellPoint, BTreeMap<EntityId, VisibleWireStroke>>,
) {
    for (point, wires) in cells {
        let Some((&highest_id, highest)) = wires.last_key_value() else {
            continue;
        };
        let glyph = if wires.len() > 1 || highest.stroke.ambiguous {
            '╳'
        } else {
            highest.stroke.connections().glyph()
        };
        buffer.write(CellWrite::new(
            point,
            CellLayer::Wire,
            CellVisual::new(
                glyph,
                tone_for_logic(highest.level),
                Some(PresentationSource::Canonical(highest_id)),
            ),
        ));
        for stroke in wires.values() {
            if stroke.stroke.end_a {
                push_pick(
                    picks,
                    point,
                    CellLayer::Wire,
                    PickTarget::WireEnd {
                        wire: stroke.wire,
                        end: WireEnd::A,
                    },
                );
            }
            if stroke.stroke.end_b {
                push_pick(
                    picks,
                    point,
                    CellLayer::Wire,
                    PickTarget::WireEnd {
                        wire: stroke.wire,
                        end: WireEnd::B,
                    },
                );
            }
            push_pick(
                picks,
                point,
                CellLayer::Wire,
                PickTarget::Entity(stroke.wire.entity_id()),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_gate(
    gate: &GateRenderRecord,
    physical: &PhysicalScaleProfile,
    coordinate_origin: FixedVec2,
    pitch: Fixed,
    bounds: GridBounds,
    bound_ports: &BTreeSet<GatePortRef>,
    buffer: &mut CellBuffer,
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    diagnostics: &mut Vec<PresenterDiagnostic>,
) {
    let entity = gate.id.entity_id();
    let Some(origin) =
        project_fixed_point(gate.origin, coordinate_origin, pitch).and_then(GridPoint::to_cell)
    else {
        diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
        return;
    };
    if bounds.contains(GridPoint::new(i64::from(origin.x), i64::from(origin.y))) {
        buffer.write(CellWrite::new(
            origin,
            CellLayer::GatePort,
            CellVisual::new(
                gate_glyph(gate.gate_type),
                tone_for_logic(gate.current_output),
                Some(PresentationSource::Canonical(entity)),
            ),
        ));
        push_pick(
            picks,
            origin,
            CellLayer::GatePort,
            PickTarget::Entity(entity),
        );
    }

    // Power has a geometric anchor but no signal endpoint/sample in GateSignalPorts. S0-M6 can
    // truthfully style and pick only the projected input/output signal ports.
    let ports = [
        (GatePort::InputA, Some(gate.input_a_level)),
        (GatePort::InputB, gate.input_b_level),
        (GatePort::Output, Some(gate.output_sample.level)),
    ];
    for (port, level) in ports {
        let Some(level) = level else {
            continue;
        };
        let Some(anchor) = gate_port_anchor(gate.gate_type, port, physical) else {
            continue;
        };
        let Some(at) = project_raw_point(
            i128::from(gate.origin.x.0) + i128::from(anchor.x.0),
            i128::from(gate.origin.y.0) + i128::from(anchor.y.0),
            coordinate_origin,
            pitch,
        )
        .and_then(GridPoint::to_cell) else {
            diagnostics.push(PresenterDiagnostic::CoordinateOutsideHostRange { entity });
            continue;
        };
        if !bounds.contains(GridPoint::new(i64::from(at.x), i64::from(at.y))) {
            continue;
        }
        let reference = GatePortRef {
            gate: gate.id,
            port,
        };
        let glyph = if bound_ports.contains(&reference) {
            '◉'
        } else {
            '○'
        };
        buffer.write(CellWrite::new(
            at,
            CellLayer::GatePort,
            CellVisual::new(
                glyph,
                tone_for_logic(level),
                Some(PresentationSource::Canonical(entity)),
            ),
        ));
        push_pick(
            picks,
            at,
            CellLayer::GatePort,
            PickTarget::GatePort(reference),
        );
    }
}

fn bound_gate_ports(snapshot: &RenderSnapshot) -> BTreeSet<GatePortRef> {
    snapshot
        .wires()
        .iter()
        .flat_map(|wire| [wire.endpoint_a, wire.endpoint_b])
        .filter_map(|endpoint| match endpoint {
            aon_sim::EndpointTarget::GatePort(port) => Some(port),
            aon_sim::EndpointTarget::Free
            | aon_sim::EndpointTarget::Junction(_)
            | aon_sim::EndpointTarget::MobilePort(_) => None,
        })
        .collect()
}

fn gate_port_anchor(
    gate_type: GateType,
    port: GatePort,
    physical: &PhysicalScaleProfile,
) -> Option<PortAnchor> {
    match gate_type {
        GateType::And => binary_gate_port_anchor(physical.gate_port_anchors.and_gate, port),
        GateType::Or => binary_gate_port_anchor(physical.gate_port_anchors.or_gate, port),
        GateType::Not => match port {
            GatePort::InputA => Some(physical.gate_port_anchors.not_gate.input),
            GatePort::InputB => None,
            GatePort::Output => Some(physical.gate_port_anchors.not_gate.output),
            GatePort::Power => Some(physical.gate_port_anchors.not_gate.power),
        },
    }
}

fn binary_gate_port_anchor(
    anchors: aon_sim::BinaryGatePortAnchors,
    port: GatePort,
) -> Option<PortAnchor> {
    Some(match port {
        GatePort::InputA => anchors.input_a,
        GatePort::InputB => anchors.input_b,
        GatePort::Output => anchors.output,
        GatePort::Power => anchors.power,
    })
}

fn push_pick(
    picks: &mut BTreeMap<CellPoint, Vec<RankedPick>>,
    point: CellPoint,
    layer: CellLayer,
    target: PickTarget,
) {
    picks
        .entry(point)
        .or_default()
        .push(RankedPick::new(layer, target));
}

fn finalize_picks(
    ranked: BTreeMap<CellPoint, Vec<RankedPick>>,
) -> BTreeMap<CellPoint, Vec<PickTarget>> {
    ranked
        .into_iter()
        .map(|(point, mut candidates)| {
            candidates.sort_unstable_by(compare_pick);
            candidates.dedup_by_key(|candidate| candidate.target);
            (
                point,
                candidates
                    .into_iter()
                    .map(|candidate| candidate.target)
                    .collect(),
            )
        })
        .collect()
}

fn compare_pick(left: &RankedPick, right: &RankedPick) -> Ordering {
    right
        .layer
        .cmp(&left.layer)
        .then_with(|| {
            right
                .target
                .parent_entity()
                .cmp(&left.target.parent_entity())
        })
        .then_with(|| {
            left.target
                .subtarget_rank()
                .cmp(&right.target.subtarget_rank())
        })
        .then_with(|| left.target.cmp(&right.target))
}

fn project_fixed_point(
    point: FixedVec2,
    coordinate_origin: FixedVec2,
    pitch: Fixed,
) -> Option<GridPoint> {
    project_raw_point(
        i128::from(point.x.0),
        i128::from(point.y.0),
        coordinate_origin,
        pitch,
    )
}

fn project_raw_point(
    x: i128,
    y: i128,
    coordinate_origin: FixedVec2,
    pitch: Fixed,
) -> Option<GridPoint> {
    let pitch = i128::from(pitch.0);
    let x = floor_div(x - i128::from(coordinate_origin.x.0), pitch).ok()?;
    let y = floor_div(y - i128::from(coordinate_origin.y.0), pitch).ok()?;
    Some(GridPoint::new(
        i64::try_from(x).ok()?,
        i64::try_from(y).ok()?,
    ))
}

fn rasterize_grid_segment_clipped(
    start: GridPoint,
    end: GridPoint,
    visible_bounds: GridBounds,
) -> Result<RasterizedGridSegment, u128> {
    let dx = i128::from(end.x) - i128::from(start.x);
    let dy = i128::from(end.y) - i128::from(start.y);
    let x_steps = dx.unsigned_abs();
    let y_steps = dy.unsigned_abs();
    if x_steps == 0 && y_steps == 0 {
        return Ok(RasterizedGridSegment {
            points: vec![start],
            edges: Vec::new(),
        });
    }

    // Resume the exact DDA state immediately before the segment enters a small viewport halo.
    // This preserves the unclipped supercover in visible cells without walking a huge off-screen
    // prefix or suffix.
    let traversal_bounds = visible_bounds.expanded(2);
    let entry = segment_entry_fraction(start, end, traversal_bounds);
    let mut crossed_x = crossings_strictly_before(x_steps, entry);
    let mut crossed_y = crossings_strictly_before(y_steps, entry);
    let step_x = i64::try_from(dx.signum()).expect("an i128 sign fits i64");
    let step_y = i64::try_from(dy.signum()).expect("an i128 sign fits i64");
    let mut point = GridPoint::new(
        coordinate_after_steps(start.x, step_x, crossed_x),
        coordinate_after_steps(start.y, step_y, crossed_y),
    );
    let mut output = RasterizedGridSegment::default();
    push_raster_point(&mut output.points, point)?;
    while crossed_x < x_steps || crossed_y < y_steps {
        if traversal_bounds.passed(point, step_x, step_y) {
            break;
        }
        if crossed_x == x_steps {
            let previous = point;
            point.y += step_y;
            crossed_y += 1;
            push_raster_edge(&mut output, previous, point)?;
            continue;
        }
        if crossed_y == y_steps {
            let previous = point;
            point.x += step_x;
            crossed_x += 1;
            push_raster_edge(&mut output, previous, point)?;
            continue;
        }
        let next_x_numerator = 2 * crossed_x + 1;
        let next_x_denominator = 2 * x_steps;
        let next_y_numerator = 2 * crossed_y + 1;
        let next_y_denominator = 2 * y_steps;
        match compare_nonnegative_fractions(
            next_x_numerator,
            next_x_denominator,
            next_y_numerator,
            next_y_denominator,
        ) {
            Ordering::Less => {
                let previous = point;
                point.x += step_x;
                crossed_x += 1;
                push_raster_edge(&mut output, previous, point)?;
            }
            Ordering::Greater => {
                let previous = point;
                point.y += step_y;
                crossed_y += 1;
                push_raster_edge(&mut output, previous, point)?;
            }
            Ordering::Equal => {
                let x_neighbor = GridPoint::new(point.x + step_x, point.y);
                let y_neighbor = GridPoint::new(point.x, point.y + step_y);
                let diagonal = GridPoint::new(x_neighbor.x, y_neighbor.y);
                push_raster_point(&mut output.points, x_neighbor)?;
                push_raster_point(&mut output.points, y_neighbor)?;
                push_raster_point(&mut output.points, diagonal)?;
                output.edges.extend([
                    (point, x_neighbor),
                    (x_neighbor, diagonal),
                    (point, y_neighbor),
                    (y_neighbor, diagonal),
                ]);
                point = diagonal;
                crossed_x += 1;
                crossed_y += 1;
            }
        }
    }
    Ok(output)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RasterizedGridSegment {
    points: Vec<GridPoint>,
    edges: Vec<(GridPoint, GridPoint)>,
}

fn push_raster_edge(
    output: &mut RasterizedGridSegment,
    start: GridPoint,
    end: GridPoint,
) -> Result<(), u128> {
    push_raster_point(&mut output.points, end)?;
    output.edges.push((start, end));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fraction {
    numerator: u128,
    denominator: u128,
}

impl Fraction {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
}

fn segment_entry_fraction(start: GridPoint, end: GridPoint, bounds: GridBounds) -> Fraction {
    let x_entry = axis_entry_fraction(start.x, end.x, bounds.left, bounds.right);
    let y_entry = axis_entry_fraction(start.y, end.y, bounds.bottom, bounds.top);
    if compare_nonnegative_fractions(
        x_entry.numerator,
        x_entry.denominator,
        y_entry.numerator,
        y_entry.denominator,
    )
    .is_lt()
    {
        y_entry
    } else {
        x_entry
    }
}

fn axis_entry_fraction(start: i64, end: i64, low: i64, high: i64) -> Fraction {
    let delta = i128::from(end) - i128::from(start);
    if delta > 0 && start < low {
        Fraction {
            numerator: (i128::from(low) - i128::from(start)).unsigned_abs(),
            denominator: delta.unsigned_abs(),
        }
    } else if delta < 0 && start > high {
        Fraction {
            numerator: (i128::from(start) - i128::from(high)).unsigned_abs(),
            denominator: delta.unsigned_abs(),
        }
    } else {
        Fraction::ZERO
    }
}

fn crossings_strictly_before(steps: u128, at: Fraction) -> u128 {
    if steps == 0 || at.numerator == 0 {
        return 0;
    }
    let common = gcd_u128(steps, at.denominator);
    let reduced_steps = steps / common;
    let reduced_denominator = at.denominator / common;
    let product = reduced_steps
        .checked_mul(at.numerator)
        .expect("reduced u64-sized grid deltas fit their u128 product");
    let whole = product / reduced_denominator;
    let remainder = product % reduced_denominator;
    whole + u128::from(remainder > reduced_denominator / 2)
}

fn coordinate_after_steps(start: i64, step: i64, count: u128) -> i64 {
    let coordinate = i128::from(start)
        + i128::from(step) * i128::try_from(count).expect("a grid delta fits i128");
    i64::try_from(coordinate).expect("a monotone intermediate coordinate stays between endpoints")
}

fn push_raster_point(output: &mut Vec<GridPoint>, point: GridPoint) -> Result<(), u128> {
    let next_len = output.len().saturating_add(1);
    let requested = u128::try_from(next_len).unwrap_or(u128::MAX);
    if requested > MAX_RASTER_POINTS_PER_SEGMENT {
        return Err(requested);
    }
    output.push(point);
    Ok(())
}

fn compare_nonnegative_fractions(
    mut left_numerator: u128,
    mut left_denominator: u128,
    mut right_numerator: u128,
    mut right_denominator: u128,
) -> Ordering {
    debug_assert!(left_denominator > 0 && right_denominator > 0);
    let mut inverted = false;
    loop {
        let left_whole = left_numerator / left_denominator;
        let right_whole = right_numerator / right_denominator;
        if left_whole != right_whole {
            let ordering = left_whole.cmp(&right_whole);
            return if inverted {
                ordering.reverse()
            } else {
                ordering
            };
        }

        let left_remainder = left_numerator % left_denominator;
        let right_remainder = right_numerator % right_denominator;
        match (left_remainder == 0, right_remainder == 0) {
            (true, true) => return Ordering::Equal,
            (true, false) => {
                return if inverted {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
            }
            (false, true) => {
                return if inverted {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (false, false) => {
                left_numerator = left_denominator;
                left_denominator = left_remainder;
                right_numerator = right_denominator;
                right_denominator = right_remainder;
                inverted = !inverted;
            }
        }
    }
}

const fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawAabb {
    min_x: i128,
    min_y: i128,
    max_x: i128,
    max_y: i128,
}

fn aabb_i128(aabb: FixedAabb) -> RawAabb {
    RawAabb {
        min_x: i128::from(aabb.min.x.0),
        min_y: i128::from(aabb.min.y.0),
        max_x: i128::from(aabb.max.x.0),
        max_y: i128::from(aabb.max.y.0),
    }
}

fn translated_aabb_i128(aabb: FixedAabb, origin: FixedVec2) -> RawAabb {
    RawAabb {
        min_x: i128::from(aabb.min.x.0) + i128::from(origin.x.0),
        min_y: i128::from(aabb.min.y.0) + i128::from(origin.y.0),
        max_x: i128::from(aabb.max.x.0) + i128::from(origin.x.0),
        max_y: i128::from(aabb.max.y.0) + i128::from(origin.y.0),
    }
}

const fn tone_for_logic(level: LogicLevel) -> CellTone {
    match level {
        LogicLevel::Low => CellTone::Low,
        LogicLevel::High => CellTone::High,
        LogicLevel::X => CellTone::Unknown,
    }
}

const fn gate_glyph(gate_type: GateType) -> char {
    match gate_type {
        GateType::And => '&',
        GateType::Or => '|',
        GateType::Not => '!',
    }
}
