use crate::host_action::{HostAction, HostActionQueue};
use crate::laboratory::LaboratorySession;
use crate::native_editor::{NativeEditorControl, NativeEditorState};
use crate::pacing::{HostRate, HostRunMode};
use crate::presenter::Viewport;
use aon_sim::{GateType, LogicLevel, PhysicalScaleProfile};
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, SystemSet)]
pub(crate) enum NativePreUpdateSet {
    CollectInput,
    ProductSwitch,
}

/// Native-only wrapper around the sole mutable Core owner.
///
/// PreUpdate and Update may read this resource. Only
/// [`advance_native_laboratory`] requests mutable access, and that system runs
/// exclusively in FixedUpdate.
#[derive(Resource)]
pub struct NativeLaboratory {
    session: LaboratorySession,
}

impl NativeLaboratory {
    pub const fn new(session: LaboratorySession) -> Self {
        Self { session }
    }

    pub const fn session(&self) -> &LaboratorySession {
        &self.session
    }

    pub(crate) fn replace_session(&mut self, session: LaboratorySession) {
        self.session = session;
    }
}

#[derive(Resource, Default)]
pub struct NativeHostActionQueue {
    queue: HostActionQueue,
}

impl NativeHostActionQueue {
    pub fn push(&mut self, action: HostAction) {
        self.queue.push(action);
    }

    pub const fn queued(&self) -> &HostActionQueue {
        &self.queue
    }

    pub(crate) fn clear(&mut self) {
        self.queue.clear();
    }
}

#[derive(Clone, Debug, Default, Resource, PartialEq, Eq)]
pub struct NativeLaboratoryStatus {
    action_rejections: Vec<String>,
    execution_error: Option<String>,
    steps_in_last_pulse: usize,
}

impl NativeLaboratoryStatus {
    pub fn action_rejections(&self) -> &[String] {
        &self.action_rejections
    }

    pub fn execution_error(&self) -> Option<&str> {
        self.execution_error.as_deref()
    }

    pub const fn steps_in_last_pulse(&self) -> usize {
        self.steps_in_last_pulse
    }
}

pub fn install_native_laboratory(
    app: &mut App,
    session: LaboratorySession,
    physical: PhysicalScaleProfile,
    viewport: Viewport,
) {
    app.insert_resource(NativeLaboratory::new(session));
    app.insert_resource(NativeEditorState::new(physical, viewport));
    app.init_resource::<NativeHostActionQueue>();
    app.init_resource::<NativeLaboratoryStatus>();
    app.add_message::<KeyboardInput>();
    app.configure_sets(
        PreUpdate,
        (
            NativePreUpdateSet::CollectInput,
            NativePreUpdateSet::ProductSwitch,
        )
            .chain(),
    );
    app.add_systems(
        PreUpdate,
        collect_native_keyboard_actions.in_set(NativePreUpdateSet::CollectInput),
    );
    app.add_systems(FixedUpdate, advance_native_laboratory);
}

fn collect_native_keyboard_actions(
    laboratory: Res<NativeLaboratory>,
    mut inputs: MessageReader<KeyboardInput>,
    mut actions: ResMut<NativeHostActionQueue>,
    mut editor: ResMut<NativeEditorState>,
) {
    let mut predicted_mode = laboratory.session.pacer().mode();
    for input in inputs.read() {
        if input.state != ButtonState::Pressed || input.repeat {
            continue;
        }
        let action = match input.key_code {
            KeyCode::Space => {
                predicted_mode = match predicted_mode {
                    HostRunMode::Paused => HostRunMode::Running,
                    HostRunMode::Running => HostRunMode::Paused,
                };
                match predicted_mode {
                    HostRunMode::Paused => HostAction::Pause,
                    HostRunMode::Running => HostAction::Resume,
                }
            }
            KeyCode::Period => HostAction::SingleStep,
            KeyCode::Digit1 => HostAction::SetRate(HostRate::Quarter),
            KeyCode::Digit2 => HostAction::SetRate(HostRate::One),
            KeyCode::Digit3 => HostAction::SetRate(HostRate::Four),
            KeyCode::KeyR => {
                editor.clear_transient();
                HostAction::Reset
            }
            _ => {
                let Some(control) = native_editor_control(input.key_code) else {
                    continue;
                };
                if let Ok(generated) = editor.apply_control(laboratory.session(), control) {
                    for action in generated {
                        actions.push(action);
                    }
                }
                continue;
            }
        };
        actions.push(action);
    }
}

const fn native_editor_control(key_code: KeyCode) -> Option<NativeEditorControl> {
    Some(match key_code {
        KeyCode::ArrowLeft | KeyCode::Numpad4 => NativeEditorControl::Move { dx: -1, dy: 0 },
        KeyCode::ArrowRight | KeyCode::Numpad6 => NativeEditorControl::Move { dx: 1, dy: 0 },
        KeyCode::ArrowDown | KeyCode::Numpad2 => NativeEditorControl::Move { dx: 0, dy: -1 },
        KeyCode::ArrowUp | KeyCode::Numpad8 => NativeEditorControl::Move { dx: 0, dy: 1 },
        KeyCode::Enter => NativeEditorControl::Pick,
        KeyCode::Escape => NativeEditorControl::Cancel,
        KeyCode::KeyN => NativeEditorControl::NetworkView,
        KeyCode::KeyC => NativeEditorControl::CircuitView,
        KeyCode::KeyA => NativeEditorControl::PlaceGate(GateType::And),
        KeyCode::KeyO => NativeEditorControl::PlaceGate(GateType::Or),
        KeyCode::KeyI => NativeEditorControl::PlaceGate(GateType::Not),
        KeyCode::KeyJ => NativeEditorControl::PlaceJunction,
        KeyCode::KeyF => NativeEditorControl::PlaceFixedSubstrate,
        KeyCode::KeyM => NativeEditorControl::PlaceMobileSubstrate,
        KeyCode::KeyW => NativeEditorControl::WireAnchor,
        KeyCode::Delete | KeyCode::Backspace => NativeEditorControl::DeleteSelection,
        KeyCode::KeyB => NativeEditorControl::BindSelection,
        KeyCode::KeyU => NativeEditorControl::UnbindSelection,
        KeyCode::KeyZ => NativeEditorControl::DriveSelection(LogicLevel::Low),
        KeyCode::KeyH => NativeEditorControl::DriveSelection(LogicLevel::High),
        KeyCode::KeyX => NativeEditorControl::DriveSelection(LogicLevel::X),
        KeyCode::KeyP => NativeEditorControl::AddProbe,
        KeyCode::KeyK => NativeEditorControl::RemoveProbe,
        _ => return None,
    })
}

fn advance_native_laboratory(
    mut laboratory: ResMut<NativeLaboratory>,
    mut actions: ResMut<NativeHostActionQueue>,
    fixed_time: Res<Time<Fixed>>,
    mut status: ResMut<NativeLaboratoryStatus>,
) {
    status.action_rejections.clear();
    status.execution_error = None;
    status.steps_in_last_pulse = 0;

    let results = laboratory.session.drain_host_actions(&mut actions.queue);
    status.action_rejections.extend(
        results
            .into_iter()
            .filter_map(Result::err)
            .map(|error| error.to_string()),
    );

    match laboratory.session.advance_frame(fixed_time.delta()) {
        Ok(reports) => status.steps_in_last_pulse = reports.len(),
        Err(error) => {
            laboratory.session.set_mode(HostRunMode::Paused);
            status.execution_error = Some(error.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded_empty_package;
    use bevy::input::keyboard::{Key, NativeKey};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn test_app() -> App {
        let package = embedded_empty_package().expect("embedded package is valid");
        let simulation_hz = package.profiles().balance().simulation_hz;
        let physical = package.profiles().physical_scale.clone();
        let session = LaboratorySession::new(package).expect("native Laboratory starts");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(simulation_hz)));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::MAX);
        install_native_laboratory(
            &mut app,
            session,
            physical,
            Viewport::new(crate::cell_buffer::CellPoint::new(-30, -12), 61, 25),
        );
        app
    }

    fn press_key(app: &mut App, key_code: KeyCode) {
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.world_mut().run_schedule(PreUpdate);
    }

    #[test]
    fn keyboard_edit_is_host_only_until_single_step_executes_the_command() {
        let mut app = test_app();
        let initial_hash = app
            .world()
            .resource::<NativeLaboratory>()
            .session()
            .state_hash();

        press_key(&mut app, KeyCode::KeyF);
        assert_eq!(
            app.world()
                .resource::<NativeHostActionQueue>()
                .queued()
                .len(),
            1
        );
        assert_eq!(
            app.world()
                .resource::<NativeLaboratory>()
                .session()
                .state_hash(),
            initial_hash
        );

        app.world_mut().run_schedule(FixedUpdate);
        let laboratory = app.world().resource::<NativeLaboratory>();
        assert_eq!(laboratory.session().edit_log().len(), 1);
        assert!(
            laboratory
                .session()
                .latest_snapshot()
                .fixed_substrates()
                .is_empty()
        );
        assert_eq!(laboratory.session().state_hash(), initial_hash);

        press_key(&mut app, KeyCode::Period);
        app.world_mut().run_schedule(FixedUpdate);
        let laboratory = app.world().resource::<NativeLaboratory>();
        assert_eq!(laboratory.session().next_tick().0, 1);
        assert_eq!(
            laboratory
                .session()
                .latest_snapshot()
                .fixed_substrates()
                .len(),
            1
        );
        assert_ne!(laboratory.session().state_hash(), initial_hash);
    }

    #[test]
    fn preupdate_host_actions_do_not_step_until_the_fixed_owner_runs() {
        let mut app = test_app();
        let initial_hash = app
            .world()
            .resource::<NativeLaboratory>()
            .session()
            .state_hash();
        app.world_mut()
            .resource_mut::<NativeHostActionQueue>()
            .push(HostAction::Resume);

        app.world_mut().run_schedule(PreUpdate);
        assert_eq!(
            app.world()
                .resource::<NativeLaboratory>()
                .session()
                .state_hash(),
            initial_hash
        );

        let fixed_step = app.world().resource::<Time<Fixed>>().timestep();
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .advance_by(fixed_step);
        app.world_mut().run_schedule(FixedUpdate);
        let laboratory = app.world().resource::<NativeLaboratory>();
        assert_eq!(laboratory.session().next_tick().0, 1);
        assert_ne!(laboratory.session().state_hash(), initial_hash);
        assert_eq!(
            app.world()
                .resource::<NativeLaboratoryStatus>()
                .steps_in_last_pulse(),
            1
        );
    }

    #[test]
    fn app_update_partitions_preserve_the_native_fixed_trace() {
        fn trace(frame_deltas: &[Duration]) -> Vec<aon_sim::StateHash> {
            let mut app = test_app();
            app.world_mut()
                .resource_mut::<NativeHostActionQueue>()
                .push(HostAction::Resume);
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
            app.update();
            for &delta in frame_deltas {
                app.insert_resource(TimeUpdateStrategy::ManualDuration(delta));
                app.update();
            }
            app.world()
                .resource::<NativeLaboratory>()
                .session()
                .hash_trace()
                .to_vec()
        }

        assert_eq!(
            trace(&[Duration::from_secs(1)]),
            trace(&[Duration::from_millis(50); 20])
        );
    }

    #[test]
    fn ordered_actions_are_drained_once_and_single_step_remains_paused() {
        let mut app = test_app();
        {
            let mut actions = app.world_mut().resource_mut::<NativeHostActionQueue>();
            actions.push(HostAction::SetRate(HostRate::Four));
            actions.push(HostAction::SingleStep);
            actions.push(HostAction::SingleStep);
        }

        app.world_mut().run_schedule(FixedUpdate);
        let laboratory = app.world().resource::<NativeLaboratory>();
        assert_eq!(laboratory.session().next_tick().0, 1);
        assert_eq!(laboratory.session().pacer().mode(), HostRunMode::Paused);
        assert_eq!(laboratory.session().pacer().rate(), HostRate::Four);
        assert_eq!(
            app.world()
                .resource::<NativeLaboratoryStatus>()
                .steps_in_last_pulse(),
            1
        );
    }

    #[test]
    fn typed_action_rejection_is_nonfatal_and_does_not_hide_later_pause() {
        let mut app = test_app();
        {
            let mut actions = app.world_mut().resource_mut::<NativeHostActionQueue>();
            actions.push(HostAction::Resume);
            actions.push(HostAction::SingleStep);
            actions.push(HostAction::Pause);
        }

        app.world_mut().run_schedule(FixedUpdate);
        let status = app.world().resource::<NativeLaboratoryStatus>();
        assert_eq!(status.action_rejections().len(), 1);
        assert_eq!(status.execution_error(), None);
        assert_eq!(status.steps_in_last_pulse(), 0);
        assert_eq!(
            app.world()
                .resource::<NativeLaboratory>()
                .session()
                .pacer()
                .mode(),
            HostRunMode::Paused
        );
    }
}
#[test]
fn arrow_and_numpad_navigation_share_the_same_editor_controls() {
    for (arrow, numpad, expected) in [
        (
            KeyCode::ArrowLeft,
            KeyCode::Numpad4,
            NativeEditorControl::Move { dx: -1, dy: 0 },
        ),
        (
            KeyCode::ArrowRight,
            KeyCode::Numpad6,
            NativeEditorControl::Move { dx: 1, dy: 0 },
        ),
        (
            KeyCode::ArrowDown,
            KeyCode::Numpad2,
            NativeEditorControl::Move { dx: 0, dy: -1 },
        ),
        (
            KeyCode::ArrowUp,
            KeyCode::Numpad8,
            NativeEditorControl::Move { dx: 0, dy: 1 },
        ),
    ] {
        assert_eq!(native_editor_control(arrow), Some(expected));
        assert_eq!(native_editor_control(numpad), Some(expected));
    }
}
