"""Validation data models for cross-layer use.

These models are placed in protocols (L0) to allow both avionics (L3)
and mission_system (L4) to use them without violating layer boundaries.

Constitutional Reference:
    - Used by metrics, validators, and other shared components
    - Part of the L0 foundation layer
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
