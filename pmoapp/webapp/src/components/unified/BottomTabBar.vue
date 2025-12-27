<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { X, ChevronLeft, ChevronRight, Server } from 'lucide-vue-next'
import { useTabs } from '@/composables/useTabs'
import { useSwipe } from '@vueuse/core'

defineProps<{
  onlineServersCount?: number
}>()

const emit = defineEmits<{
  'open-drawer': []
}>()

const { tabs, activeTabId, switchTab, closeTab, nextTab, previousTab, compactMode } = useTabs()

const tabBarRef = ref<HTMLElement | null>(null)
const scrollContainerRef = ref<HTMLElement | null>(null)

// Gestion du swipe pour changer d'onglet
useSwipe(tabBarRef, {
  threshold: 50,
  onSwipeEnd(_e: TouchEvent, swipeDirection: string) {
    if (swipeDirection === 'left') {
      nextTab()
    } else if (swipeDirection === 'right') {
      previousTab()
    }
  },
})

// Scroll vers l'onglet actif
function scrollToActiveTab() {
  if (!scrollContainerRef.value) return

  const activeTabElement = scrollContainerRef.value.querySelector('.tab-item.active')
  if (activeTabElement) {
    activeTabElement.scrollIntoView({
      behavior: 'smooth',
      block: 'nearest',
      inline: 'center',
    })
  }
}

// Scroll vers l'onglet actif quand il change
watch(() => activeTabId.value, () => {
  scrollToActiveTab()
})

// Gestion des boutons de scroll
const showLeftScroll = ref(false)
const showRightScroll = ref(false)

function updateScrollButtons() {
  if (!scrollContainerRef.value) return

  const { scrollLeft, scrollWidth, clientWidth } = scrollContainerRef.value
  showLeftScroll.value = scrollLeft > 10
  showRightScroll.value = scrollLeft < scrollWidth - clientWidth - 10
}

function scrollLeft() {
  if (!scrollContainerRef.value) return
  scrollContainerRef.value.scrollBy({ left: -200, behavior: 'smooth' })
}

function scrollRight() {
  if (!scrollContainerRef.value) return
  scrollContainerRef.value.scrollBy({ left: 200, behavior: 'smooth' })
}

onMounted(() => {
  if (scrollContainerRef.value) {
    scrollContainerRef.value.addEventListener('scroll', updateScrollButtons)
    updateScrollButtons()
  }
})

onUnmounted(() => {
  if (scrollContainerRef.value) {
    scrollContainerRef.value.removeEventListener('scroll', updateScrollButtons)
  }
})

// Gestion du clic sur un onglet
function handleTabClick(tabId: string) {
  switchTab(tabId)
}

// Gestion du clic sur le bouton fermer
function handleCloseClick(event: Event, tabId: string) {
  event.stopPropagation()
  closeTab(tabId)
}

// Gestion du clic sur le bouton server drawer
function handleServerDrawerClick() {
  emit('open-drawer')
}
</script>

<template>
  <div ref="tabBarRef" class="bottom-tab-bar">
    <!-- Bouton pour ouvrir le drawer des servers -->
    <button
      class="server-drawer-button"
      @click="handleServerDrawerClick"
      :aria-label="`Open media servers (${onlineServersCount || 0} online)`"
      :title="`Media Servers (${onlineServersCount || 0} online)`"
    >
      <Server :size="24" />
      <span v-if="onlineServersCount && onlineServersCount > 0" class="server-badge">{{ onlineServersCount }}</span>
    </button>

    <!-- Bouton scroll gauche -->
    <button
      v-if="showLeftScroll"
      class="scroll-button scroll-left"
      @click="scrollLeft"
      aria-label="Scroll left"
    >
      <ChevronLeft :size="20" />
    </button>

    <!-- Container avec scroll horizontal -->
    <div ref="scrollContainerRef" class="tabs-scroll-container">
      <div class="tabs-container">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="tab-item"
          :class="{
            active: tab.id === activeTabId,
            compact: compactMode
          }"
          @click="handleTabClick(tab.id)"
          :title="tab.fullTitle"
          :aria-label="`Switch to ${tab.fullTitle} tab`"
          :aria-current="tab.id === activeTabId ? 'page' : undefined"
        >
          <!-- Icône -->
          <component :is="tab.icon" class="tab-icon" :size="compactMode ? 28 : 24" />

          <!-- Titre (masqué en mode compact) -->
          <span v-if="!compactMode" class="tab-title">{{ tab.title }}</span>

          <!-- Bouton fermer (seulement pour les onglets fermables, masqué en mode compact) -->
          <button
            v-if="tab.closeable && !compactMode"
            class="tab-close"
            @click="(e) => handleCloseClick(e, tab.id)"
            :aria-label="`Close ${tab.fullTitle} tab`"
          >
            <X :size="16" />
          </button>
        </button>
      </div>
    </div>

    <!-- Bouton scroll droite -->
    <button
      v-if="showRightScroll"
      class="scroll-button scroll-right"
      @click="scrollRight"
      aria-label="Scroll right"
    >
      <ChevronRight :size="20" />
    </button>
  </div>
</template>

<style scoped>
.bottom-tab-bar {
  position: relative;
  display: flex;
  align-items: center;
  height: 64px;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(30px) saturate(180%);
  -webkit-backdrop-filter: blur(30px) saturate(180%);
  border-top: 1px solid rgba(255, 255, 255, 0.2);
  box-shadow: 0 -4px 24px rgba(0, 0, 0, 0.1);
  z-index: 100;
  overflow: hidden;
}

/* Support pour thème sombre */
@media (prefers-color-scheme: dark) {
  .bottom-tab-bar {
    background: rgba(0, 0, 0, 0.25);
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
}

.tabs-scroll-container {
  flex: 1;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none; /* Firefox */
  -ms-overflow-style: none; /* IE/Edge */
}

.tabs-scroll-container::-webkit-scrollbar {
  display: none; /* Chrome/Safari */
}

.tabs-container {
  display: flex;
  gap: 4px;
  padding: 0 8px;
  min-width: 100%;
}

.tab-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 100px;
  max-width: 200px;
  height: 64px;
  padding: 8px 16px;
  background: transparent;
  border: none;
  border-bottom: 4px solid transparent;
  cursor: pointer;
  transition: all 0.3s ease;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  font-family: inherit;
  white-space: nowrap;
  flex-shrink: 0;
}

/* Mode compact: icônes seulement */
.tab-item.compact {
  min-width: 60px;
  max-width: 60px;
  padding: 8px;
  justify-content: center;
}

.tab-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--color-text);
}

.tab-item.active {
  background: rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border-bottom-color: var(--color-primary);
  color: var(--color-text);
  font-weight: 600;
}

@media (prefers-color-scheme: dark) {
  .tab-item:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .tab-item.active {
    background: rgba(255, 255, 255, 0.25);
  }
}

.tab-icon {
  flex-shrink: 0;
  color: currentColor;
}

.tab-title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}

.tab-close {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  background: rgba(255, 255, 255, 0.1);
  border: none;
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.2s ease;
  color: var(--color-text-secondary);
}

.tab-close:hover {
  background: rgba(255, 255, 255, 0.3);
  color: var(--color-text);
  transform: scale(1.1);
}

.tab-close:active {
  transform: scale(0.95);
}

/* Boutons de scroll */
.scroll-button {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  background: rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.2s ease;
  color: var(--color-text);
}

.scroll-button:hover {
  background: rgba(255, 255, 255, 0.3);
  transform: translateY(-50%) scale(1.1);
}

.scroll-button:active {
  transform: translateY(-50%) scale(0.95);
}

.scroll-left {
  left: 8px;
}

.scroll-right {
  right: 8px;
}

/* Responsive mobile */
@media (max-width: 768px) {
  .tab-item {
    min-width: 80px;
    max-width: 150px;
    padding: 8px 12px;
    gap: 6px;
  }

  .tab-title {
    font-size: 12px;
  }

  .tab-icon {
    width: 20px;
    height: 20px;
  }

  .tab-close {
    width: 24px;
    height: 24px;
  }
}

/* Animation d'entrée */
@keyframes slideInUp {
  from {
    transform: translateY(100%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}

.bottom-tab-bar {
  animation: slideInUp 0.3s ease-out;
}

/* Effet de swipe visuel */
.tabs-scroll-container {
  touch-action: pan-x;
}

/* Bouton server drawer */
.server-drawer-button {
  position: relative;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  margin: 0 8px;
  background: rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.3s ease;
  color: var(--color-text);
}

.server-drawer-button:hover {
  background: rgba(255, 255, 255, 0.3);
  transform: scale(1.1);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.server-drawer-button:active {
  transform: scale(0.95);
}

@media (prefers-color-scheme: dark) {
  .server-drawer-button {
    background: rgba(255, 255, 255, 0.15);
  }

  .server-drawer-button:hover {
    background: rgba(255, 255, 255, 0.25);
  }
}

/* Badge pour le nombre de servers online */
.server-badge {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 20px;
  height: 20px;
  padding: 0 6px;
  font-size: 11px;
  font-weight: 700;
  color: white;
  background: rgba(102, 126, 234, 0.9);
  border: 2px solid var(--color-bg);
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}
</style>
