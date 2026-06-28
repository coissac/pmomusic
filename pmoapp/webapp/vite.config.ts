import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { VitePWA } from 'vite-plugin-pwa'
import { fileURLToPath, URL } from 'node:url'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    VitePWA({
      registerType: 'autoUpdate',
      base: '/app/',
      manifest: {
        name: 'PMOMusic',
        short_name: 'PMOMusic',
        description: 'Contrôleur UPnP/DLNA pour votre musique',
        start_url: '/app/',
        display: 'standalone',
        orientation: 'any',
        theme_color: '#111827',
        background_color: '#111827',
        icons: [
          {
            src: '/app/icons/icon-192.png',
            sizes: '192x192',
            type: 'image/png',
          },
          {
            src: '/app/icons/icon-512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'any maskable',
          },
        ],
        share_target: {
          action: '/app/',
          method: 'GET',
          params: {
            url: 'share_url',
            title: 'share_title',
            text: 'share_text',
          },
        },
      },
      workbox: {
        navigateFallback: '/app/index.html',
        globPatterns: ['**/*.{js,css,html,ico,png,svg,woff2}'],
        runtimeCaching: [
          {
            urlPattern: /^\/api\//,
            handler: 'NetworkFirst',
            options: {
              cacheName: 'api-cache',
              networkTimeoutSeconds: 5,
            },
          },
          {
            urlPattern: /^\/audio\//,
            handler: 'NetworkOnly',
          },
        ],
      },
    }),
  ],
  base: '/app/',  // Base path pour le déploiement
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  server: {
    proxy: {
      '/api/webrenderer/ws': {
        target: 'ws://localhost:8080',
        ws: true,
        changeOrigin: true,
      },
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
