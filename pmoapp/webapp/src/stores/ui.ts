// Store Pinia pour l'état UI global
import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface Notification {
  id: string
  type: 'info' | 'success' | 'warning' | 'error'
  message: string
  duration?: number  // ms, undefined = permanent
}

export const useUIStore = defineStore('ui', () => {
  // État
  const selectedRendererId = ref<string | null>(null)
  const selectedServerId = ref<string | null>(null)
  const showEventLog = ref(false)
  const sseConnected = ref(false)
  const notifications = ref<Notification[]>([])

  // Actions
  function selectRenderer(id: string | null) {
    selectedRendererId.value = id
  }

  function selectServer(id: string | null) {
    selectedServerId.value = id
  }

  function toggleEventLog() {
    showEventLog.value = !showEventLog.value
  }

  function setSSEConnected(connected: boolean) {
    sseConnected.value = connected
  }

  function addNotification(
    type: Notification['type'],
    message: string,
    duration?: number
  ) {
    const id = `notif-${Date.now()}-${Math.random()}`
    const notification: Notification = {
      id,
      type,
      message,
      duration,
    }

    notifications.value.push(notification)

    // Auto-remove après duration (défaut: 5s)
    const timeout = duration !== undefined ? duration : 5000
    if (timeout > 0) {
      setTimeout(() => {
        removeNotification(id)
      }, timeout)
    }

    return id
  }

  function removeNotification(id: string) {
    const index = notifications.value.findIndex(n => n.id === id)
    if (index !== -1) {
      notifications.value.splice(index, 1)
    }
  }

  function clearNotifications() {
    notifications.value = []
  }

  // Raccourcis pour les types de notifications
  function notifySuccess(message: string, duration?: number) {
    return addNotification('success', message, duration)
  }

  function notifyError(message: string, duration?: number) {
    return addNotification('error', message, duration || 7000)  // 7s pour erreurs
  }

  function notifyWarning(message: string, duration?: number) {
    return addNotification('warning', message, duration)
  }

  function notifyInfo(message: string, duration?: number) {
    return addNotification('info', message, duration)
  }

  return {
    // État
    selectedRendererId,
    selectedServerId,
    showEventLog,
    sseConnected,
    notifications,
    // Actions
    selectRenderer,
    selectServer,
    toggleEventLog,
    setSSEConnected,
    addNotification,
    removeNotification,
    clearNotifications,
    notifySuccess,
    notifyError,
    notifyWarning,
    notifyInfo,
  }
})
