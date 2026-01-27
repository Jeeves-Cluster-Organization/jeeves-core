# Telegram Homelab Capability - Quick Start Guide

This guide will help you get the Telegram Homelab capability up and running in minutes.

## Prerequisites

1. **Telegram Bot Token**
   - Open Telegram and search for `@BotFather`
   - Send `/newbot` and follow the instructions
   - Save your bot token (looks like `123456789:ABCdefGHIjklMNOpqrsTUVwxyz`)

2. **Your Telegram User ID**
   - Search for `@userinfobot` on Telegram
   - Send any message to get your user ID
   - Save this number (e.g., `123456789`)

3. **Python Dependencies**
   ```bash
   pip install python-telegram-bot

   # Optional: For calendar support
   pip install icalendar requests
   ```

## Step 1: Configure the Bot

1. Copy the example configuration:
   ```bash
   cd mission_system/capabilities/telegram_homelab
   cp .env.example .env
   ```

2. Edit `.env` with your information:
   ```bash
   # Minimum required configuration
   TELEGRAM_BOT_TOKEN=your_bot_token_here
   TELEGRAM_ADMIN_USER_IDS=your_user_id_here

   # SSH (if you want SSH commands)
   SSH_HOSTS=["your-server.local:22"]
   SSH_PRIVATE_KEY_PATH=/home/user/.ssh/id_rsa
   SSH_KNOWN_HOSTS_PATH=/home/user/.ssh/known_hosts

   # Files (if you want file access)
   HOMELAB_ALLOWED_DIRS=["/home/user/homelab"]

   # Calendar (if you want calendar integration)
   CALENDAR_API_TYPE=ics
   CALENDAR_URL=http://your-calendar-url/calendar.ics

   # Notes (if you want notes search)
   NOTES_PATH=/home/user/notes
   ```

## Step 2: Test the Configuration

Run the test script to verify everything is set up correctly:

```bash
cd mission_system/capabilities/telegram_homelab
python test_capability.py
```

You should see output like:
```
============================================================
Telegram Homelab Capability Test Suite
============================================================
Testing configuration loading...
✓ Configuration loaded successfully
...
```

## Step 3: Run the Bot

### Option A: Standalone Bot

Run the bot directly:

```bash
cd mission_system/capabilities/telegram_homelab
python run_bot.py
```

You should see:
```
============================================================
Telegram Homelab Bot
============================================================
Starting Telegram bot...
Press Ctrl+C to stop
============================================================
Telegram bot is running...
```

### Option B: Integrated with jeeves-core

1. Register the capability in `mission_system/capability_wiring.py`:

   ```python
   def _discover_capabilities() -> List[str]:
       discovered = os.getenv("JEEVES_CAPABILITIES", "")
       if discovered:
           return [c.strip() for c in discovered.split(",") if c.strip()]

       return [
           "mission_system.capabilities.telegram_homelab.wiring",
           # ... other capabilities
       ]
   ```

2. Start jeeves-core:
   ```bash
   # From jeeves-core root
   python -m mission_system.bootstrap
   ```

3. In a separate terminal, start the bot:
   ```bash
   cd mission_system/capabilities/telegram_homelab
   python run_bot.py
   ```

## Step 4: Test the Bot on Telegram

1. Open Telegram and search for your bot by name
2. Send `/start` - you should get a welcome message
3. Try some commands:

   **Test file access:**
   ```
   List files in /home/user/homelab
   ```

   **Test SSH (if configured):**
   ```
   Execute 'uptime' on your-server
   ```

   **Test calendar (if configured):**
   ```
   Show my calendar for today
   ```

   **Test notes (if configured):**
   ```
   Search notes for "docker"
   ```

## Troubleshooting

### Bot doesn't start

**Error: "No module named 'telegram'"**
```bash
pip install python-telegram-bot
```

**Error: "TELEGRAM_BOT_TOKEN is required"**
- Check your `.env` file has `TELEGRAM_BOT_TOKEN=...`
- If using system env vars, export them: `export TELEGRAM_BOT_TOKEN=...`

### Bot starts but doesn't respond

**Check bot is authorized:**
- Verify your user ID is in `TELEGRAM_ADMIN_USER_IDS`
- Check bot logs for "not authorized" messages

**Check logs:**
```bash
tail -f telegram_bot.log
```

### SSH commands fail

**Error: "Hostname 'X' is not in the allowed SSH hosts list"**
- Add your host to `SSH_HOSTS` in `.env`:
  ```bash
  SSH_HOSTS=["server1.local:22", "server2.local:22"]
  ```

**Error: "SSH client not found"**
- Install OpenSSH client: `apt-get install openssh-client` (Ubuntu/Debian)

**Test SSH manually:**
```bash
ssh -i /home/user/.ssh/id_rsa user@your-server uptime
```

### File access fails

**Error: "Path 'X' is not within allowed directories"**
- Add directory to `HOMELAB_ALLOWED_DIRS`:
  ```bash
  HOMELAB_ALLOWED_DIRS=["/home/user/homelab", "/etc/configs"]
  ```

**Error: "Permission denied"**
- Check file permissions: `ls -la /path/to/file`
- Ensure the user running the bot has read access

### Calendar doesn't work

**For ICS calendars:**
```bash
# Test URL is accessible
curl -I http://your-calendar-url/calendar.ics

# Install required library
pip install icalendar requests
```

**For CalDAV:**
```bash
pip install caldav
```

**For Google Calendar:**
```bash
pip install google-auth google-auth-oauthlib google-api-python-client
```

## Next Steps

### Secure Your Bot

1. **Limit admin access:**
   ```bash
   # Only allow specific users
   TELEGRAM_ADMIN_USER_IDS=123456789,987654321
   ```

2. **Enable confirmations for destructive operations:**
   ```bash
   TELEGRAM_HOMELAB_ENABLE_CONFIRMATIONS=true
   ```

3. **Use strict SSH host key checking:**
   ```bash
   SSH_STRICT_HOST_KEY_CHECKING=true
   ```

### Add More Capabilities

1. **Add more SSH hosts:**
   ```bash
   SSH_HOSTS=["server1:22", "server2:22", "nas.local:22"]
   ```

2. **Allow more directories:**
   ```bash
   HOMELAB_ALLOWED_DIRS=["/home/homelab", "/var/www", "/etc/nginx"]
   ```

3. **Configure calendar backend:**
   ```bash
   # For CalDAV
   CALENDAR_API_TYPE=caldav
   CALDAV_URL=https://caldav.example.com/
   CALDAV_USERNAME=user
   CALDAV_PASSWORD=pass
   ```

### Customize Prompts

Edit prompts in `prompts/agent_prompts.py` to change how the agents interpret and respond to messages.

### Add LLM Support

For better intent classification and planning, integrate an LLM provider:

```python
from mission_system.capabilities.telegram_homelab import TelegramBot
from your_llm_provider import YourLLMProvider

llm = YourLLMProvider()
bot = TelegramBot(llm_provider=llm)
await bot.start()
```

## Getting Help

- Check the logs: `tail -f telegram_bot.log`
- Run tests: `python test_capability.py --debug`
- Review README.md for detailed documentation
- File issues on the jeeves-core GitHub repository

## Example Configuration

Here's a complete minimal configuration:

```bash
# .env file
TELEGRAM_BOT_TOKEN=123456789:ABCdefGHIjklMNOpqrsTUVwxyz
TELEGRAM_ADMIN_USER_IDS=123456789

SSH_HOSTS=["homelab.local:22"]
SSH_PRIVATE_KEY_PATH=/home/user/.ssh/id_rsa
SSH_KNOWN_HOSTS_PATH=/home/user/.ssh/known_hosts
SSH_DEFAULT_USER=root

HOMELAB_ALLOWED_DIRS=["/home/user/homelab"]
NOTES_PATH=/home/user/notes

CALENDAR_API_TYPE=ics
CALENDAR_URL=http://homelab.local/calendar.ics
```

With this configuration, you can:
- Execute SSH commands on `homelab.local`
- Access files in `/home/user/homelab`
- Search notes in `/home/user/notes`
- Query calendar from ICS URL

## Summary

You now have a working Telegram bot that can:
- 🔧 Execute SSH commands on your homelab servers
- 📁 List, read, and search files
- 📅 Query calendar events
- 📝 Search notes

Start chatting with your bot on Telegram and manage your homelab! 🎉
