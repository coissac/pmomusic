<template>
  <div class="app-container">
    <main class="main-content">
      <router-view />
    </main>

    <!-- Notifications Toast -->
    <NotificationToast />
  </div>
</template>

<script setup lang="ts">
import { watch } from 'vue'
import NotificationToast from '@/components/NotificationToast.vue'
import { useShareTarget } from '@/composables/useShareTarget'
import { useUIStore } from '@/stores/ui'

const ui = useUIStore()
const { shareError } = useShareTarget()

watch(shareError, (err) => {
  if (err) ui.notifyError(err)
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

.main-content {
  flex: 1;
  width: 100%;
  box-sizing: border-box;
  overflow-x: hidden;
}
</style>
