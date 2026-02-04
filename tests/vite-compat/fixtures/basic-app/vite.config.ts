import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': '/src',
      '@components': '/src/components',
    },
  },
  define: {
    __APP_VERSION__: JSON.stringify('1.0.0'),
  },
  server: {
    port: 5173,
  },
});
