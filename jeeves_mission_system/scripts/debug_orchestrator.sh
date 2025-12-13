#!/bin/bash
# Debug script for orchestrator container issues
# Run this to identify why the gRPC container isn't starting

set -e

echo "=========================================="
echo "  Orchestrator Debug Script"
echo "=========================================="

# 1. Check if containers exist
echo -e "\n[1] Container Status:"
docker compose ps -a

# 2. Check assistant container logs
echo -e "\n[2] Assistant (orchestrator) logs (last 50 lines):"
docker compose logs assistant --tail=50 2>/dev/null || echo "No logs - container may not have started"

# 3. Try to build with verbose output
echo -e "\n[3] Building orchestrator with verbose output:"
docker compose build assistant --progress=plain 2>&1 | tail -30

# 4. Test if the orchestrator can import successfully
echo -e "\n[4] Testing orchestrator imports:"
docker compose run --rm assistant python -c "
import sys
print('Python path:', sys.path)

try:
    from proto import jeeves_pb2, jeeves_pb2_grpc
    print('✓ Proto stubs imported successfully')
except ImportError as e:
    print('✗ Proto import failed:', e)

try:
    from config.settings import get_settings
    print('✓ Config imported successfully')
except ImportError as e:
    print('✗ Config import failed:', e)

try:
    from orchestrator.server import OrchestratorServer
    print('✓ OrchestratorServer imported successfully')
except Exception as e:
    print('✗ OrchestratorServer import failed:', e)
"

# 5. Check environment variables
echo -e "\n[5] Environment variables in container:"
docker compose run --rm assistant env | grep -E "POSTGRES|GRPC|LLM|LLAMASERVER" | sort

# 6. Check if postgres is healthy
echo -e "\n[6] Postgres health:"
docker compose exec postgres pg_isready -U assistant 2>/dev/null || echo "Postgres not ready"

# 7. Check network connectivity
echo -e "\n[7] Network connectivity from orchestrator:"
docker compose run --rm assistant python -c "
import socket
for host, port in [('postgres', 5432), ('llama-server', 8080)]:
    try:
        s = socket.socket()
        s.settimeout(2)
        s.connect((host, port))
        s.close()
        print(f'✓ Can connect to {host}:{port}')
    except Exception as e:
        print(f'✗ Cannot connect to {host}:{port}: {e}')
"

echo -e "\n=========================================="
echo "  Debug Complete"
echo "=========================================="
