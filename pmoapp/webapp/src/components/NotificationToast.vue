<script setup lang="ts">
import { useUIStore } from '@/stores/ui'
import { AlertCircle, CheckCircle, Info, X, AlertTriangle } from 'lucide-vue-next'
import type { Notification } from '@/stores/ui'

const uiStore = useUIStore()

function removeNotification(id: string) {
  uiStore.removeNotification(id)
}

function getIcon(type: Notification['type']) {
  switch (type) {
    case 'success':
      return CheckCircle
    case 'error':
      return AlertCircle
    case 'warning':
      return AlertTriangle
    case 'info':
      return Info
  }
}

function getTypeClass(type: Notification['type']) {
  return `notification-${type}`
}
</script>

<template>
  <div class="notification-container">
    <TransitionGroup name="notification-list">
      <div
        v-for="notification in uiStore.notifications"
        :key="notification.id"
        :class="['notification', getTypeClass(notification.type)]"
      >
        <component :is="getIcon(notification.type)" :size="20" class="notification-icon" />
        <p class="notification-message">{{ notification.message }}</p>
        <button
          class="notification-close"
          @click="removeNotification(notification.id)"
          title="Fermer"
        >
          <X :size="18" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.notification-container {
  position: fixed;
  top: var(--spacing-lg);
  right: var(--spacing-lg);
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
  max-width: 400px;
  pointer-events: none;
}

.notification {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: var(--spacing-md);
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  box-shadow: var(--shadow-lg);
  pointer-events: auto;
  min-width: 300px;
}

.notification-icon {
  flex-shrink: 0;
}

.notification-message {
  flex: 1;
  margin: 0;
  font-size: var(--text-sm);
  color: var(--color-text);
  line-height: 1.4;
}

.notification-close {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.notification-close:hover {
  background-color: var(--color-bg);
  color: var(--color-text);
}

/* Type-specific styles */
.notification-success {
  border-left: 4px solid var(--status-playing);
}

.notification-success .notification-icon {
  color: var(--status-playing);
}

.notification-error {
  border-left: 4px solid var(--status-offline);
}

.notification-error .notification-icon {
  color: var(--status-offline);
}

.notification-warning {
  border-left: 4px solid var(--status-paused);
}

.notification-warning .notification-icon {
  color: var(--status-paused);
}

.notification-info {
  border-left: 4px solid var(--status-transitioning);
}

.notification-info .notification-icon {
  color: var(--status-transitioning);
}

/* Transitions */
.notification-list-enter-active,
.notification-list-leave-active {
  transition: all 0.3s ease;
}

.notification-list-enter-from {
  opacity: 0;
  transform: translateX(100%);
}

.notification-list-leave-to {
  opacity: 0;
  transform: translateX(100%);
}

/* Mobile responsive */
@media (max-width: 767px) {
  .notification-container {
    top: var(--spacing-md);
    right: var(--spacing-md);
    left: var(--spacing-md);
    max-width: none;
  }

  .notification {
    min-width: 0;
  }
}
</style>
