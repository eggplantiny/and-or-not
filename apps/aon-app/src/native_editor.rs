use crate::cell_buffer::CellPoint;
use crate::host_action::HostAction;
use crate::laboratory::LaboratorySession;
use crate::presenter::{
    PickTarget, PresenterError, SnapshotPresentation, ViewMode, Viewport, project_snapshot,
};
use aon_sim::{
    BindPortCommand, Command, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GatePort,
    GateType, LogicLevel, PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand,
    RoutingDomain, SetExternalDriverCommand, SignalProbeTarget,
};
use bevy::prelude::Resource;
use thiserror::Error;

const DEFAULT_SUBSTRATE_HALF_EXTENT_CELLS: i64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEditorControl {
    Move { dx: i32, dy: i32 },
    Pick,
    Cancel,
    NetworkView,
    CircuitView,
    PlaceGate(GateType),
    PlaceJunction,
    PlaceFixedSubstrate,
    PlaceMobileSubstrate,
    WireAnchor,
    DeleteSelection,
    BindSelection,
    UnbindSelection,
    DriveSelection(LogicLevel),
    AddProbe,
    RemoveProbe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WireAnchor {
    cell: CellPoint,
    point: FixedVec2,
    domain: RoutingDomain,
}

#[derive(Clone, Debug, Resource)]
pub struct NativeEditorState {
    physical: PhysicalScaleProfile,
    viewport: Viewport,
    cursor: CellPoint,
    wire_anchor: Option<WireAnchor>,
    feedback: String,
}

impl NativeEditorState {
    pub fn new(physical: PhysicalScaleProfile, viewport: Viewport) -> Self {
        Self {
            physical,
            viewport,
            cursor: CellPoint::default(),
            wire_anchor: None,
            feedback: "ready".to_owned(),
        }
    }

    pub const fn cursor(&self) -> CellPoint {
        self.cursor
    }

    pub const fn wire_anchor_cell(&self) -> Option<CellPoint> {
        match self.wire_anchor {
            Some(anchor) => Some(anchor.cell),
            None => None,
        }
    }

    pub fn feedback(&self) -> &str {
        &self.feedback
    }

    pub fn clear_transient(&mut self) {
        self.wire_anchor = None;
        self.feedback = "session reset".to_owned();
    }

    pub fn presentation(
        &self,
        session: &LaboratorySession,
    ) -> Result<SnapshotPresentation, PresenterError> {
        project_snapshot(
            session.latest_snapshot(),
            &self.physical,
            session.view(),
            self.viewport,
        )
    }

    pub fn apply_control(
        &mut self,
        session: &LaboratorySession,
        control: NativeEditorControl,
    ) -> Result<Vec<HostAction>, NativeEditorError> {
        let result = self.apply_control_inner(session, control);
        match &result {
            Ok(actions) => {
                if !matches!(control, NativeEditorControl::Move { .. })
                    && !matches!(control, NativeEditorControl::WireAnchor)
                {
                    self.feedback = control.success_message(actions.len()).to_owned();
                }
            }
            Err(error) => self.feedback = error.to_string(),
        }
        result
    }

    fn apply_control_inner(
        &mut self,
        session: &LaboratorySession,
        control: NativeEditorControl,
    ) -> Result<Vec<HostAction>, NativeEditorError> {
        match control {
            NativeEditorControl::Move { dx, dy } => {
                self.cursor = CellPoint::new(
                    self.cursor
                        .x
                        .checked_add(dx)
                        .ok_or(NativeEditorError::CursorOverflow)?,
                    self.cursor
                        .y
                        .checked_add(dy)
                        .ok_or(NativeEditorError::CursorOverflow)?,
                );
                self.feedback = format!("cursor=({}, {})", self.cursor.x, self.cursor.y);
                Ok(Vec::new())
            }
            NativeEditorControl::Pick => {
                let target = self.presentation(session)?.primary_pick(self.cursor);
                Ok(vec![
                    target.map_or(HostAction::ClearSelection, HostAction::Select),
                ])
            }
            NativeEditorControl::Cancel => {
                self.wire_anchor = None;
                Ok(vec![HostAction::ClearPreview, HostAction::ClearSelection])
            }
            NativeEditorControl::NetworkView => {
                self.wire_anchor = None;
                Ok(vec![HostAction::SetView(ViewMode::Network)])
            }
            NativeEditorControl::CircuitView => {
                let substrate = selected_entity(session)?;
                let fixed = session
                    .latest_snapshot()
                    .fixed_substrates()
                    .iter()
                    .any(|record| record.id == substrate);
                let mobile = session
                    .latest_snapshot()
                    .mobiles()
                    .iter()
                    .any(|record| record.id.entity_id() == substrate);
                if !fixed && !mobile {
                    return Err(NativeEditorError::SelectionIsNotCircuitSubstrate);
                }
                self.wire_anchor = None;
                Ok(vec![HostAction::SetView(ViewMode::Circuit { substrate })])
            }
            NativeEditorControl::PlaceGate(gate_type) => {
                let (origin, routing_domain) = self.cursor_target(session)?;
                Ok(vec![HostAction::QueueEdit(Command::PlaceGate(
                    PlaceGateCommand {
                        gate_type,
                        origin,
                        routing_domain,
                    },
                ))])
            }
            NativeEditorControl::PlaceJunction => {
                let (position, routing_domain) = self.cursor_target(session)?;
                Ok(vec![HostAction::QueueEdit(Command::PlaceJunction(
                    PlaceJunctionCommand {
                        routing_domain,
                        position,
                    },
                ))])
            }
            NativeEditorControl::PlaceFixedSubstrate => {
                if session.view() != ViewMode::Network {
                    return Err(NativeEditorError::SubstrateRequiresNetworkView);
                }
                let (origin, _) = self.cursor_target(session)?;
                let half_extent = self
                    .physical
                    .world_routing_pitch
                    .0
                    .checked_mul(DEFAULT_SUBSTRATE_HALF_EXTENT_CELLS)
                    .ok_or(NativeEditorError::CoordinateOverflow)?;
                let bounds = FixedAabb::new(
                    FixedVec2::new(Fixed(-half_extent), Fixed(-half_extent)),
                    FixedVec2::new(Fixed(half_extent), Fixed(half_extent)),
                );
                Ok(vec![HostAction::QueueEdit(Command::PlaceFixedSubstrate(
                    PlaceFixedSubstrateCommand {
                        origin,
                        routing_area: bounds,
                        footprint: bounds,
                    },
                ))])
            }
            NativeEditorControl::PlaceMobileSubstrate => {
                if session.view() != ViewMode::Network {
                    return Err(NativeEditorError::SubstrateRequiresNetworkView);
                }
                let (origin, _) = self.cursor_target(session)?;
                let half_extent = self
                    .physical
                    .circuit_routing_pitch
                    .0
                    .checked_mul(DEFAULT_SUBSTRATE_HALF_EXTENT_CELLS)
                    .ok_or(NativeEditorError::CoordinateOverflow)?;
                let bounds = FixedAabb::new(
                    FixedVec2::new(Fixed(-half_extent), Fixed(-half_extent)),
                    FixedVec2::new(Fixed(half_extent), Fixed(half_extent)),
                );
                Ok(vec![HostAction::QueueEdit(Command::PlaceMobileSubstrate(
                    PlaceMobileSubstrateCommand {
                        origin,
                        routing_area: bounds,
                        footprint: bounds,
                    },
                ))])
            }
            NativeEditorControl::WireAnchor => self.toggle_wire_anchor(session),
            NativeEditorControl::DeleteSelection => Ok(vec![HostAction::QueueEdit(
                Command::RemoveEntity(RemoveEntityCommand {
                    target: selected_entity(session)?,
                }),
            )]),
            NativeEditorControl::BindSelection => {
                let PickTarget::WireEnd { wire, end } = selected_target(session)? else {
                    return Err(NativeEditorError::SelectionIsNotWireEnd);
                };
                let presentation = self.presentation(session)?;
                let target = presentation
                    .pick_targets(self.cursor)
                    .iter()
                    .copied()
                    .find_map(|candidate| endpoint_target(session, candidate))
                    .ok_or(NativeEditorError::CursorHasNoBindableTarget)?;
                Ok(vec![HostAction::QueueEdit(Command::BindPort(
                    BindPortCommand { wire, end, target },
                ))])
            }
            NativeEditorControl::UnbindSelection => {
                let PickTarget::WireEnd { wire, end } = selected_target(session)? else {
                    return Err(NativeEditorError::SelectionIsNotWireEnd);
                };
                Ok(vec![HostAction::QueueEdit(Command::BindPort(
                    BindPortCommand {
                        wire,
                        end,
                        target: EndpointTarget::Free,
                    },
                ))])
            }
            NativeEditorControl::DriveSelection(level) => {
                let driver = selected_external_driver(session)?;
                Ok(vec![HostAction::QueueEdit(Command::SetExternalDriver(
                    SetExternalDriverCommand {
                        driver,
                        level,
                        strength: session.nominal_external_drive_strength(),
                    },
                ))])
            }
            NativeEditorControl::AddProbe => {
                Ok(vec![HostAction::AddProbe(selected_probe_target(session)?)])
            }
            NativeEditorControl::RemoveProbe => Ok(vec![HostAction::RemoveProbe(
                selected_probe_target(session)?,
            )]),
        }
    }

    fn toggle_wire_anchor(
        &mut self,
        session: &LaboratorySession,
    ) -> Result<Vec<HostAction>, NativeEditorError> {
        let (point, domain) = self.cursor_target(session)?;
        let Some(anchor) = self.wire_anchor.take() else {
            self.wire_anchor = Some(WireAnchor {
                cell: self.cursor,
                point,
                domain,
            });
            self.feedback = format!("wire start=({}, {})", self.cursor.x, self.cursor.y);
            return Ok(Vec::new());
        };
        if anchor.domain != domain {
            return Err(NativeEditorError::WireCrossesViewDomain);
        }
        if anchor.point == point {
            self.wire_anchor = Some(anchor);
            return Err(NativeEditorError::WireHasZeroLength);
        }
        self.feedback = "wire queued".to_owned();
        Ok(vec![HostAction::QueueEdit(Command::PlaceWire(
            PlaceWireCommand {
                routing_domain: domain,
                points: vec![anchor.point, point],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            },
        ))])
    }

    fn cursor_target(
        &self,
        session: &LaboratorySession,
    ) -> Result<(FixedVec2, RoutingDomain), NativeEditorError> {
        let (coordinate_origin, pitch, domain) = match session.view() {
            ViewMode::Network => (
                FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                self.physical.world_routing_pitch,
                RoutingDomain::OpenWorld,
            ),
            ViewMode::Circuit { substrate } => {
                if let Some(origin) = session
                    .latest_snapshot()
                    .fixed_substrates()
                    .iter()
                    .find(|record| record.id == substrate)
                    .map(|record| record.origin)
                {
                    (
                        origin,
                        self.physical.circuit_routing_pitch,
                        RoutingDomain::FixedSubstrate(substrate),
                    )
                } else if session
                    .latest_snapshot()
                    .mobiles()
                    .iter()
                    .any(|record| record.id.entity_id() == substrate)
                {
                    (
                        FixedVec2::default(),
                        self.physical.circuit_routing_pitch,
                        RoutingDomain::MobileSubstrate(substrate),
                    )
                } else {
                    return Err(NativeEditorError::MissingCircuitSubstrate);
                }
            }
        };
        let x = i64::from(self.cursor.x)
            .checked_mul(pitch.0)
            .and_then(|value| value.checked_add(coordinate_origin.x.0))
            .ok_or(NativeEditorError::CoordinateOverflow)?;
        let y = i64::from(self.cursor.y)
            .checked_mul(pitch.0)
            .and_then(|value| value.checked_add(coordinate_origin.y.0))
            .ok_or(NativeEditorError::CoordinateOverflow)?;
        Ok((FixedVec2::new(Fixed(x), Fixed(y)), domain))
    }
}

impl NativeEditorControl {
    const fn success_message(self, action_count: usize) -> &'static str {
        match self {
            Self::Pick => "selection updated",
            Self::Cancel => "selection and preview cleared",
            Self::NetworkView => "network view queued",
            Self::CircuitView => "circuit view queued",
            Self::PlaceGate(_) => "gate queued",
            Self::PlaceJunction => "junction queued",
            Self::PlaceFixedSubstrate => "fixed substrate queued",
            Self::PlaceMobileSubstrate => "mobile substrate queued",
            Self::DeleteSelection => "delete queued",
            Self::BindSelection => "binding queued",
            Self::UnbindSelection => "unbind queued",
            Self::DriveSelection(_) => "external drive queued",
            Self::AddProbe => "probe add queued",
            Self::RemoveProbe => "probe remove queued",
            Self::Move { .. } => "cursor moved",
            Self::WireAnchor if action_count == 0 => "wire anchor set",
            Self::WireAnchor => "wire queued",
        }
    }
}

fn selected_target(session: &LaboratorySession) -> Result<PickTarget, NativeEditorError> {
    session
        .selection()
        .ok_or(NativeEditorError::NothingSelected)
}

fn selected_entity(session: &LaboratorySession) -> Result<EntityId, NativeEditorError> {
    Ok(selected_target(session)?.parent_entity())
}

fn endpoint_target(session: &LaboratorySession, candidate: PickTarget) -> Option<EndpointTarget> {
    match candidate {
        PickTarget::GatePort(reference) => Some(EndpointTarget::GatePort(reference)),
        PickTarget::MobilePort(reference) => Some(EndpointTarget::MobilePort(reference)),
        PickTarget::Entity(entity) => session
            .latest_snapshot()
            .junctions()
            .iter()
            .find(|record| record.id.entity_id() == entity)
            .map(|record| EndpointTarget::Junction(record.id)),
        PickTarget::WireEnd { .. } => None,
    }
}

fn selected_external_driver(
    session: &LaboratorySession,
) -> Result<aon_sim::DriverId, NativeEditorError> {
    let PickTarget::GatePort(reference) = selected_target(session)? else {
        return Err(NativeEditorError::SelectionIsNotGateInput);
    };
    let gate = session
        .latest_snapshot()
        .gates()
        .iter()
        .find(|gate| gate.id == reference.gate)
        .ok_or(NativeEditorError::SelectionIsNotGateInput)?;
    match reference.port {
        GatePort::InputA => Ok(gate.ports.input_a.external_driver),
        GatePort::InputB => gate
            .ports
            .input_b
            .map(|input| input.external_driver)
            .ok_or(NativeEditorError::SelectionIsNotGateInput),
        GatePort::Output | GatePort::Power => Err(NativeEditorError::SelectionIsNotGateInput),
    }
}

fn selected_probe_target(
    session: &LaboratorySession,
) -> Result<SignalProbeTarget, NativeEditorError> {
    let selection = selected_target(session)?;
    match selection {
        PickTarget::GatePort(reference) => match reference.port {
            GatePort::InputA => Ok(SignalProbeTarget::GateInputA(reference.gate)),
            GatePort::InputB => Ok(SignalProbeTarget::GateInputB(reference.gate)),
            GatePort::Output => Ok(SignalProbeTarget::GateOutput(reference.gate)),
            GatePort::Power => Err(NativeEditorError::SelectionCannotBeProbed),
        },
        PickTarget::MobilePort(reference) => {
            let mobile = session
                .latest_snapshot()
                .mobiles()
                .iter()
                .find(|mobile| mobile.id == reference.mobile)
                .ok_or(NativeEditorError::SelectionCannotBeProbed)?;
            Ok(SignalProbeTarget::Sink(match reference.port {
                aon_sim::MobilePort::Stop => mobile.ports.stop,
                aon_sim::MobilePort::Left => mobile.ports.left,
                aon_sim::MobilePort::Right => mobile.ports.right,
                aon_sim::MobilePort::Build => mobile
                    .ports
                    .build
                    .ok_or(NativeEditorError::SelectionCannotBeProbed)?,
            }))
        }
        PickTarget::WireEnd { wire, .. } => Ok(SignalProbeTarget::Wire(wire)),
        PickTarget::Entity(entity) => {
            if let Some(gate) = session
                .latest_snapshot()
                .gates()
                .iter()
                .find(|record| record.id.entity_id() == entity)
            {
                return Ok(SignalProbeTarget::GateOutput(gate.id));
            }
            session
                .latest_snapshot()
                .wires()
                .iter()
                .find(|record| record.id.entity_id() == entity)
                .map(|record| SignalProbeTarget::Wire(record.id))
                .ok_or(NativeEditorError::SelectionCannotBeProbed)
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum NativeEditorError {
    #[error(transparent)]
    Presenter(#[from] PresenterError),

    #[error("the editor cursor exceeded the supported host coordinate range")]
    CursorOverflow,

    #[error("the selected grid coordinate exceeds the fixed-point world range")]
    CoordinateOverflow,

    #[error("the active Circuit View substrate is no longer present")]
    MissingCircuitSubstrate,

    #[error("fixed substrates can only be placed from Network View")]
    SubstrateRequiresNetworkView,

    #[error("the wire endpoints must remain in the same routing domain")]
    WireCrossesViewDomain,

    #[error("the wire endpoint must differ from its start point")]
    WireHasZeroLength,

    #[error("nothing is selected")]
    NothingSelected,

    #[error("the selected entity is not a fixed or mobile circuit substrate")]
    SelectionIsNotCircuitSubstrate,

    #[error("select a Wire end before binding or unbinding")]
    SelectionIsNotWireEnd,

    #[error("the cursor has no Gate port or Junction binding target")]
    CursorHasNoBindableTarget,

    #[error("select a Gate input port before setting an external drive")]
    SelectionIsNotGateInput,

    #[error("the selected target cannot be probed")]
    SelectionCannotBeProbed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded_empty_package;
    use aon_sim::{
        GateId, GatePortRef, JunctionId, PlaceJunctionCommand, PlaceWireCommand, WireEnd, WireId,
    };

    fn editor_and_session() -> (NativeEditorState, LaboratorySession) {
        let package = embedded_empty_package().expect("embedded package");
        let physical = package.profiles().physical_scale.clone();
        let session = LaboratorySession::new(package).expect("laboratory starts");
        (
            NativeEditorState::new(physical, Viewport::new(CellPoint::new(-30, -12), 61, 25)),
            session,
        )
    }

    #[test]
    fn keyboard_editor_places_substrate_enters_circuit_and_queues_gate() {
        let (mut editor, mut session) = editor_and_session();
        let initial_hash = session.state_hash();
        let actions = editor
            .apply_control(&session, NativeEditorControl::PlaceFixedSubstrate)
            .expect("substrate action");
        assert_eq!(actions.len(), 1);
        for action in actions {
            session.apply_host_action(action).expect("action queues");
        }
        assert_eq!(session.state_hash(), initial_hash);
        session.step_once().expect("substrate step");
        assert_eq!(session.latest_snapshot().fixed_substrates().len(), 1);

        session.set_selection(Some(PickTarget::Entity(EntityId(1))));
        for action in editor
            .apply_control(&session, NativeEditorControl::CircuitView)
            .expect("circuit action")
        {
            session.apply_host_action(action).expect("view changes");
        }
        assert_eq!(
            session.view(),
            ViewMode::Circuit {
                substrate: EntityId(1)
            }
        );
        let actions = editor
            .apply_control(&session, NativeEditorControl::PlaceGate(GateType::Not))
            .expect("gate action");
        assert!(matches!(
            actions.as_slice(),
            [HostAction::QueueEdit(Command::PlaceGate(
                PlaceGateCommand {
                    routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                    ..
                }
            ))]
        ));
    }

    #[test]
    fn keyboard_editor_places_mobile_and_edits_its_local_circuit_domain() {
        let (mut editor, mut session) = editor_and_session();
        let world_pitch = editor.physical.world_routing_pitch.0;
        let circuit_pitch = editor.physical.circuit_routing_pitch.0;
        session
            .queue_command(Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![
                    FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                    FixedVec2::new(Fixed(4 * world_pitch), Fixed::ZERO),
                ],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }))
            .expect("track queues");
        session.step_once().expect("track places");

        editor
            .apply_control(&session, NativeEditorControl::Move { dx: 1, dy: 0 })
            .expect("cursor moves onto track");
        for action in editor
            .apply_control(&session, NativeEditorControl::PlaceMobileSubstrate)
            .expect("mobile action")
        {
            session.apply_host_action(action).expect("mobile queues");
        }
        let report = session.step_once().expect("mobile places");
        assert!(report.command_rejections.is_empty());
        let mobile = session.latest_snapshot().mobiles()[0].id;

        session.set_selection(Some(PickTarget::Entity(mobile.entity_id())));
        for action in editor
            .apply_control(&session, NativeEditorControl::CircuitView)
            .expect("mobile circuit action")
        {
            session.apply_host_action(action).expect("view changes");
        }
        assert_eq!(
            session.view(),
            ViewMode::Circuit {
                substrate: mobile.entity_id()
            }
        );
        assert!(matches!(
            editor
                .apply_control(&session, NativeEditorControl::PlaceGate(GateType::Not))
                .expect("mobile-local gate action")
                .as_slice(),
            [HostAction::QueueEdit(Command::PlaceGate(PlaceGateCommand {
                origin,
                routing_domain: RoutingDomain::MobileSubstrate(substrate),
                ..
            }))] if *origin == FixedVec2::new(Fixed(circuit_pitch), Fixed::ZERO)
                && *substrate == mobile.entity_id()
        ));

        let stop = session.latest_snapshot().mobiles()[0].ports.stop;
        session.set_selection(Some(PickTarget::MobilePort(aon_sim::MobilePortRef {
            mobile,
            port: aon_sim::MobilePort::Stop,
        })));
        assert_eq!(
            editor
                .apply_control(&session, NativeEditorControl::AddProbe)
                .expect("mobile STOP probe resolves"),
            vec![HostAction::AddProbe(SignalProbeTarget::Sink(stop))]
        );
    }

    #[test]
    fn wire_preview_is_host_only_and_second_anchor_queues_one_wire() {
        let (mut editor, session) = editor_and_session();
        let initial_hash = session.state_hash();
        assert!(
            editor
                .apply_control(&session, NativeEditorControl::WireAnchor)
                .expect("first anchor")
                .is_empty()
        );
        assert_eq!(session.state_hash(), initial_hash);
        editor
            .apply_control(&session, NativeEditorControl::Move { dx: 1, dy: 0 })
            .expect("cursor moves");
        let actions = editor
            .apply_control(&session, NativeEditorControl::WireAnchor)
            .expect("wire commits");
        assert!(matches!(
            actions.as_slice(),
            [HostAction::QueueEdit(Command::PlaceWire(PlaceWireCommand { points, .. }))]
                if points.len() == 2
        ));
        assert_eq!(session.state_hash(), initial_hash);
    }

    #[test]
    fn selected_wire_gate_and_probe_controls_resolve_snapshot_identities() {
        let package = embedded_empty_package().expect("embedded package");
        let physical = package.profiles().physical_scale.clone();
        let circuit_pitch = physical.circuit_routing_pitch.0;
        let world_pitch = physical.world_routing_pitch.0;
        let mut session = LaboratorySession::new(package).expect("laboratory starts");
        let bounds = FixedAabb::new(
            FixedVec2::new(Fixed(-8 * world_pitch), Fixed(-8 * world_pitch)),
            FixedVec2::new(Fixed(8 * world_pitch), Fixed(8 * world_pitch)),
        );
        for command in [
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_area: bounds,
                footprint: bounds,
            }),
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
            }),
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                position: FixedVec2::new(Fixed(4 * circuit_pitch), Fixed::ZERO),
            }),
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                points: vec![
                    FixedVec2::new(Fixed(4 * circuit_pitch), Fixed::ZERO),
                    FixedVec2::new(Fixed(4 * circuit_pitch), Fixed(circuit_pitch)),
                ],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        ] {
            session.queue_command(command).expect("command queues");
            let report = session.step_once().expect("command step succeeds");
            assert_eq!(report.command_acceptances.len(), 1);
        }
        session
            .set_view(ViewMode::Circuit {
                substrate: EntityId(1),
            })
            .expect("circuit view");
        let mut editor =
            NativeEditorState::new(physical, Viewport::new(CellPoint::new(-30, -12), 61, 25));
        editor
            .apply_control(&session, NativeEditorControl::Move { dx: 4, dy: 0 })
            .expect("cursor moves to junction");
        session.set_selection(Some(PickTarget::WireEnd {
            wire: WireId(EntityId(4)),
            end: WireEnd::A,
        }));
        assert!(matches!(
            editor
                .apply_control(&session, NativeEditorControl::BindSelection)
                .expect("binding resolves")
                .as_slice(),
            [HostAction::QueueEdit(Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(4)),
                end: WireEnd::A,
                target: EndpointTarget::Junction(JunctionId(EntityId(3))),
            }))]
        ));

        let input = GatePortRef {
            gate: GateId(EntityId(2)),
            port: GatePort::InputA,
        };
        let expected_driver = session.latest_snapshot().gates()[0]
            .ports
            .input_a
            .external_driver;
        session.set_selection(Some(PickTarget::GatePort(input)));
        let nominal_drive = session.nominal_external_drive_strength();
        let high_actions = editor
            .apply_control(
                &session,
                NativeEditorControl::DriveSelection(LogicLevel::High),
            )
            .expect("external input resolves");
        assert!(matches!(
            high_actions.as_slice(),
            [HostAction::QueueEdit(Command::SetExternalDriver(
                SetExternalDriverCommand {
                    driver,
                    level: LogicLevel::High,
                    strength,
                }
            ))] if *driver == expected_driver && *strength == nominal_drive
        ));
        for action in high_actions {
            session
                .apply_host_action(action)
                .expect("HIGH drive queues");
        }
        session.step_once().expect("HIGH drive reaches Core");
        assert_eq!(
            session.latest_snapshot().gates()[0].input_a_level,
            LogicLevel::High
        );

        let x_actions = editor
            .apply_control(&session, NativeEditorControl::DriveSelection(LogicLevel::X))
            .expect("unknown external input resolves");
        for action in x_actions {
            session.apply_host_action(action).expect("X drive queues");
        }
        session.step_once().expect("X drive reaches Core");
        assert_eq!(
            session.latest_snapshot().gates()[0].input_a_level,
            LogicLevel::X
        );
        session.set_selection(Some(PickTarget::Entity(EntityId(2))));
        assert_eq!(
            editor
                .apply_control(&session, NativeEditorControl::AddProbe)
                .expect("gate probe resolves"),
            vec![HostAction::AddProbe(SignalProbeTarget::GateOutput(GateId(
                EntityId(2)
            )))]
        );
    }
}
