"""
Prompt versions for Code Analysis Agent.

Per Engineering Plan v4.2: No _v1 suffix - single version per agent.
Prompts are automatically registered when modules are imported.

7-Agent Pipeline Prompts:
- perception: Session state loading (PerceptionAgent)
- intent: Intent extraction and clarification (IntentAgent)
- planner: Plan generation and tool selection (PlannerAgent)
- traverser: Code traversal execution (TraverserAgent)
- synthesizer: Understanding synthesis (SynthesizerAgent)
- critic: Response validation and hallucination detection (CriticAgent)
- integration: Final response building (IntegrationAgent)

See INDEX.md for complete prompt reference.
"""
