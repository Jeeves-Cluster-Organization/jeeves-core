#!/usr/bin/env python3
"""
Simple server launcher - handles all the setup automatically

**Constitution R7 Compliance:**
- register_capability() is called BEFORE uvicorn starts
- This ensures resources are registered BEFORE infrastructure initialization
"""

import os
import sys

# Load .env manually (lightweight) before applying defaults so we don't overwrite user config
def _load_dotenv(path: str = ".env") -> None:
    if not os.path.exists(path):
        return

    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            if "=" not in stripped:
                continue
            key, value = stripped.split("=", 1)
            key = key.strip()
            value = value.strip().strip('"').strip("'")
            # Do not overwrite existing environment variables
            if key and key not in os.environ:
                os.environ[key] = value


_load_dotenv()

# Set default provider only if still not set after loading .env and environment
if 'LLM_PROVIDER' not in os.environ:
    os.environ['LLM_PROVIDER'] = 'llamaserver'
    print("Using LLM_PROVIDER=llamaserver (default)")
    print("To use a different provider, set environment variable first:")
    print("  export LLM_PROVIDER=openai")
    print("  export LLM_PROVIDER=anthropic")
    print("  export LLM_PROVIDER=mock")
    print()

# Import and run uvicorn
try:
    import uvicorn
except ImportError:
    print("ERROR: uvicorn not installed")
    print("Install with: pip install uvicorn[standard]")
    sys.exit(1)

if __name__ == "__main__":
    print("=" * 60)
    print("Starting Code Analysis Agent Server")
    print("=" * 60)
    print(f"Provider: {os.environ.get('LLM_PROVIDER', 'llamaserver')}")
    print(f"Port: {os.environ.get('API_PORT', '8000')}")
    print("=" * 60)
    print()

    # Constitution R7: Register capability BEFORE infrastructure initialization
    # This must happen before uvicorn imports the app module
    from jeeves_capability_code_analyser import register_capability
    register_capability()
    print("Capability registered (Constitution R7)")
    print()

    uvicorn.run(
        "mission_system.api.server:app",
        host=os.environ.get('API_HOST', '0.0.0.0'),
        port=int(os.environ.get('API_PORT', '8000')),
        reload=os.environ.get('API_RELOAD', 'false').lower() == 'true',
        log_level=os.environ.get('LOG_LEVEL', 'info').lower()
    )
