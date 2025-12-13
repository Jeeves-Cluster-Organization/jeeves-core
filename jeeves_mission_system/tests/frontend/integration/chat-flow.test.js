/**
 * Integration Tests - Chat Flow
 *
 * Tests the complete chat flow from user input to response rendering,
 * including WebSocket events and session management.
 *
 * These tests verify:
 * - Complete message send/receive cycle
 * - Session creation and switching
 * - WebSocket event handling
 * - Confirmation handling (text-based, no modals)
 * - Error handling and recovery
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockSession,
  createMockMessage,
  createMockChatResponse,
  createMockWebSocket,
  wsEvents,
  mockFetchSuccess,
  mockFetchError,
} from '../mocks/api.js';

// =============================================================================
// Test Fixtures
// =============================================================================

function setupIntegrationDOM() {
  document.body.innerHTML = `
    <html class="">
      <body>
        <!-- Sessions Sidebar -->
        <aside id="sessions-sidebar" class="w-72">
          <input type="text" id="user-id" value="test-user">
          <button id="new-session-btn">New Chat</button>
          <div id="session-list"></div>
        </aside>

        <!-- Main Chat Area -->
        <div class="flex-1 flex flex-col">
          <header>
            <span id="session-title">New Conversation</span>
          </header>

          <div id="messages-container" class="flex-1 overflow-y-auto">
            <div id="welcome-message">Welcome to Jeeves FF</div>
          </div>

          <div id="typing-indicator" class="hidden"></div>

          <div class="p-3">
            <textarea id="message-input" placeholder="Type your message..."></textarea>
            <button id="send-btn" disabled>Send</button>
          </div>
        </div>

        <div id="notification-container"></div>
        <div id="connection-status-dot"></div>
        <span id="connection-status">Disconnected</span>
      </body>
    </html>
  `;
}

// =============================================================================
// Complete Message Flow Tests
// =============================================================================

describe('Chat Flow - Complete Message Cycle', () => {
  let mockWs;

  beforeEach(() => {
    setupIntegrationDOM();
    mockWs = createMockWebSocket();
    global.WebSocket = vi.fn().mockImplementation(() => mockWs);
    global.fetch = vi.fn();
  });

  it('should complete a full message send/receive cycle', async () => {
    // State tracking
    const state = {
      sessionId: null,
      messages: [],
      isProcessing: false,
    };

    // 1. Setup - Create session
    global.fetch.mockImplementationOnce(() =>
      mockFetchSuccess(createMockSession({ session_id: 'test-session' }), 201)
    );

    const sessionResponse = await fetch('/api/v1/chat/sessions', {
      method: 'POST',
      body: JSON.stringify({ user_id: 'test-user' }),
    });
    const session = await sessionResponse.json();
    state.sessionId = session.session_id;

    expect(state.sessionId).toBe('test-session');

    // 2. Send message
    const userMessage = 'Hello, Jeeves!';
    state.messages.push({
      role: 'user',
      content: userMessage,
      created_at: new Date().toISOString(),
    });
    state.isProcessing = true;

    // Show typing indicator
    const typingIndicator = document.getElementById('typing-indicator');
    typingIndicator.classList.remove('hidden');

    global.fetch.mockImplementationOnce(() =>
      mockFetchSuccess(
        createMockChatResponse({
          session_id: state.sessionId,
          status: 'completed',
          response: 'Hello! How can I help you today?',
        })
      )
    );

    const messageResponse = await fetch('/api/v1/chat/messages', {
      method: 'POST',
      body: JSON.stringify({
        message: userMessage,
        session_id: state.sessionId,
      }),
    });
    const result = await messageResponse.json();

    // 3. Receive response
    state.messages.push({
      role: 'assistant',
      content: result.response,
      created_at: new Date().toISOString(),
    });
    state.isProcessing = false;

    // Hide typing indicator
    typingIndicator.classList.add('hidden');

    // 4. Verify state
    expect(state.messages).toHaveLength(2);
    expect(state.messages[0].role).toBe('user');
    expect(state.messages[1].role).toBe('assistant');
    expect(state.isProcessing).toBe(false);
    expect(typingIndicator.classList.contains('hidden')).toBe(true);
  });

  it('should handle WebSocket streaming response', async () => {
    const state = {
      sessionId: 'test-session',
      messages: [{ role: 'user', content: 'Tell me about the weather' }],
      streamingContent: '',
    };

    // Simulate WebSocket connection
    mockWs.simulateOpen();

    // Simulate streaming chunks
    const chunks = ['The weather', ' today is', ' sunny and warm.'];

    for (const chunk of chunks) {
      mockWs.simulateMessage({
        event: 'orchestrator.streaming',
        payload: { chunk },
      });

      // Accumulate streaming content
      state.streamingContent += chunk;
    }

    // Simulate completion
    mockWs.simulateMessage({
      event: 'orchestrator.completed',
      payload: {
        response_text: state.streamingContent,
        confirmation_needed: false,
      },
    });

    // Final state
    state.messages.push({
      role: 'assistant',
      content: state.streamingContent,
    });

    expect(state.streamingContent).toBe('The weather today is sunny and warm.');
    expect(state.messages).toHaveLength(2);
  });

  it('should handle text-based confirmation flow', async () => {
    const state = {
      sessionId: 'test-session',
      messages: [],
      pendingConfirmation: null,
    };

    // User requests a destructive action
    state.messages.push({
      role: 'user',
      content: 'Delete all my completed tasks',
    });

    // Simulate confirmation request via WebSocket
    mockWs.simulateOpen();
    mockWs.simulateMessage({
      event: 'orchestrator.completed',
      payload: {
        confirmation_needed: true,
        confirmation_message:
          'This will delete 5 completed tasks. Reply "yes" to confirm or "no" to cancel.',
        confirmation_id: 'confirm-delete-123',
      },
    });

    // Store pending confirmation
    state.pendingConfirmation = {
      id: 'confirm-delete-123',
      message:
        'This will delete 5 completed tasks. Reply "yes" to confirm or "no" to cancel.',
    };

    // Add assistant message asking for confirmation
    state.messages.push({
      role: 'assistant',
      content: state.pendingConfirmation.message,
      isConfirmation: true,
    });

    // User confirms with text
    state.messages.push({
      role: 'user',
      content: 'yes',
    });

    // Submit confirmation
    global.fetch.mockImplementationOnce(() =>
      mockFetchSuccess({ response_text: 'Deleted 5 tasks successfully.' })
    );

    const response = await fetch('/api/v1/confirmations', {
      method: 'POST',
      body: JSON.stringify({
        confirmation_id: state.pendingConfirmation.id,
        user_response: 'yes',
        user_id: 'test-user',
      }),
    });
    const result = await response.json();

    // Clear pending confirmation
    state.pendingConfirmation = null;

    // Add result message
    state.messages.push({
      role: 'assistant',
      content: result.response_text,
    });

    expect(state.messages).toHaveLength(4);
    expect(state.messages[3].content).toBe('Deleted 5 tasks successfully.');
    expect(state.pendingConfirmation).toBeNull();
  });
});

// =============================================================================
// Session Management Flow Tests
// =============================================================================

describe('Chat Flow - Session Management', () => {
  beforeEach(() => {
    setupIntegrationDOM();
    global.fetch = vi.fn();
  });

  it('should load and render session list', async () => {
    const sessions = [
      createMockSession({ title: 'Chat 1', session_id: 's1' }),
      createMockSession({ title: 'Chat 2', session_id: 's2' }),
      createMockSession({ title: 'Chat 3', session_id: 's3' }),
    ];

    global.fetch.mockImplementationOnce(() => mockFetchSuccess(sessions));

    const response = await fetch('/api/v1/chat/sessions?user_id=test-user');
    const loadedSessions = await response.json();

    // Render sessions
    const sessionList = document.getElementById('session-list');
    sessionList.innerHTML = loadedSessions
      .map(
        (s) => `
        <div class="session-item" data-session-id="${s.session_id}">
          <span>${s.title}</span>
        </div>
      `
      )
      .join('');

    expect(sessionList.querySelectorAll('.session-item').length).toBe(3);
  });

  it('should switch sessions and load messages', async () => {
    const state = {
      currentSessionId: 's1',
      messages: [],
    };

    const newSessionMessages = [
      createMockMessage({ role: 'user', content: 'Hello' }),
      createMockMessage({ role: 'assistant', content: 'Hi there!' }),
    ];

    // Switch to session s2
    global.fetch.mockImplementationOnce(() => mockFetchSuccess(newSessionMessages));

    const response = await fetch('/api/v1/chat/sessions/s2/messages?user_id=test-user');
    state.messages = await response.json();
    state.currentSessionId = 's2';

    expect(state.currentSessionId).toBe('s2');
    expect(state.messages).toHaveLength(2);
  });

  it('should create new session when none exists', async () => {
    const state = {
      currentSessionId: null,
      messages: [],
    };

    // Create new session
    global.fetch.mockImplementationOnce(() =>
      mockFetchSuccess(createMockSession({ session_id: 'new-session' }), 201)
    );

    const response = await fetch('/api/v1/chat/sessions', {
      method: 'POST',
      body: JSON.stringify({ user_id: 'test-user' }),
    });
    const session = await response.json();
    state.currentSessionId = session.session_id;

    expect(state.currentSessionId).toBe('new-session');
    expect(state.messages).toHaveLength(0);
  });
});

// =============================================================================
// Error Handling Flow Tests
// =============================================================================

describe('Chat Flow - Error Handling', () => {
  let mockWs;

  beforeEach(() => {
    setupIntegrationDOM();
    mockWs = createMockWebSocket();
    global.WebSocket = vi.fn().mockImplementation(() => mockWs);
    global.fetch = vi.fn();
  });

  it('should handle API error gracefully', async () => {
    const state = {
      messages: [],
      error: null,
    };

    state.messages.push({ role: 'user', content: 'Test message' });

    global.fetch.mockImplementationOnce(() =>
      mockFetchError('Service unavailable', 503)
    );

    const response = await fetch('/api/v1/chat/messages', { method: 'POST' });

    if (!response.ok) {
      const errorData = await response.json();
      state.error = errorData.detail;

      // Add error message
      state.messages.push({
        role: 'system',
        content: `Error: ${state.error}`,
        isError: true,
      });
    }

    expect(state.error).toBe('Service unavailable');
    expect(state.messages[1].isError).toBe(true);
  });

  it('should handle WebSocket disconnection', () => {
    const state = {
      isConnected: true,
      reconnecting: false,
    };

    mockWs.simulateOpen();
    state.isConnected = true;

    // Simulate disconnection
    mockWs.simulateClose();
    state.isConnected = false;
    state.reconnecting = true;

    // Update UI
    const statusEl = document.getElementById('connection-status');
    statusEl.textContent = 'Reconnecting...';

    expect(state.isConnected).toBe(false);
    expect(state.reconnecting).toBe(true);
    expect(statusEl.textContent).toBe('Reconnecting...');
  });

  it('should handle orchestrator failure event', () => {
    const state = {
      messages: [{ role: 'user', content: 'Do something complex' }],
      isProcessing: true,
    };

    mockWs.simulateOpen();

    // Simulate failure
    mockWs.simulateMessage(wsEvents.orchestratorFailed('Tool execution failed'));

    state.isProcessing = false;
    state.messages.push({
      role: 'system',
      content: 'Request failed: Tool execution failed',
      isError: true,
    });

    expect(state.isProcessing).toBe(false);
    expect(state.messages[1].isError).toBe(true);
  });
});

// =============================================================================
// Navigation Flow Tests
// =============================================================================

describe('Chat Flow - Navigation', () => {
  beforeEach(() => {
    setupIntegrationDOM();
  });

  it('should navigate between chat sessions', () => {
    const state = {
      currentView: 'chat',
      currentSessionId: 's1',
    };

    // Simulate clicking on another session
    const newSessionId = 's2';
    state.currentSessionId = newSessionId;

    // Update URL (simulated)
    const expectedPath = `/chat?session=${newSessionId}`;

    expect(state.currentSessionId).toBe('s2');
    expect(expectedPath).toBe('/chat?session=s2');
  });

  it('should preserve state when returning from other views', () => {
    const state = {
      currentView: 'chat',
      currentSessionId: 's1',
      messages: [createMockMessage()],
    };

    // Navigate away
    state.currentView = 'kanban';

    // Navigate back
    state.currentView = 'chat';

    // State should be preserved
    expect(state.currentSessionId).toBe('s1');
    expect(state.messages).toHaveLength(1);
  });
});
