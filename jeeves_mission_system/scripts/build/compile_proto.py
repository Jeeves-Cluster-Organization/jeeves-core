#!/usr/bin/env python3
"""
Compile proto files to Python gRPC code.

Usage:
    python scripts/compile_proto.py

This generates:
    proto/jeeves_pb2.py      - Message classes
    proto/jeeves_pb2_grpc.py - Service stubs

Requirements:
    pip install grpcio-tools
"""

import subprocess
import sys
from pathlib import Path


def main():
    # Find project root
    script_dir = Path(__file__).parent
    project_root = script_dir.parent

    # Proto file
    proto_file = project_root / "proto" / "jeeves.proto"
    if not proto_file.exists():
        print(f"ERROR: Proto file not found: {proto_file}")
        sys.exit(1)

    print(f"Compiling {proto_file}...")

    # Run protoc
    cmd = [
        sys.executable, "-m", "grpc_tools.protoc",
        f"-I{project_root}",
        f"--python_out={project_root}",
        f"--grpc_python_out={project_root}",
        str(proto_file),
    ]

    try:
        result = subprocess.run(cmd, check=True, capture_output=True, text=True)
        if result.stdout:
            print(result.stdout)
    except subprocess.CalledProcessError as e:
        print(f"ERROR: Proto compilation failed")
        print(e.stderr)
        sys.exit(1)
    except FileNotFoundError:
        print("ERROR: grpc_tools not found. Install with: pip install grpcio-tools")
        sys.exit(1)

    # Verify generated files
    pb2_file = project_root / "proto" / "jeeves_pb2.py"
    grpc_file = project_root / "proto" / "jeeves_pb2_grpc.py"

    if pb2_file.exists() and grpc_file.exists():
        print(f"SUCCESS: Generated {pb2_file.name} and {grpc_file.name}")
    else:
        print("WARNING: Some generated files may be missing")

    # Fix imports in generated files (Python 3.10+ compatibility)
    # The generated code uses `import jeeves_pb2` which needs to be `from proto import jeeves_pb2`
    fix_imports(grpc_file)

    print("Proto compilation complete!")


def fix_imports(grpc_file: Path):
    """Fix relative imports in generated gRPC file."""
    if not grpc_file.exists():
        return

    content = grpc_file.read_text()

    # Replace absolute import with relative import for proto package
    old_import = "import jeeves_pb2 as jeeves__pb2"
    new_import = "from proto import jeeves_pb2 as jeeves__pb2"

    if old_import in content:
        content = content.replace(old_import, new_import)
        grpc_file.write_text(content)
        print(f"Fixed imports in {grpc_file.name}")


if __name__ == "__main__":
    main()
