<template>
  <div class="log-viewer">
    <div class="header">
      <h2>📋 System Logs</h2>
      <div class="controls">
        <button @click="toggleAutoScroll" :class="{ active: autoScroll }">
          {{ autoScroll ? '📌 Auto-scroll ON' : '📌 Auto-scroll OFF' }}
        </button>
        <button @click="clearLogs">🗑️ Clear</button>

        <!-- Sélection du niveau côté serveur -->
        <select v-model="serverLogLevel" @change="updateServerLogLevel" class="filter server-level">
          <option value="ERROR">🔴 ERROR only</option>
          <option value="WARN">🟡 WARN+</option>
          <option value="INFO">🟢 INFO+</option>
          <option value="DEBUG">🔵 DEBUG+</option>
          <option value="TRACE">⚪ TRACE (all)</option>
        </select>

        <!-- Filtre côté client -->
        <select v-model="levelFilter" class="filter">
          <option value="ALL">All Levels</option>
          <option value="ERROR">ERROR</option>
          <option value="WARN">WARN</option>
          <option value="INFO">INFO</option>
          <option value="DEBUG">DEBUG</option>
          <option value="TRACE">TRACE</option>
        </select>
      </div>
    </div>

    <div class="log-container" ref="logContainer">
      <div
        v-for="(log, index) in filteredLogs"
        :key="index"
        :class="['log-entry', `level-${log.level.toLowerCase()}`, { 'is-history': log.isHistory }]"
      >
        <div class="log-header">
          <span class="timestamp">{{ formatTimestamp(log.timestamp) }}</span>
          <span class="level">{{ log.level }}</span>
          <span class="target">{{ log.target }}</span>
        </div>
        <div class="log-content">
          <div class="message markdown-content">
            <details v-if="log.isTooLong" class="log-details">
              <summary class="log-summary">
                <span class="truncated-text">{{ log.truncatedMessage }}</span>
              </summary>
              <div class="full-message" v-html="log.renderedHtml"></div>
            </details>
            <div v-else v-html="log.renderedHtml"></div>
          </div>
        </div>
      </div>

      <div v-if="isLoadingHistory" class="loading-state">
        ⏳ Loading history...
      </div>

      <div v-else-if="filteredLogs.length === 0" class="empty-state">
        {{ isConnected ? 'Waiting for logs...' : 'Connecting to log stream...' }}
      </div>
    </div>

    <div class="footer">
      <span :class="['status', { connected: isConnected }]">
        {{ isConnected ? '🟢 Connected' : '🔴 Disconnected' }}
      </span>
      <span class="server-info">Server level: {{ serverLogLevel }}</span>
      <span class="count">{{ filteredLogs.length }} logs</span>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

// Configurer marked pour un rendu inline simple
marked.setOptions({
  breaks: true,
  gfm: true,
})

const logs = ref([])
const autoScroll = ref(true)
const isConnected = ref(false)
const isLoadingHistory = ref(true)
const levelFilter = ref('ALL')
const serverLogLevel = ref('TRACE')
const logContainer = ref(null)
let eventSource = null
let historyLoaded = false
const seenLogIds = new Set() // Pour détecter les duplicatas

// Ordre de gravité des niveaux (du plus grave au moins grave)
const levelOrder = {
  'ERROR': 0,
  'WARN': 1,
  'INFO': 2,
  'DEBUG': 3,
  'TRACE': 4
}

// Pré-calculer filteredLogs de manière optimisée
const filteredLogs = computed(() => {
  if (levelFilter.value === 'ALL') {
    return logs.value
  }
  // Utiliser la référence directe pour éviter des copies inutiles
  const filter = levelFilter.value
  return logs.value.filter(log => log.level === filter)
})

// Fonction pour mettre à jour le niveau de log côté serveur
async function updateServerLogLevel() {
  try {
    const response = await fetch('/api/log_setup', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        level: serverLogLevel.value
      })
    })

    if (response.ok) {
      const data = await response.json()
      console.log('Log level updated:', data.current_level)
    } else {
      console.error('Failed to update log level')
    }
  } catch (error) {
    console.error('Error updating log level:', error)
  }
}

// Charger le niveau de log actuel au démarrage
async function loadServerLogLevel() {
  try {
    const response = await fetch('/api/log_setup')
    if (response.ok) {
      const data = await response.json()
      serverLogLevel.value = data.current_level
    }
  } catch (error) {
    console.error('Error loading log level:', error)
  }
}

function formatTimestamp(timestamp) {
  const date = new Date(timestamp.secs_since_epoch * 1000)
  return date.toLocaleTimeString('fr-FR', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    fractionalSecondDigits: 3
  })
}

// Pré-traiter un log : calculer HTML, troncature, etc. UNE SEULE FOIS
function preprocessLog(message) {
  // ÉTAPE 1: Déterminer si trop long
  const firstLineEnd = message.indexOf('\n')
  const isTooLong = firstLineEnd !== -1 || message.length > 200

  // ÉTAPE 2: Calculer le message tronqué si nécessaire
  const truncatedMessage = isTooLong
    ? (firstLineEnd !== -1
        ? message.substring(0, firstLineEnd).trim()
        : message.substring(0, 200).trim())
    : null

  // ÉTAPE 3: Pré-processing pour détecter et protéger le XML
  let processedText = message

  // Détecter si le message contient du XML
  const hasXml = /<\?xml|<(scpd|root|service|device|actionList|stateVariable)[>\s]/i.test(message)

  if (hasXml) {
    const xmlStartMatch = message.match(/<\?xml[\s\S]*$/)

    if (xmlStartMatch) {
      const xmlContent = xmlStartMatch[0]
      const beforeXml = message.substring(0, message.indexOf(xmlContent))
      processedText = beforeXml + '\n```xml\n' + xmlContent + '\n```\n'
    } else {
      const xmlMatch = message.match(/<([a-zA-Z][a-zA-Z0-9:-]*)[>\s][\s\S]*/)
      if (xmlMatch) {
        const xmlContent = xmlMatch[0]
        const beforeXml = message.substring(0, message.indexOf(xmlContent))
        processedText = beforeXml + '\n```xml\n' + xmlContent + '\n```\n'
      }
    }
  }

  // ÉTAPE 4: Détecter et transformer les liens d'images
  const imageUrlPattern = /(https?:\/\/[^\s]+\.(?:png|jpg|jpeg|gif|webp|svg)(?:\?[^\s]*)?)/gi
  processedText = processedText.replace(imageUrlPattern, (match) => {
    return `\n![Image](${match})\n`
  })

  // ÉTAPE 5: Convertir markdown en HTML
  const rawHtml = marked.parse(processedText, { async: false })

  // ÉTAPE 6: Nettoyer pour la sécurité
  const renderedHtml = DOMPurify.sanitize(rawHtml, {
    ALLOWED_TAGS: ['strong', 'em', 'code', 'pre', 'a', 'ul', 'ol', 'li', 'p', 'br', 'span', 'img'],
    ALLOWED_ATTR: ['href', 'target', 'class', 'src', 'alt', 'title']
  })

  return {
    isTooLong,
    truncatedMessage,
    renderedHtml
  }
}

function toggleAutoScroll() {
  autoScroll.value = !autoScroll.value
  if (autoScroll.value) {
    scrollToBottom()
  }
}

function clearLogs() {
  logs.value = []
  seenLogIds.clear()
}

function scrollToBottom() {
  if (!logContainer.value || !autoScroll.value) return
  nextTick(() => {
    logContainer.value.scrollTop = logContainer.value.scrollHeight
  })
}

function connectSSE() {
  // Ajuste l'URL selon ton setup
  const baseUrl = window.location.origin
  eventSource = new EventSource(`${baseUrl}/log-sse`)

  eventSource.onopen = () => {
    isConnected.value = true
    console.log('SSE connection opened')
  }

  eventSource.onmessage = (event) => {
    try {
      const logEntry = JSON.parse(event.data)

      // Créer un ID unique basé sur timestamp + message + target
      const logId = `${logEntry.timestamp.secs_since_epoch}-${logEntry.timestamp.nanos_since_epoch}-${logEntry.message}-${logEntry.target}`

      // Ignorer les duplicatas
      if (seenLogIds.has(logId)) {
        return
      }
      seenLogIds.add(logId)

      // Marquer les logs historiques
      if (!historyLoaded) {
        logEntry.isHistory = true
      }

      // PRÉ-TRAITER le log UNE SEULE FOIS à la réception
      const processed = preprocessLog(logEntry.message)
      logEntry.isTooLong = processed.isTooLong
      logEntry.truncatedMessage = processed.truncatedMessage
      logEntry.renderedHtml = processed.renderedHtml

      logs.value.push(logEntry)

      // Limiter à 1000 logs en mémoire
      if (logs.value.length > 1000) {
        const removed = logs.value.shift()
        // Nettoyer aussi le Set pour éviter qu'il grandisse indéfiniment
        const removedId = `${removed.timestamp.secs_since_epoch}-${removed.timestamp.nanos_since_epoch}-${removed.message}-${removed.target}`
        seenLogIds.delete(removedId)
      }

      scrollToBottom()
    } catch (error) {
      console.error('Failed to parse log entry:', error)
    }
  }

  eventSource.onerror = () => {
    isConnected.value = false
    isLoadingHistory.value = false
    console.error('SSE connection error')

    // Reconnexion automatique après 3 secondes
    setTimeout(() => {
      if (eventSource.readyState === EventSource.CLOSED) {
        historyLoaded = false
        connectSSE()
      }
    }, 3000)
  }

  // Détecter la fin du chargement de l'historique
  // (on considère qu'après 500ms sans log, l'historique est chargé)
  let historyTimeout
  const originalOnMessage = eventSource.onmessage
  eventSource.onmessage = (event) => {
    clearTimeout(historyTimeout)
    originalOnMessage(event)

    if (!historyLoaded) {
      historyTimeout = setTimeout(() => {
        historyLoaded = true
        isLoadingHistory.value = false
        console.log('History loaded, now streaming live logs')
      }, 500)
    }
  }
}

onMounted(() => {
  loadServerLogLevel()
  connectSSE()
})

onUnmounted(() => {
  if (eventSource) {
    eventSource.close()
  }
})

// Désactiver auto-scroll si l'utilisateur scroll manuellement
let scrollHandler = null
watch(logContainer, (container, oldContainer) => {
  // Nettoyer l'ancien listener si existant
  if (oldContainer && scrollHandler) {
    oldContainer.removeEventListener('scroll', scrollHandler)
  }

  if (!container) return

  scrollHandler = () => {
    const isAtBottom =
      container.scrollHeight - container.scrollTop <= container.clientHeight + 50

    if (!isAtBottom && autoScroll.value) {
      autoScroll.value = false
    }
  }

  container.addEventListener('scroll', scrollHandler, { passive: true })
})

// Nettoyer au démontage
onUnmounted(() => {
  if (logContainer.value && scrollHandler) {
    logContainer.value.removeEventListener('scroll', scrollHandler)
  }
})
</script>

<style scoped>
.log-viewer {
  display: flex;
  flex-direction: column;
  height: 80vh;
  width: 100vw;
  margin: 0;
  padding: 0;
  background: #1e1e1e;
  color: #d4d4d4;
  font-family: 'Consolas', 'Monaco', monospace;
  box-sizing: border-box;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 1.5rem;
  background: #252526;
  border-bottom: 1px solid #3e3e42;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.header h2 {
  margin: 0;
  color: #ffffff;
  font-size: 1.2rem;
  flex-shrink: 0;
}

@media (max-width: 768px) {
  .header {
    padding: 0.75rem 1rem;
  }

  .header h2 {
    font-size: 1rem;
    width: 100%;
  }
}

.controls {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

@media (max-width: 768px) {
  .controls {
    width: 100%;
    justify-content: space-between;
  }
}

button {
  padding: 0.5rem 1rem;
  background: #3c3c3c;
  color: #d4d4d4;
  border: 1px solid #555;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
  transition: all 0.2s;
  white-space: nowrap;
}

@media (max-width: 768px) {
  button {
    padding: 0.4rem 0.7rem;
    font-size: 0.8rem;
    flex: 1;
    min-width: 0;
  }
}

button:hover {
  background: #505050;
}

button.active {
  background: #0e639c;
  border-color: #1177bb;
}

.filter {
  padding: 0.5rem;
  background: #3c3c3c;
  color: #d4d4d4;
  border: 1px solid #555;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
}

.filter.server-level {
  background: #1e3a5f;
  border-color: #569cd6;
  font-weight: bold;
}

@media (max-width: 768px) {
  .filter {
    padding: 0.4rem;
    font-size: 0.8rem;
    flex: 1;
    min-width: 0;
  }
}

.log-container {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
  background: #1e1e1e;
}

.log-entry {
  display: flex;
  flex-direction: column;
  padding: 0.5rem;
  margin-bottom: 0.25rem;
  border-left: 3px solid transparent;
  font-size: 0.9rem;
  line-height: 1.4;
  gap: 0.5rem;
}

@media (max-width: 768px) {
  .log-entry {
    padding: 0.75rem 0.5rem;
    font-size: 0.85rem;
    border-left-width: 4px;
  }
}

.log-entry:hover {
  background: #2d2d30;
}

.log-entry.is-history {
  opacity: 0.7;
}

.log-header {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
}

@media (max-width: 768px) {
  .log-header {
    gap: 0.5rem;
  }
}

.log-content {
  padding-left: 0;
}

.timestamp {
  color: #858585;
  font-weight: 500;
  flex-shrink: 0;
}

@media (max-width: 768px) {
  .timestamp {
    font-size: 0.75rem;
  }
}

.level {
  font-weight: bold;
  text-transform: uppercase;
  padding: 0.1rem 0.5rem;
  border-radius: 3px;
  text-align: center;
  flex-shrink: 0;
}

@media (max-width: 768px) {
  .level {
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
  }
}

.target {
  color: #4ec9b0;
  font-style: italic;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 1;
  min-width: 0;
}

@media (max-width: 768px) {
  .target {
    font-size: 0.8rem;
    color: #6eb8a5;
  }
}

.message {
  color: #d4d4d4;
  word-break: break-word;
  text-align: left;
}

.log-details {
  margin: 0;
}

.log-summary {
  cursor: pointer;
  color: #569cd6;
  list-style: none;
  user-select: none;
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
}

.log-summary::-webkit-details-marker {
  display: none;
}

.log-summary::marker {
  content: '';
}

.log-summary::before {
  content: '▶';
  display: inline-block;
  width: 1em;
  transition: transform 0.2s;
  color: #569cd6;
  font-size: 0.8em;
}

.log-details[open] .log-summary::before {
  transform: rotate(90deg);
}

.log-summary:hover {
  color: #6fa8dc;
}

.log-summary:hover::before {
  color: #6fa8dc;
}

.truncated-text {
  color: #d4d4d4;
  font-family: 'Consolas', 'Monaco', monospace;
  white-space: pre-wrap;
  word-break: break-word;
}

.full-message {
  margin-top: 0.5rem;
  padding-left: 1.5em;
  border-left: 2px solid #569cd6;
  padding-top: 0.5rem;
}

.markdown-content {
  line-height: 1.5;
}

.markdown-content :deep(code) {
  background: #3c3c3c;
  padding: 0.1rem 0.3rem;
  border-radius: 3px;
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 0.85em;
  color: #ce9178;
}

.markdown-content :deep(pre) {
  background: #2d2d30;
  padding: 0.75rem;
  border-radius: 4px;
  overflow-x: auto;
  margin: 0.5rem 0;
  border: 1px solid #3e3e42;
  max-height: 400px;
  overflow-y: auto;
}

.markdown-content :deep(pre code) {
  background: transparent;
  padding: 0;
  color: #d4d4d4;
  font-size: 0.85em;
  line-height: 1.5;
  display: block;
}

/* Coloration pour les blocs XML */
.markdown-content :deep(pre code.language-xml) {
  color: #ce9178;
}

/* Style pour les images */
.markdown-content :deep(img) {
  max-width: 100%;
  height: auto;
  border-radius: 4px;
  margin: 0.5rem 0;
  border: 1px solid #3e3e42;
  display: block;
}

/* Scrollbar pour les blocs de code longs */
.markdown-content :deep(pre)::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.markdown-content :deep(pre)::-webkit-scrollbar-track {
  background: #1e1e1e;
}

.markdown-content :deep(pre)::-webkit-scrollbar-thumb {
  background: #424242;
  border-radius: 4px;
}

.markdown-content :deep(pre)::-webkit-scrollbar-thumb:hover {
  background: #4e4e4e;
}

.markdown-content :deep(strong) {
  color: #ffffff;
  font-weight: bold;
}

.markdown-content :deep(em) {
  color: #dcdcaa;
  font-style: italic;
}

.markdown-content :deep(a) {
  color: #569cd6;
  text-decoration: none;
}

.markdown-content :deep(a:hover) {
  text-decoration: underline;
}

.markdown-content :deep(p) {
  margin: 0;
  display: inline;
}

.markdown-content :deep(ul),
.markdown-content :deep(ol) {
  margin: 0.25rem 0;
  padding-left: 1.5rem;
}

/* Level colors - Classés par ordre de gravité */
.level-error {
  border-left-color: #f48771;
}

.level-error .level {
  background: #5a1e1e;
  color: #f48771;
}

.level-warn {
  border-left-color: #dcdcaa;
}

.level-warn .level {
  background: #4d4d2a;
  color: #dcdcaa;
}

.level-info {
  border-left-color: #4ec9b0;
}

.level-info .level {
  background: #1e4d42;
  color: #4ec9b0;
}

.level-debug {
  border-left-color: #569cd6;
}

.level-debug .level {
  background: #1e3a5f;
  color: #569cd6;
}

.level-trace {
  border-left-color: #808080;
}

.level-trace .level {
  background: #3a3a3a;
  color: #a0a0a0;
}

.empty-state {
  text-align: center;
  padding: 3rem;
  color: #858585;
  font-size: 1.1rem;
}

.loading-state {
  text-align: center;
  padding: 3rem;
  color: #569cd6;
  font-size: 1.1rem;
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.footer {
  display: flex;
  justify-content: space-between;
  padding: 0.75rem 1.5rem;
  background: #252526;
  border-top: 1px solid #3e3e42;
  font-size: 0.9rem;
  gap: 1rem;
}

@media (max-width: 768px) {
  .footer {
    padding: 0.6rem 1rem;
    font-size: 0.8rem;
  }
}

.status {
  color: #f48771;
}

.status.connected {
  color: #4ec9b0;
}

.server-info {
  color: #569cd6;
  font-weight: bold;
}

.count {
  color: #858585;
}

/* Scrollbar styling */
.log-container::-webkit-scrollbar {
  width: 12px;
}

.log-container::-webkit-scrollbar-track {
  background: #1e1e1e;
}

.log-container::-webkit-scrollbar-thumb {
  background: #424242;
  border-radius: 6px;
}

.log-container::-webkit-scrollbar-thumb:hover {
  background: #4e4e4e;
}
</style>
