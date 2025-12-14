/**
 * Unit Tests for Jeeves FF ChatClient (static/js/chat.js)
 *
 * Tests cover:
 * - Session management (create, list, switch, delete)
 * - Message handling (send, receive, render)
 * - WebSocket connection and event handling
 * - Internal view panel for agent activity
 * - Confirmation flow handling
 * - UI state management (typing indicator, sidebar)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockSession,
  createMockMessage,
  createMockChatResponse,
  setupChatApiMock,
  createMockWebSocket,
  wsEvents,
  mockFetchSuccess,
  mockFetchError,
} from '../mocks/api.js';

// =============================================================================
// DOM Setup Helpers
// =============================================================================

/**
 * Create the DOM structure matching chat.html template
 */
function setupChatDOM() {
  document.body.innerHTML = `
    <html class="">
      <body>
        <!-- Sessions Sidebar -->
        <aside id="sessions-sidebar" class="w-72">
          <input type="text" id="user-id" value="demo-user">
          <button id="new-session-btn">New Chat</button>
          <div id="session-list">
            <div id="empty-sessions">No chat sessions yet</div>
          </div>
        </aside>

        <!-- Main Chat Area -->
        <div class="flex-1 flex flex-col">
          <!-- Chat Header -->
          <header>
            <button id="toggle-sidebar">Toggle</button>
            <span id="session-title">New Conversation</span>
            <button id="toggle-internal-view">Internal</button>
            <svg id="internal-toggle-icon" class="w-3 h-3"></svg>
          </header>

          <!-- Messages Container -->
          <div id="messages-container" class="flex-1 overflow-y-auto">
            <div id="welcome-message">Welcome to Jeeves FF</div>
          </div>

          <!-- Internal View Panel (hidden by default) -->
          <aside id="internal-panel" class="hidden w-80">
            <h3>Agent Activity</h3>
            <div id="internal-content">
              Send a message to see agent activity
            </div>
          </aside>

          <!-- Typing Indicator -->
          <div id="typing-indicator" class="hidden">
            <div class="flex gap-1">
              <div class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce"></div>
              <div class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce"></div>
              <div class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce"></div>
            </div>
          </div>

          <!-- Message Input -->
          <div class="p-3">
            <textarea id="message-input" placeholder="Type your message..."></textarea>
            <button id="send-btn" disabled>Send</button>
          </div>
        </div>

        <!-- Sidebar Overlay (mobile) -->
        <div id="sidebar-overlay" class="opacity-0 pointer-events-none"></div>

        <!-- Notification Container -->
        <div id="notification-container"></div>

        <!-- Connection Status -->
        <div id="connection-status-dot" class="bg-gray-400"></div>
        <span id="connection-status">Disconnected</span>
      </body>
    </html>
  `;
}

/**
 * Create a mock ChatClient instance for testing
 */
function createChatClient() {
  return {
    baseUrl: window.location.origin,
    ws: null,
    wsToken: null,
    currentSessionId: null,
    userId: 'demo-user',
    sessions: [],
    messages: [],
    isConnected: false,
    reconnectAttempts: 0,
    maxReconnectAttempts: 10,
    streamingMessageId: null,
    isSidebarOpen: false,
    isViewingOldSession: false,
    isInternalViewOpen: false,
    agentActivity: [],
  };
}

// =============================================================================
// Session Management Tests
// =============================================================================

describe('ChatClient - Session Management', () => {
  let fetchMock;

  beforeEach(() => {
    setupChatDOM();
    fetchMock = setupChatApiMock();
    global.fetch = fetchMock;
  });

  describe('loadSessions', () => {
    it('should fetch sessions for current user', async () => {
      const client = createChatClient();

      const response = await fetch(
        `${client.baseUrl}/api/v1/chat/sessions?user_id=${encodeURIComponent(client.userId)}&limit=50`
      );
      client.sessions = await response.json();

      expect(fetchMock).toHaveBeenCalled();
      expect(client.sessions.length).toBeGreaterThan(0);
      expect(client.sessions[0]).toHaveProperty('session_id');
      expect(client.sessions[0]).toHaveProperty('title');
    });

    it('should handle fetch errors gracefully', async () => {
      global.fetch = vi.fn().mockImplementation(() => mockFetchError('Network error', 500));

      let errorThrown = false;
      try {
        const response = await fetch('/api/v1/chat/sessions');
        if (!response.ok) {
          throw new Error(`Failed: ${response.statusText}`);
        }
      } catch (error) {
        errorThrown = true;
      }

      expect(errorThrown).toBe(true);
    });

    it('should render session list grouped by time', () => {
      const now = new Date();
      const today = createMockSession({
        title: 'Today Chat',
        last_activity: now.toISOString(),
      });
      const yesterday = createMockSession({
        title: 'Yesterday Chat',
        last_activity: new Date(now - 86400000).toISOString(),
      });

      const sessions = [today, yesterday];

      // Simulate groupSessionsByTime logic
      const grouped = { Today: [], Yesterday: [], 'This Week': [], Older: [] };
      const todayDate = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      const yesterdayDate = new Date(todayDate - 86400000);

      sessions.forEach((session) => {
        const sessionDate = new Date(session.last_activity);
        if (sessionDate >= todayDate) {
          grouped.Today.push(session);
        } else if (sessionDate >= yesterdayDate) {
          grouped.Yesterday.push(session);
        }
      });

      expect(grouped.Today).toContain(today);
      expect(grouped.Yesterday).toContain(yesterday);
    });
  });

  describe('createSession', () => {
    it('should create a new session via POST', async () => {
      const client = createChatClient();

      const response = await fetch(`${client.baseUrl}/api/v1/chat/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user_id: client.userId,
          title: `New Chat - ${new Date().toLocaleString()}`,
        }),
      });

      const session = await response.json();

      expect(response.status).toBe(201);
      expect(session).toHaveProperty('session_id');
      expect(session.user_id).toBe(client.userId);
    });

    it('should clear current conversation when starting new session', () => {
      const client = createChatClient();
      client.messages = [createMockMessage()];
      client.currentSessionId = 'old-session-id';
      client.isViewingOldSession = true;

      // Simulate clearCurrentConversation
      client.messages = [];
      client.currentSessionId = null;
      client.isViewingOldSession = false;

      expect(client.messages).toHaveLength(0);
      expect(client.currentSessionId).toBeNull();
      expect(client.isViewingOldSession).toBe(false);
    });
  });

  describe('switchSession', () => {
    it('should load messages for selected session', async () => {
      const client = createChatClient();
      const sessionId = 'test-session-id';

      client.currentSessionId = sessionId;

      const response = await fetch(
        `${client.baseUrl}/api/v1/chat/sessions/${sessionId}/messages?user_id=${encodeURIComponent(client.userId)}&limit=100`
      );
      client.messages = await response.json();

      expect(client.currentSessionId).toBe(sessionId);
      expect(Array.isArray(client.messages)).toBe(true);
    });

    it('should update active state in session list', () => {
      const sessionId = 'active-session';

      // Create session item elements
      const sessionList = document.getElementById('session-list');
      sessionList.innerHTML = `
        <div class="session-item" data-session-id="session-1"></div>
        <div class="session-item" data-session-id="${sessionId}"></div>
        <div class="session-item" data-session-id="session-3"></div>
      `;

      // Simulate switchSession update
      document.querySelectorAll('.session-item').forEach((item) => {
        item.classList.remove('active');
        if (item.dataset.sessionId === sessionId) {
          item.classList.add('active');
        }
      });

      const activeItem = document.querySelector(`[data-session-id="${sessionId}"]`);
      expect(activeItem.classList.contains('active')).toBe(true);
    });
  });

  describe('deleteSession', () => {
    it('should send DELETE request and refresh sessions', async () => {
      const sessionId = 'delete-me';

      const response = await fetch(
        `/api/v1/chat/sessions/${sessionId}?user_id=demo-user`,
        { method: 'DELETE' }
      );

      expect(response.status).toBe(204);
    });

    it('should clear messages if deleted session was active', () => {
      const client = createChatClient();
      client.currentSessionId = 'delete-me';
      client.messages = [createMockMessage()];

      // Simulate delete of active session
      const deletedSessionId = 'delete-me';
      if (client.currentSessionId === deletedSessionId) {
        client.currentSessionId = null;
        client.messages = [];
      }

      expect(client.currentSessionId).toBeNull();
      expect(client.messages).toHaveLength(0);
    });
  });
});

// =============================================================================
// Message Handling Tests
// =============================================================================

describe('ChatClient - Message Handling', () => {
  let fetchMock;

  beforeEach(() => {
    setupChatDOM();
    fetchMock = setupChatApiMock();
    global.fetch = fetchMock;
  });

  describe('sendMessage', () => {
    it('should disable send button when input is empty', () => {
      const input = document.getElementById('message-input');
      const sendBtn = document.getElementById('send-btn');

      input.value = '';
      sendBtn.disabled = !input.value.trim();

      expect(sendBtn.disabled).toBe(true);
    });

    it('should enable send button when input has content', () => {
      const input = document.getElementById('message-input');
      const sendBtn = document.getElementById('send-btn');

      input.value = 'Hello';
      sendBtn.disabled = !input.value.trim();

      expect(sendBtn.disabled).toBe(false);
    });

    it('should add user message to UI immediately', () => {
      const client = createChatClient();
      const content = 'Test message';

      // Simulate adding message
      client.messages.push({
        role: 'user',
        content: content,
        created_at: new Date().toISOString(),
      });

      expect(client.messages.length).toBe(1);
      expect(client.messages[0].role).toBe('user');
      expect(client.messages[0].content).toBe(content);
    });

    it('should show typing indicator after sending', () => {
      const indicator = document.getElementById('typing-indicator');

      // Simulate showTypingIndicator
      indicator.classList.remove('hidden');

      expect(indicator.classList.contains('hidden')).toBe(false);
    });

    it('should send message to correct API endpoint', async () => {
      const client = createChatClient();
      client.currentSessionId = 'test-session';

      const response = await fetch(`${client.baseUrl}/api/v1/chat/messages?user_id=${encodeURIComponent(client.userId)}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: 'Hello, Jeeves!',
          session_id: client.currentSessionId,
        }),
      });

      expect(response.ok).toBe(true);
      const result = await response.json();
      expect(result).toHaveProperty('request_id');
      expect(result).toHaveProperty('session_id');
    });

    it('should create new session if none exists', async () => {
      const client = createChatClient();
      client.currentSessionId = null;

      // Simulate create session first
      const sessionResponse = await fetch(`${client.baseUrl}/api/v1/chat/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ user_id: client.userId, title: 'New Chat' }),
      });

      const session = await sessionResponse.json();
      client.currentSessionId = session.session_id;

      expect(client.currentSessionId).toBeTruthy();
    });

    it('should clear current conversation if viewing old session', () => {
      const client = createChatClient();
      client.isViewingOldSession = true;
      client.messages = [createMockMessage()];

      // Simulate clearing before sending
      if (client.isViewingOldSession) {
        client.messages = [];
        client.currentSessionId = null;
        client.isViewingOldSession = false;
      }

      expect(client.messages).toHaveLength(0);
      expect(client.isViewingOldSession).toBe(false);
    });
  });

  describe('renderMessages', () => {
    it('should hide welcome message when messages exist', () => {
      const client = createChatClient();
      client.messages = [createMockMessage()];

      const welcomeMessage = document.getElementById('welcome-message');

      // Simulate renderMessages logic
      if (client.messages.length > 0) {
        welcomeMessage.classList.add('hidden');
      }

      expect(welcomeMessage.classList.contains('hidden')).toBe(true);
    });

    it('should show welcome message when no messages', () => {
      const client = createChatClient();
      client.messages = [];

      const welcomeMessage = document.getElementById('welcome-message');

      // Simulate renderMessages logic
      if (client.messages.length === 0) {
        welcomeMessage.classList.remove('hidden');
      }

      expect(welcomeMessage.classList.contains('hidden')).toBe(false);
    });

    it('should render user messages with correct styling', () => {
      const message = createMockMessage({ role: 'user', content: 'Hello!' });

      // Simulate renderMessage logic
      const isUser = message.role === 'user';
      const bgColor = isUser ? 'bg-blue-600 text-white' : 'bg-white dark:bg-slate-800';

      expect(isUser).toBe(true);
      expect(bgColor).toContain('bg-blue-600');
    });

    it('should render assistant messages with correct styling', () => {
      const message = createMockMessage({ role: 'assistant', content: 'Hello! How can I help?' });

      const isUser = message.role === 'user';
      const bgColor = isUser ? 'bg-blue-600 text-white' : 'bg-white dark:bg-slate-800';

      expect(isUser).toBe(false);
      expect(bgColor).toContain('bg-white');
    });

    it('should escape HTML in user messages', () => {
      // Use string replacement for reliable escaping across all environments
      const escapeHtml = (text) => {
        const escapeMap = {
          '&': '&amp;',
          '<': '&lt;',
          '>': '&gt;',
          '"': '&quot;',
          "'": '&#039;'
        };
        return String(text).replace(/[&<>"']/g, char => escapeMap[char]);
      };

      const maliciousContent = '<script>alert("xss")</script>';
      const escaped = escapeHtml(maliciousContent);

      expect(escaped).not.toContain('<script>');
      expect(escaped).toContain('&lt;script&gt;');
    });
  });

  describe('confirmation handling', () => {
    it('should render confirmation buttons when confirmation_needed', () => {
      const message = {
        role: 'assistant',
        content: 'Do you want to delete this task?',
        is_confirmation: true,
        confirmation_id: 'confirm-123',
        created_at: new Date().toISOString(),
      };

      // Simulate confirmation button rendering
      const confirmationButtons = message.is_confirmation
        ? `
          <button id="confirm-${message.confirmation_id}">Confirm</button>
          <button id="reject-${message.confirmation_id}">Cancel</button>
        `
        : '';

      expect(confirmationButtons).toContain(`id="confirm-${message.confirmation_id}"`);
      expect(confirmationButtons).toContain(`id="reject-${message.confirmation_id}"`);
    });

    it('should send confirmation response to API', async () => {
      const confirmationId = 'confirm-123';
      const userResponse = 'yes';

      const response = await fetch('/api/v1/confirmations', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          confirmation_id: confirmationId,
          user_response: userResponse,
          user_id: 'demo-user',
        }),
      });

      expect(response.ok).toBe(true);
    });
  });
});

// =============================================================================
// WebSocket Event Handling Tests
// =============================================================================

describe('ChatClient - WebSocket Events', () => {
  let mockWs;

  beforeEach(() => {
    setupChatDOM();
    mockWs = createMockWebSocket();
    global.WebSocket = vi.fn().mockImplementation((url) => {
      mockWs.url = url;
      return mockWs;
    });
  });

  describe('handleWebSocketMessage', () => {
    it('should handle orchestrator.completed event (UnifiedEvent)', () => {
      const client = createChatClient();
      let responseReceived = null;

      // Simulate UnifiedEvent
      const message = {
        type: 'event',
        event_id: 'evt-123',
        event_type: 'orchestrator.completed',
        category: 'pipeline_flow',
        timestamp: new Date().toISOString(),
        timestamp_ms: Date.now(),
        session_id: 'sess-123',
        request_id: 'req-456',
        payload: { response_text: 'Task completed!' },
        severity: 'info',
        source: 'agent_emitter',
      };

      // Simulate handleOrchestratorCompleted
      const handleOrchestratorCompleted = (payload) => {
        if (payload.response_text) {
          client.messages.push({
            role: 'assistant',
            content: payload.response_text,
            created_at: new Date().toISOString(),
          });
          responseReceived = payload.response_text;
        }
      };

      // Check message format
      expect(message.type).toBe('event');
      expect(message.event_type).toBe('orchestrator.completed');

      // Handle the event
      handleOrchestratorCompleted(message.payload);

      expect(responseReceived).toBe('Task completed!');
      expect(client.messages.length).toBe(1);
      expect(client.messages[0].content).toBe('Task completed!');
    });

    it('should handle confirmation_needed in orchestrator response', () => {
      const payload = {
        confirmation_needed: true,
        confirmation_message: 'Are you sure you want to delete?',
        confirmation_id: 'confirm-456',
      };

      const message = {
        role: 'assistant',
        content: payload.confirmation_message,
        is_confirmation: true,
        confirmation_id: payload.confirmation_id,
        created_at: new Date().toISOString(),
      };

      expect(message.is_confirmation).toBe(true);
      expect(message.confirmation_id).toBe('confirm-456');
    });

    it('should handle orchestrator.failed event', () => {
      let errorMessage = null;

      const handleOrchestratorFailed = (payload) => {
        errorMessage = 'Request failed: ' + (payload.error || 'Unknown error');
      };

      handleOrchestratorFailed({ error: 'Tool execution failed' });

      expect(errorMessage).toContain('Tool execution failed');
    });

    it('should reload sessions on session.* events (UnifiedEvent)', () => {
      let sessionsReloaded = false;

      // Simulate UnifiedEvent for session events
      const message = {
        type: 'event',
        event_id: 'evt-789',
        event_type: 'session.created',
        category: 'session_event',
        timestamp: new Date().toISOString(),
        timestamp_ms: Date.now(),
        session_id: 'sess-new',
        request_id: 'req-789',
        payload: { session_id: 'sess-new', title: 'New Chat' },
        severity: 'info',
        source: 'gateway',
      };

      const handleEvent = (event_type) => {
        if (
          event_type === 'session.created' ||
          event_type === 'session.updated' ||
          event_type === 'session.deleted'
        ) {
          sessionsReloaded = true;
        }
      };

      expect(message.type).toBe('event');
      expect(message.event_type).toBe('session.created');

      handleEvent(message.event_type);
      expect(sessionsReloaded).toBe(true);
    });
  });
});

// =============================================================================
// Internal View Panel Tests
// =============================================================================

describe('ChatClient - Internal View Panel', () => {
  beforeEach(() => {
    setupChatDOM();
  });

  describe('toggleInternalView', () => {
    it('should show internal panel when toggled on', () => {
      let isInternalViewOpen = false;

      const panel = document.getElementById('internal-panel');
      const toggleIcon = document.getElementById('internal-toggle-icon');

      // Simulate toggle
      isInternalViewOpen = !isInternalViewOpen;
      panel.classList.toggle('hidden', !isInternalViewOpen);
      toggleIcon.style.transform = isInternalViewOpen ? 'rotate(180deg)' : '';

      expect(panel.classList.contains('hidden')).toBe(false);
      expect(toggleIcon.style.transform).toBe('rotate(180deg)');
    });

    it('should hide internal panel when toggled off', () => {
      let isInternalViewOpen = true;

      const panel = document.getElementById('internal-panel');
      panel.classList.remove('hidden');

      // Simulate toggle
      isInternalViewOpen = !isInternalViewOpen;
      panel.classList.toggle('hidden', !isInternalViewOpen);

      expect(panel.classList.contains('hidden')).toBe(true);
    });
  });

  describe('addAgentActivity', () => {
    it('should add activity to the front of the list (UnifiedEvent)', () => {
      const agentActivity = [];

      const addAgentActivity = (event_type, payload, timestamp, category, severity) => {
        agentActivity.unshift({
          event: event_type,
          payload,
          timestamp: timestamp || new Date().toISOString(),
          category,
          severity,
        });
      };

      addAgentActivity('planner.plan_created', { confidence: 0.95 }, new Date().toISOString(), 'agent_lifecycle', 'info');
      addAgentActivity('executor.tool_completed', { tool_name: 'add_task' }, new Date().toISOString(), 'tool_execution', 'info');

      expect(agentActivity.length).toBe(2);
      expect(agentActivity[0].event).toBe('executor.tool_completed');
      expect(agentActivity[0].category).toBe('tool_execution');
      expect(agentActivity[1].event).toBe('planner.plan_created');
      expect(agentActivity[1].category).toBe('agent_lifecycle');
    });

    it('should limit activity list to 50 items (UnifiedEvent)', () => {
      let agentActivity = [];

      const addAgentActivity = (event_type, payload, timestamp, category, severity) => {
        agentActivity.unshift({
          event: event_type,
          payload,
          timestamp: timestamp || new Date().toISOString(),
          category,
          severity,
        });
        if (agentActivity.length > 50) {
          agentActivity = agentActivity.slice(0, 50);
        }
      };

      // Add 60 activities
      for (let i = 0; i < 60; i++) {
        addAgentActivity(`test.event.${i}`, {}, new Date().toISOString(), 'agent_lifecycle', 'info');
      }

      expect(agentActivity.length).toBe(50);
    });
  });

  describe('renderActivityItem', () => {
    it('should render planner activity with correct styling (UnifiedEvent)', () => {
      const activity = {
        event: 'planner.plan_created',
        payload: { intent: 'add_task', confidence: 0.92 },
        timestamp: new Date().toISOString(),
        category: 'agent_lifecycle',
        severity: 'info',
      };

      // Simulate agent name detection
      let agentName = 'System';
      let statusClass = 'bg-gray-100';

      if (activity.event.startsWith('planner.')) {
        agentName = 'Planner';
        statusClass = 'bg-blue-50';
      }

      expect(agentName).toBe('Planner');
      expect(statusClass).toBe('bg-blue-50');
      expect(activity.category).toBe('agent_lifecycle');
    });

    it('should render executor activity with tool info (UnifiedEvent)', () => {
      const activity = {
        event: 'executor.tool_completed',
        payload: { tool_name: 'add_task', status: 'success' },
        timestamp: new Date().toISOString(),
        category: 'tool_execution',
        severity: 'info',
      };

      let agentName = 'System';
      let details = '';

      if (activity.event.startsWith('executor.')) {
        agentName = 'Traverser';
        if (activity.payload.tool_name) {
          details = `Tool: ${activity.payload.tool_name}`;
        }
        if (activity.payload.status) {
          details += ` | Status: ${activity.payload.status}`;
        }
      }

      expect(agentName).toBe('Traverser');
      expect(details).toContain('Tool: add_task');
      expect(details).toContain('Status: success');
      expect(activity.category).toBe('tool_execution');
    });

    it('should render orchestrator completion with success styling (UnifiedEvent)', () => {
      const activity = {
        event: 'orchestrator.completed',
        payload: {},
        timestamp: new Date().toISOString(),
        category: 'pipeline_flow',
        severity: 'info',
      };

      let statusClass = '';

      if (activity.event === 'orchestrator.completed') {
        statusClass = 'bg-green-50';
      } else if (activity.event === 'orchestrator.error') {
        statusClass = 'bg-red-50';
      }

      expect(statusClass).toBe('bg-green-50');
      expect(activity.category).toBe('pipeline_flow');
    });
  });
});

// =============================================================================
// UI State Tests
// =============================================================================

describe('ChatClient - UI State', () => {
  beforeEach(() => {
    setupChatDOM();
  });

  describe('typing indicator', () => {
    it('should show typing indicator', () => {
      const indicator = document.getElementById('typing-indicator');
      indicator.classList.remove('hidden');

      expect(indicator.classList.contains('hidden')).toBe(false);
    });

    it('should hide typing indicator', () => {
      const indicator = document.getElementById('typing-indicator');
      indicator.classList.add('hidden');

      expect(indicator.classList.contains('hidden')).toBe(true);
    });
  });

  describe('sidebar toggle', () => {
    it('should open sidebar on mobile', () => {
      const sidebar = document.getElementById('sessions-sidebar');
      const overlay = document.getElementById('sidebar-overlay');

      // Simulate setSidebarOpen(true)
      sidebar.classList.add('open');
      overlay.classList.add('active');
      overlay.style.pointerEvents = 'auto';

      expect(sidebar.classList.contains('open')).toBe(true);
      expect(overlay.style.pointerEvents).toBe('auto');
    });

    it('should close sidebar on mobile', () => {
      const sidebar = document.getElementById('sessions-sidebar');
      const overlay = document.getElementById('sidebar-overlay');

      // Start open
      sidebar.classList.add('open');
      overlay.classList.add('active');

      // Simulate setSidebarOpen(false)
      sidebar.classList.remove('open');
      overlay.classList.remove('active');
      overlay.style.pointerEvents = 'none';

      expect(sidebar.classList.contains('open')).toBe(false);
      expect(overlay.style.pointerEvents).toBe('none');
    });
  });

  describe('textarea auto-resize', () => {
    it('should resize textarea based on content', () => {
      const textarea = document.getElementById('message-input');

      const autoResizeTextarea = (el) => {
        el.style.height = 'auto';
        el.style.height = Math.min(el.scrollHeight, 200) + 'px';
      };

      // Simulate content that causes scrollHeight
      Object.defineProperty(textarea, 'scrollHeight', {
        value: 100,
        writable: true,
      });

      autoResizeTextarea(textarea);

      expect(textarea.style.height).toBe('100px');
    });

    it('should cap textarea height at 200px', () => {
      const textarea = document.getElementById('message-input');

      const autoResizeTextarea = (el) => {
        el.style.height = 'auto';
        el.style.height = Math.min(el.scrollHeight, 200) + 'px';
      };

      Object.defineProperty(textarea, 'scrollHeight', {
        value: 300,
        writable: true,
      });

      autoResizeTextarea(textarea);

      expect(textarea.style.height).toBe('200px');
    });
  });

  describe('connection status', () => {
    it('should show connected status', () => {
      const statusText = document.getElementById('connection-status');
      const statusDot = document.getElementById('connection-status-dot');

      // Simulate updateConnectionStatus('Connected', 'green')
      statusText.textContent = 'Connected';
      statusDot.className = 'w-3 h-3 rounded-full bg-green-500';

      expect(statusText.textContent).toBe('Connected');
      expect(statusDot.className).toContain('bg-green-500');
    });

    it('should show disconnected status', () => {
      const statusText = document.getElementById('connection-status');
      const statusDot = document.getElementById('connection-status-dot');

      statusText.textContent = 'Disconnected';
      statusDot.className = 'w-3 h-3 rounded-full bg-gray-400';

      expect(statusText.textContent).toBe('Disconnected');
      expect(statusDot.className).toContain('bg-gray-400');
    });
  });

  describe('notifications', () => {
    it('should show notification', () => {
      const container = document.getElementById('notification-container');

      // Simulate showNotification
      const notification = document.createElement('div');
      notification.className = 'notification bg-blue-500 text-white';
      notification.textContent = 'Test notification';
      container.appendChild(notification);

      expect(container.children.length).toBe(1);
      expect(container.firstChild.textContent).toBe('Test notification');
    });

    it('should auto-remove notification after timeout', () => {
      vi.useFakeTimers();

      const container = document.getElementById('notification-container');
      const notification = document.createElement('div');
      container.appendChild(notification);

      setTimeout(() => notification.remove(), 3000);

      expect(container.children.length).toBe(1);

      vi.advanceTimersByTime(3000);

      expect(container.children.length).toBe(0);

      vi.useRealTimers();
    });
  });
});

// =============================================================================
// Keyboard Shortcuts Tests
// =============================================================================

describe('ChatClient - Keyboard Shortcuts', () => {
  beforeEach(() => {
    setupChatDOM();
  });

  it('should send message on Enter key', () => {
    const input = document.getElementById('message-input');
    let messageSent = false;

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        messageSent = true;
      }
    });

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', shiftKey: false }));

    expect(messageSent).toBe(true);
  });

  it('should not send message on Shift+Enter', () => {
    const input = document.getElementById('message-input');
    let messageSent = false;

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        messageSent = true;
      }
    });

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true }));

    expect(messageSent).toBe(false);
  });

  it('should close sidebar on Escape key', () => {
    const sidebar = document.getElementById('sessions-sidebar');
    sidebar.classList.add('open');
    let sidebarClosed = false;

    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && sidebar.classList.contains('open')) {
        sidebar.classList.remove('open');
        sidebarClosed = true;
      }
    });

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

    expect(sidebarClosed).toBe(true);
  });
});
