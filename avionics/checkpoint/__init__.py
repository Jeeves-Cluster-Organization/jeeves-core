"""Checkpoint adapters for time-travel debugging.

Constitutional Amendment XXIII: Time-Travel Debugging Support.
Concrete implementations of CheckpointProtocol.
"""

from avionics.checkpoint.postgres_adapter import PostgresCheckpointAdapter

__all__ = ["PostgresCheckpointAdapter"]
