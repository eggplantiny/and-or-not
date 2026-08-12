use crate::cell_buffer::{CellBuffer, CellTone, TextPanel};
use bevy::prelude::*;
use bevy::text::{FontSize, LineHeight};
use skrifa::prelude::{FontRef, LocationRef, MetadataProvider, Size};
use thiserror::Error;

pub const NOTO_SANS_MONO_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/noto-sans-mono/NotoSansMono-Regular.ttf"
));
pub const NOTO_SANS_MONO_LICENSE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/noto-sans-mono/OFL.txt"
));

const REQUIRED_GLYPHS: &[char] = &[
    '·', '#', '■', '&', '|', '!', '●', '○', '◉', '╳', '>', '<', '^', 'v', '─', '│', '└', '┘', '┌',
    '┐', '┬', '┴', '├', '┤', '┼', '0', '1', 'X', 'r', 'A', 'S', 'L', 'R', '?', '[', ']', ':', '-',
    '+', '/', ' ',
];

const FONT_SIZE_PX: f32 = 15.0;
const DOCUMENT_LEFT_PX: f32 = 12.0;
const DOCUMENT_TOP_PX: f32 = 12.0;
const ADVANCE_TOLERANCE_PX: f32 = 0.01;

/// An sRGBA8 presentation color. Keeping the palette in integer form makes the
/// CellTone-to-native-style contract exact and directly testable without a GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeProbeColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl NativeProbeColor {
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    const fn bevy(self) -> Color {
        Color::srgba_u8(self.red, self.green, self.blue, self.alpha)
    }
}

/// Exact foreground/background pair used for one contiguous native text run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeProbeStyle {
    pub foreground: NativeProbeColor,
    pub background: NativeProbeColor,
}

impl NativeProbeStyle {
    pub const NEUTRAL: Self = Self::new(
        NativeProbeColor::new(0xD2, 0xE0, 0xDA, 0xFF),
        NativeProbeColor::new(0x10, 0x18, 0x15, 0xFF),
    );
    pub const LOW: Self = Self::new(
        NativeProbeColor::new(0x6B, 0x7B, 0x75, 0xFF),
        NativeProbeColor::new(0x10, 0x18, 0x15, 0xFF),
    );
    pub const HIGH: Self = Self::new(
        NativeProbeColor::new(0x58, 0xFF, 0x8A, 0xFF),
        NativeProbeColor::new(0x10, 0x18, 0x15, 0xFF),
    );
    pub const UNKNOWN: Self = Self::new(
        NativeProbeColor::new(0xFF, 0xF1, 0xA8, 0xFF),
        NativeProbeColor::new(0x7A, 0x26, 0x1E, 0xFF),
    );
    pub const GHOST: Self = Self::new(
        NativeProbeColor::new(0xB8, 0x8B, 0x5A, 0xFF),
        NativeProbeColor::new(0x24, 0x1A, 0x12, 0xFF),
    );
    pub const HIGHLIGHT: Self = Self::new(
        NativeProbeColor::new(0x10, 0x18, 0x15, 0xFF),
        NativeProbeColor::new(0x8E, 0xE6, 0xD2, 0xFF),
    );

    pub const fn new(foreground: NativeProbeColor, background: NativeProbeColor) -> Self {
        Self {
            foreground,
            background,
        }
    }

    pub const fn for_cell_tone(tone: CellTone) -> Self {
        match tone {
            CellTone::Neutral => Self::NEUTRAL,
            CellTone::Low => Self::LOW,
            CellTone::High => Self::HIGH,
            CellTone::Unknown => Self::UNKNOWN,
            CellTone::Ghost => Self::GHOST,
            CellTone::Highlight => Self::HIGHLIGHT,
        }
    }
}

/// A single row-local sequence whose cells share one exact foreground and
/// background. A run never contains a line break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeProbeRun {
    row: usize,
    column: usize,
    text: String,
    style: NativeProbeStyle,
}

impl NativeProbeRun {
    pub const fn row(&self) -> usize {
        self.row
    }

    pub const fn column(&self) -> usize {
        self.column
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn style(&self) -> NativeProbeStyle {
        self.style
    }

    pub fn cell_count(&self) -> usize {
        self.text.chars().count()
    }
}

/// Snapshot-only native presentation plan. It contains no canonical identity
/// and therefore cannot be used for picking or simulation mutation.
#[derive(Clone, Debug, Default, Resource, PartialEq, Eq)]
pub struct NativeProbeDocument {
    text: String,
    rows: usize,
    columns: usize,
    runs: Vec<NativeProbeRun>,
}

impl NativeProbeDocument {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn row_count(&self) -> usize {
        self.rows
    }

    pub const fn column_count(&self) -> usize {
        self.columns
    }

    pub fn runs(&self) -> &[NativeProbeRun] {
        &self.runs
    }

    /// Replaces the document with neutral plain text. This compatibility API
    /// is suitable for status/error output that has no CellTone information.
    pub fn replace(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            if *self != Self::default() {
                *self = Self::default();
            }
            return;
        }
        let rows = text
            .split('\n')
            .map(|line| {
                line.chars()
                    .map(|glyph| StyledGlyph::new(glyph, NativeProbeStyle::NEUTRAL))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let next = Self::from_styled_rows(rows);
        if *self != next {
            *self = next;
        }
    }

    /// Replaces the document with a tone-preserving CellBuffer panel followed
    /// by ordinary neutral TextPanels. The result uses the same panel framing
    /// and horizontal gap convention as `compose_panels`, while retaining the
    /// authoritative CellTone of every grid cell.
    pub fn replace_cell_buffer_with_panels(
        &mut self,
        buffer_title: &str,
        buffer: &CellBuffer,
        panels: &[TextPanel],
        gap: usize,
    ) {
        let mut blocks = Vec::with_capacity(panels.len() + 1);
        blocks.push(styled_cell_buffer_panel(buffer_title, buffer));
        blocks.extend(panels.iter().map(styled_plain_panel));
        let next = Self::from_styled_rows(compose_styled_blocks(&blocks, gap));
        if *self != next {
            *self = next;
        }
    }

    /// Places the tone-preserving CellBuffer on the left and stacks ordinary TextPanels in one
    /// neutral column on the right. This keeps waveform and inspector evidence visible in a
    /// bounded native window even when the world grid is wide.
    pub fn replace_cell_buffer_with_stacked_panels(
        &mut self,
        buffer_title: &str,
        buffer: &CellBuffer,
        panels: &[TextPanel],
        horizontal_gap: usize,
        vertical_gap: usize,
    ) {
        let buffer_block = styled_cell_buffer_panel(buffer_title, buffer);
        let panel_blocks = panels.iter().map(styled_plain_panel).collect::<Vec<_>>();
        let panel_column = compose_styled_block_column(&panel_blocks, vertical_gap);
        let next = Self::from_styled_rows(compose_styled_blocks(
            &[buffer_block, panel_column],
            horizontal_gap,
        ));
        if *self != next {
            *self = next;
        }
    }

    fn from_styled_rows(rows: Vec<Vec<StyledGlyph>>) -> Self {
        let row_count = rows.len();
        let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut text = String::new();
        let mut runs = Vec::new();

        for (row_index, row) in rows.iter().enumerate() {
            if row_index != 0 {
                text.push('\n');
            }
            text.extend(row.iter().map(|glyph| glyph.glyph));

            let mut start = 0;
            while start < row.len() {
                let style = row[start].style;
                let mut end = start + 1;
                while end < row.len() && row[end].style == style {
                    end += 1;
                }
                runs.push(NativeProbeRun {
                    row: row_index,
                    column: start,
                    text: row[start..end].iter().map(|glyph| glyph.glyph).collect(),
                    style,
                });
                start = end;
            }
        }

        Self {
            text,
            rows: row_count,
            columns: column_count,
            runs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeProbeMetrics {
    font_size_px: f32,
    cell_advance_px: f32,
    line_height_px: f32,
}

impl NativeProbeMetrics {
    pub const fn font_size_px(self) -> f32 {
        self.font_size_px
    }

    pub const fn cell_advance_px(self) -> f32 {
        self.cell_advance_px
    }

    pub const fn line_height_px(self) -> f32 {
        self.line_height_px
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StyledGlyph {
    glyph: char,
    style: NativeProbeStyle,
}

impl StyledGlyph {
    const fn new(glyph: char, style: NativeProbeStyle) -> Self {
        Self { glyph, style }
    }

    const fn neutral(glyph: char) -> Self {
        Self::new(glyph, NativeProbeStyle::NEUTRAL)
    }
}

#[derive(Clone, Resource)]
struct NativeProbeFont(Handle<Font>);

#[derive(Clone, Copy, Resource)]
struct NativeProbeLayoutMetrics(NativeProbeMetrics);

#[derive(Default, Resource)]
struct NativeProbeRunPool {
    entities: Vec<Entity>,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum NativeProbeError {
    #[error("the embedded Noto Sans Mono asset is not a readable OpenType font")]
    InvalidEmbeddedFont,

    #[error("the embedded Noto Sans Mono asset is missing required glyph U+{codepoint:04X}")]
    MissingRequiredGlyph { codepoint: u32 },

    #[error("required glyph U+{codepoint:04X} does not share the embedded monospaced advance")]
    InconsistentRequiredGlyphAdvance { codepoint: u32 },

    #[error("the embedded Noto Sans Mono asset has unusable advance or line-height metrics")]
    InvalidEmbeddedFontMetrics,

    #[error("the embedded font license is not the SIL Open Font License 1.1 text")]
    InvalidEmbeddedFontLicense,
}

pub fn validate_embedded_font() -> Result<(), NativeProbeError> {
    embedded_font_metrics()?;
    let license = std::str::from_utf8(NOTO_SANS_MONO_LICENSE)
        .map_err(|_| NativeProbeError::InvalidEmbeddedFontLicense)?;
    if !license.contains("SIL OPEN FONT LICENSE Version 1.1") {
        return Err(NativeProbeError::InvalidEmbeddedFontLicense);
    }
    Ok(())
}

/// Measures the exact checked-in font at the renderer's fixed size. This is
/// public so a native smoke harness can report the actual cell grid metrics.
pub fn embedded_font_metrics() -> Result<NativeProbeMetrics, NativeProbeError> {
    measure_font_bytes(NOTO_SANS_MONO_BYTES)
}

pub fn install_native_probe_renderer(app: &mut App) -> Result<(), NativeProbeError> {
    validate_embedded_font()?;
    let metrics = embedded_font_metrics()?;
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(NOTO_SANS_MONO_BYTES.to_vec()));
    app.insert_resource(NativeProbeFont(handle));
    app.insert_resource(NativeProbeLayoutMetrics(metrics));
    app.init_resource::<NativeProbeDocument>();
    app.init_resource::<NativeProbeRunPool>();
    app.add_systems(Startup, spawn_native_probe_camera);
    app.add_systems(PostUpdate, sync_native_probe_runs);
    Ok(())
}

fn measure_font_bytes(bytes: &[u8]) -> Result<NativeProbeMetrics, NativeProbeError> {
    let font = FontRef::new(bytes).map_err(|_| NativeProbeError::InvalidEmbeddedFont)?;
    let global = font.metrics(Size::new(FONT_SIZE_PX), LocationRef::default());

    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(Size::new(FONT_SIZE_PX), LocationRef::default());
    let mut expected_advance = None;
    for glyph in REQUIRED_GLYPHS {
        let codepoint = u32::from(*glyph);
        let glyph_id = charmap
            .map(codepoint)
            .ok_or(NativeProbeError::MissingRequiredGlyph { codepoint })?;
        let advance = glyph_metrics
            .advance_width(glyph_id)
            .ok_or(NativeProbeError::InvalidEmbeddedFontMetrics)?;
        if !advance.is_finite() || advance <= 0.0 {
            return Err(NativeProbeError::InvalidEmbeddedFontMetrics);
        }
        if let Some(expected) = expected_advance
            && f32::abs(expected - advance) > ADVANCE_TOLERANCE_PX
        {
            return Err(NativeProbeError::InconsistentRequiredGlyphAdvance { codepoint });
        }
        expected_advance = Some(advance);
    }

    let cell_advance_px = expected_advance.ok_or(NativeProbeError::InvalidEmbeddedFontMetrics)?;
    let line_height_px = global.ascent - global.descent + global.leading;
    if !line_height_px.is_finite() || line_height_px <= 0.0 {
        return Err(NativeProbeError::InvalidEmbeddedFontMetrics);
    }

    Ok(NativeProbeMetrics {
        font_size_px: FONT_SIZE_PX,
        cell_advance_px,
        line_height_px,
    })
}

fn styled_cell_buffer_panel(title: &str, buffer: &CellBuffer) -> Vec<Vec<StyledGlyph>> {
    let width = usize::try_from(buffer.width())
        .unwrap_or(usize::MAX)
        .max(title.chars().count());
    let mut rows = Vec::with_capacity(
        usize::try_from(buffer.height())
            .unwrap_or(usize::MAX)
            .saturating_add(1),
    );
    let title_padding = width.saturating_sub(title.chars().count());
    let mut title_row = Vec::with_capacity(width.saturating_add(2));
    title_row.push(StyledGlyph::neutral('['));
    title_row.extend(title.chars().map(StyledGlyph::neutral));
    title_row.extend(std::iter::repeat_n(
        StyledGlyph::neutral(' '),
        title_padding,
    ));
    title_row.push(StyledGlyph::neutral(']'));
    rows.push(title_row);

    for buffer_row in buffer.rows() {
        let mut row = Vec::with_capacity(width.saturating_add(2));
        row.push(StyledGlyph::neutral(' '));
        row.extend(buffer_row.map(|visual| {
            StyledGlyph::new(visual.glyph, NativeProbeStyle::for_cell_tone(visual.tone))
        }));
        row.extend(std::iter::repeat_n(
            StyledGlyph::neutral(' '),
            width.saturating_sub(usize::try_from(buffer.width()).unwrap_or(usize::MAX)),
        ));
        row.push(StyledGlyph::neutral(' '));
        rows.push(row);
    }
    rows
}

fn styled_plain_panel(panel: &TextPanel) -> Vec<Vec<StyledGlyph>> {
    panel
        .to_text()
        .split('\n')
        .map(|line| line.chars().map(StyledGlyph::neutral).collect())
        .collect()
}

fn compose_styled_blocks(blocks: &[Vec<Vec<StyledGlyph>>], gap: usize) -> Vec<Vec<StyledGlyph>> {
    let widths = blocks
        .iter()
        .map(|block| block.iter().map(Vec::len).max().unwrap_or(0))
        .collect::<Vec<_>>();
    let height = blocks.iter().map(Vec::len).max().unwrap_or(0);
    let mut rows = Vec::with_capacity(height);

    for row_index in 0..height {
        let mut row = Vec::new();
        for (block_index, block) in blocks.iter().enumerate() {
            if block_index != 0 {
                row.extend(std::iter::repeat_n(StyledGlyph::neutral(' '), gap));
            }
            let source = block.get(row_index).map_or(&[][..], Vec::as_slice);
            row.extend_from_slice(source);
            row.extend(std::iter::repeat_n(
                StyledGlyph::neutral(' '),
                widths[block_index].saturating_sub(source.len()),
            ));
        }
        rows.push(row);
    }
    rows
}

fn compose_styled_block_column(
    blocks: &[Vec<Vec<StyledGlyph>>],
    gap: usize,
) -> Vec<Vec<StyledGlyph>> {
    let width = blocks
        .iter()
        .flat_map(|block| block.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    let total_rows = blocks
        .iter()
        .map(Vec::len)
        .sum::<usize>()
        .saturating_add(gap.saturating_mul(blocks.len().saturating_sub(1)));
    let mut rows = Vec::with_capacity(total_rows);
    for (block_index, block) in blocks.iter().enumerate() {
        if block_index != 0 {
            rows.extend(std::iter::repeat_n(
                vec![StyledGlyph::neutral(' '); width],
                gap,
            ));
        }
        rows.extend(block.iter().map(|source| {
            let mut row = source.clone();
            row.extend(std::iter::repeat_n(
                StyledGlyph::neutral(' '),
                width.saturating_sub(row.len()),
            ));
            row
        }));
    }
    rows
}

fn spawn_native_probe_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn sync_native_probe_runs(
    mut commands: Commands,
    document: Res<NativeProbeDocument>,
    font: Res<NativeProbeFont>,
    metrics: Res<NativeProbeLayoutMetrics>,
    mut pool: ResMut<NativeProbeRunPool>,
) {
    if !document.is_changed() {
        return;
    }

    while pool.entities.len() < document.runs.len() {
        let entity = commands.spawn_empty().id();
        pool.entities.push(entity);
    }

    for (slot, entity) in pool.entities.iter().copied().enumerate() {
        let mut entity_commands = commands.entity(entity);
        if let Some(run) = document.runs.get(slot) {
            entity_commands.insert(active_run_components(run, &font.0, metrics.0));
        } else {
            entity_commands.insert((
                Text::new(""),
                BackgroundColor(Color::NONE),
                Node {
                    display: Display::None,
                    ..default()
                },
            ));
        }
    }
}

fn active_run_components(
    run: &NativeProbeRun,
    font: &Handle<Font>,
    metrics: NativeProbeMetrics,
) -> impl Bundle {
    let left = DOCUMENT_LEFT_PX + run.column as f32 * metrics.cell_advance_px;
    let top = DOCUMENT_TOP_PX + run.row as f32 * metrics.line_height_px;
    let width = run.cell_count() as f32 * metrics.cell_advance_px;
    (
        Text::new(run.text.clone()),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(metrics.font_size_px),
            ..default()
        },
        LineHeight::Px(metrics.line_height_px),
        TextColor(run.style.foreground.bevy()),
        BackgroundColor(run.style.background.bevy()),
        TextLayout::no_wrap().with_justify(Justify::Left),
        Node {
            display: Display::Block,
            position_type: PositionType::Absolute,
            overflow: Overflow::clip(),
            left: px(left),
            top: px(top),
            width: px(width),
            height: px(metrics.line_height_px),
            ..default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_buffer::{CellLayer, CellPoint, CellVisual, CellWrite};

    #[test]
    fn checked_in_font_and_license_are_valid_and_cover_the_probe_glyphs() {
        validate_embedded_font().expect("checked-in Noto Sans Mono and OFL are valid");
        let metrics = embedded_font_metrics().expect("font metrics are measurable");
        assert!(NOTO_SANS_MONO_BYTES.len() > 500_000);
        assert!(NOTO_SANS_MONO_LICENSE.starts_with(b"Copyright 2018 The Noto Project Authors"));
        assert_eq!(metrics.font_size_px(), FONT_SIZE_PX);
        assert!(metrics.cell_advance_px() > 0.0);
        assert!(metrics.line_height_px() >= metrics.font_size_px());
    }

    #[test]
    fn malformed_font_is_a_typed_startup_error() {
        assert_eq!(
            measure_font_bytes(b"not a font"),
            Err(NativeProbeError::InvalidEmbeddedFont)
        );
    }

    #[test]
    fn cell_tones_have_exact_distinguishable_native_styles() {
        let styles = [
            NativeProbeStyle::for_cell_tone(CellTone::Neutral),
            NativeProbeStyle::for_cell_tone(CellTone::Low),
            NativeProbeStyle::for_cell_tone(CellTone::High),
            NativeProbeStyle::for_cell_tone(CellTone::Unknown),
            NativeProbeStyle::for_cell_tone(CellTone::Ghost),
            NativeProbeStyle::for_cell_tone(CellTone::Highlight),
        ];
        for (index, style) in styles.iter().enumerate() {
            assert!(!styles[..index].contains(style));
        }
        assert_ne!(
            NativeProbeStyle::LOW.foreground,
            NativeProbeStyle::HIGH.foreground
        );
        assert_ne!(
            NativeProbeStyle::UNKNOWN.background,
            NativeProbeStyle::LOW.background
        );
        assert_eq!(
            NativeProbeStyle::HIGHLIGHT.foreground,
            NativeProbeStyle::NEUTRAL.background
        );
        assert_ne!(
            NativeProbeStyle::HIGHLIGHT.background,
            NativeProbeStyle::GHOST.background
        );
    }

    #[test]
    fn document_coalesces_only_adjacent_equal_row_styles() {
        let mut buffer = CellBuffer::new(CellPoint::new(0, 0), 6, 1).expect("valid buffer");
        for (x, (glyph, tone)) in [
            ('n', CellTone::Neutral),
            ('l', CellTone::Low),
            ('l', CellTone::Low),
            ('h', CellTone::High),
            ('x', CellTone::Unknown),
            ('s', CellTone::Highlight),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(buffer.write(CellWrite::new(
                CellPoint::new(i32::try_from(x).expect("small coordinate"), 0),
                CellLayer::Wire,
                CellVisual::new(glyph, tone, None),
            )));
        }
        let status = TextPanel::new("Status", ["PAUSED"]);
        let mut document = NativeProbeDocument::default();
        document.replace_cell_buffer_with_panels("Grid", &buffer, &[status], 2);

        let low_run = document
            .runs()
            .iter()
            .find(|run| run.text() == "ll")
            .expect("adjacent LOW cells share one run");
        assert_eq!(low_run.row(), 1);
        assert_eq!(low_run.column(), 2);
        assert_eq!(low_run.style(), NativeProbeStyle::LOW);
        assert!(
            document
                .runs()
                .iter()
                .any(|run| { run.text() == "x" && run.style() == NativeProbeStyle::UNKNOWN })
        );
        assert!(
            document
                .runs()
                .iter()
                .any(|run| { run.text() == "s" && run.style() == NativeProbeStyle::HIGHLIGHT })
        );
        assert!(document.text().contains("[Status]"));
    }

    #[test]
    fn run_pool_reuses_entities_and_hides_surplus_slots() {
        let metrics = embedded_font_metrics().expect("font metrics");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(NativeProbeFont(Handle::default()));
        app.insert_resource(NativeProbeLayoutMetrics(metrics));
        app.init_resource::<NativeProbeDocument>();
        app.init_resource::<NativeProbeRunPool>();
        app.add_systems(Update, sync_native_probe_runs);

        app.world_mut()
            .resource_mut::<NativeProbeDocument>()
            .replace("first\nsecond");
        app.update();
        let original_entities = app
            .world()
            .resource::<NativeProbeRunPool>()
            .entities
            .clone();
        assert_eq!(original_entities.len(), 2);

        app.world_mut()
            .resource_mut::<NativeProbeDocument>()
            .replace("replacement");
        app.update();
        let pool = app.world().resource::<NativeProbeRunPool>();
        assert_eq!(pool.entities, original_entities);
        let slots = original_entities
            .iter()
            .enumerate()
            .map(|(slot, &entity)| {
                let entity = app.world().entity(entity);
                let text = entity.get::<Text>().expect("pooled run has Text");
                let node = entity.get::<Node>().expect("pooled run has Node");
                (slot, text.0.clone(), node.display)
            })
            .collect::<Vec<_>>();
        assert_eq!(slots[0], (0, "replacement".to_owned(), Display::Block));
        assert_eq!(slots[1], (1, String::new(), Display::None));
    }
}
