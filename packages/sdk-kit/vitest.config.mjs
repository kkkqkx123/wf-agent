import { defineConfig } from 'vitest/config';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    globals: true,
    environment: 'node',
    include: ['__tests__/**/*.{test,spec}.ts'],
    exclude: ['node_modules', 'dist', 'src'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        '__tests__/',
        'dist/',
        '**/*.spec.ts',
        '**/*.test.ts',
        '**/index.ts'
      ],
      lines: 85,
      functions: 85,
      branches: 85,
      statements: 85
    },
    testTimeout: 30000,
    clearMocks: true,
    restoreMocks: true
  }
});


