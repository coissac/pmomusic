<script setup lang="ts">
import { RadioTower, RefreshCw } from 'lucide-vue-next'
import { useRenderers } from '@/composables/useRenderers'
import { ref } from 'vue'

const { fetchRenderers } = useRenderers()
const isRefreshing = ref(false)

async function handleRefresh() {
  isRefreshing.value = true
  try {
    await fetchRenderers(true) // Force refresh
  } finally {
    // Petit délai pour montrer l'animation
    setTimeout(() => {
      isRefreshing.value = false
    }, 500)
  }
}
</script>

<template>
  <div class="empty-state">
    <div class="empty-content">
      <!-- Icône -->
      <div class="empty-icon">
        <RadioTower :size="80" />
      </div>

      <!-- Message -->
      <h2 class="empty-title">Aucun renderer détecté</h2>
      <p class="empty-description">
        Vérifiez que vos renderers (Chromecast, UPnP, OpenHome) sont allumés et connectés au réseau.
      </p>

      <!-- Bouton refresh -->
      <button
        class="btn-refresh"
        :class="{ refreshing: isRefreshing }"
        @click="handleRefresh"
        :disabled="isRefreshing"
      >
        <RefreshCw :size="20" :class="{ spin: isRefreshing }" />
        <span>{{ isRefreshing ? 'Actualisation...' : 'Actualiser' }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--spacing-xl);
  background: linear-gradient(135deg, rgba(99, 102, 241, 0.05) 0%, rgba(139, 92, 246, 0.05) 100%);
}

.empty-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-lg);
  max-width: 500px;
  padding: var(--spacing-2xl);
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.12);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.18);
  box-shadow:
    0 8px 32px 0 rgba(31, 38, 135, 0.15),
    inset 0 1px 0 0 rgba(255, 255, 255, 0.3);
  text-align: center;
}

@media (prefers-color-scheme: dark) {
  .empty-content {
    background: rgba(0, 0, 0, 0.3);
    border-color: rgba(255, 255, 255, 0.12);
  }
}

.empty-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 120px;
  height: 120px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 2px solid rgba(255, 255, 255, 0.2);
  color: var(--color-text-secondary);
  margin-bottom: var(--spacing-md);
}

.empty-title {
  font-size: var(--text-2xl);
  font-weight: 700;
  color: var(--color-text);
  margin: 0;
}

.empty-description {
  font-size: var(--text-base);
  color: var(--color-text-secondary);
  line-height: 1.6;
  margin: 0;
  max-width: 400px;
}

.btn-refresh {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 12px 24px;
  margin-top: var(--spacing-md);
  font-size: var(--text-base);
  font-weight: 600;
  font-family: inherit;
  color: white;
  background: rgba(102, 126, 234, 0.8);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(102, 126, 234, 0.4);
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.3s ease;
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}

.btn-refresh:hover:not(:disabled) {
  background: rgba(102, 126, 234, 1);
  border-color: rgba(102, 126, 234, 0.6);
  transform: translateY(-2px);
  box-shadow: 0 6px 16px rgba(102, 126, 234, 0.4);
}

.btn-refresh:active:not(:disabled) {
  transform: translateY(0);
}

.btn-refresh:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.btn-refresh .spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* Responsive mobile */
@media (max-width: 768px) {
  .empty-state {
    padding: var(--spacing-lg);
  }

  .empty-content {
    padding: var(--spacing-xl);
  }

  .empty-icon {
    width: 100px;
    height: 100px;
  }

  .empty-icon svg {
    width: 64px;
    height: 64px;
  }

  .empty-title {
    font-size: var(--text-xl);
  }

  .empty-description {
    font-size: var(--text-sm);
  }
}

/* Fallback pour navigateurs sans backdrop-filter */
@supports not (backdrop-filter: blur(20px)) {
  .empty-content {
    background: rgba(255, 255, 255, 0.95);
  }

  @media (prefers-color-scheme: dark) {
    .empty-content {
      background: rgba(20, 20, 30, 0.95);
    }
  }
}
</style>
