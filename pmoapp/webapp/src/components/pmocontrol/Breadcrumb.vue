<script setup lang="ts">
import { ChevronRight, Home } from 'lucide-vue-next'

export interface BreadcrumbItem {
  id: string
  title: string
}

defineProps<{
  items: BreadcrumbItem[]
  serverId: string
}>()

const emit = defineEmits<{
  navigate: [containerId: string]
}>()

function handleNavigate(containerId: string) {
  emit('navigate', containerId)
}

function goHome() {
  emit('navigate', '0')
}
</script>

<template>
  <nav class="breadcrumb" aria-label="Fil d'Ariane">
    <ol class="breadcrumb-list">
      <!-- Home -->
      <li class="breadcrumb-item">
        <button class="breadcrumb-link" @click="goHome" title="Accueil">
          <Home :size="16" />
          <span>Accueil</span>
        </button>
      </li>

      <!-- Path items -->
      <template v-for="(item, index) in items" :key="item.id">
        <li class="breadcrumb-separator" aria-hidden="true">
          <ChevronRight :size="16" />
        </li>
        <li class="breadcrumb-item">
          <button
            v-if="index < items.length - 1"
            class="breadcrumb-link"
            @click="handleNavigate(item.id)"
            :title="item.title"
          >
            {{ item.title }}
          </button>
          <span v-else class="breadcrumb-current" :title="item.title">
            {{ item.title }}
          </span>
        </li>
      </template>
    </ol>
  </nav>
</template>

<style scoped>
.breadcrumb {
  width: 100%;
  overflow-x: auto;
  overflow-y: hidden;
  padding: var(--spacing-sm) 0;
}

.breadcrumb-list {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  list-style: none;
  margin: 0;
  padding: 0;
  white-space: nowrap;
}

.breadcrumb-item {
  display: flex;
  align-items: center;
}

.breadcrumb-separator {
  display: flex;
  align-items: center;
  color: var(--color-text-tertiary);
}

.breadcrumb-link {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  padding: var(--spacing-xs) var(--spacing-sm);
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.breadcrumb-link:hover {
  background-color: var(--color-bg-secondary);
  color: var(--color-primary);
}

.breadcrumb-link:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}

.breadcrumb-current {
  display: inline-block;
  padding: var(--spacing-xs) var(--spacing-sm);
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text);
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Scrollbar styling */
.breadcrumb::-webkit-scrollbar {
  height: 4px;
}

.breadcrumb::-webkit-scrollbar-track {
  background: var(--color-bg-secondary);
  border-radius: var(--radius-full);
}

.breadcrumb::-webkit-scrollbar-thumb {
  background: var(--color-border);
  border-radius: var(--radius-full);
}

.breadcrumb::-webkit-scrollbar-thumb:hover {
  background: var(--color-text-tertiary);
}

/* Mobile */
@media (max-width: 768px) {
  .breadcrumb-link,
  .breadcrumb-current {
    max-width: 150px;
  }
}
</style>
