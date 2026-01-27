"""Calendar tools for homelab integration."""

import logging
from datetime import datetime, timedelta
from typing import Optional

from .ssh_tools import ToolResult
from ..config import get_config

logger = logging.getLogger(__name__)


class CalendarAccessor:
    """Calendar accessor supporting multiple backends."""

    def __init__(self):
        self.config = get_config().calendar

    async def query_events(
        self,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None,
        query_filter: Optional[str] = None,
    ) -> ToolResult:
        """
        Query calendar events.

        Args:
            start_date: Start date (YYYY-MM-DD) or None for today
            end_date: End date (YYYY-MM-DD) or None for default_days_ahead
            query_filter: Optional filter string to match event titles

        Returns:
            ToolResult with calendar events
        """
        # Parse dates
        try:
            if start_date:
                start_dt = datetime.strptime(start_date, "%Y-%m-%d")
            else:
                start_dt = datetime.now().replace(hour=0, minute=0, second=0, microsecond=0)

            if end_date:
                end_dt = datetime.strptime(end_date, "%Y-%m-%d")
            else:
                end_dt = start_dt + timedelta(days=self.config.default_days_ahead)

        except ValueError as e:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Invalid date format (use YYYY-MM-DD): {str(e)}",
            )

        # Route to appropriate backend
        if self.config.api_type == "ics":
            return await self._query_ics(start_dt, end_dt, query_filter)
        elif self.config.api_type == "caldav":
            return await self._query_caldav(start_dt, end_dt, query_filter)
        elif self.config.api_type == "google":
            return await self._query_google(start_dt, end_dt, query_filter)
        else:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Unsupported calendar backend: {self.config.api_type}",
            )

    async def _query_ics(
        self, start_dt: datetime, end_dt: datetime, query_filter: Optional[str]
    ) -> ToolResult:
        """Query ICS calendar file."""
        if not self.config.calendar_url:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message="CALENDAR_URL not configured for ICS backend",
            )

        try:
            # Import icalendar if available
            try:
                from icalendar import Calendar
                import requests
            except ImportError:
                return ToolResult(
                    status="error",
                    data={},
                    citations=[],
                    error_message="icalendar library not installed. Install with: pip install icalendar requests",
                )

            # Fetch ICS file
            response = requests.get(self.config.calendar_url, timeout=10)
            response.raise_for_status()

            # Parse calendar
            cal = Calendar.from_ical(response.content)
            events = []

            for component in cal.walk():
                if component.name == "VEVENT":
                    event_start = component.get("dtstart").dt
                    event_end = component.get("dtend").dt if component.get("dtend") else event_start
                    summary = str(component.get("summary", ""))

                    # Convert date to datetime if needed
                    if isinstance(event_start, datetime):
                        event_start_dt = event_start
                    else:
                        event_start_dt = datetime.combine(event_start, datetime.min.time())

                    # Filter by date range
                    if event_start_dt < start_dt or event_start_dt > end_dt:
                        continue

                    # Filter by query string
                    if query_filter and query_filter.lower() not in summary.lower():
                        continue

                    events.append(
                        {
                            "summary": summary,
                            "start": event_start.isoformat(),
                            "end": event_end.isoformat() if event_end else event_start.isoformat(),
                            "description": str(component.get("description", "")),
                            "location": str(component.get("location", "")),
                        }
                    )

            # Limit results
            limited_events = events[: self.config.max_events_per_query]

            return ToolResult(
                status="success",
                data={
                    "start_date": start_dt.strftime("%Y-%m-%d"),
                    "end_date": end_dt.strftime("%Y-%m-%d"),
                    "events": limited_events,
                    "total_events": len(events),
                    "limited_to": len(limited_events),
                },
                citations=[
                    {
                        "type": "calendar_query",
                        "backend": "ics",
                        "url": self.config.calendar_url,
                        "event_count": str(len(events)),
                    }
                ],
            )

        except Exception as e:
            logger.exception("Failed to query ICS calendar")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Failed to query ICS calendar: {str(e)}",
            )

    async def _query_caldav(
        self, start_dt: datetime, end_dt: datetime, query_filter: Optional[str]
    ) -> ToolResult:
        """Query CalDAV calendar."""
        return ToolResult(
            status="error",
            data={},
            citations=[],
            error_message="CalDAV backend not yet implemented. Configure CALENDAR_API_TYPE=ics for now.",
        )

    async def _query_google(
        self, start_dt: datetime, end_dt: datetime, query_filter: Optional[str]
    ) -> ToolResult:
        """Query Google Calendar."""
        return ToolResult(
            status="error",
            data={},
            citations=[],
            error_message="Google Calendar backend not yet implemented. Configure CALENDAR_API_TYPE=ics for now.",
        )


# Global accessor instance
_calendar_accessor: Optional[CalendarAccessor] = None


def get_calendar_accessor() -> CalendarAccessor:
    """Get or create the global calendar accessor instance."""
    global _calendar_accessor
    if _calendar_accessor is None:
        _calendar_accessor = CalendarAccessor()
    return _calendar_accessor


# Tool function for registration
async def calendar_query(
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
    filter: Optional[str] = None,
) -> ToolResult:
    """
    Query calendar events.

    Args:
        start_date: Start date (YYYY-MM-DD) or None for today
        end_date: End date (YYYY-MM-DD) or None for 7 days ahead
        filter: Optional filter string to match event titles
    """
    accessor = get_calendar_accessor()
    return await accessor.query_events(start_date, end_date, filter)
