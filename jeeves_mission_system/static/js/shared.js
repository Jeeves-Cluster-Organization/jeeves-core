/**
 * Jeeves FF - Shared JavaScript
 * Common functionality across all views (Chat, Kanban, Journal, Governance)
 */

// =========================================================================
// Theme Management
// =========================================================================

const ThemeManager = {
    init() {
        this.loadTheme();
        this.setupToggle();
    },

    loadTheme() {
        const theme = localStorage.getItem('jeeves-theme') ||
                      (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
        this.setTheme(theme);
    },

    setTheme(theme) {
        if (theme === 'dark') {
            document.documentElement.classList.add('dark');
        } else {
            document.documentElement.classList.remove('dark');
        }
        localStorage.setItem('jeeves-theme', theme);
    },

    toggle() {
        const isDark = document.documentElement.classList.toggle('dark');
        localStorage.setItem('jeeves-theme', isDark ? 'dark' : 'light');
        return isDark;
    },

    setupToggle() {
        const toggle = document.getElementById('theme-toggle');
        if (toggle) {
            toggle.addEventListener('click', () => this.toggle());
        }
    }
};

// =========================================================================
// Mobile Menu
// =========================================================================

const MobileMenu = {
    init() {
        const btn = document.getElementById('mobile-menu-btn');
        const menu = document.getElementById('mobile-menu');

        if (btn && menu) {
            btn.addEventListener('click', () => {
                menu.classList.toggle('hidden');
            });

            // Close menu when clicking outside
            document.addEventListener('click', (e) => {
                if (!btn.contains(e.target) && !menu.contains(e.target)) {
                    menu.classList.add('hidden');
                }
            });
        }
    }
};

// =========================================================================
// WebSocket Connection Manager (shared across views)
// =========================================================================

class WebSocketManager {
    constructor() {
        this.ws = null;
        this.isConnected = false;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 10;
        this.listeners = new Map();
        this.token = 'local-dev-token';
    }

    connect() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws?token=${encodeURIComponent(this.token)}`;

        try {
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('[WS] Connected');
                this.isConnected = true;
                this.reconnectAttempts = 0;
                this.updateStatus('Connected', 'connected');
                this.emit('connect');
            };

            this.ws.onmessage = (event) => {
                if (event.data === 'ping') {
                    this.ws.send('pong');
                    return;
                }
                if (event.data === 'pong') return;

                try {
                    const message = JSON.parse(event.data);
                    this.handleMessage(message);
                } catch (error) {
                    console.error('[WS] Failed to parse message:', error);
                }
            };

            this.ws.onerror = (error) => {
                console.error('[WS] Error:', error);
                this.updateStatus('Error', 'error');
            };

            this.ws.onclose = () => {
                console.log('[WS] Disconnected');
                this.isConnected = false;
                this.updateStatus('Disconnected', 'disconnected');
                this.emit('disconnect');
                this.scheduleReconnect();
            };
        } catch (error) {
            console.error('[WS] Failed to connect:', error);
            this.updateStatus('Error', 'error');
        }
    }

    handleMessage(message) {
        // UnifiedEvent format (constitutional standard)
        if (message.type === 'event') {
            const event_type = message.event_type;
            const payload = message.payload || {};

            // Emit with event_type as event name
            this.emit(event_type, payload);

            // Also emit the full UnifiedEvent
            this.emit('message', message);
        } else {
            console.warn('[WS] Received non-UnifiedEvent message:', message);
        }
    }

    scheduleReconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
            console.log(`[WS] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
            setTimeout(() => this.connect(), delay);
        }
    }

    updateStatus(text, status) {
        const statusText = document.getElementById('connection-status');
        const statusDot = document.getElementById('connection-status-dot');

        if (statusText) statusText.textContent = text;
        if (statusDot) {
            statusDot.className = 'w-2.5 h-2.5 rounded-full';
            const colors = {
                connected: 'bg-green-500',
                error: 'bg-red-500',
                disconnected: 'bg-gray-400'
            };
            statusDot.classList.add(colors[status] || 'bg-gray-400');
        }
    }

    send(data) {
        if (this.ws && this.isConnected) {
            this.ws.send(typeof data === 'string' ? data : JSON.stringify(data));
        }
    }

    on(event, callback) {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event).push(callback);
    }

    off(event, callback) {
        if (this.listeners.has(event)) {
            const listeners = this.listeners.get(event);
            const index = listeners.indexOf(callback);
            if (index > -1) {
                listeners.splice(index, 1);
            }
        }
    }

    emit(event, data) {
        if (this.listeners.has(event)) {
            this.listeners.get(event).forEach(callback => callback(data));
        }
    }
}

// Global WebSocket instance
const wsManager = new WebSocketManager();

// =========================================================================
// Notification System
// =========================================================================

const Notifications = {
    container: null,

    init() {
        this.container = document.getElementById('notification-container');
    },

    show(message, type = 'info', duration = 3000) {
        if (!this.container) return;

        const colors = {
            success: 'bg-green-500',
            error: 'bg-red-500',
            info: 'bg-blue-500',
            warning: 'bg-yellow-500'
        };

        const notification = document.createElement('div');
        notification.className = `notification ${colors[type]} text-white px-4 py-3 rounded-lg shadow-lg flex items-center gap-2 text-sm`;
        notification.textContent = message;

        this.container.appendChild(notification);

        if (duration > 0) {
            setTimeout(() => {
                notification.classList.add('hide');
                setTimeout(() => notification.remove(), 300);
            }, duration);
        }

        return notification;
    },

    success(message, duration) {
        return this.show(message, 'success', duration);
    },

    error(message, duration = 5000) {
        return this.show(message, 'error', duration);
    },

    info(message, duration) {
        return this.show(message, 'info', duration);
    },

    warning(message, duration = 4000) {
        return this.show(message, 'warning', duration);
    }
};

// =========================================================================
// Utility Functions
// =========================================================================

const Utils = {
    escapeHtml(text) {
        // Use string replacement for reliable escaping across all environments
        // (including JSDOM which doesn't properly escape via DOM manipulation)
        const escapeMap = {
            '&': '&amp;',
            '<': '&lt;',
            '>': '&gt;',
            '"': '&quot;',
            "'": '&#039;'
        };
        return String(text).replace(/[&<>"']/g, char => escapeMap[char]);
    },

    formatTimeAgo(timestamp) {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`;

        return date.toLocaleDateString();
    },

    formatDate(timestamp) {
        return new Date(timestamp).toLocaleDateString('en-US', {
            year: 'numeric',
            month: 'long',
            day: 'numeric'
        });
    },

    formatDateTime(timestamp) {
        return new Date(timestamp).toLocaleString('en-US', {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        });
    },

    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    },

    async fetchJson(url, options = {}) {
        const response = await fetch(url, {
            ...options,
            headers: {
                'Content-Type': 'application/json',
                ...options.headers
            }
        });

        if (!response.ok) {
            const error = await response.text();
            throw new Error(`HTTP ${response.status}: ${error}`);
        }

        return response.json();
    }
};

// =========================================================================
// Initialize on DOM Ready
// =========================================================================

document.addEventListener('DOMContentLoaded', () => {
    ThemeManager.init();
    MobileMenu.init();
    Notifications.init();

    // Connect WebSocket if not on a static page
    if (document.body.dataset.wsConnect !== 'false') {
        wsManager.connect();
    }
});

// Export for use in other scripts
window.Jeeves = {
    theme: ThemeManager,
    ws: wsManager,
    notify: Notifications,
    utils: Utils
};
