import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 8090,
    proxy: {
      '/build': {
        target: process.env.VITE_BUILD_SERVICE_URL ?? 'http://localhost:8091',
        changeOrigin: true,
      },
      '/api': {
        target: process.env.VITE_BUILD_SERVICE_URL ?? 'http://localhost:8091',
        changeOrigin: true,
      },
    },
  },
});
