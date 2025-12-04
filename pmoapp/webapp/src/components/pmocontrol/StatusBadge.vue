<script setup lang="ts">
defineProps<{
  status: 'PLAYING' | 'PAUSED' | 'STOPPED' | 'TRANSITIONING' | 'NO_MEDIA' | 'UNKNOWN' | 'OFFLINE'
}>()

function getStatusClass(status: string): string {
  switch (status) {
    case 'PLAYING':
      return 'playing'
    case 'PAUSED':
      return 'paused'
    case 'STOPPED':
    case 'NO_MEDIA':
      return 'stopped'
    case 'TRANSITIONING':
      return 'transitioning'
    case 'OFFLINE':
      return 'offline'
    default:
      return 'stopped'
  }
}

function getStatusLabel(status: string): string {
  switch (status) {
    case 'PLAYING':
      return 'En lecture'
    case 'PAUSED':
      return 'En pause'
    case 'STOPPED':
      return 'Arrêté'
    case 'NO_MEDIA':
      return 'Aucun média'
    case 'TRANSITIONING':
      return 'Transition...'
    case 'OFFLINE':
      return 'Hors ligne'
    default:
      return status
  }
}
</script>

<template>
  <span :class="['status-badge', getStatusClass(status)]">
    {{ getStatusLabel(status) }}
  </span>
</template>

<style scoped>
/* Les styles sont définis globalement dans pmocontrol.css */
</style>
