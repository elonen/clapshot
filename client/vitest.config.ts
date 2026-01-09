/// <reference types="vitest" />
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import sveltePreprocess from 'svelte-preprocess'
import * as path from 'path'

export default defineConfig({
  plugins: [
    svelte({
      preprocess: [sveltePreprocess({ typescript: true })],
      hot: !process.env.VITEST,
      configFile: false,
      ...(process.env.VITEST ? {
        compilerOptions: {
          hydratable: true,
          compatibility: {
            componentApi: 4,
          },
        },
      } : {}),
    }),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@clapshot_protobuf': path.resolve(__dirname, '../protobuf/libs')
    },
    conditions: process.env.VITEST ? ['browser'] : [],
  },
  test: {
    environment: 'happy-dom',
    setupFiles: ['./src/__tests__/setup.ts'],
    globals: true,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'src/__tests__/',
        'dist/',
        '**/*.d.ts',
        '**/*.config.*',
        '**/protobuf/**', // Exclude generated protobuf code
      ],
      thresholds: {
        global: {
          branches: 70,
          functions: 70,
          lines: 70,
          statements: 70,
        },
      },
    },
    include: ['src/**/*.{test,spec}.{js,ts}'],
    exclude: [
      'node_modules', 
      'dist', 
      '.svelte-kit',
      'src/__tests__/setup.ts',
      'src/__tests__/mocks/**',
      'src/__tests__/README.md'
    ],
  },
  define: {
    'process.env.NODE_ENV': '"test"',
    'process.env.CLAPSHOT_MIN_SERVER_VERSION': '"0.9.0"',
    'process.env.CLAPSHOT_MAX_SERVER_VERSION': '"0.9.2"',
    'process.env.CLAPSHOT_CLIENT_VERSION': '"0.9.2"',
    'import.meta.env.SSR': false,
  },
})
