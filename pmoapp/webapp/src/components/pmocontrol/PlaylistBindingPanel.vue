<script setup lang="ts">
import { computed } from 'vue'
import { useRenderersStore } from '@/stores/renderers'
import { Link, Unlink } from 'lucide-vue-next'

const props = defineProps<{
  rendererId: string
}>()

const renderersStore = useRenderersStore()

const binding = computed(() => renderersStore.getBindingById(props.rendererId))
const isAttached = computed(() => !!binding.value)

async function handleDetach() {
  try {
    await renderersStore.detachPlaylist(props.rendererId)
  } catch (error) {
    console.error('Erreur detach:', error)
  }
}
</script>

<template>
  <div class="playlist-binding-panel">
    <h4 class="panel-title">Synchronisation Playlist</h4>

    <!-- État attaché -->
    <div v-if="isAttached" class="binding-status attached">
      <div class="status-info">
        <Link :size="20" />
        <div class="status-text">
          <p class="status-label">Attachée à une playlist</p>
          <p class="status-details">
            <span class="detail-label">Serveur:</span>
            {{ binding?.server_id }}
          </p>
          <p class="status-details">
            <span class="detail-label">Container:</span>
            {{ binding?.container_id }}
          </p>
        </div>
      </div>

      <button class="btn" @click="handleDetach">
        <Unlink :size="16" />
        Détacher
      </button>
    </div>

    <!-- État détaché -->
    <div v-else class="binding-status detached">
      <div class="status-info">
        <Unlink :size="20" />
        <div class="status-text">
          <p class="status-label">Non attachée</p>
          <p class="status-description">
            La file d'attente n'est pas synchronisée avec une playlist.
          </p>
        </div>
      </div>

      <div class="help-section">
        <p class="help-text">
          <strong>📌 Playlist Dynamique</strong><br>
          Attachez cette queue à une playlist/album du serveur média.
          La queue se synchronisera automatiquement : les pistes ajoutées ou
          retirées sur le serveur apparaîtront ici en temps réel.
        </p>
        <p class="help-text help-text-warning">
          ⚠️ <em>Fonctionnalité en cours de développement</em> :
          Les actions "Attacher" ne sont pas encore disponibles dans le navigateur de médias.
          Pour l'instant, utilisez l'API REST directement.
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.playlist-binding-panel {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
  padding: var(--spacing-lg);
  background-color: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border);
}

.panel-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.binding-status {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

.status-info {
  display: flex;
  gap: var(--spacing-md);
  align-items: flex-start;
}

.status-info > svg {
  flex-shrink: 0;
  margin-top: var(--spacing-xs);
}

.attached .status-info > svg {
  color: var(--status-playing);
}

.detached .status-info > svg {
  color: var(--color-text-tertiary);
}

.status-text {
  flex: 1;
}

.status-label {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--color-text);
  margin: 0 0 var(--spacing-xs);
}

.status-description {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 0;
}

.status-details {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: var(--spacing-xs) 0 0;
  font-family: var(--font-mono);
}

.detail-label {
  font-weight: 600;
  color: var(--color-text);
}

.help-section {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}

.help-text {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  margin: 0;
  padding: var(--spacing-sm);
  background-color: var(--color-bg);
  border-radius: var(--radius-md);
  border-left: 3px solid var(--status-transitioning);
  line-height: 1.6;
}

.help-text strong {
  color: var(--color-text);
  display: block;
  margin-bottom: var(--spacing-xs);
}

.help-text-warning {
  border-left-color: var(--status-paused);
  background-color: var(--status-paused-bg);
  font-style: italic;
}
</style>
