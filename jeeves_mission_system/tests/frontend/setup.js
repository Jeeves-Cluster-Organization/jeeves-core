/**
 * Vitest setup file for Jeeves FF frontend tests
 *
 * This file runs before each test file and sets up the testing environment.
 */

import { expect, afterEach, beforeEach, vi } from 'vitest';
import * as matchers from '@testing-library/jest-dom/matchers';

// Extend Vitest's expect with jest-dom matchers
expect.extend(matchers);

// Clean up DOM after each test
afterEach(() => {
  // Clear the document body (DOM cleanup for non-React tests)
  document.body.innerHTML = '';
  vi.clearAllMocks();

  // Clear localStorage and sessionStorage
  if (typeof localStorage !== 'undefined') {
    localStorage.clear();
  }
  if (typeof sessionStorage !== 'undefined') {
    sessionStorage.clear();
  }
});

// Setup common browser globals
beforeEach(() => {
  // Mock WebSocket
  global.WebSocket = vi.fn().mockImplementation((url) => ({
    url,
    readyState: 1, // OPEN
    send: vi.fn(),
    close: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    onopen: null,
    onclose: null,
    onerror: null,
    onmessage: null,
  }));

  // Mock fetch
  global.fetch = vi.fn();

  // Mock matchMedia
  global.matchMedia = vi.fn().mockImplementation((query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));

  // Mock ResizeObserver
  global.ResizeObserver = vi.fn().mockImplementation(() => ({
    observe: vi.fn(),
    unobserve: vi.fn(),
    disconnect: vi.fn(),
  }));

  // Mock console methods to reduce noise
  vi.spyOn(console, 'log').mockImplementation(() => {});
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});

// Export utilities for tests
export { vi, expect };
