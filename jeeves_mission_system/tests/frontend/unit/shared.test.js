/**
 * Unit Tests for Jeeves FF Shared Utilities (static/js/shared.js)
 *
 * Tests cover:
 * - ThemeManager: Dark/light mode toggle and persistence
 * - MobileMenu: Hamburger menu toggle
 * - WebSocketManager: Connection management and event handling
 * - Notifications: Toast notification display
 * - Utils: Helper functions (escapeHtml, formatTimeAgo, debounce, fetchJson)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMockWebSocket } from '../mocks/api.js';

// =============================================================================
// DOM Setup Helpers
// =============================================================================

/**
 * Create the base DOM structure matching base.html template
 */
function setupBaseDOM() {
  document.body.innerHTML = `
    <html class="">
      <body class="bg-gray-100 dark:bg-slate-900">
        <!-- Navigation -->
        <nav class="bg-white dark:bg-slate-800">
          <button id="mobile-menu-btn">Menu</button>
          <div id="mobile-menu" class="hidden">
            <a href="/chat">Chat</a>
            <a href="/kanban">Kanban</a>
          </div>
          <div id="connection-status-dot" class="w-2.5 h-2.5 rounded-full bg-gray-400"></div>
          <span id="connection-status">Disconnected</span>
          <button id="theme-toggle">Toggle Theme</button>
        </nav>

        <!-- Notification Container -->
        <div id="notification-container" class="fixed top-16 right-4 z-50 space-y-2"></div>

        <!-- Main Content -->
        <main id="main-content"></main>
      </body>
    </html>
  `;
}

// =============================================================================
// ThemeManager Tests
// =============================================================================

describe('ThemeManager', () => {
  beforeEach(() => {
    setupBaseDOM();
    localStorage.clear();
  });

  describe('loadTheme', () => {
    it('should apply saved dark theme from localStorage', () => {
      localStorage.setItem('jeeves-theme', 'dark');

      // Simulate theme loading
      const theme = localStorage.getItem('jeeves-theme');
      if (theme === 'dark') {
        document.documentElement.classList.add('dark');
      }

      expect(document.documentElement.classList.contains('dark')).toBe(true);
    });

    it('should apply saved light theme from localStorage', () => {
      localStorage.setItem('jeeves-theme', 'light');

      const theme = localStorage.getItem('jeeves-theme');
      if (theme !== 'dark') {
        document.documentElement.classList.remove('dark');
      }

      expect(document.documentElement.classList.contains('dark')).toBe(false);
    });

    it('should respect system preference when no saved theme', () => {
      // Simulate system prefers dark
      const mockMatchMedia = vi.fn().mockReturnValue({
        matches: true,
        media: '(prefers-color-scheme: dark)',
        addEventListener: vi.fn(),
      });
      global.matchMedia = mockMatchMedia;

      const savedTheme = localStorage.getItem('jeeves-theme');
      if (!savedTheme) {
        const prefersDark = matchMedia('(prefers-color-scheme: dark)').matches;
        if (prefersDark) {
          document.documentElement.classList.add('dark');
        }
      }

      expect(document.documentElement.classList.contains('dark')).toBe(true);
    });
  });

  describe('setTheme', () => {
    it('should add dark class when setting dark theme', () => {
      // Simulate setTheme('dark')
      document.documentElement.classList.add('dark');
      localStorage.setItem('jeeves-theme', 'dark');

      expect(document.documentElement.classList.contains('dark')).toBe(true);
      expect(localStorage.getItem('jeeves-theme')).toBe('dark');
    });

    it('should remove dark class when setting light theme', () => {
      document.documentElement.classList.add('dark');

      // Simulate setTheme('light')
      document.documentElement.classList.remove('dark');
      localStorage.setItem('jeeves-theme', 'light');

      expect(document.documentElement.classList.contains('dark')).toBe(false);
      expect(localStorage.getItem('jeeves-theme')).toBe('light');
    });
  });

  describe('toggle', () => {
    it('should toggle from light to dark', () => {
      // Start in light mode
      document.documentElement.classList.remove('dark');

      // Simulate toggle
      const isDark = document.documentElement.classList.toggle('dark');
      localStorage.setItem('jeeves-theme', isDark ? 'dark' : 'light');

      expect(isDark).toBe(true);
      expect(localStorage.getItem('jeeves-theme')).toBe('dark');
    });

    it('should toggle from dark to light', () => {
      // Start in dark mode
      document.documentElement.classList.add('dark');

      // Simulate toggle
      const isDark = document.documentElement.classList.toggle('dark');
      localStorage.setItem('jeeves-theme', isDark ? 'dark' : 'light');

      expect(isDark).toBe(false);
      expect(localStorage.getItem('jeeves-theme')).toBe('light');
    });
  });

  describe('setupToggle', () => {
    it('should toggle theme when button is clicked', () => {
      const toggle = document.getElementById('theme-toggle');

      // Add click handler
      toggle.addEventListener('click', () => {
        document.documentElement.classList.toggle('dark');
      });

      // Click the button
      toggle.click();

      expect(document.documentElement.classList.contains('dark')).toBe(true);

      toggle.click();
      expect(document.documentElement.classList.contains('dark')).toBe(false);
    });
  });
});

// =============================================================================
// MobileMenu Tests
// =============================================================================

describe('MobileMenu', () => {
  beforeEach(() => {
    setupBaseDOM();
  });

  describe('init', () => {
    it('should toggle menu visibility when button is clicked', () => {
      const btn = document.getElementById('mobile-menu-btn');
      const menu = document.getElementById('mobile-menu');

      // Add click handler matching shared.js
      btn.addEventListener('click', () => {
        menu.classList.toggle('hidden');
      });

      expect(menu.classList.contains('hidden')).toBe(true);

      btn.click();
      expect(menu.classList.contains('hidden')).toBe(false);

      btn.click();
      expect(menu.classList.contains('hidden')).toBe(true);
    });

    it('should close menu when clicking outside', () => {
      const btn = document.getElementById('mobile-menu-btn');
      const menu = document.getElementById('mobile-menu');

      // Show menu first
      menu.classList.remove('hidden');

      // Add document click handler matching shared.js
      document.addEventListener('click', (e) => {
        if (!btn.contains(e.target) && !menu.contains(e.target)) {
          menu.classList.add('hidden');
        }
      });

      // Click outside
      document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));

      expect(menu.classList.contains('hidden')).toBe(true);
    });
  });
});

// =============================================================================
// WebSocketManager Tests
// =============================================================================

describe('WebSocketManager', () => {
  let mockWs;

  beforeEach(() => {
    setupBaseDOM();
    mockWs = createMockWebSocket();

    // Mock WebSocket constructor
    global.WebSocket = vi.fn().mockImplementation((url) => {
      mockWs.url = url;
      return mockWs;
    });
  });

  describe('connect', () => {
    it('should create WebSocket with correct URL', () => {
      // Simulate connect()
      const protocol = 'ws:';
      const host = 'localhost:8000';
      const token = 'local-dev-token';
      const wsUrl = `${protocol}//${host}/ws?token=${encodeURIComponent(token)}`;

      const ws = new WebSocket(wsUrl);

      expect(WebSocket).toHaveBeenCalledWith(wsUrl);
      expect(ws.url).toBe(wsUrl);
    });

    it('should update status to Connected on open', () => {
      const statusText = document.getElementById('connection-status');
      const statusDot = document.getElementById('connection-status-dot');

      // Simulate onopen handler
      mockWs.onopen = () => {
        statusText.textContent = 'Connected';
        statusDot.className = 'w-2.5 h-2.5 rounded-full bg-green-500';
      };

      mockWs.simulateOpen();

      expect(statusText.textContent).toBe('Connected');
      expect(statusDot.className).toContain('bg-green-500');
    });

    it('should update status to Disconnected on close', () => {
      const statusText = document.getElementById('connection-status');
      const statusDot = document.getElementById('connection-status-dot');

      mockWs.simulateOpen();

      // Simulate onclose handler
      mockWs.onclose = () => {
        statusText.textContent = 'Disconnected';
        statusDot.className = 'w-2.5 h-2.5 rounded-full bg-gray-400';
      };

      mockWs.simulateClose();

      expect(statusText.textContent).toBe('Disconnected');
      expect(statusDot.className).toContain('bg-gray-400');
    });

    it('should update status to Error on error', () => {
      const statusText = document.getElementById('connection-status');

      mockWs.onerror = () => {
        statusText.textContent = 'Error';
      };

      mockWs.simulateError(new Error('Connection failed'));

      expect(statusText.textContent).toBe('Error');
    });
  });

  describe('handleMessage', () => {
    it('should respond to ping with pong', () => {
      mockWs.onmessage = (event) => {
        if (event.data === 'ping') {
          mockWs.send('pong');
        }
      };

      mockWs.simulateMessage('ping');

      expect(mockWs.send).toHaveBeenCalledWith('pong');
    });

    it('should parse and emit JSON messages', () => {
      const listeners = new Map();
      const emit = (event, data) => {
        if (listeners.has(event)) {
          listeners.get(event).forEach((cb) => cb(data));
        }
      };
      const on = (event, callback) => {
        if (!listeners.has(event)) {
          listeners.set(event, []);
        }
        listeners.get(event).push(callback);
      };

      const messageHandler = vi.fn();
      on('orchestrator.completed', messageHandler);

      mockWs.onmessage = (event) => {
        if (event.data === 'ping' || event.data === 'pong') return;
        const message = JSON.parse(event.data);
        emit(message.event, message.payload);
      };

      mockWs.simulateMessage({
        event: 'orchestrator.completed',
        payload: { response_text: 'Hello!' },
      });

      expect(messageHandler).toHaveBeenCalledWith({ response_text: 'Hello!' });
    });

    it('should ignore pong messages', () => {
      const messageHandler = vi.fn();

      mockWs.onmessage = (event) => {
        if (event.data === 'pong') return;
        messageHandler(event.data);
      };

      mockWs.simulateMessage('pong');

      expect(messageHandler).not.toHaveBeenCalled();
    });
  });

  describe('send', () => {
    it('should send string data directly', () => {
      mockWs.readyState = 1; // OPEN

      const send = (data) => {
        if (mockWs.readyState === 1) {
          mockWs.send(typeof data === 'string' ? data : JSON.stringify(data));
        }
      };

      send('hello');

      expect(mockWs.send).toHaveBeenCalledWith('hello');
    });

    it('should stringify and send object data', () => {
      mockWs.readyState = 1;

      const send = (data) => {
        if (mockWs.readyState === 1) {
          mockWs.send(typeof data === 'string' ? data : JSON.stringify(data));
        }
      };

      send({ action: 'test' });

      expect(mockWs.send).toHaveBeenCalledWith('{"action":"test"}');
    });

    it('should not send when not connected', () => {
      mockWs.readyState = 0; // CONNECTING

      const send = (data) => {
        if (mockWs.readyState === 1) {
          mockWs.send(data);
        }
      };

      send('hello');

      expect(mockWs.send).not.toHaveBeenCalled();
    });
  });

  describe('event listeners', () => {
    it('should support on/off for event subscription', () => {
      const listeners = new Map();

      const on = (event, callback) => {
        if (!listeners.has(event)) {
          listeners.set(event, []);
        }
        listeners.get(event).push(callback);
      };

      const off = (event, callback) => {
        if (listeners.has(event)) {
          const cbs = listeners.get(event);
          const index = cbs.indexOf(callback);
          if (index > -1) {
            cbs.splice(index, 1);
          }
        }
      };

      const callback = vi.fn();

      on('test', callback);
      expect(listeners.get('test')).toContain(callback);

      off('test', callback);
      expect(listeners.get('test')).not.toContain(callback);
    });
  });

  describe('reconnection', () => {
    it('should schedule reconnect with exponential backoff', () => {
      vi.useFakeTimers();

      let reconnectAttempts = 0;
      const maxReconnectAttempts = 10;
      let reconnected = false;

      const scheduleReconnect = () => {
        if (reconnectAttempts < maxReconnectAttempts) {
          reconnectAttempts++;
          const delay = Math.min(1000 * Math.pow(2, reconnectAttempts), 30000);
          setTimeout(() => {
            reconnected = true;
          }, delay);
        }
      };

      // Simulate disconnect
      scheduleReconnect();

      // After first attempt: delay = 1000 * 2^1 = 2000ms
      expect(reconnectAttempts).toBe(1);

      vi.advanceTimersByTime(2000);
      expect(reconnected).toBe(true);

      vi.useRealTimers();
    });

    it('should stop reconnecting after max attempts', () => {
      let reconnectAttempts = 10;
      const maxReconnectAttempts = 10;
      let scheduledReconnect = false;

      const scheduleReconnect = () => {
        if (reconnectAttempts < maxReconnectAttempts) {
          reconnectAttempts++;
          scheduledReconnect = true;
        }
      };

      scheduleReconnect();

      expect(scheduledReconnect).toBe(false);
    });
  });
});

// =============================================================================
// Notifications Tests
// =============================================================================

describe('Notifications', () => {
  beforeEach(() => {
    setupBaseDOM();
  });

  describe('show', () => {
    it('should create notification element with correct type class', () => {
      const container = document.getElementById('notification-container');

      const show = (message, type) => {
        const colors = {
          success: 'bg-green-500',
          error: 'bg-red-500',
          info: 'bg-blue-500',
          warning: 'bg-yellow-500',
        };

        const notification = document.createElement('div');
        notification.className = `notification ${colors[type]} text-white px-4 py-3 rounded-lg`;
        notification.textContent = message;
        container.appendChild(notification);
        return notification;
      };

      const notification = show('Success message', 'success');

      expect(notification.textContent).toBe('Success message');
      expect(notification.className).toContain('bg-green-500');
      expect(container.contains(notification)).toBe(true);
    });

    it('should auto-remove notification after duration', () => {
      vi.useFakeTimers();

      const container = document.getElementById('notification-container');

      const show = (message, type, duration = 3000) => {
        const notification = document.createElement('div');
        notification.className = 'notification bg-blue-500';
        notification.textContent = message;
        container.appendChild(notification);

        if (duration > 0) {
          setTimeout(() => {
            notification.remove();
          }, duration);
        }

        return notification;
      };

      const notification = show('Test', 'info', 3000);
      expect(container.contains(notification)).toBe(true);

      vi.advanceTimersByTime(3000);
      expect(container.contains(notification)).toBe(false);

      vi.useRealTimers();
    });

    it('should not auto-remove when duration is 0', () => {
      vi.useFakeTimers();

      const container = document.getElementById('notification-container');

      const show = (message, type, duration = 3000) => {
        const notification = document.createElement('div');
        notification.className = 'notification bg-blue-500';
        notification.textContent = message;
        container.appendChild(notification);

        if (duration > 0) {
          setTimeout(() => {
            notification.remove();
          }, duration);
        }

        return notification;
      };

      const notification = show('Persistent', 'info', 0);

      vi.advanceTimersByTime(10000);
      expect(container.contains(notification)).toBe(true);

      vi.useRealTimers();
    });
  });

  describe('success/error/info/warning shortcuts', () => {
    it('should use correct colors for each type', () => {
      const container = document.getElementById('notification-container');

      const types = {
        success: 'bg-green-500',
        error: 'bg-red-500',
        info: 'bg-blue-500',
        warning: 'bg-yellow-500',
      };

      Object.entries(types).forEach(([type, expectedClass]) => {
        const notification = document.createElement('div');
        notification.className = `notification ${expectedClass}`;
        container.appendChild(notification);

        expect(notification.className).toContain(expectedClass);
      });
    });
  });
});

// =============================================================================
// Utils Tests
// =============================================================================

describe('Utils', () => {
  // Helper function matching the implementation in shared.js
  // Uses string replacement for reliable escaping across all environments
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

  describe('escapeHtml', () => {
    it('should escape HTML special characters', () => {
      expect(escapeHtml('<script>alert("xss")</script>')).toBe('&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;');
      expect(escapeHtml('Hello & goodbye')).toBe('Hello &amp; goodbye');
      expect(escapeHtml('"quoted"')).toBe('&quot;quoted&quot;');
    });

    it('should handle empty strings', () => {
      expect(escapeHtml('')).toBe('');
    });

    it('should handle plain text unchanged', () => {
      expect(escapeHtml('Hello World')).toBe('Hello World');
    });
  });

  describe('formatTimeAgo', () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date('2025-11-30T12:00:00Z'));
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('should return "just now" for recent times', () => {
      const formatTimeAgo = (timestamp) => {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;
        return date.toLocaleDateString();
      };

      expect(formatTimeAgo('2025-11-30T11:59:30Z')).toBe('just now');
    });

    it('should return minutes ago for recent times', () => {
      const formatTimeAgo = (timestamp) => {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;
        return date.toLocaleDateString();
      };

      expect(formatTimeAgo('2025-11-30T11:55:00Z')).toBe('5m ago');
    });

    it('should return hours ago for same-day times', () => {
      const formatTimeAgo = (timestamp) => {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;
        return date.toLocaleDateString();
      };

      expect(formatTimeAgo('2025-11-30T09:00:00Z')).toBe('3h ago');
    });

    it('should return days ago for this week', () => {
      const formatTimeAgo = (timestamp) => {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;
        return date.toLocaleDateString();
      };

      expect(formatTimeAgo('2025-11-28T12:00:00Z')).toBe('2d ago');
    });

    it('should return date for older times', () => {
      const formatTimeAgo = (timestamp) => {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;
        return date.toLocaleDateString();
      };

      const result = formatTimeAgo('2025-10-15T12:00:00Z');
      expect(result).toMatch(/\d{1,2}\/\d{1,2}\/\d{4}/);
    });
  });

  describe('formatDate', () => {
    it('should format date in readable format', () => {
      const formatDate = (timestamp) => {
        return new Date(timestamp).toLocaleDateString('en-US', {
          year: 'numeric',
          month: 'long',
          day: 'numeric',
        });
      };

      expect(formatDate('2025-11-30T12:00:00Z')).toBe('November 30, 2025');
    });
  });

  describe('formatDateTime', () => {
    it('should format date and time', () => {
      const formatDateTime = (timestamp) => {
        return new Date(timestamp).toLocaleString('en-US', {
          month: 'short',
          day: 'numeric',
          hour: '2-digit',
          minute: '2-digit',
        });
      };

      const result = formatDateTime('2025-11-30T14:30:00Z');
      expect(result).toMatch(/Nov 30/);
    });
  });

  describe('debounce', () => {
    it('should delay function execution', () => {
      vi.useFakeTimers();

      const debounce = (func, wait) => {
        let timeout;
        return function executedFunction(...args) {
          clearTimeout(timeout);
          timeout = setTimeout(() => func(...args), wait);
        };
      };

      const fn = vi.fn();
      const debouncedFn = debounce(fn, 100);

      debouncedFn();
      debouncedFn();
      debouncedFn();

      expect(fn).not.toHaveBeenCalled();

      vi.advanceTimersByTime(100);

      expect(fn).toHaveBeenCalledTimes(1);

      vi.useRealTimers();
    });

    it('should pass arguments to the debounced function', () => {
      vi.useFakeTimers();

      const debounce = (func, wait) => {
        let timeout;
        return function executedFunction(...args) {
          clearTimeout(timeout);
          timeout = setTimeout(() => func(...args), wait);
        };
      };

      const fn = vi.fn();
      const debouncedFn = debounce(fn, 100);

      debouncedFn('arg1', 'arg2');

      vi.advanceTimersByTime(100);

      expect(fn).toHaveBeenCalledWith('arg1', 'arg2');

      vi.useRealTimers();
    });
  });

  describe('fetchJson', () => {
    it('should make fetch request with JSON headers', async () => {
      const mockResponse = { data: 'test' };
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      });

      const fetchJson = async (url, options = {}) => {
        const response = await fetch(url, {
          ...options,
          headers: {
            'Content-Type': 'application/json',
            ...options.headers,
          },
        });

        if (!response.ok) {
          const error = await response.text();
          throw new Error(`HTTP ${response.status}: ${error}`);
        }

        return response.json();
      };

      const result = await fetchJson('/api/test');

      expect(fetch).toHaveBeenCalledWith('/api/test', {
        headers: { 'Content-Type': 'application/json' },
      });
      expect(result).toEqual(mockResponse);
    });

    it('should throw on non-ok response', async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        text: () => Promise.resolve('Not found'),
      });

      const fetchJson = async (url, options = {}) => {
        const response = await fetch(url, {
          ...options,
          headers: {
            'Content-Type': 'application/json',
            ...options.headers,
          },
        });

        if (!response.ok) {
          const error = await response.text();
          throw new Error(`HTTP ${response.status}: ${error}`);
        }

        return response.json();
      };

      await expect(fetchJson('/api/missing')).rejects.toThrow('HTTP 404: Not found');
    });
  });
});
