use aon_sim::{
    BindPortCommand, Command, CommandEnvelope, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceWireCommand, RemoveEntityCommand, SetExternalDriverCommand, Tick,
};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditIntent {
    PlaceGate(PlaceGateCommand),
    PlaceWire(PlaceWireCommand),
    PlaceJunction(PlaceJunctionCommand),
    PlaceFixedSubstrate(PlaceFixedSubstrateCommand),
    RemoveEntity(RemoveEntityCommand),
    BindPort(BindPortCommand),
    SetExternalDriver(SetExternalDriverCommand),
}

impl EditIntent {
    pub fn into_command(self) -> Command {
        match self {
            Self::PlaceGate(command) => Command::PlaceGate(command),
            Self::PlaceWire(command) => Command::PlaceWire(command),
            Self::PlaceJunction(command) => Command::PlaceJunction(command),
            Self::PlaceFixedSubstrate(command) => Command::PlaceFixedSubstrate(command),
            Self::RemoveEntity(command) => Command::RemoveEntity(command),
            Self::BindPort(command) => Command::BindPort(command),
            Self::SetExternalDriver(command) => Command::SetExternalDriver(command),
        }
    }
}

impl From<EditIntent> for Command {
    fn from(intent: EditIntent) -> Self {
        intent.into_command()
    }
}

impl TryFrom<Command> for EditIntent {
    type Error = EditScopeError;

    fn try_from(command: Command) -> Result<Self, Self::Error> {
        match command {
            Command::PlaceGate(command) => Ok(Self::PlaceGate(command)),
            Command::PlaceWire(command) => Ok(Self::PlaceWire(command)),
            Command::PlaceJunction(command) => Ok(Self::PlaceJunction(command)),
            Command::PlaceFixedSubstrate(command) => Ok(Self::PlaceFixedSubstrate(command)),
            Command::RemoveEntity(command) => Ok(Self::RemoveEntity(command)),
            Command::BindPort(command) => Ok(Self::BindPort(command)),
            Command::SetExternalDriver(command) => Ok(Self::SetExternalDriver(command)),
            Command::PlaceMobileSubstrate(_) => Err(EditScopeError::OutOfScopeCommand),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhostPreview {
    target_tick: Tick,
    command: Command,
}

impl GhostPreview {
    pub const fn target_tick(&self) -> Tick {
        self.target_tick
    }

    pub const fn command(&self) -> &Command {
        &self.command
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingCommands {
    next_ordinal: u64,
    commands: Vec<CommandEnvelope>,
}

impl PendingCommands {
    pub fn queue(
        &mut self,
        target_tick: Tick,
        command: impl Into<Command>,
    ) -> Result<u64, PendingCommandError> {
        let ordinal = self.next_ordinal;
        let next_ordinal = ordinal
            .checked_add(1)
            .ok_or(PendingCommandError::OrdinalExhausted)?;
        self.commands.push(CommandEnvelope {
            target_tick,
            ordinal,
            command: command.into(),
        });
        self.next_ordinal = next_ordinal;
        Ok(ordinal)
    }

    pub fn preview(&self, target_tick: Tick, command: impl Into<Command>) -> GhostPreview {
        GhostPreview {
            target_tick,
            command: command.into(),
        }
    }

    pub fn commands(&self) -> &[CommandEnvelope] {
        &self.commands
    }

    pub fn commands_for_tick(&self, tick: Tick) -> Vec<CommandEnvelope> {
        let mut commands = self
            .commands
            .iter()
            .filter(|command| command.target_tick == tick)
            .cloned()
            .collect::<Vec<_>>();
        commands.sort_unstable_by_key(|command| command.ordinal);
        commands
    }

    pub fn discard_tick(&mut self, tick: Tick) {
        self.commands.retain(|command| command.target_tick != tick);
    }

    pub fn drain_for_tick(&mut self, tick: Tick) -> Vec<CommandEnvelope> {
        let commands = self.commands_for_tick(tick);
        self.discard_tick(tick);
        commands
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.next_ordinal = 0;
    }

    pub const fn next_ordinal(&self) -> u64 {
        self.next_ordinal
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PendingCommandError {
    #[error("host pending-command ordinal space is exhausted")]
    OrdinalExhausted,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum EditScopeError {
    #[error("the command is outside the S0-M6 interactive editor scope")]
    OutOfScopeCommand,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aon_sim::{Fixed, FixedVec2, GateType, RoutingDomain};

    #[test]
    fn ordinal_exhaustion_is_typed_and_does_not_enqueue() {
        let mut pending = PendingCommands {
            next_ordinal: u64::MAX,
            commands: Vec::new(),
        };
        let command = EditIntent::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            routing_domain: RoutingDomain::OpenWorld,
        });

        assert_eq!(
            pending.queue(Tick(7), command),
            Err(PendingCommandError::OrdinalExhausted)
        );
        assert_eq!(pending.next_ordinal(), u64::MAX);
        assert!(pending.is_empty());
    }
}
