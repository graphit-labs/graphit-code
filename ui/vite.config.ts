/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { viteSingleFile } from 'vite-plugin-singlefile'
import path from 'path'

export default defineConfig({
  base: '/',
  plugins: [react(), viteSingleFile({ useRecommendedBuildConfig: false })],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    // Each fork owns a DOM. Keep the default pool small enough for component
    // interactions to finish reliably alongside native builds.
    minWorkers: 1,
    maxWorkers: 2,
  },
  build: {
    outDir: 'dist',

    assetsInlineLimit: 100_000_000,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8100',
        changeOrigin: true,
        timeout: 0,
      },
    },
  },
})
