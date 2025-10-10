<template>
  <div class="variables-list">
    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
      <p>Loading variables...</p>
    </div>

    <div v-else-if="error" class="error-state">
      <span class="error-icon">⚠️</span>
      <p>{{ error }}</p>
      <button @click="loadVariables" class="retry-btn">Retry</button>
    </div>

    <div v-else-if="variables.length === 0" class="empty-state">
      <span class="empty-icon">📭</span>
      <p>No variables found for this service</p>
    </div>

    <div v-else class="variables-content">
      <div class="variables-header">
        <h4>State Variables ({{ variables.length }})</h4>
        <button @click="loadVariables" class="refresh-btn" :disabled="loading">
          🔄 Refresh
        </button>
      </div>

      <div class="variables-grid">
        <div
          v-for="variable in variables"
          :key="variable.name"
          class="variable-card"
          :class="{ 'has-events': variable.sends_events, 'has-value': variable.value }"
        >
          <div class="variable-header">
            <span class="variable-name">{{ variable.name }}</span>
            <div class="header-badges">
              <span v-if="variable.sends_events" class="event-badge" title="Sends events">
                🔔
              </span>
              <span class="type-badge" :title="variable.data_type">
                {{ variable.data_type }}
              </span>
            </div>
          </div>

          <div class="variable-details">
            <!-- Valeur actuelle - toujours affichée en premier -->
            <div class="variable-row current-value">
              <span class="variable-label">Current Value:</span>
              <div class="value-display">
                <code class="variable-value" :class="{ empty: !variable.value }">
                  {{ variable.value || '(empty)' }}
                </code>
                <button
                  v-if="variable.value"
                  @click="editingVar = editingVar === variable.name ? null : variable.name"
                  class="edit-btn"
                  title="Edit value"
                >
                  ✏️
                </button>
              </div>
            </div>

            <!-- Formulaire d'édition -->
            <div v-if="editingVar === variable.name" class="edit-form">
              <input
                v-model="editValue"
                :type="getInputType(variable.data_type)"
                :placeholder="`Enter ${variable.data_type} value`"
                class="edit-input"
                @keyup.enter="saveValue(variable)"
                @keyup.escape="editingVar = null"
              />
              <div class="edit-actions">
                <button @click="saveValue(variable)" class="save-btn">💾 Save</button>
                <button @click="editingVar = null" class="cancel-btn">✖ Cancel</button>
              </div>
            </div>

            <div v-if="variable.default_value" class="variable-row">
              <span class="variable-label">Default:</span>
              <code class="variable-value">{{ variable.default_value }}</code>
            </div>

            <div v-if="variable.allowed_values && variable.allowed_values.length > 0" class="variable-row">
              <span class="variable-label">Allowed:</span>
              <div class="allowed-values">
                <code
                  v-for="(value, idx) in variable.allowed_values"
                  :key="idx"
                  class="allowed-value"
                >
                  {{ value }}
                </code>
              </div>
            </div>

            <div v-if="variable.min || variable.max" class="variable-row">
              <span class="variable-label">Range:</span>
              <code class="variable-value">
                {{ variable.min ?? '−∞' }} → {{ variable.max ?? '+∞' }}
                <span v-if="variable.step"> (step: {{ variable.step }})</span>
              </code>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch } from 'vue'

const props = defineProps({
  deviceUdn: {
    type: String,
    required: true
  },
  serviceName: {
    type: String,
    required: true
  }
})

const variables = ref([])
const loading = ref(false)
const error = ref(null)
const editingVar = ref(null)
const editValue = ref('')

function getInputType(dataType) {
  if (dataType.includes('int') || dataType.includes('ui')) return 'number'
  if (dataType.includes('bool')) return 'checkbox'
  return 'text'
}

async function loadVariables() {
  if (!props.deviceUdn || !props.serviceName) return

  loading.value = true
  error.value = null

  try {
    const url = `/api/upnp/devices/${encodeURIComponent(props.deviceUdn)}/services/${encodeURIComponent(props.serviceName)}/variables`
    const response = await fetch(url)

    if (!response.ok) throw new Error(`HTTP ${response.status}`)

    const data = await response.json()
    variables.value = data.variables || []
  } catch (err) {
    error.value = err.message || 'Failed to load variables'
    console.error('Error loading variables:', err)
  } finally {
    loading.value = false
  }
}

async function saveValue(variable) {
  // TODO: Implement API call to update variable value
  console.log(`Saving ${variable.name} = ${editValue.value}`)
  editingVar.value = null
  editValue.value = ''
  // Refresh to get updated value
  await loadVariables()
}

// Load on mount
onMounted(() => {
  loadVariables()
})

// Reload when props change
watch(() => [props.deviceUdn, props.serviceName], () => {
  loadVariables()
})

// Set edit value when starting to edit
watch(editingVar, (newVar) => {
  if (newVar) {
    const variable = variables.value.find(v => v.name === newVar)
    if (variable) {
      editValue.value = variable.value || ''
    }
  }
})
</script>

<style scoped>
.variables-list {
  min-height: 200px;
  display: flex;
  flex-direction: column;
}

/* Loading state */
.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 3rem;
  color: #95a5a6;
}

.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid rgba(52, 152, 219, 0.3);
  border-top-color: #3498db;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin-bottom: 1rem;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Error state */
.error-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 3rem;
  color: #e74c3c;
}

.error-icon {
  font-size: 3rem;
  margin-bottom: 1rem;
}

.error-state p {
  margin: 0 0 1rem 0;
  color: #ecf0f1;
}

.retry-btn {
  padding: 0.5rem 1rem;
  background: #e74c3c;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.9rem;
  transition: all 0.2s;
}

.retry-btn:hover {
  background: #c0392b;
  transform: translateY(-2px);
}

/* Empty state */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 3rem;
  color: #95a5a6;
}

.empty-icon {
  font-size: 3rem;
  margin-bottom: 1rem;
  opacity: 0.5;
}

.empty-state p {
  margin: 0;
}

/* Variables content */
.variables-content {
  flex: 1;
}

.variables-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 6px;
  margin-bottom: 1rem;
}

.variables-header h4 {
  margin: 0;
  color: #ecf0f1;
  font-size: 1rem;
  font-weight: 600;
}

.refresh-btn {
  padding: 0.5rem 1rem;
  background: rgba(52, 152, 219, 0.2);
  color: #3498db;
  border: 1px solid rgba(52, 152, 219, 0.3);
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.85rem;
  transition: all 0.2s;
}

.refresh-btn:hover:not(:disabled) {
  background: rgba(52, 152, 219, 0.3);
  border-color: #3498db;
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Variables grid */
.variables-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 1rem;
}

.variable-card {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(52, 152, 219, 0.3);
  border-radius: 8px;
  padding: 1rem;
  transition: all 0.2s;
}

.variable-card:hover {
  border-color: rgba(52, 152, 219, 0.6);
  background: rgba(0, 0, 0, 0.4);
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.2);
}

.variable-card.has-events {
  border-color: rgba(46, 204, 113, 0.4);
}

.variable-card.has-events:hover {
  border-color: rgba(46, 204, 113, 0.7);
}

.variable-card.has-value {
  border-left: 3px solid #3498db;
}

.variable-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid rgba(52, 152, 219, 0.2);
}

.variable-name {
  font-weight: 600;
  color: #3498db;
  font-size: 0.95rem;
}

.header-badges {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.event-badge {
  font-size: 1rem;
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.type-badge {
  padding: 0.2rem 0.5rem;
  background: rgba(155, 89, 182, 0.2);
  border: 1px solid rgba(155, 89, 182, 0.3);
  border-radius: 4px;
  color: #9b59b6;
  font-size: 0.75rem;
  font-weight: 600;
  font-family: 'Courier New', monospace;
}

.variable-details {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.variable-row {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.variable-row.current-value {
  background: rgba(52, 152, 219, 0.1);
  padding: 0.5rem;
  border-radius: 4px;
  border-left: 3px solid #3498db;
}

.variable-label {
  font-size: 0.75rem;
  color: #95a5a6;
  text-transform: uppercase;
  font-weight: 600;
  letter-spacing: 0.5px;
}

.value-display {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.variable-value {
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
  color: #ecf0f1;
  background: rgba(0, 0, 0, 0.3);
  padding: 0.3rem 0.6rem;
  border-radius: 4px;
  flex: 1;
}

.variable-value.empty {
  color: #7f8c8d;
  font-style: italic;
}

.edit-btn {
  padding: 0.3rem 0.5rem;
  background: rgba(241, 196, 15, 0.2);
  border: 1px solid rgba(241, 196, 15, 0.3);
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
  transition: all 0.2s;
}

.edit-btn:hover {
  background: rgba(241, 196, 15, 0.3);
  border-color: #f1c40f;
}

/* Edit form */
.edit-form {
  background: rgba(241, 196, 15, 0.1);
  padding: 0.75rem;
  border-radius: 4px;
  border: 1px solid rgba(241, 196, 15, 0.3);
  margin-top: 0.5rem;
}

.edit-input {
  width: 100%;
  padding: 0.5rem;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(241, 196, 15, 0.3);
  border-radius: 4px;
  color: #ecf0f1;
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
  margin-bottom: 0.5rem;
}

.edit-input:focus {
  outline: none;
  border-color: #f1c40f;
  background: rgba(0, 0, 0, 0.4);
}

.edit-actions {
  display: flex;
  gap: 0.5rem;
}

.save-btn,
.cancel-btn {
  flex: 1;
  padding: 0.5rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.85rem;
  font-weight: 600;
  transition: all 0.2s;
}

.save-btn {
  background: #27ae60;
  color: white;
}

.save-btn:hover {
  background: #229954;
}

.cancel-btn {
  background: rgba(231, 76, 60, 0.2);
  color: #e74c3c;
  border: 1px solid rgba(231, 76, 60, 0.3);
}

.cancel-btn:hover {
  background: rgba(231, 76, 60, 0.3);
  border-color: #e74c3c;
}

.allowed-values {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.allowed-value {
  font-family: 'Courier New', monospace;
  font-size: 0.75rem;
  color: #2ecc71;
  background: rgba(46, 204, 113, 0.1);
  padding: 0.2rem 0.4rem;
  border-radius: 4px;
  border: 1px solid rgba(46, 204, 113, 0.3);
}

/* Responsive */
@media (max-width: 768px) {
  .variables-grid {
    grid-template-columns: 1fr;
  }
}
</style>
