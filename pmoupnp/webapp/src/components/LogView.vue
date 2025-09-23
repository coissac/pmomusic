<template>
  <div class="log-viewer">
    <div class="header">
      <h2>📋 System Logs</h2>
      <div class="controls">
        <button @click="toggleAutoScroll" :class="{ active: autoScroll }">
          {{ autoScroll ? '📌 Auto-scroll ON' : '📌 Auto-scroll OFF' }}
        </button>
        <button @click="clearLogs">🗑️ Clear</button>
        <select v-model="levelFilter" class="filter">
          <option value="ALL">All Levels</option>
          <option value="TRACE">TRACE</option>
          <option value="DEBUG">DEBUG</option>
          <option value="INFO">INFO</option>
          <option value="WARN">WARN</option>
          <option value="ERROR">ERROR</option>
        </select>
      </div>
    </div>

    <div class="log-container" ref="logContainer">
      <div
        v-for="(log, index) in filteredLogs"
        :key="index"
        :class="['log-entry', `level-${log.level.toLowerCase()}`, { 'is-history': log.isHistory }]"
      >
        <span class="timestamp">{{ formatTimestamp(log.timestamp) }}</span>
        <span class="level">{{ log.level }}</span>
        <span class="target">{{ log.target }}</span>
        <span class="message markdown-content" v-html="renderMarkdown(log.message)"></span>
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
const logContainer = ref(null)
let eventSource = null
let historyLoaded = false
const seenLogIds = new Set() // Pour détecter les duplicatas

const filteredLogs = computed(() => {
  if (levelFilter.value === 'ALL') {
    return logs.value
  }
  return logs.value.filter(log => log.level === levelFilter.value)
})

function formatTimestamp(timestamp) {
  const date = new Date(timestamp.secs_since_epoch * 1000)
  return date.toLocaleTimeString('fr-FR', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    fractionalSecondDigits: 3
  })
}

function renderMarkdown(text) {
  // Convertir markdown en HTML et nettoyer pour la sécurité
  const rawHtml = marked.parse(text, { async: false })
  return DOMPurify.sanitize(rawHtml, {
    ALLOWED_TAGS: ['strong', 'em', 'code', 'pre', 'a', 'ul', 'ol', 'li', 'p', 'br'],
    ALLOWED_ATTR: ['href', 'target']
  })
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
  connectSSE()
})

onUnmounted(() => {
  if (eventSource) {
    eventSource.close()
  }
})

// Désactiver auto-scroll si l'utilisateur scroll manuellement
watch(logContainer, (container) => {
  if (!container) return
  
  container.addEventListener('scroll', () => {
    const isAtBottom = 
      container.scrollHeight - container.scrollTop <= container.clientHeight + 50
    
    if (!isAtBottom && autoScroll.value) {
      autoScroll.value = false
    }
  })
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
  display: grid;
  grid-template-columns: 130px 80px 200px 1fr;
  gap: 1rem;
  padding: 0.5rem;
  margin-bottom: 0.25rem;
  border-left: 3px solid transparent;
  font-size: 0.9rem;
  line-height: 1.4;
}

@media (max-width: 768px) {
  .log-entry {
    grid-template-columns: 1fr;
    gap: 0.3rem;
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

.timestamp {
  color: #858585;
  font-weight: 500;
}

@media (max-width: 768px) {
  .timestamp {
    font-size: 0.75rem;
    order: 1;
  }
}

.level {
  font-weight: bold;
  text-transform: uppercase;
  padding: 0.1rem 0.5rem;
  border-radius: 3px;
  text-align: center;
}

@media (max-width: 768px) {
  .level {
    order: 2;
    width: fit-content;
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
  }
}

.target {
  color: #4ec9b0;
  font-style: italic;
}

@media (max-width: 768px) {
  .target {
    order: 3;
    font-size: 0.8rem;
    color: #6eb8a5;
  }
}

.message {
  color: #d4d4d4;
  word-break: break-word;
}

@media (max-width: 768px) {
  .message {
    order: 4;
    margin-top: 0.25rem;
  }
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
  padding: 0.5rem;
  border-radius: 4px;
  overflow-x: auto;
  margin: 0.25rem 0;
}

.markdown-content :deep(pre code) {
  background: transparent;
  padding: 0;
  color: #d4d4d4;
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

/* Level colors */
.level-trace {
  border-left-color: #808080;
}

.level-trace .level {
  background: #3a3a3a;
  color: #a0a0a0;
}

.level-debug {
  border-left-color: #569cd6;
}

.level-debug .level {
  background: #1e3a5f;
  color: #569cd6;
}

.level-info {
  border-left-color: #4ec9b0;
}

.level-info .level {
  background: #1e4d42;
  color: #4ec9b0;
}

.level-warn {
  border-left-color: #dcdcaa;
}

.level-warn .level {
  background: #4d4d2a;
  color: #dcdcaa;
}

.level-error {
  border-left-color: #f48771;
}

.level-error .level {
  background: #5a1e1e;
  color: #f48771;
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