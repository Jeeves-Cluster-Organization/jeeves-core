# Telegram Homelab Capability

A Telegram bot capability for jeeves-core that provides secure homelab management through a conversational interface.

## Features

- **SSH Command Execution**: Execute commands on homelab servers with security boundaries
- **File System Access**: List, read, and search files with path validation
- **Calendar Integration**: Query calendar events (ICS, CalDAV, Google Calendar)
- **Notes Search**: Search through markdown/text notes

## Architecture

This capability implements a simplified 4-agent pipeline:

1. **Intent Agent** - Classifies user intent and extracts parameters
2. **Planner Agent** - Creates execution plan with tool selection
3. **Traverser Agent** - Executes tools with proper security boundaries
4. **Synthesizer Agent** - Formats results for Telegram delivery

## Installation

### Prerequisites

```bash
# Install Telegram bot library
pip install python-telegram-bot

# Optional: Install calendar support
pip install icalendar requests

# Optional: Install for other backends
pip install caldav google-auth google-auth-oauthlib google-api-python-client
```

### Configuration

Create a `.env` file or set environment variables:

```bash
# Telegram Configuration
TELEGRAM_BOT_TOKEN=your_bot_token_from_botfather
TELEGRAM_ADMIN_USER_IDS=123456789,987654321  # Comma-separated user IDs
TELEGRAM_MAX_MESSAGE_LENGTH=4096
TELEGRAM_POLLING_TIMEOUT=30

# SSH Configuration
SSH_HOSTS=["server1.local:22", "server2.local:22"]
SSH_PRIVATE_KEY_PATH=/home/user/.ssh/id_rsa
SSH_KNOWN_HOSTS_PATH=/home/user/.ssh/known_hosts
SSH_DEFAULT_USER=root
SSH_TIMEOUT_SECONDS=30
SSH_MAX_OUTPUT_CHARS=8000
SSH_STRICT_HOST_KEY_CHECKING=true

# File System Configuration
HOMELAB_BASE_PATH=/home/homelab
HOMELAB_ALLOWED_DIRS=["/home/homelab/scripts", "/home/homelab/configs", "/home/homelab/docs"]
HOMELAB_FILE_READ_LIMIT_KB=500
HOMELAB_SEARCH_MAX_RESULTS=50
HOMELAB_MAX_FILE_LISTING_DEPTH=3

# Calendar Configuration
CALENDAR_API_TYPE=ics  # ics, caldav, or google
CALENDAR_URL=http://calendar.local/user.ics
# For CalDAV:
# CALDAV_URL=https://caldav.example.com/
# CALDAV_USERNAME=user
# CALDAV_PASSWORD=pass
# For Google Calendar:
# GOOGLE_CALENDAR_CREDENTIALS_PATH=/path/to/credentials.json

# Notes Configuration
NOTES_BACKEND=filesystem  # filesystem, sqlite, or postgresql
NOTES_PATH=/home/homelab/notes
NOTES_SEARCH_MAX_RESULTS=20
NOTES_MAX_NOTE_PREVIEW_CHARS=500
NOTES_SUPPORTED_EXTENSIONS=.md,.txt,.org

# Capability Settings
TELEGRAM_HOMELAB_ENABLE_CONFIRMATIONS=true
TELEGRAM_HOMELAB_MAX_CONCURRENT_REQUESTS=5
```

## Usage

### As a Standalone Bot

Run the bot directly:

```bash
python -m mission_system.capabilities.telegram_homelab.bot
```

Or programmatically:

```python
import asyncio
from mission_system.capabilities.telegram_homelab import run_bot

asyncio.run(run_bot())
```

### Integrated with jeeves-core

1. Register the capability in `mission_system/capability_wiring.py`:

```python
def _discover_capabilities() -> List[str]:
    return [
        "mission_system.capabilities.telegram_homelab.wiring",
        # ... other capabilities
    ]
```

2. The capability will be automatically wired during bootstrap.

3. Start the Telegram bot separately or integrate it into your API server.

## Security Features

### SSH Security

- **Host Whitelist**: Only configured hosts can be accessed
- **Timeout Enforcement**: Commands timeout after configured duration
- **Output Size Limits**: Command output is truncated to prevent memory issues
- **Strict Host Key Checking**: Optionally enforce SSH host key verification

### File Access Security

- **Path Validation**: All paths validated against allowed directories
- **Path Traversal Protection**: `..` sequences blocked
- **File Size Limits**: Files limited to configured size (default 500KB)
- **Search Result Limits**: Maximum number of search results enforced

### Authorization

- **Admin User List**: Only configured Telegram user IDs can use the bot
- **Confirmation Flow**: Destructive operations can require confirmation

## Usage Examples

### SSH Commands

```
Execute 'uptime' on server1
Run 'docker ps' on server2.local
Show disk space on all servers
```

### File Access

```
List files in /home/homelab/scripts
Show me the contents of /home/homelab/configs/nginx.conf
Find all Python files in /home/homelab
Search for *.yaml files
```

### Calendar Queries

```
Show my calendar for today
What events do I have this week?
Find meetings with keyword "standup"
```

### Notes Search

```
Search notes for "kubernetes"
Find notes about Docker
Show notes containing "backup procedure"
```

## Development

### Project Structure

```
telegram_homelab/
├── __init__.py           # Package exports
├── README.md            # This file
├── bot.py               # Telegram bot implementation
├── servicer.py          # Capability servicer (agent pipeline)
├── wiring.py            # Registration entry point
├── agents/              # Agent implementations (future)
│   └── __init__.py
├── config/              # Configuration models
│   ├── __init__.py
│   ├── loader.py        # Environment variable loading
│   └── models.py        # Configuration dataclasses
├── prompts/             # Agent prompts
│   ├── __init__.py
│   └── agent_prompts.py # LLM prompts for agents
└── tools/               # Tool implementations
    ├── __init__.py
    ├── ssh_tools.py     # SSH execution tools
    ├── file_tools.py    # File access tools
    ├── calendar_tools.py # Calendar integration
    └── notes_tools.py   # Notes search tools
```

### Adding New Tools

1. Create tool function in appropriate tools module
2. Return `ToolResult` with status, data, and citations
3. Register tool in `wiring.py` tools_initializer
4. Update prompts to include new tool in planning

### Extending Agents

The current implementation uses a simplified 4-agent pipeline. To extend:

1. Create agent modules in `agents/` directory
2. Implement agent logic following jeeves-core Agent protocol
3. Update servicer to use new agents
4. Add agent-specific prompts in `prompts/`

## Troubleshooting

### Bot doesn't respond

- Check `TELEGRAM_BOT_TOKEN` is set correctly
- Verify bot is running: `/status` command
- Check logs for errors

### SSH commands fail

- Verify hostname is in `SSH_HOSTS` whitelist
- Check SSH key permissions: `chmod 600 ~/.ssh/id_rsa`
- Ensure known_hosts file exists
- Test SSH manually: `ssh user@hostname command`

### File access denied

- Check path is in `HOMELAB_ALLOWED_DIRS`
- Verify file permissions on server
- Check for path traversal attempts (`..`)

### Calendar not working

- For ICS: Verify `CALENDAR_URL` is accessible
- For CalDAV: Install `caldav` library
- For Google: Install google libraries and configure credentials

## License

Part of jeeves-core project.

## Contributing

Submit issues and pull requests to the jeeves-core repository.
