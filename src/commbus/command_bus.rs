//! Command dispatch methods for CommBus.

use super::types::Command;
use super::CommBus;
use crate::types::{Error, Result};
use tokio::sync::mpsc;

impl CommBus {
    /// Send a command to a registered handler (fire-and-forget).
    ///
    /// # Errors
    ///
    /// Returns error if no handler registered for this command type.
    pub fn send_command(&mut self, command: Command) -> Result<()> {
        let handler = self.command_handlers
            .get(&command.command_type)
            .ok_or_else(|| {
                Error::validation(format!(
                    "No handler registered for command type: {}",
                    command.command_type
                ))
            })?;

        // Fire-and-forget send
        handler.try_send(command.clone()).map_err(|_| {
            Error::internal(format!(
                "Failed to send command to handler: {}",
                command.command_type
            ))
        })?;

        // Update stats
        self.stats.commands_sent += 1;

        tracing::debug!("Sent command type={}", command.command_type);

        Ok(())
    }

    /// Register a command handler.
    ///
    /// Returns receiver channel for receiving commands.
    ///
    /// # Errors
    ///
    /// Returns error if handler already registered for this command type.
    pub fn register_command_handler(
        &mut self,
        command_type: String,
    ) -> Result<mpsc::Receiver<Command>> {
        let (tx, rx) = mpsc::channel(super::types::CHANNEL_CAPACITY);

        if self.command_handlers.contains_key(&command_type) {
            return Err(Error::validation(format!(
                "Command handler already registered: {}",
                command_type
            )));
        }

        self.command_handlers.insert(command_type.clone(), tx);

        // Update stats
        self.stats.registered_command_handlers = self.command_handlers.len();

        tracing::debug!("Registered command handler: {}", command_type);

        Ok(rx)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_send_command_no_handler() {
        let mut bus = CommBus::new();

        let result = bus.send_command(Command::test("test.command", b"{}".to_vec()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No handler registered"));
    }

    #[test]
    fn test_register_command_handler_and_send() {
        let mut bus = CommBus::new();

        let mut rx = bus.register_command_handler("test.cmd".to_string()).unwrap();

        bus.send_command(Command::test("test.cmd", b"{\"action\":\"do_thing\"}".to_vec())).unwrap();
        let received = rx.try_recv().unwrap();
        assert_eq!(received.command_type, "test.cmd");
    }
}
