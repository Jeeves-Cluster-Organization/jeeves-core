import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Use happy-dom for DOM simulation
    environment: 'happy-dom',

    // Test file patterns
    include: ['**/*.{test,spec}.{js,mjs,ts}'],

    // Setup files run before each test file
    setupFiles: ['./setup.js'],

    // Global test timeout
    testTimeout: 10000,

    // Coverage configuration
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['../../static/js/**/*.js'],
      exclude: ['**/node_modules/**', '**/tests/**'],
    },

    // Globals for browser-like environment
    globals: true,
  },
});
