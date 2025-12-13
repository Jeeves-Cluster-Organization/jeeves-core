/**
 * Jeeves FF Frontend Configuration
 *
 * Centralized configuration for all frontend modules.
 * This module provides:
 * - API endpoint configuration
 * - WebSocket configuration
 * - UI constants
 * - Feature flags
 *
 * Import this in other modules:
 *   import { Config } from './config.js';
 */

// =============================================================================
// API Configuration
// =============================================================================

export const API = {
  // Base URL - auto-detect from window.location
  get baseUrl() {
    return window.location.origin;
  },

  // API version prefix
  version: 'v1',

  // Full API base path
  get basePath() {
    return `${this.baseUrl}/api/${this.version}`;
  },

  // Endpoint paths (relative to basePath)
  endpoints: {
    // Chat endpoints
    chat: {
      sessions: '/chat/sessions',
      messages: '/chat/messages',
      search: '/chat/search',
    },

    // Kanban endpoints
    kanban: {
      tasks: '/kanban/tasks',
      rebalance: '/kanban/tasks/rebalance',
    },

    // Journal endpoints
    journal: {
      entries: '/journal/entries',
    },

    // Governance endpoints
    governance: {
      dashboard: '/governance/dashboard',
      health: '/governance/health',
    },

    // Confirmation endpoints
    confirmations: '/confirmations',
  },

  // Request defaults
  defaults: {
    headers: {
      'Content-Type': 'application/json',
    },
    timeout: 30000, // 30 seconds
  },
};

// =============================================================================
// WebSocket Configuration
// =============================================================================

export const WebSocketConfig = {
  // Auto-detect WebSocket URL
  get url() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${protocol}//${window.location.host}/ws`;
  },

  // Reconnection settings
  reconnect: {
    enabled: true,
    maxAttempts: 10,
    baseDelay: 1000, // 1 second
    maxDelay: 30000, // 30 seconds
    backoffMultiplier: 2,
  },

  // Heartbeat settings
  heartbeat: {
    enabled: true,
    interval: 30000, // 30 seconds
    timeout: 10000, // 10 seconds
  },

  // Event types
  events: {
    // Orchestrator events
    orchestrator: {
      completed: 'orchestrator.completed',
      failed: 'orchestrator.failed',
      streaming: 'orchestrator.streaming',
      confirmationRequested: 'orchestrator.confirmation_requested',
    },

    // Agent events
    planner: {
      generated: 'planner.generated',
    },
    executor: {
      started: 'executor.started',
      completed: 'executor.completed',
    },

    // Task events
    task: {
      created: 'task.created',
      updated: 'task.updated',
      deleted: 'task.deleted',
      rebalanced: 'task.rebalanced',
    },

    // Session events
    session: {
      created: 'chat.session.created',
      updated: 'chat.session.updated',
      deleted: 'chat.session.deleted',
    },
  },
};

// =============================================================================
// UI Configuration
// =============================================================================

export const UI = {
  // Theme settings
  theme: {
    storageKey: 'jeeves-theme',
    default: 'system', // 'light', 'dark', or 'system'
    darkClass: 'dark',
  },

  // Notification settings
  notifications: {
    duration: 4000, // 4 seconds
    maxVisible: 5,
    position: 'bottom-right',
    types: {
      success: {
        bg: 'bg-green-500',
        icon: 'check-circle',
      },
      error: {
        bg: 'bg-red-500',
        icon: 'x-circle',
      },
      warning: {
        bg: 'bg-yellow-500',
        icon: 'exclamation-triangle',
      },
      info: {
        bg: 'bg-blue-500',
        icon: 'information-circle',
      },
    },
  },

  // Chat settings
  chat: {
    maxMessageLength: 10000,
    typingIndicatorDelay: 300, // ms
    maxInternalActivityItems: 50,
    sessionTimeGroups: ['Today', 'Yesterday', 'This Week', 'Older'],
  },

  // Kanban settings
  kanban: {
    statuses: ['pending', 'in_progress', 'completed', 'cancelled'],
    statusLabels: {
      pending: 'Pending',
      in_progress: 'In Progress',
      completed: 'Completed',
      cancelled: 'Cancelled',
    },
    statusColors: {
      pending: 'gray',
      in_progress: 'blue',
      completed: 'green',
      cancelled: 'gray',
    },
    priorities: {
      high: { value: 0, label: 'High', icon: '🔥', class: 'bg-red-500' },
      medium: { value: 1, label: 'Medium', icon: '⚡', class: 'bg-yellow-400' },
      low: { value: 2, label: 'Low', icon: '✓', class: 'bg-green-400' },
    },
    columnOrderGap: 100,
  },

  // Breakpoints
  breakpoints: {
    mobile: 768,
    tablet: 1024,
    desktop: 1280,
  },

  // Animation durations (ms)
  animations: {
    fast: 150,
    normal: 300,
    slow: 500,
  },
};

// =============================================================================
// Feature Flags
// =============================================================================

export const Features = {
  // Enable/disable features
  internalViewPanel: true, // Show agent activity in chat
  confirmationFlow: true, // Enable confirmation for destructive actions
  realtimeUpdates: true, // WebSocket real-time updates
  darkMode: true, // Dark mode support
  sessionSearch: true, // Full-text search in chat
  exportSession: true, // Export chat sessions
  mobileStatusMenu: true, // Touch-friendly status change on mobile
  keyboardShortcuts: true, // Keyboard shortcuts
};

// =============================================================================
// User Preferences
// =============================================================================

export const Preferences = {
  // Storage keys
  keys: {
    userId: 'jeeves-user-id',
    theme: 'jeeves-theme',
    sidebarOpen: 'jeeves-sidebar-open',
    internalViewOpen: 'jeeves-internal-view-open',
  },

  // Get preference from localStorage
  get(key, defaultValue = null) {
    try {
      const value = localStorage.getItem(this.keys[key] || key);
      return value !== null ? JSON.parse(value) : defaultValue;
    } catch {
      return defaultValue;
    }
  },

  // Set preference in localStorage
  set(key, value) {
    try {
      localStorage.setItem(this.keys[key] || key, JSON.stringify(value));
    } catch (error) {
      console.warn('Failed to save preference:', error);
    }
  },

  // Remove preference
  remove(key) {
    try {
      localStorage.removeItem(this.keys[key] || key);
    } catch (error) {
      console.warn('Failed to remove preference:', error);
    }
  },
};

// =============================================================================
// Default Export
// =============================================================================

export const Config = {
  API,
  WebSocketConfig,
  UI,
  Features,
  Preferences,
};

export default Config;
