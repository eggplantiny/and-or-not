use aon_app::cell_buffer::{
    AsciiPrimitive, CellBuffer, CellLayer, CellPoint, CellTone, CellVisual, CellWrite,
    PresentationSource, TextPanel, WireConnections, compose_panels, project_ascii_primitives,
};
use aon_sim::{EntityId, GateType, LogicLevel};

fn golden_primitives() -> Vec<AsciiPrimitive> {
    vec![
        AsciiPrimitive::FixedSubstrate {
            at: CellPoint::new(0, 4),
            entity: EntityId(10),
        },
        AsciiPrimitive::Wire {
            at: CellPoint::new(1, 2),
            entity: EntityId(1),
            connections: WireConnections::new(false, true, false, true),
            level: LogicLevel::High,
        },
        AsciiPrimitive::Wire {
            at: CellPoint::new(1, 2),
            entity: EntityId(7),
            connections: WireConnections::new(false, true, false, true),
            level: LogicLevel::High,
        },
        AsciiPrimitive::Wire {
            at: CellPoint::new(2, 2),
            entity: EntityId(2),
            connections: WireConnections::new(false, true, false, true),
            level: LogicLevel::X,
        },
        AsciiPrimitive::Wire {
            at: CellPoint::new(3, 2),
            entity: EntityId(3),
            connections: WireConnections::new(true, true, true, true),
            level: LogicLevel::Low,
        },
        AsciiPrimitive::Junction {
            at: CellPoint::new(3, 2),
            entity: EntityId(30),
        },
        AsciiPrimitive::Gate {
            at: CellPoint::new(4, 2),
            entity: EntityId(4),
            gate_type: GateType::Not,
            level: LogicLevel::High,
        },
        AsciiPrimitive::Gate {
            at: CellPoint::new(5, 2),
            entity: EntityId(5),
            gate_type: GateType::And,
            level: LogicLevel::Low,
        },
        AsciiPrimitive::Gate {
            at: CellPoint::new(6, 2),
            entity: EntityId(6),
            gate_type: GateType::Or,
            level: LogicLevel::X,
        },
        AsciiPrimitive::Ghost {
            at: CellPoint::new(3, 2),
            glyph: '?',
        },
        AsciiPrimitive::Selection {
            at: CellPoint::new(4, 2),
        },
        AsciiPrimitive::FixedSubstrate {
            at: CellPoint::new(0, 0),
            entity: EntityId(20),
        },
        AsciiPrimitive::Blocked {
            at: CellPoint::new(6, 0),
        },
    ]
}

#[test]
fn cell_projection_has_stable_layers_screen_orientation_and_golden_text() {
    let mut forward = CellBuffer::new(CellPoint::new(0, 0), 7, 5).expect("buffer fits");
    project_ascii_primitives(&mut forward, golden_primitives());
    forward.write(CellWrite::new(
        CellPoint::new(2, 2),
        CellLayer::GhostAndDebug,
        CellVisual::new('^', CellTone::Highlight, None),
    ));

    let mut primitives = golden_primitives();
    primitives.reverse();
    let mut reversed = CellBuffer::new(CellPoint::new(0, 0), 7, 5).expect("buffer fits");
    project_ascii_primitives(&mut reversed, primitives);
    reversed.write(CellWrite::new(
        CellPoint::new(2, 2),
        CellLayer::GhostAndDebug,
        CellVisual::new('^', CellTone::Highlight, None),
    ));

    let expected = "■······\n·······\n·─^?!&|\n·······\n■·····#";
    assert_eq!(forward.to_text(), expected);
    assert_eq!(forward, reversed);

    assert_eq!(
        forward.pick(CellPoint::new(1, 2)),
        Some(PresentationSource::Canonical(EntityId(7))),
        "the higher EntityId wins an ordinary same-layer visual and pick tie"
    );
    assert_eq!(
        forward.pick(CellPoint::new(2, 2)),
        Some(PresentationSource::Canonical(EntityId(2))),
        "a source-less debug layer must let picking fall through"
    );
    assert_eq!(
        forward.pick(CellPoint::new(3, 2)),
        Some(PresentationSource::Canonical(EntityId(30))),
        "ghost overlays must not replace canonical picking"
    );
    assert_eq!(
        forward.pick(CellPoint::new(4, 2)),
        Some(PresentationSource::Canonical(EntityId(4)))
    );
    assert_eq!(
        forward.visual(CellPoint::new(6, 2)).unwrap().tone,
        CellTone::Unknown
    );
    assert_eq!(forward.visual(CellPoint::new(99, 99)), None);
}

#[test]
fn wire_connectivity_and_panel_composition_are_stable_text_projections() {
    let cases = [
        (WireConnections::new(true, false, true, false), '│'),
        (WireConnections::new(false, true, false, true), '─'),
        (WireConnections::new(true, true, false, false), '└'),
        (WireConnections::new(false, true, true, false), '┌'),
        (WireConnections::new(true, true, true, false), '├'),
        (WireConnections::new(true, true, true, true), '┼'),
    ];
    for (connections, expected) in cases {
        assert_eq!(connections.glyph(), expected);
    }

    let left = TextPanel::new("Grid", ["@!"]);
    let right = TextPanel::new("Inspector", ["tick=7", "logic=HIGH"]);
    assert_eq!(
        compose_panels(&[left, right], 2),
        "[Grid]  [Inspector ]\n @!     tick=7     \n        logic=HIGH "
    );
}

#[test]
fn zero_sized_buffers_are_rejected_and_offscreen_writes_are_clipped() {
    assert!(CellBuffer::new(CellPoint::new(0, 0), 0, 1).is_err());
    let mut buffer = CellBuffer::new(CellPoint::new(-2, -2), 2, 2).expect("buffer fits");
    assert!(!buffer.write(CellWrite::new(
        CellPoint::new(i32::MAX, i32::MAX),
        CellLayer::GhostAndDebug,
        CellVisual::new('!', CellTone::Highlight, None),
    )));
    assert_eq!(buffer.to_text(), "··\n··");
}
