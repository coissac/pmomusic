import { ref, onMounted } from 'vue'
import { searchSource } from '@/services/pmosource'
import { useRenderers } from '@/composables/useRenderers'

export interface ShareTargetResult {
  url: string
  title: string | null
  containerId: string
}

const pendingShare = ref<ShareTargetResult | null>(null)
const shareError = ref<string | null>(null)

let localServerId: string | null = null

async function fetchLocalServerId(): Promise<string | null> {
  if (localServerId) return localServerId
  try {
    const resp = await fetch('/api/info')
    if (!resp.ok) return null
    const data = await resp.json()
    localServerId = data.local_server_id ?? null
    return localServerId
  } catch {
    return null
  }
}

export function useShareTarget() {
  const { selectedRendererId, attachAndPlayPlaylist } = useRenderers()

  async function handleShareIfPresent() {
    const params = new URLSearchParams(window.location.search)
    const sharedUrl = params.get('share_url') ?? params.get('share_text') ?? null
    const sharedTitle = params.get('share_title')

    if (!sharedUrl) return

    const clean = new URL(window.location.href)
    clean.searchParams.delete('share_url')
    clean.searchParams.delete('share_title')
    clean.searchParams.delete('share_text')
    window.history.replaceState({}, '', clean.toString())

    try {
      shareError.value = null
      const result = await searchSource('url', sharedUrl)
      if (result.total === 0) {
        shareError.value = `Aucun contenu trouvé pour : ${sharedUrl}`
        return
      }

      const container = result.containers[0] ?? null
      const containerId = container?.id ?? result.items[0]?.id

      if (!containerId) {
        shareError.value = 'Contenu résolu mais sans identifiant jouable'
        return
      }

      const serverId = await fetchLocalServerId()
      const rendererId = selectedRendererId.value

      if (!serverId || !rendererId) {
        // Pas de renderer sélectionné ou serveur inconnu : stocker pour affichage manuel
        pendingShare.value = { url: sharedUrl, title: sharedTitle, containerId }
        return
      }

      await attachAndPlayPlaylist(rendererId, serverId, containerId)
    } catch (e) {
      shareError.value = e instanceof Error ? e.message : 'Erreur lors de la résolution'
    }
  }

  function clearShare() {
    pendingShare.value = null
    shareError.value = null
  }

  onMounted(() => {
    handleShareIfPresent()
  })

  return {
    pendingShare,
    shareError,
    clearShare,
  }
}
