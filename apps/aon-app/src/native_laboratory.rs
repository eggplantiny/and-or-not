use crate::host_action::{HostAction, HostActionQueue};
use crate::laboratory::LaboratorySession;
use crate::pacing::{HostRate, HostRunMode};
use crate::presenter::ViewMode;
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::prelude::*;

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

pub fn install_native_laboratory(app: &mut App, session: LaboratorySession) {
    app.insert_resource(NativeLaboratory::new(session));
    app.init_resource::<NativeHostActionQueue>();
    app.init_resource::<NativeLaboratoryStatus>();
    app.add_systems(PreUpdate, collect_native_keyboard_actions);
    app.add_systems(FixedUpdate, advance_native_laboratory);
}

fn collect_native_keyboard_actions(
    laboratory: Res<NativeLaboratory>,
    mut inputs: MessageReader<KeyboardInput>,
    mut actions: ResMut<NativeHostActionQueue>,
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
            KeyCode::KeyR => HostAction::Reset,
            KeyCode::KeyN => HostAction::SetView(ViewMode::Network),
            KeyCode::Escape => HostAction::ClearSelection,
            _ => continue,
        };
        actions.push(action);
    }
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
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn test_app() -> App {
        let package = embedded_empty_package().expect("embedded package is valid");
        let simulation_hz = package.profiles().balance().simulation_hz;
        let session = LaboratorySession::new(package).expect("native Laboratory starts");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(simulation_hz)));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::MAX);
        install_native_laboratory(&mut app, session);
        app
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
