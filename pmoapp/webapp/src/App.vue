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
import NotificationToast from '@/components/NotificationToast.vue'
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
