import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  base: '/app/',  // Base path pour le déploiement
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/audio': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
