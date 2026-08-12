use aon_sim::{EntityId, GateType, LogicLevel};
use std::cmp::Ordering;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellPoint {
    pub x: i32,
    pub y: i32,
}

impl CellPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellLayer {
    Terrain,
    Substrate,
    Wire,
    Junction,
    Mobile,
    MainCore,
    GatePort,
    Selection,
    GhostAndDebug,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellTone {
    #[default]
    Neutral,
    Low,
    High,
    Unknown,
    Ghost,
    Highlight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PresentationSource {
    Canonical(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellVisual {
    pub glyph: char,
    pub tone: CellTone,
    pub source: Option<PresentationSource>,
}

impl CellVisual {
    pub const EMPTY: Self = Self {
        glyph: '·',
        tone: CellTone::Neutral,
        source: None,
    };

    pub const fn new(glyph: char, tone: CellTone, source: Option<PresentationSource>) -> Self {
        Self {
            glyph,
            tone,
            source,
        }
    }

    fn deterministic_cmp(self, other: Self) -> Ordering {
        (self.source, self.glyph, self.tone).cmp(&(other.source, other.glyph, other.tone))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellWrite {
    pub point: CellPoint,
    pub layer: CellLayer,
    pub visual: CellVisual,
}

impl CellWrite {
    pub const fn new(point: CellPoint, layer: CellLayer, visual: CellVisual) -> Self {
        Self {
            point,
            layer,
            visual,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CellStack {
    terrain: Option<CellVisual>,
    substrate: Option<CellVisual>,
    wire: Option<CellVisual>,
    junction: Option<CellVisual>,
    mobile: Option<CellVisual>,
    main_core: Option<CellVisual>,
    gate_port: Option<CellVisual>,
    selection: Option<CellVisual>,
    ghost_and_debug: Option<CellVisual>,
}

impl CellStack {
    fn slot_mut(&mut self, layer: CellLayer) -> &mut Option<CellVisual> {
        match layer {
            CellLayer::Terrain => &mut self.terrain,
            CellLayer::Substrate => &mut self.substrate,
            CellLayer::Wire => &mut self.wire,
            CellLayer::Junction => &mut self.junction,
            CellLayer::Mobile => &mut self.mobile,
            CellLayer::MainCore => &mut self.main_core,
            CellLayer::GatePort => &mut self.gate_port,
            CellLayer::Selection => &mut self.selection,
            CellLayer::GhostAndDebug => &mut self.ghost_and_debug,
        }
    }

    fn write(&mut self, layer: CellLayer, visual: CellVisual) {
        let slot = self.slot_mut(layer);
        if slot.is_none_or(|current| visual.deterministic_cmp(current).is_gt()) {
            *slot = Some(visual);
        }
    }

    fn visible(&self) -> CellVisual {
        let base = self
            .gate_port
            .or(self.main_core)
            .or(self.mobile)
            .or(self.junction)
            .or(self.wire)
            .or(self.substrate)
            .or(self.terrain)
            .unwrap_or(CellVisual::EMPTY);
        let selected = self.selection.map_or(base, |_| CellVisual {
            tone: CellTone::Highlight,
            ..base
        });
        self.ghost_and_debug.map_or(selected, |overlay| CellVisual {
            source: selected.source,
            ..overlay
        })
    }

    fn pick(&self) -> Option<PresentationSource> {
        [
            self.ghost_and_debug,
            self.selection,
            self.gate_port,
            self.main_core,
            self.junction,
            self.wire,
            self.substrate,
            self.terrain,
        ]
        .into_iter()
        .flatten()
        .find_map(|visual| visual.source)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellBuffer {
    origin: CellPoint,
    width: u32,
    height: u32,
    cells: Vec<CellStack>,
}

impl CellBuffer {
    pub fn new(origin: CellPoint, width: u32, height: u32) -> Result<Self, CellBufferError> {
        if width == 0 || height == 0 {
            return Err(CellBufferError::ZeroDimension { width, height });
        }
        let cell_count = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(CellBufferError::DimensionsTooLarge { width, height })?;
        Ok(Self {
            origin,
            width,
            height,
            cells: vec![CellStack::default(); cell_count],
        })
    }

    pub const fn origin(&self) -> CellPoint {
        self.origin
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn clear(&mut self) {
        self.cells.fill(CellStack::default());
    }

    pub fn write(&mut self, write: CellWrite) -> bool {
        let Some(index) = self.index(write.point) else {
            return false;
        };
        self.cells[index].write(write.layer, write.visual);
        true
    }

    pub fn project(&mut self, writes: impl IntoIterator<Item = CellWrite>) {
        for write in writes {
            self.write(write);
        }
    }

    pub fn visual(&self, point: CellPoint) -> Option<CellVisual> {
        self.index(point).map(|index| self.cells[index].visible())
    }

    pub fn pick(&self, point: CellPoint) -> Option<PresentationSource> {
        self.index(point).and_then(|index| self.cells[index].pick())
    }

    pub fn rows(&self) -> impl Iterator<Item = impl Iterator<Item = CellVisual> + '_> + '_ {
        self.cells
            .chunks_exact(self.width as usize)
            .rev()
            .map(|row| row.iter().map(CellStack::visible))
    }

    pub fn glyph_text(&self) -> String {
        let mut output = String::new();
        for (row_index, row) in self.rows().enumerate() {
            if row_index != 0 {
                output.push('\n');
            }
            output.extend(row.map(|visual| visual.glyph));
        }
        output
    }

    pub fn to_text(&self) -> String {
        self.glyph_text()
    }

    fn index(&self, point: CellPoint) -> Option<usize> {
        let x = i64::from(point.x) - i64::from(self.origin.x);
        let y = i64::from(point.y) - i64::from(self.origin.y);
        let x = u32::try_from(x).ok()?;
        let y = u32::try_from(y).ok()?;
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = u64::from(y)
            .checked_mul(u64::from(self.width))?
            .checked_add(u64::from(x))?;
        usize::try_from(index).ok()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CellBufferError {
    #[error("CellBuffer dimensions must be nonzero, got {width}x{height}")]
    ZeroDimension { width: u32, height: u32 },

    #[error("CellBuffer dimensions {width}x{height} exceed the host address space")]
    DimensionsTooLarge { width: u32, height: u32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WireConnections(u8);

impl WireConnections {
    const NORTH: u8 = 1 << 0;
    const EAST: u8 = 1 << 1;
    const SOUTH: u8 = 1 << 2;
    const WEST: u8 = 1 << 3;

    pub const fn new(north: bool, east: bool, south: bool, west: bool) -> Self {
        let mut bits = 0;
        if north {
            bits |= Self::NORTH;
        }
        if east {
            bits |= Self::EAST;
        }
        if south {
            bits |= Self::SOUTH;
        }
        if west {
            bits |= Self::WEST;
        }
        Self(bits)
    }

    pub const fn glyph(self) -> char {
        match self.0 {
            0b0101 => '│',
            0b1010 => '─',
            0b0011 => '└',
            0b1001 => '┘',
            0b0110 => '┌',
            0b1100 => '┐',
            0b0111 => '├',
            0b1101 => '┤',
            0b1110 => '┬',
            0b1011 => '┴',
            0b1111 => '┼',
            bits if bits & (Self::NORTH | Self::SOUTH) != 0 => '│',
            bits if bits & (Self::EAST | Self::WEST) != 0 => '─',
            _ => '─',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsciiPrimitive {
    Blocked {
        at: CellPoint,
    },
    Wire {
        at: CellPoint,
        entity: EntityId,
        connections: WireConnections,
        level: LogicLevel,
    },
    Gate {
        at: CellPoint,
        entity: EntityId,
        gate_type: GateType,
        level: LogicLevel,
    },
    Junction {
        at: CellPoint,
        entity: EntityId,
    },
    FixedSubstrate {
        at: CellPoint,
        entity: EntityId,
    },
    Ghost {
        at: CellPoint,
        glyph: char,
    },
    Selection {
        at: CellPoint,
    },
}

impl AsciiPrimitive {
    pub const fn cell_write(self) -> CellWrite {
        match self {
            Self::Blocked { at } => CellWrite::new(
                at,
                CellLayer::Terrain,
                CellVisual::new('#', CellTone::Neutral, None),
            ),
            Self::Wire {
                at,
                entity,
                connections,
                level,
            } => CellWrite::new(
                at,
                CellLayer::Wire,
                CellVisual::new(
                    connections.glyph(),
                    tone_for_logic(level),
                    Some(PresentationSource::Canonical(entity)),
                ),
            ),
            Self::Gate {
                at,
                entity,
                gate_type,
                level,
            } => CellWrite::new(
                at,
                CellLayer::GatePort,
                CellVisual::new(
                    gate_glyph(gate_type),
                    tone_for_logic(level),
                    Some(PresentationSource::Canonical(entity)),
                ),
            ),
            Self::Junction { at, entity } => CellWrite::new(
                at,
                CellLayer::Junction,
                CellVisual::new(
                    '●',
                    CellTone::Neutral,
                    Some(PresentationSource::Canonical(entity)),
                ),
            ),
            Self::FixedSubstrate { at, entity } => CellWrite::new(
                at,
                CellLayer::Substrate,
                CellVisual::new(
                    '■',
                    CellTone::Neutral,
                    Some(PresentationSource::Canonical(entity)),
                ),
            ),
            Self::Ghost { at, glyph } => CellWrite::new(
                at,
                CellLayer::GhostAndDebug,
                CellVisual::new(glyph, CellTone::Ghost, None),
            ),
            Self::Selection { at } => CellWrite::new(
                at,
                CellLayer::Selection,
                CellVisual::new(' ', CellTone::Highlight, None),
            ),
        }
    }
}

pub fn project_ascii_primitives(
    buffer: &mut CellBuffer,
    primitives: impl IntoIterator<Item = AsciiPrimitive>,
) {
    buffer.project(primitives.into_iter().map(AsciiPrimitive::cell_write));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPanel {
    title: String,
    lines: Vec<String>,
}

impl TextPanel {
    pub fn new(
        title: impl Into<String>,
        lines: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            title: title.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        }
    }

    pub fn from_cell_buffer(title: impl Into<String>, buffer: &CellBuffer) -> Self {
        Self::new(title, buffer.to_text().lines())
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn to_text(&self) -> String {
        self.rendered_lines().join("\n")
    }

    fn rendered_lines(&self) -> Vec<String> {
        let content_width = self
            .lines
            .iter()
            .map(|line| line.chars().count())
            .chain(std::iter::once(self.title.chars().count()))
            .max()
            .unwrap_or(0);
        let mut output = Vec::with_capacity(self.lines.len() + 2);
        output.push(format!("[{:<width$}]", self.title, width = content_width));
        for line in &self.lines {
            output.push(format!(" {:<width$} ", line, width = content_width));
        }
        if self.lines.is_empty() {
            output.push(" ".repeat(content_width + 2));
        }
        output
    }
}

pub fn compose_panels(panels: &[TextPanel], gap: usize) -> String {
    if panels.is_empty() {
        return String::new();
    }

    let rendered = panels
        .iter()
        .map(TextPanel::rendered_lines)
        .collect::<Vec<_>>();
    let widths = rendered
        .iter()
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let height = rendered.iter().map(Vec::len).max().unwrap_or(0);
    let separator = " ".repeat(gap);
    let mut output = String::new();

    for row in 0..height {
        if row != 0 {
            output.push('\n');
        }
        for (panel_index, lines) in rendered.iter().enumerate() {
            if panel_index != 0 {
                output.push_str(&separator);
            }
            let line = lines.get(row).map_or("", String::as_str);
            output.push_str(line);
            let padding = widths[panel_index].saturating_sub(line.chars().count());
            output.extend(std::iter::repeat_n(' ', padding));
        }
    }

    output
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
