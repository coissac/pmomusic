<template>
  <div class="app-container">
    <nav class="main-nav">
      <router-link to="/">🏠 Accueil</router-link>

      <!-- Menu déroulant Debug -->
      <div class="dropdown" @mouseenter="showDebugMenu = true" @mouseleave="showDebugMenu = false">
        <button class="dropdown-toggle" :class="{ active: isDebugRoute }">
          🔧 Debug
          <span class="arrow">{{ showDebugMenu ? '▼' : '▶' }}</span>
        </button>
        <div v-show="showDebugMenu" class="dropdown-menu">
          <router-link to="/logs" @click="showDebugMenu = false">📋 Logs</router-link>
          <router-link to="/upnp" @click="showDebugMenu = false">🎵 UPnP Explorer</router-link>
          <router-link to="/covers-cache" @click="showDebugMenu = false">🎨 Cover Cache</router-link>
          <router-link to="/audio-cache" @click="showDebugMenu = false">🎵 Audio Cache</router-link>
          <router-link to="/api-dashboard" @click="showDebugMenu = false">🚀 API Dashboard</router-link>

          <div class="submenu-divider">Sources</div>
          <router-link to="/radio-paradise" @click="showDebugMenu = false">📻 Radio Paradise</router-link>
        </div>
      </div>
    </nav>
    <main class="main-content">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute } from 'vue-router'

const showDebugMenu = ref(false)
const route = useRoute()

const isDebugRoute = computed(() => {
  return ['/logs', '/upnp', '/covers-cache', '/audio-cache', '/api-dashboard', '/radio-paradise'].includes(route.path)
})
</script>

<style scoped>
.app-container {
  width: 100%;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
}

.main-nav {
  background: #333;
  width: 100%;
  padding: 0.75rem 1rem;
  box-sizing: border-box;
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
  position: sticky;
  top: 0;
  z-index: 1000;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
}

.main-nav a {
  color: #eee;
  padding: 0.5rem 1rem;
  border-radius: 4px;
  transition: all 0.2s;
  text-decoration: none;
  white-space: nowrap;
}

.main-nav a:hover {
  background: #555;
  color: #fff;
}

.main-nav a.router-link-active {
  background: #569cd6;
  color: #fff;
  font-weight: bold;
}

/* Dropdown menu */
.dropdown {
  position: relative;
  display: inline-block;
}

.dropdown-toggle {
  color: #eee;
  padding: 0.5rem 1rem;
  border-radius: 4px;
  transition: all 0.2s;
  background: transparent;
  border: none;
  cursor: pointer;
  font-size: 1rem;
  font-family: inherit;
  white-space: nowrap;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.dropdown-toggle:hover {
  background: #555;
  color: #fff;
}

.dropdown-toggle.active {
  background: #569cd6;
  color: #fff;
  font-weight: bold;
}

.dropdown-toggle .arrow {
  font-size: 0.7em;
  transition: transform 0.2s;
}

.dropdown-menu {
  position: absolute;
  top: 100%;
  left: 0;
  background: #2d2d2d;
  border: 1px solid #555;
  border-radius: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  min-width: 200px;
  margin-top: 0;
  z-index: 1001;
  display: flex;
  flex-direction: column;
  padding: 0.5rem 0;
}

.dropdown-menu a {
  padding: 0.75rem 1rem;
  color: #eee;
  text-decoration: none;
  transition: all 0.2s;
  border-radius: 0;
  display: block;
}

.dropdown-menu a:hover {
  background: #555;
  color: #fff;
}

.dropdown-menu a.router-link-active {
  background: #569cd6;
  color: #fff;
  font-weight: bold;
}

.submenu-divider {
  padding: 0.5rem 1rem;
  margin-top: 0.5rem;
  border-top: 1px solid #555;
  color: #999;
  font-size: 0.85em;
  font-weight: bold;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.main-content {
  flex: 1;
  width: 100%;
  box-sizing: border-box;
  overflow-x: hidden;
}

/* Responsive pour petits écrans */
@media (max-width: 768px) {
  .main-nav {
    padding: 0.5rem;
  }

  .main-nav a {
    font-size: 0.9rem;
    padding: 0.4rem 0.8rem;
  }

  .dropdown-toggle {
    font-size: 0.9rem;
    padding: 0.4rem 0.8rem;
  }

  .dropdown-menu {
    min-width: 180px;
  }

  .dropdown-menu a {
    font-size: 0.9rem;
    padding: 0.6rem 0.8rem;
  }
}
</style>
