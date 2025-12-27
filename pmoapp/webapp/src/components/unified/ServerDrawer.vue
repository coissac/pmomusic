<script setup lang="ts">
import { computed, watch } from 'vue'
import { X, Server as ServerIcon, Circle } from 'lucide-vue-next'
import { useMediaServers } from '@/composables/useMediaServers'
import type { MediaServerSummary } from '@/services/pmocontrol/types'

const props = defineProps<{
  modelValue: boolean // v-model pour contrôler l'ouverture
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  'server-selected': [server: MediaServerSummary]
}>()

const { allServers, fetchServers } = useMediaServers()

// Rafraîchir la liste quand le drawer s'ouvre
watch(() => props.modelValue, (isOpen) => {
  if (isOpen) {
    fetchServers()
  }
})

const onlineServers = computed(() => allServers.value.filter((s: MediaServerSummary) => s.online))
const offlineServers = computed(() => allServers.value.filter((s: MediaServerSummary) => !s.online))

function close() {
  emit('update:modelValue', false)
}

function handleServerClick(server: MediaServerSummary) {
  if (!server.online) return
  emit('server-selected', server)
  close()
}
</script>

<template>
  <div>
    <!-- Backdrop -->
    <Transition name="backdrop">
      <div v-if="modelValue" class="drawer-backdrop" @click="close"></div>
    </Transition>

    <!-- Drawer -->
    <Transition name="drawer">
      <aside v-if="modelValue" class="server-drawer">
        <!-- Header -->
        <header class="drawer-header">
          <div class="drawer-title-section">
            <ServerIcon :size="24" />
            <h2 class="drawer-title">Media Servers</h2>
          </div>
          <button class="drawer-close-btn" @click="close" aria-label="Fermer">
            <X :size="24" />
          </button>
        </header>

        <!-- Liste des servers -->
        <div class="drawer-content">
          <!-- Servers online -->
          <section v-if="onlineServers.length > 0" class="server-section">
            <h3 class="section-title">Disponibles ({{ onlineServers.length }})</h3>
            <ul class="server-list">
              <li
                v-for="server in onlineServers"
                :key="server.id"
                class="server-item online"
                @click="handleServerClick(server)"
              >
                <div class="server-icon">
                  <ServerIcon :size="20" />
                </div>
                <div class="server-info">
                  <p class="server-name">{{ server.friendly_name }}</p>
                  <p v-if="server.model_name" class="server-model">{{ server.model_name }}</p>
                </div>
                <div class="server-status">
                  <Circle :size="8" fill="currentColor" />
                </div>
              </li>
            </ul>
          </section>

          <!-- Servers offline -->
          <section v-if="offlineServers.length > 0" class="server-section">
            <h3 class="section-title">Hors ligne ({{ offlineServers.length }})</h3>
            <ul class="server-list">
              <li
                v-for="server in offlineServers"
                :key="server.id"
                class="server-item offline"
              >
                <div class="server-icon">
                  <ServerIcon :size="20" />
                </div>
                <div class="server-info">
                  <p class="server-name">{{ server.friendly_name }}</p>
                  <p v-if="server.model_name" class="server-model">{{ server.model_name }}</p>
                </div>
                <div class="server-status">
                  <Circle :size="8" fill="currentColor" />
                </div>
              </li>
            </ul>
          </section>

          <!-- Aucun server -->
          <div v-if="allServers.length === 0" class="empty-servers">
            <ServerIcon :size="48" />
            <p>Aucun serveur multimédia détecté</p>
          </div>
        </div>
      </aside>
    </Transition>
  </div>
</template>

<style scoped>
.drawer-backdrop {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 200;
}

.server-drawer {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: 320px;
  max-width: 80vw;
  background: rgba(255, 255, 255, 0.12);
  backdrop-filter: blur(30px) saturate(180%);
  -webkit-backdrop-filter: blur(30px) saturate(180%);
  border-right: 1px solid rgba(255, 255, 255, 0.2);
  box-shadow: 4px 0 24px rgba(0, 0, 0, 0.2);
  z-index: 201;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

@media (prefers-color-scheme: dark) {
  .server-drawer {
    background: rgba(0, 0, 0, 0.4);
    border-right-color: rgba(255, 255, 255, 0.1);
  }
}

/* Header */
.drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--spacing-lg);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  flex-shrink: 0;
}

.drawer-title-section {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  color: var(--color-text);
}

.drawer-title {
  font-size: var(--text-xl);
  font-weight: 700;
  margin: 0;
}

.drawer-close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  padding: 0;
  background: rgba(255, 255, 255, 0.1);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.2s ease;
  color: var(--color-text);
}

.drawer-close-btn:hover {
  background: rgba(255, 255, 255, 0.2);
  transform: scale(1.1);
}

.drawer-close-btn:active {
  transform: scale(0.95);
}

/* Content */
.drawer-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-md);
}

.server-section {
  margin-bottom: var(--spacing-lg);
}

.section-title {
  font-size: var(--text-sm);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--color-text-secondary);
  margin: 0 0 var(--spacing-sm) 0;
  padding: 0 var(--spacing-sm);
}

.server-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.server-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: var(--spacing-md);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  transition: all 0.2s ease;
}

.server-item.online {
  cursor: pointer;
}

.server-item.online:hover {
  background: rgba(255, 255, 255, 0.15);
  border-color: rgba(255, 255, 255, 0.2);
  transform: translateX(4px);
}

.server-item.online:active {
  transform: translateX(2px);
}

.server-item.offline {
  opacity: 0.5;
  cursor: not-allowed;
}

.server-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.1);
  color: var(--color-text-secondary);
}

.server-info {
  flex: 1;
  min-width: 0;
}

.server-name {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.server-model {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 2px 0 0 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.server-status {
  flex-shrink: 0;
  color: var(--status-playing);
}

.server-item.offline .server-status {
  color: var(--status-offline);
}

.empty-servers {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-md);
  padding: var(--spacing-2xl);
  text-align: center;
  color: var(--color-text-secondary);
}

.empty-servers p {
  margin: 0;
  font-size: var(--text-base);
}

/* Animations */
.backdrop-enter-active,
.backdrop-leave-active {
  transition: opacity 0.3s ease;
}

.backdrop-enter-from,
.backdrop-leave-to {
  opacity: 0;
}

.drawer-enter-active,
.drawer-leave-active {
  transition: transform 0.3s ease;
}

.drawer-enter-from,
.drawer-leave-to {
  transform: translateX(-100%);
}

/* Scrollbar styling */
.drawer-content::-webkit-scrollbar {
  width: 6px;
}

.drawer-content::-webkit-scrollbar-track {
  background: rgba(255, 255, 255, 0.05);
  border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 3px;
}

.drawer-content::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}

/* Mobile responsive */
@media (max-width: 768px) {
  .server-drawer {
    width: 280px;
  }

  .drawer-header {
    padding: var(--spacing-md);
  }

  .drawer-title {
    font-size: var(--text-lg);
  }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(30px)) {
  .server-drawer {
    background: rgba(255, 255, 255, 0.98);
  }

  @media (prefers-color-scheme: dark) {
    .server-drawer {
      background: rgba(20, 20, 30, 0.98);
    }
  }
}
</style>
