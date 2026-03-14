# Jeeves Core

Rust micro-kernel for AI agent orchestration. Consumable as a Rust library, PyO3 Python module, or MCP stdio server.

## Quick Start

```bash
# Build and test
cargo test                              # 302 tests (lib + integration)
cargo clippy --all-features             # lint

# PyO3 Python module
pip install -e .                        # build via maturin
python -c "from jeeves_core import PipelineRunner"

# MCP stdio server (tool proxy)
cargo run --features mcp-stdio
```

## Repository Structure

```
jeeves-core/
├── src/
│   ├── kernel/               # Orchestration engine
│   │   ├── orchestrator.rs   # Pipeline sessions, routing, bounds
│   │   ├── orchestrator_types.rs  # PipelineConfig, PipelineStage, Instruction
│   │   ├── routing.rs        # RoutingExpr evaluation (13 ops, 5 scopes)
│   │   ├── builder.rs        # PipelineBuilder DSL
│   │   ├── agent_card.rs     # AgentCard registry (federation discovery)
│   │   ├── lifecycle.rs      # Process state machine
│   │   ├── resources.rs      # Quota enforcement
│   │   ├── interrupts.rs     # Human-in-the-loop
│   │   ├── rate_limiter.rs   # Per-user sliding window
│   │   ├── cleanup.rs        # Background GC
│   │   └── services.rs       # Service registry
│   ├── worker/               # Agent execution
│   │   ├── actor.rs          # Kernel actor (mpsc channel, typed dispatch)
│   │   ├── handle.rs         # KernelHandle (typed channel wrapper, Clone)
│   │   ├── agent.rs          # Agent trait, LlmAgent, DeterministicAgent, PipelineAgent
│   │   ├── llm/              # LlmProvider trait, OpenAI HTTP client, streaming
│   │   ├── mcp.rs            # MCP tool executor (HTTP + stdio transports)
│   │   ├── mcp_server.rs     # MCP stdio server (behind mcp-stdio feature)
│   │   ├── tools.rs          # ToolExecutor trait + ToolRegistry
│   │   └── prompts.rs        # Prompt template loading + {var} substitution
│   ├── python/               # PyO3 bindings (behind py-bindings feature)
│   │   ├── runner.rs         # PipelineRunner facade
│   │   ├── tool_bridge.rs    # Python→Rust tool bridge
│   │   └── event_iter.rs     # Streaming event iterator
│   ├── envelope/             # Envelope types, bounds, TerminalReason
│   ├── commbus/              # Message bus (events, commands, queries)
│   ├── types/                # IDs, errors, config
│   └── main.rs               # MCP stdio binary (behind mcp-stdio feature)
└── tests/                    # Integration tests
```

## Consumer Patterns

### PyO3 (primary)

Capabilities import the kernel as a Python library — no HTTP, no subprocess.

```python
from jeeves_core import PipelineRunner, tool

@tool(name="search", description="Search the web")
def search(params):
    return {"results": [...]}

runner = PipelineRunner.from_json("pipeline.json", prompts_dir="prompts/")
runner.register_tool(search)

result = runner.run("hello", user_id="user1")           # Buffered
for event in runner.stream("hello", user_id="user1"):   # Streaming
    if event["type"] == "delta": print(event["content"], end="")
```

### Rust crate

```rust
use jeeves_core::kernel::Kernel;
use jeeves_core::worker::agent::{Agent, AgentContext, AgentOutput};
use jeeves_core::worker::handle::KernelHandle;
use jeeves_core::worker::actor::spawn_kernel;
```

### MCP stdio server

Binary that proxies upstream MCP servers as tools via JSON-RPC on stdin/stdout.

```bash
JEEVES_MCP_SERVERS='[{"name":"fs","transport":"stdio","command":"npx","args":["-y","@anthropic-ai/mcp-filesystem"]}]' \
  cargo run --features mcp-stdio
```

## Feature Flags

| Feature | Dependencies | Purpose |
|---------|-------------|---------|
| `mcp-stdio` | clap | MCP stdio server binary (`jeeves-kernel`) |
| `py-bindings` | pyo3, inventory | Python importable module (`from jeeves_core import ...`) |
| `test-harness` | — | Test utilities for integration tests |

Default: none. `cargo test` uses rlib only. `pip install -e .` activates `py-bindings`.

## Architecture

```
Python capabilities (PyO3)        MCP clients (stdio)
       │ direct fn call                │ JSON-RPC
       ▼                               ▼
┌──────────────────────────────────────────────┐
│  Kernel actor ← mpsc ← KernelHandle         │
│  (tokio task)   (typed)  (Clone, Send+Sync)  │
│       │                    ↑                 │
│       ▼                    │                 │
│  Agent tasks (concurrent tokio tasks)        │
│  ├── LlmAgent (OpenAI HTTP)                  │
│  ├── McpDelegatingAgent (tool dispatch)      │
│  ├── PipelineAgent (child pipeline)          │
│  └── DeterministicAgent (passthrough)        │
│       │                                      │
│  CommBus (pub/sub, commands, queries)        │
└──────────────────────────────────────────────┘
```

## Pipeline Composition (A2A)

A parent pipeline can run a child pipeline as a stage via `PipelineAgent`:

```json
{
  "name": "orchestrator",
  "stages": [
    {"name": "classify", "agent": "classify", "has_llm": true, "default_next": "run_assistant"},
    {"name": "run_assistant", "agent": "assistant", "child_pipeline": "assistant", "default_next": "respond"},
    {"name": "respond", "agent": "respond", "has_llm": true}
  ]
}
```

Child outputs nest under the parent stage's output_key. Streaming events carry dotted pipeline names (`orchestrator.assistant`).

## CommBus Federation

Pipelines can subscribe to cross-pipeline events via `PipelineConfig.subscriptions`:

```json
{
  "name": "npc_dialogue",
  "subscriptions": ["game.event", "world.update"],
  "publishes": ["npc.response"]
}
```

Events accumulate in `envelope.event_inbox` between agent executions.

## Prerequisites

- Rust 1.75+
- Python 3.10+ (for PyO3 bindings)

## License

Apache License 2.0
