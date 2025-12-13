/**
 * API Mock Utilities for Jeeves FF Frontend Tests
 *
 * These mocks replicate the actual API contract from:
 * - api/chat.py
 * - api/kanban.py
 * - docs/architecture/API_CONTRACT.md
 */

import { vi } from 'vitest';

// =============================================================================
// Mock Data Factories
// =============================================================================

/**
 * Create a mock session matching SessionResponse from chat.py
 */
export function createMockSession(overrides = {}) {
  const now = new Date().toISOString();
  return {
    session_id: `session-${Math.random().toString(36).substr(2, 9)}`,
    user_id: 'demo-user',
    title: 'Test Chat Session',
    message_count: 0,
    created_at: now,
    last_activity: now,
    deleted_at: null,
    archived_at: null,
    ...overrides,
  };
}

/**
 * Create a mock message matching MessageResponse from chat.py
 */
export function createMockMessage(overrides = {}) {
  return {
    message_id: Math.floor(Math.random() * 10000),
    session_id: 'session-abc-123',
    role: 'user',
    content: 'Test message content',
    created_at: new Date().toISOString(),
    deleted_at: null,
    edited_at: null,
    original_content: null,
    ...overrides,
  };
}

/**
 * Create a mock chat API response matching MessageSendResponse from chat.py
 */
export function createMockChatResponse(overrides = {}) {
  return {
    request_id: `req-${Math.random().toString(36).substr(2, 9)}`,
    session_id: 'session-abc-123',
    status: 'completed',
    response: 'This is the assistant response.',
    confirmation_needed: false,
    confirmation_message: null,
    confirmation_id: null,
    ...overrides,
  };
}

/**
 * Create a mock task matching TaskResponse from kanban.py
 */
export function createMockTask(overrides = {}) {
  const now = new Date().toISOString();
  return {
    task_id: `task-${Math.random().toString(36).substr(2, 9)}`,
    user_id: 'demo-user',
    title: 'Test Task',
    description: 'Test task description',
    due_at: null,
    priority: 1,
    status: 'pending',
    tags: [],
    notes: null,
    version: 0,
    column_order: 100,
    created_at: now,
    updated_at: now,
    completed_at: null,
    deleted_at: null,
    ...overrides,
  };
}

// =============================================================================
// Fetch Mock Helpers
// =============================================================================

/**
 * Create a successful fetch response
 */
export function mockFetchSuccess(data, status = 200) {
  return Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    statusText: 'OK',
    json: () => Promise.resolve(data),
    text: () => Promise.resolve(JSON.stringify(data)),
  });
}

/**
 * Create an error fetch response
 */
export function mockFetchError(message, status = 500) {
  return Promise.resolve({
    ok: false,
    status,
    statusText: message,
    json: () => Promise.resolve({ detail: message }),
    text: () => Promise.resolve(message),
  });
}

/**
 * Setup fetch mock for chat API endpoints
 */
export function setupChatApiMock() {
  const sessions = [
    createMockSession({ title: 'Today Chat', last_activity: new Date().toISOString() }),
    createMockSession({ title: 'Yesterday Chat', last_activity: new Date(Date.now() - 86400000).toISOString() }),
  ];

  const messages = [
    createMockMessage({ role: 'user', content: 'Hello, Jeeves!' }),
    createMockMessage({ role: 'assistant', content: 'Hello! How can I help you today?' }),
  ];

  return vi.fn().mockImplementation((url, options = {}) => {
    const urlStr = typeof url === 'string' ? url : url.toString();

    // GET /api/v1/chat/sessions
    if (urlStr.includes('/api/v1/chat/sessions') && !options.method) {
      return mockFetchSuccess(sessions);
    }

    // POST /api/v1/chat/sessions
    if (urlStr.includes('/api/v1/chat/sessions') && options.method === 'POST') {
      const body = JSON.parse(options.body);
      return mockFetchSuccess(createMockSession({ title: body.title }), 201);
    }

    // DELETE /api/v1/chat/sessions/{id}
    if (urlStr.match(/\/api\/v1\/chat\/sessions\/[^/]+$/) && options.method === 'DELETE') {
      return mockFetchSuccess(null, 204);
    }

    // POST /api/v1/chat/messages - must check before GET to /messages
    if (urlStr.includes('/api/v1/chat/messages') && options.method === 'POST') {
      return mockFetchSuccess(createMockChatResponse());
    }

    // GET /api/v1/chat/sessions/{id}/messages
    if (urlStr.includes('/messages') && (!options.method || options.method === 'GET')) {
      return mockFetchSuccess(messages);
    }

    // POST /api/v1/confirmations
    if (urlStr.includes('/api/v1/confirmations') && options.method === 'POST') {
      return mockFetchSuccess({ response_text: 'Action confirmed and executed.' });
    }

    return mockFetchError('Not found', 404);
  });
}

/**
 * Setup fetch mock for kanban API endpoints
 */
export function setupKanbanApiMock() {
  const tasks = [
    createMockTask({ title: 'Pending Task 1', status: 'pending', priority: 0 }),
    createMockTask({ title: 'Pending Task 2', status: 'pending', priority: 1 }),
    createMockTask({ title: 'In Progress Task', status: 'in_progress', priority: 1 }),
    createMockTask({ title: 'Completed Task', status: 'completed', priority: 2 }),
  ];

  return vi.fn().mockImplementation((url, options = {}) => {
    const urlStr = typeof url === 'string' ? url : url.toString();

    // GET /api/v1/kanban/tasks
    if (urlStr.includes('/api/v1/kanban/tasks') && !options.method) {
      return mockFetchSuccess(tasks);
    }

    // POST /api/v1/kanban/tasks
    if (urlStr.includes('/api/v1/kanban/tasks') && options.method === 'POST') {
      const body = JSON.parse(options.body);
      return mockFetchSuccess(createMockTask({ title: body.title }), 201);
    }

    // PATCH /api/v1/kanban/tasks/{id}
    if (urlStr.match(/\/api\/v1\/kanban\/tasks\/[^/]+$/) && options.method === 'PATCH') {
      const body = JSON.parse(options.body);
      const existingTask = tasks.find((t) => urlStr.includes(t.task_id));

      if (existingTask && body.version !== existingTask.version) {
        return mockFetchError('Version conflict', 409);
      }

      return mockFetchSuccess(createMockTask({ ...body, version: (body.version || 0) + 1 }));
    }

    // DELETE /api/v1/kanban/tasks/{id}
    if (urlStr.match(/\/api\/v1\/kanban\/tasks\/[^/]+$/) && options.method === 'DELETE') {
      return mockFetchSuccess(null, 204);
    }

    // POST /api/v1/kanban/tasks/rebalance
    if (urlStr.includes('/rebalance') && options.method === 'POST') {
      return mockFetchSuccess({ rebalanced: 4, status: 'pending' });
    }

    return mockFetchError('Not found', 404);
  });
}

// =============================================================================
// WebSocket Mock Helpers
// =============================================================================

/**
 * Create a mock WebSocket instance that can simulate events
 */
export function createMockWebSocket() {
  const listeners = new Map();

  const ws = {
    url: '',
    readyState: 0, // CONNECTING
    send: vi.fn(),
    close: vi.fn(),

    // Event listeners
    onopen: null,
    onclose: null,
    onerror: null,
    onmessage: null,

    addEventListener: vi.fn((event, callback) => {
      if (!listeners.has(event)) {
        listeners.set(event, []);
      }
      listeners.get(event).push(callback);
    }),

    removeEventListener: vi.fn((event, callback) => {
      if (listeners.has(event)) {
        const callbacks = listeners.get(event);
        const index = callbacks.indexOf(callback);
        if (index > -1) {
          callbacks.splice(index, 1);
        }
      }
    }),

    // Test helpers to simulate events
    simulateOpen: () => {
      ws.readyState = 1; // OPEN
      if (ws.onopen) ws.onopen({ type: 'open' });
      listeners.get('open')?.forEach((cb) => cb({ type: 'open' }));
    },

    simulateClose: () => {
      ws.readyState = 3; // CLOSED
      if (ws.onclose) ws.onclose({ type: 'close' });
      listeners.get('close')?.forEach((cb) => cb({ type: 'close' }));
    },

    simulateError: (error) => {
      if (ws.onerror) ws.onerror({ type: 'error', error });
      listeners.get('error')?.forEach((cb) => cb({ type: 'error', error }));
    },

    simulateMessage: (data) => {
      const event = { type: 'message', data: typeof data === 'string' ? data : JSON.stringify(data) };
      if (ws.onmessage) ws.onmessage(event);
      listeners.get('message')?.forEach((cb) => cb(event));
    },
  };

  return ws;
}

/**
 * Create WebSocket event payloads matching the API contract
 */
export const wsEvents = {
  orchestratorCompleted: (response_text) => ({
    event: 'orchestrator.completed',
    payload: {
      response_text,
      confirmation_needed: false,
    },
  }),

  orchestratorConfirmationRequested: (message, confirmationId) => ({
    event: 'orchestrator.confirmation_requested',
    payload: {
      confirmation_needed: true,
      confirmation_message: message,
      confirmation_id: confirmationId,
    },
  }),

  orchestratorFailed: (error) => ({
    event: 'orchestrator.failed',
    payload: { error },
  }),

  plannerGenerated: (intent, confidence) => ({
    event: 'planner.generated',
    payload: { intent, confidence },
  }),

  executorCompleted: (tool_name, status) => ({
    event: 'executor.completed',
    payload: { tool_name, status },
  }),

  taskCreated: (task) => ({
    event: 'task.created',
    payload: { task },
  }),

  taskUpdated: (task, old_status) => ({
    event: 'task.updated',
    payload: { task, old_status },
  }),

  taskDeleted: (task_id) => ({
    event: 'task.deleted',
    payload: { task_id },
  }),
};
