"""Shared data models used across core and verticals.

These models are intentionally placed in common/ to avoid circular dependencies
between agents/, orchestrator/, and common/ modules.

RULE 3 Compliance: common/ must not import from agents/.
Models that need to be used in common/ live here.
"""

from pydantic import BaseModel, Field
from typing import List, Optional


class MetaValidationIssue(BaseModel):
    """An issue found during meta-validation (hallucination detection).

    Used by metrics, validators, and other shared components to track
    issues like hallucinations, data omissions, and success lies.

    Note: This is distinct from ToolResultValidationIssue in
    contracts/code_analysis/validation.py which validates tool outputs.
    """

    type: str  # success_lie, data_hallucination, error_omission, data_omission
    severity: str  # high, medium, low
    evidence: str
    ground_truth: str


class VerificationReport(BaseModel):
    """Report from validation processes.

    Used by metrics, validators, and other shared components.
    """

    approved: bool
    confidence: float = Field(ge=0.0, le=1.0)
    issues_found: List[MetaValidationIssue]
    suggested_correction: Optional[str] = None
    requires_user_notification: bool
