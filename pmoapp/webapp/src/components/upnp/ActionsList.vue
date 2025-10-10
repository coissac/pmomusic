<template>
  <div class="actions-list">
    <div v-if="!service.actions || service.actions.length === 0" class="empty-state">
      <span class="empty-icon">⚡</span>
      <p>No actions available for this service</p>
    </div>

    <div v-else class="actions-content">
      <div class="actions-header">
        <h4>Actions ({{ service.actions.length }})</h4>
      </div>

      <div class="actions-grid">
        <div
          v-for="action in service.actions"
          :key="action.name"
          class="action-card"
          :class="{ expanded: expandedAction === action.name }"
          @click="toggleAction(action.name)"
        >
          <div class="action-header">
            <div class="action-title">
              <span class="action-icon">⚡</span>
              <span class="action-name">{{ action.name }}</span>
            </div>
            <div class="action-badges">
              <span v-if="action.in_arguments.length > 0" class="badge in-badge" title="Input arguments">
                ➡️ {{ action.in_arguments.length }}
              </span>
              <span v-if="action.out_arguments.length > 0" class="badge out-badge" title="Output arguments">
                ⬅️ {{ action.out_arguments.length }}
              </span>
              <span class="expand-indicator">
                {{ expandedAction === action.name ? '▼' : '▶' }}
              </span>
            </div>
          </div>

          <transition name="expand-args">
            <div v-if="expandedAction === action.name" class="action-details">
              <!-- Input arguments -->
              <div v-if="action.in_arguments.length > 0" class="arguments-section">
                <h5 class="section-title">
                  <span class="section-icon">➡️</span>
                  Input Arguments
                </h5>
                <div class="arguments-list">
                  <div
                    v-for="arg in action.in_arguments"
                    :key="arg.name"
                    class="argument-item"
                  >
                    <div class="argument-header">
                      <span class="argument-name">{{ arg.name }}</span>
                      <span class="var-link" @click.stop="scrollToVariable(arg.related_state_variable)">
                        {{ arg.related_state_variable }}
                      </span>
                    </div>
                    <div v-if="getVariableInfo(arg.related_state_variable)" class="variable-preview">
                      <div class="preview-row">
                        <span class="preview-label">Type:</span>
                        <code class="preview-value type">{{ getVariableInfo(arg.related_state_variable).data_type }}</code>
                      </div>
                      <div class="preview-row">
                        <span class="preview-label">Value:</span>
                        <code class="preview-value" :class="{ empty: !getVariableInfo(arg.related_state_variable).value }">
                          {{ getVariableInfo(arg.related_state_variable).value || '(empty)' }}
                        </code>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Output arguments -->
              <div v-if="action.out_arguments.length > 0" class="arguments-section">
                <h5 class="section-title">
                  <span class="section-icon">⬅️</span>
                  Output Arguments
                </h5>
                <div class="arguments-list">
                  <div
                    v-for="arg in action.out_arguments"
                    :key="arg.name"
                    class="argument-item out"
                  >
                    <div class="argument-header">
                      <span class="argument-name">{{ arg.name }}</span>
                      <span class="var-link" @click.stop="scrollToVariable(arg.related_state_variable)">
                        {{ arg.related_state_variable }}
                      </span>
                    </div>
                    <div v-if="getVariableInfo(arg.related_state_variable)" class="variable-preview">
                      <div class="preview-row">
                        <span class="preview-label">Type:</span>
                        <code class="preview-value type">{{ getVariableInfo(arg.related_state_variable).data_type }}</code>
                      </div>
                      <div class="preview-row">
                        <span class="preview-label">Value:</span>
                        <code class="preview-value" :class="{ empty: !getVariableInfo(arg.related_state_variable).value }">
                          {{ getVariableInfo(arg.related_state_variable).value || '(empty)' }}
                        </code>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- No arguments -->
              <div v-if="action.in_arguments.length === 0 && action.out_arguments.length === 0" class="no-arguments">
                <span class="no-args-icon">∅</span>
                <p>This action has no arguments</p>
              </div>
            </div>
          </transition>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'

const props = defineProps({
  service: {
    type: Object,
    required: true
  },
  deviceUdn: {
    type: String,
    required: true
  }
})

const expandedAction = ref(null)
const variables = ref([])

function toggleAction(actionName) {
  expandedAction.value = expandedAction.value === actionName ? null : actionName
}

function getVariableInfo(varName) {
  return variables.value.find(v => v.name === varName)
}

function scrollToVariable(varName) {
  // TODO: Implement scroll to variable in Variables tab
  console.log('Scroll to variable:', varName)
}

async function loadVariables() {
  if (!props.deviceUdn || !props.service.name) return

  try {
    const url = `/api/upnp/devices/${encodeURIComponent(props.deviceUdn)}/services/${encodeURIComponent(props.service.name)}/variables`
    const response = await fetch(url)

    if (!response.ok) throw new Error(`HTTP ${response.status}`)

    const data = await response.json()
    variables.value = data.variables || []
  } catch (err) {
    console.error('Error loading variables for actions:', err)
  }
}

onMounted(() => {
  loadVariables()
})
</script>

<style scoped>
.actions-list {
  min-height: 200px;
  display: flex;
  flex-direction: column;
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

/* Actions content */
.actions-content {
  flex: 1;
}

.actions-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 6px;
  margin-bottom: 1rem;
}

.actions-header h4 {
  margin: 0;
  color: #ecf0f1;
  font-size: 1rem;
  font-weight: 600;
}

/* Actions grid */
.actions-grid {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.action-card {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(52, 152, 219, 0.3);
  border-radius: 8px;
  overflow: hidden;
  transition: all 0.2s;
  cursor: pointer;
}

.action-card:hover {
  border-color: rgba(52, 152, 219, 0.6);
  background: rgba(0, 0, 0, 0.4);
}

.action-card.expanded {
  border-color: #3498db;
  background: rgba(52, 152, 219, 0.05);
}

.action-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem;
  transition: background 0.2s;
}

.action-card:hover .action-header {
  background: rgba(52, 152, 219, 0.05);
}

.action-title {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
}

.action-icon {
  font-size: 1.2rem;
}

.action-name {
  font-weight: 600;
  color: #ecf0f1;
  font-size: 0.95rem;
}

.action-badges {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.badge {
  padding: 0.2rem 0.5rem;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 600;
}

.in-badge {
  background: rgba(52, 152, 219, 0.2);
  color: #3498db;
  border: 1px solid rgba(52, 152, 219, 0.3);
}

.out-badge {
  background: rgba(46, 204, 113, 0.2);
  color: #2ecc71;
  border: 1px solid rgba(46, 204, 113, 0.3);
}

.expand-indicator {
  color: #3498db;
  font-size: 0.9rem;
  transition: transform 0.3s;
  margin-left: 0.5rem;
}

.action-card.expanded .expand-indicator {
  transform: rotate(0deg);
}

/* Action details */
.action-details {
  padding: 0 1rem 1rem 1rem;
  border-top: 1px solid rgba(52, 152, 219, 0.2);
}

.arguments-section {
  margin-top: 1rem;
}

.arguments-section:first-child {
  margin-top: 0.5rem;
}

.section-title {
  margin: 0 0 0.75rem 0;
  font-size: 0.85rem;
  font-weight: 600;
  color: #95a5a6;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.section-icon {
  font-size: 1rem;
}

.arguments-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.argument-item {
  background: rgba(52, 152, 219, 0.1);
  border: 1px solid rgba(52, 152, 219, 0.2);
  border-left: 3px solid #3498db;
  border-radius: 4px;
  padding: 0.75rem;
}

.argument-item.out {
  background: rgba(46, 204, 113, 0.1);
  border: 1px solid rgba(46, 204, 113, 0.2);
  border-left: 3px solid #2ecc71;
}

.argument-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.argument-name {
  font-weight: 600;
  color: #ecf0f1;
  font-size: 0.9rem;
}

.var-link {
  font-size: 0.75rem;
  color: #9b59b6;
  background: rgba(155, 89, 182, 0.2);
  border: 1px solid rgba(155, 89, 182, 0.3);
  padding: 0.2rem 0.5rem;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s;
  font-family: 'Courier New', monospace;
}

.var-link:hover {
  background: rgba(155, 89, 182, 0.3);
  border-color: #9b59b6;
  transform: translateY(-1px);
}

/* Variable preview */
.variable-preview {
  background: rgba(0, 0, 0, 0.2);
  border-radius: 4px;
  padding: 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.preview-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.preview-label {
  font-size: 0.7rem;
  color: #7f8c8d;
  text-transform: uppercase;
  font-weight: 600;
  min-width: 50px;
}

.preview-value {
  font-family: 'Courier New', monospace;
  font-size: 0.8rem;
  color: #ecf0f1;
  background: rgba(0, 0, 0, 0.3);
  padding: 0.15rem 0.4rem;
  border-radius: 3px;
}

.preview-value.type {
  color: #9b59b6;
  background: rgba(155, 89, 182, 0.15);
}

.preview-value.empty {
  color: #7f8c8d;
  font-style: italic;
}

/* No arguments state */
.no-arguments {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2rem;
  color: #95a5a6;
}

.no-args-icon {
  font-size: 2rem;
  margin-bottom: 0.5rem;
  opacity: 0.5;
}

.no-arguments p {
  margin: 0;
  font-size: 0.9rem;
}

/* Expand animation */
.expand-args-enter-active,
.expand-args-leave-active {
  transition: all 0.3s ease;
  max-height: 1000px;
  overflow: hidden;
}

.expand-args-enter-from,
.expand-args-leave-to {
  max-height: 0;
  opacity: 0;
}

/* Responsive */
@media (max-width: 768px) {
  .action-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .action-badges {
    width: 100%;
    justify-content: flex-end;
  }

  .argument-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
  }
}
</style>
