"""Services layer for business logic and centralized mutations.

Unified Interrupt System (v4.0):
- ConfirmationCoordinator has been REMOVED
- All interrupt handling now goes through jeeves_control_tower.services.InterruptService

Constitutional Amendments:
- DebugAPIService: Time-travel debugging (Amendment XXIII)
- WorkerCoordinator: Horizontal scaling (Amendment XXIV)
"""

from jeeves_mission_system.services.chat_service import (
    ChatService,
)

from jeeves_mission_system.services.debug_api import (
    DebugAPIService,
    ExecutionTimeline,
    InspectionResult,
    ReplayResult,
)

from jeeves_mission_system.services.worker_coordinator import (
    WorkerCoordinator,
    WorkerConfig,
    WorkerStatus,
    DistributedPipelineRunner,
)

__all__ = [
    # Chat Service
    "ChatService",
    # Amendment XXIII: Time-Travel Debugging
    "DebugAPIService",
    "ExecutionTimeline",
    "InspectionResult",
    "ReplayResult",
    # Amendment XXIV: Horizontal Scaling
    "WorkerCoordinator",
    "WorkerConfig",
    "WorkerStatus",
    "DistributedPipelineRunner",
]
