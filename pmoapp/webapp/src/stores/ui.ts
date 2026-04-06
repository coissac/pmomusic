// Store Pinia pour l'état UI global
import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface Notification {
  id: string
  type: 'info' | 'success' | 'warning' | 'error'
  message: string
  duration?: number  // ms, undefined = permanent
}

const MAX_NOTIFICATIONS = 5

export const useUIStore = defineStore('ui', () => {
  // État
  const selectedRendererId = ref<string | null>(null)
  const selectedServerId = ref<string | null>(null)
  const showEventLog = ref(false)
  const sseConnected = ref(false)
  const notifications = ref<Notification[]>([])

  // Map pour suivre les timers et permettre le cleanup
  const notificationTimers = new Map<string, ReturnType<typeof setTimeout>>()

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
    // Limiter le nombre de notifications (P15)
    if (notifications.value.length >= MAX_NOTIFICATIONS) {
      const oldest = notifications.value.shift()
      if (oldest) {
        const timer = notificationTimers.get(oldest.id)
        if (timer) {
          clearTimeout(timer)
          notificationTimers.delete(oldest.id)
        }
      }
    }

    const id = `notif-${Date.now()}-${Math.random()}`
    const notification: Notification = {
      id,
      type,
      message,
      duration,
    }

    notifications.value.push(notification)

    // Auto-remove après duration (défaut: 5s) - avec tracking pour cleanup
    const timeout = duration !== undefined ? duration : 5000
    if (timeout > 0) {
      const timer = setTimeout(() => {
        removeNotification(id)
      }, timeout)
      notificationTimers.set(id, timer)
    }

    return id
  }

  function removeNotification(id: string) {
    const index = notifications.value.findIndex(n => n.id === id)
    if (index !== -1) {
      notifications.value.splice(index, 1)
    }
    // Nettoyer le timer associated
    const timer = notificationTimers.get(id)
    if (timer) {
      clearTimeout(timer)
      notificationTimers.delete(id)
    }
  }

  function clearNotifications() {
    // Nettoyer tous les timers
    notificationTimers.forEach(timer => clearTimeout(timer))
    notificationTimers.clear()
    notifications.value = []
  }

  // Cleanup function pour appeler lors du unmount de l'app
  function $dispose() {
    notificationTimers.forEach(timer => clearTimeout(timer))
    notificationTimers.clear()
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
    $dispose,
  }
})
