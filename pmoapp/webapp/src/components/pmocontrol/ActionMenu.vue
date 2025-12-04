<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRenderersStore } from '@/stores/renderers'
import { Play, ListPlus, Link, ChevronRight } from 'lucide-vue-next'

defineProps<{
  type: 'container' | 'item'
  entryId: string
  serverId: string
}>()

const emit = defineEmits<{
  playNow: [rendererId: string]
  addToQueue: [rendererId: string]
  attachPlaylist: [rendererId: string]
}>()

const renderersStore = useRenderersStore()
const showMenu = ref(false)
const showRendererSubmenu = ref<'play' | 'queue' | 'attach' | null>(null)

const onlineRenderers = computed(() => renderersStore.onlineRenderers)

function handleAction(action: 'play' | 'queue' | 'attach', rendererId: string) {
  showMenu.value = false
  showRendererSubmenu.value = null

  switch (action) {
    case 'play':
      emit('playNow', rendererId)
      break
    case 'queue':
      emit('addToQueue', rendererId)
      break
    case 'attach':
      emit('attachPlaylist', rendererId)
      break
  }
}

function toggleMenu() {
  showMenu.value = !showMenu.value
  if (!showMenu.value) {
    showRendererSubmenu.value = null
  }
}
</script>

<template>
  <div class="action-menu-container" @click.stop>
    <button class="action-menu-trigger" @click="toggleMenu" :title="'Actions'">
      ⋮
    </button>

    <div v-if="showMenu" class="action-menu" @click.stop>
      <!-- Play Now -->
      <div
        class="menu-item"
        @mouseenter="showRendererSubmenu = 'play'"
        @mouseleave="showRendererSubmenu = null"
      >
        <Play :size="16" />
        <span>Lire maintenant</span>
        <ChevronRight :size="16" class="submenu-arrow" />

        <!-- Submenu renderers -->
        <div v-if="showRendererSubmenu === 'play'" class="renderer-submenu">
          <div
            v-for="renderer in onlineRenderers"
            :key="renderer.id"
            class="submenu-item"
            @click="handleAction('play', renderer.id)"
          >
            {{ renderer.friendly_name }}
          </div>
          <div v-if="onlineRenderers.length === 0" class="submenu-empty">
            Aucun renderer disponible
          </div>
        </div>
      </div>

      <!-- Add to Queue -->
      <div
        class="menu-item"
        @mouseenter="showRendererSubmenu = 'queue'"
        @mouseleave="showRendererSubmenu = null"
      >
        <ListPlus :size="16" />
        <span>Ajouter à la queue</span>
        <ChevronRight :size="16" class="submenu-arrow" />

        <div v-if="showRendererSubmenu === 'queue'" class="renderer-submenu">
          <div
            v-for="renderer in onlineRenderers"
            :key="renderer.id"
            class="submenu-item"
            @click="handleAction('queue', renderer.id)"
          >
            {{ renderer.friendly_name }}
          </div>
          <div v-if="onlineRenderers.length === 0" class="submenu-empty">
            Aucun renderer disponible
          </div>
        </div>
      </div>

      <!-- Attach Playlist (only for containers) -->
      <div
        v-if="type === 'container'"
        class="menu-item"
        @mouseenter="showRendererSubmenu = 'attach'"
        @mouseleave="showRendererSubmenu = null"
      >
        <Link :size="16" />
        <span>Attacher la queue de</span>
        <ChevronRight :size="16" class="submenu-arrow" />

        <div v-if="showRendererSubmenu === 'attach'" class="renderer-submenu">
          <div
            v-for="renderer in onlineRenderers"
            :key="renderer.id"
            class="submenu-item"
            @click="handleAction('attach', renderer.id)"
          >
            {{ renderer.friendly_name }}
          </div>
          <div v-if="onlineRenderers.length === 0" class="submenu-empty">
            Aucun renderer disponible
          </div>
        </div>
      </div>
    </div>

    <!-- Backdrop to close menu -->
    <div v-if="showMenu" class="menu-backdrop" @click="showMenu = false"></div>
  </div>
</template>

<style scoped>
.action-menu-container {
  position: relative;
}

.action-menu-trigger {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
  font-size: 1.5rem;
  font-weight: bold;
  line-height: 1;
}

.action-menu-trigger:hover {
  background-color: var(--color-bg-tertiary);
  color: var(--color-text);
}

.action-menu {
  position: absolute;
  right: 0;
  top: 100%;
  margin-top: var(--spacing-xs);
  background-color: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  min-width: 200px;
  z-index: var(--z-dropdown);
  overflow: visible;
}

.menu-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-sm) var(--spacing-md);
  color: var(--color-text);
  cursor: pointer;
  transition: background-color var(--transition-fast);
}

.menu-item:hover {
  background-color: var(--color-bg-secondary);
}

.menu-item:first-child {
  border-radius: var(--radius-md) var(--radius-md) 0 0;
}

.menu-item:last-child {
  border-radius: 0 0 var(--radius-md) var(--radius-md);
}

.menu-item:only-child {
  border-radius: var(--radius-md);
}

.menu-item svg:first-child {
  color: var(--color-primary);
  flex-shrink: 0;
}

.menu-item span {
  flex: 1;
  font-size: var(--text-sm);
}

.submenu-arrow {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
  margin-left: auto;
}

.renderer-submenu {
  position: absolute;
  left: 100%;
  top: 0;
  margin-left: var(--spacing-xs);
  background-color: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  min-width: 180px;
  max-height: 300px;
  overflow-y: auto;
  z-index: calc(var(--z-dropdown) + 1);
}

.submenu-item {
  padding: var(--spacing-sm) var(--spacing-md);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
  transition: background-color var(--transition-fast);
}

.submenu-item:hover {
  background-color: var(--color-bg-secondary);
}

.submenu-empty {
  padding: var(--spacing-sm) var(--spacing-md);
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
  font-style: italic;
}

.menu-backdrop {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: calc(var(--z-dropdown) - 1);
}

/* Scrollbar for long renderer lists */
.renderer-submenu::-webkit-scrollbar {
  width: 6px;
}

.renderer-submenu::-webkit-scrollbar-track {
  background: var(--color-bg-secondary);
}

.renderer-submenu::-webkit-scrollbar-thumb {
  background: var(--color-border);
  border-radius: var(--radius-full);
}

.renderer-submenu::-webkit-scrollbar-thumb:hover {
  background: var(--color-text-tertiary);
}
</style>
