"""
Generated gRPC code for Jeeves services.

This package contains Python code generated from proto/jeeves.proto.

To regenerate:
    python scripts/build/compile_proto.py

Or manually:
    python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. proto/jeeves.proto
"""

# Import generated modules (will fail if not compiled yet)
try:
    from proto import jeeves_pb2
    from proto import jeeves_pb2_grpc
except ImportError:
    jeeves_pb2 = None
    jeeves_pb2_grpc = None

__all__ = ["jeeves_pb2", "jeeves_pb2_grpc"]
