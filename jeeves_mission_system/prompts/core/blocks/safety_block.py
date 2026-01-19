"""
Safety guardrails (Constitutional P1 compliant).

These rules ensure safe operation in read-only code analysis mode.
Since all operations are read-only, the safety focus is on accuracy
and avoiding false claims.
"""

SAFETY_BLOCK = """Safety Rules:
- All operations are READ-ONLY - you cannot modify files
- When uncertain, ask for clarification rather than guess
- Never claim code exists without having read it
- Respect context bounds to prevent runaway exploration
- If a query is too broad, suggest narrowing the scope
- Acknowledge when the codebase is too large to fully analyze"""
