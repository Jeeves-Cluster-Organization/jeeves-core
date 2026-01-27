"""Telegram bot integration for Homelab capability."""

import asyncio
import logging
from typing import Optional

from .config import get_config
from .servicer import TelegramHomelabServicer

logger = logging.getLogger(__name__)


class TelegramBot:
    """
    Telegram bot that integrates with jeeves-core Homelab capability.

    This bot can be run standalone or integrated into jeeves-core's API server.
    """

    def __init__(self, servicer: Optional[TelegramHomelabServicer] = None, llm_provider=None):
        self.config = get_config().telegram
        self.servicer = servicer or TelegramHomelabServicer(llm_provider=llm_provider)
        self.bot = None

    async def initialize(self) -> None:
        """Initialize the Telegram bot."""
        try:
            # Import telegram libraries
            try:
                from telegram import Update
                from telegram.ext import (
                    Application,
                    CommandHandler,
                    MessageHandler,
                    filters,
                )
            except ImportError:
                raise ImportError(
                    "python-telegram-bot library not installed. "
                    "Install with: pip install python-telegram-bot"
                )

            # Create application
            self.bot = (
                Application.builder()
                .token(self.config.bot_token)
                .build()
            )

            # Register handlers
            self.bot.add_handler(CommandHandler("start", self._handle_start))
            self.bot.add_handler(CommandHandler("help", self._handle_help))
            self.bot.add_handler(CommandHandler("status", self._handle_status))
            self.bot.add_handler(
                MessageHandler(filters.TEXT & ~filters.COMMAND, self._handle_message)
            )

            logger.info("Telegram bot initialized successfully")

        except Exception as e:
            logger.exception(f"Failed to initialize Telegram bot: {e}")
            raise

    async def start(self) -> None:
        """Start the Telegram bot with polling."""
        if not self.bot:
            await self.initialize()

        try:
            logger.info("Starting Telegram bot polling...")
            await self.bot.initialize()
            await self.bot.start()
            await self.bot.updater.start_polling(
                timeout=self.config.polling_timeout,
                allowed_updates=self.config.allowed_updates,
            )

            # Keep the bot running
            logger.info("Telegram bot is running. Press Ctrl+C to stop.")
            await asyncio.Event().wait()

        except KeyboardInterrupt:
            logger.info("Stopping Telegram bot...")
        finally:
            await self.stop()

    async def stop(self) -> None:
        """Stop the Telegram bot."""
        if self.bot:
            try:
                await self.bot.updater.stop()
                await self.bot.stop()
                await self.bot.shutdown()
                logger.info("Telegram bot stopped")
            except Exception as e:
                logger.error(f"Error stopping bot: {e}")

    async def _handle_start(self, update, context) -> None:
        """Handle /start command."""
        user = update.effective_user
        welcome_message = f"""
👋 Welcome to Jeeves Homelab Assistant, {user.first_name}!

I can help you manage your homelab through Telegram:

🔧 **SSH Commands** - Execute commands on your servers
📁 **File Access** - List, read, and search files
📅 **Calendar** - Query your calendar events
📝 **Notes** - Search your notes

Type /help for more information.
"""
        await update.message.reply_text(welcome_message.strip())

    async def _handle_help(self, update, context) -> None:
        """Handle /help command."""
        help_message = """
📖 **Jeeves Homelab Assistant Help**

**Available Commands:**
/start - Start the bot
/help - Show this help message
/status - Show bot status

**Usage Examples:**

**SSH Commands:**
- "Execute 'uptime' on server1"
- "Run 'docker ps' on my server"

**File Access:**
- "List files in /home/homelab/scripts"
- "Show me the contents of config.yaml"
- "Find all Python files"

**Calendar:**
- "Show my calendar for today"
- "What events do I have this week?"

**Notes:**
- "Search notes for 'kubernetes'"
- "Find notes about Docker"

Just send me a message and I'll help you!
"""
        await update.message.reply_text(help_message.strip())

    async def _handle_status(self, update, context) -> None:
        """Handle /status command."""
        status_message = f"""
✅ **Bot Status**

🤖 Bot: Online
🔧 SSH: {"✓ Configured" if self.config else "✗ Not configured"}
📁 Files: {"✓ Configured" if self.config else "✗ Not configured"}
📅 Calendar: {"✓ Configured" if self.config else "✗ Not configured"}
📝 Notes: {"✓ Configured" if self.config else "✗ Not configured"}
"""
        await update.message.reply_text(status_message.strip())

    async def _handle_message(self, update, context) -> None:
        """Handle regular text messages."""
        user_id = str(update.effective_user.id)
        chat_id = str(update.effective_chat.id)
        message_text = update.message.text

        # Check if user is authorized (if admin list is configured)
        if self.config.admin_user_ids and int(user_id) not in self.config.admin_user_ids:
            await update.message.reply_text(
                "⚠️ You are not authorized to use this bot. "
                "Please contact the administrator."
            )
            return

        logger.info(f"Received message from user {user_id}: {message_text}")

        try:
            # Send "typing" action
            await context.bot.send_chat_action(
                chat_id=chat_id, action="typing"
            )

            # Process message through servicer
            response_text = ""
            async for event in self.servicer.process_request(
                user_id=user_id,
                session_id=chat_id,
                message=message_text,
                context={
                    "telegram_user": update.effective_user.to_dict(),
                    "chat_type": update.effective_chat.type,
                },
            ):
                event_type = event.get("type")

                if event_type == "response":
                    # Final response
                    response_text = event.get("data", {}).get("text", "")
                    citations = event.get("data", {}).get("citations", [])

                    # Add citations if present
                    if citations:
                        response_text += "\n\n📎 **Sources:**\n"
                        for citation in citations:
                            if isinstance(citation, dict):
                                response_text += f"- {citation.get('type', 'source')}\n"
                            else:
                                response_text += f"- {citation}\n"

                elif event_type == "error":
                    # Error occurred
                    error_msg = event.get("data", {}).get("error", "Unknown error")
                    response_text = f"❌ **Error:** {error_msg}"

            # Send response
            if response_text:
                # Split long messages if needed
                await self._send_long_message(update, response_text)
            else:
                await update.message.reply_text(
                    "✅ Request processed, but no response generated."
                )

        except Exception as e:
            logger.exception(f"Error handling message: {e}")
            await update.message.reply_text(
                f"❌ An error occurred while processing your request: {str(e)}"
            )

    async def _send_long_message(self, update, text: str) -> None:
        """Send potentially long message, splitting if needed."""
        max_length = self.config.max_message_length

        if len(text) <= max_length:
            await update.message.reply_text(text)
        else:
            # Split into chunks
            chunks = []
            current_chunk = ""

            for line in text.split("\n"):
                if len(current_chunk) + len(line) + 1 > max_length:
                    chunks.append(current_chunk)
                    current_chunk = line
                else:
                    current_chunk += "\n" + line if current_chunk else line

            if current_chunk:
                chunks.append(current_chunk)

            # Send chunks
            for i, chunk in enumerate(chunks):
                if i == 0:
                    await update.message.reply_text(chunk)
                else:
                    await update.message.reply_text(f"(continued...)\n\n{chunk}")


async def run_bot(llm_provider=None) -> None:
    """
    Run the Telegram bot.

    This is a standalone entry point for running the bot.

    Args:
        llm_provider: Optional LLM provider for agent inference
    """
    bot = TelegramBot(llm_provider=llm_provider)
    await bot.start()


if __name__ == "__main__":
    # Run bot standalone
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )
    asyncio.run(run_bot())
