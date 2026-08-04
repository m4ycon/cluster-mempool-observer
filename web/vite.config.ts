/// <reference types="vitest/config" />

import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig, loadEnv } from 'vite';

const ENV_DIR = '..';

export default defineConfig(({ mode }) => {
  const { WEB_PORT } = loadEnv(mode, ENV_DIR, '');

  return {
    envDir: ENV_DIR,
    plugins: [react(), tailwindcss()],
    server: {
      port: Number(WEB_PORT) || 3000,
    },
    test: {
      environment: 'jsdom',
      globals: true,
      setupFiles: ['./vitest.setup.ts'],
      passWithNoTests: true,
    },
  };
});
