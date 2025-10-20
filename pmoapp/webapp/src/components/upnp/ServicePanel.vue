<template>
  <div class="service-panel" :class="{ expanded: isExpanded }">
    <div class="service-header" @click="toggleExpand">
      <div class="service-icon">🔧</div>
      <div class="service-info">
        <h4 class="service-name">{{ service.name }}</h4>
        <p class="service-type">{{ formatServiceType(service.service_type) }}</p>
      </div>
      <div class="service-badge">
        {{ isExpanded ? '▼' : '▶' }}
      </div>
    </div>

    <transition name="expand">
      <div v-if="isExpanded" class="service-content">
        <!-- URLs du service -->
        <div class="service-urls">
          <div class="url-item">
            <span class="url-label">Control:</span>
            <a :href="service.control_url" target="_blank" class="url-value">
              {{ service.control_url }}
            </a>
          </div>
          <div class="url-item">
            <span class="url-label">Events:</span>
            <a :href="service.event_url" target="_blank" class="url-value">
              {{ service.event_url }}
            </a>
          </div>
          <div class="url-item">
            <span class="url-label">SCPD:</span>
            <a :href="service.scpd_url" target="_blank" class="url-value">
              {{ service.scpd_url }}
            </a>
          </div>
        </div>

        <!-- Onglets pour Variables / Actions -->
        <div class="tabs">
          <button
            :class="['tab', { active: activeTab === 'variables' }]"
            @click="activeTab = 'variables'"
          >
            📊 Variables
            <span class="badge">{{ variablesCount }}</span>
          </button>
          <button
            :class="['tab', { active: activeTab === 'actions' }]"
            @click="activeTab = 'actions'"
          >
            ⚡ Actions
            <span class="badge">{{ actionsCount }}</span>
          </button>
        </div>

        <!-- Contenu des onglets -->
        <div class="tab-content">
          <VariablesList
            v-if="activeTab === 'variables'"
            :device-udn="deviceUdn"
            :service-name="service.name"
          />
          <ActionsList
            v-else
            :service="service"
            :device-udn="deviceUdn"
          />
        </div>
      </div>
    </transition>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue'
import VariablesList from './VariablesList.vue'
import ActionsList from './ActionsList.vue'

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

const isExpanded = ref(false)
const activeTab = ref('variables')

const variablesCount = computed(() => {
  // Sera mis à jour dynamiquement par VariablesList
  return '...'
})

const actionsCount = computed(() => {
  return '...'
})

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}

function formatServiceType(serviceType) {
  const match = serviceType.match(/service:([^:]+)/)
  return match ? match[1] : serviceType
}
</script>

<style scoped>
.service-panel {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(52, 152, 219, 0.3);
  border-radius: 8px;
  margin-bottom: 1rem;
  overflow: hidden;
  transition: all 0.3s;
}

.service-panel:hover {
  border-color: rgba(52, 152, 219, 0.6);
  box-shadow: 0 2px 8px rgba(52, 152, 219, 0.2);
}

.service-panel.expanded {
  border-color: #3498db;
}

.service-header {
  display: flex;
  align-items: center;
  padding: 1rem;
  cursor: pointer;
  gap: 0.75rem;
  transition: background 0.2s;
}

.service-header:hover {
  background: rgba(52, 152, 219, 0.1);
}

.service-icon {
  font-size: 1.5rem;
  flex-shrink: 0;
}

.service-info {
  flex: 1;
  min-width: 0;
}

.service-name {
  margin: 0 0 0.25rem 0;
  font-size: 1rem;
  font-weight: 600;
  color: #ecf0f1;
}

.service-type {
  margin: 0;
  font-size: 0.8rem;
  color: #7f8c8d;
}

.service-badge {
  color: #3498db;
  font-size: 1rem;
  transition: transform 0.3s;
  flex-shrink: 0;
}

.service-content {
  padding: 0 1rem 1rem 1rem;
}

.service-urls {
  background: rgba(0, 0, 0, 0.2);
  border-radius: 6px;
  padding: 0.75rem;
  margin-bottom: 1rem;
}

.url-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  margin-bottom: 0.5rem;
}

.url-item:last-child {
  margin-bottom: 0;
}

.url-label {
  font-weight: 600;
  color: #95a5a6;
  min-width: 80px;
  font-size: 0.85rem;
}

.url-value {
  color: #3498db;
  text-decoration: none;
  font-size: 0.85rem;
  word-break: break-all;
}

.url-value:hover {
  text-decoration: underline;
  color: #5dade2;
}

.tabs {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
  border-bottom: 2px solid rgba(52, 152, 219, 0.2);
}

.tab {
  flex: 1;
  padding: 0.75rem 1rem;
  background: transparent;
  border: none;
  color: #95a5a6;
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  border-bottom: 3px solid transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
}

.tab:hover {
  background: rgba(52, 152, 219, 0.1);
  color: #ecf0f1;
}

.tab.active {
  color: #3498db;
  border-bottom-color: #3498db;
  background: rgba(52, 152, 219, 0.05);
}

.badge {
  background: rgba(52, 152, 219, 0.3);
  padding: 0.2rem 0.5rem;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 600;
}

.tab.active .badge {
  background: #3498db;
  color: white;
}

.tab-content {
  min-height: 200px;
}

/* Animations */
.expand-enter-active,
.expand-leave-active {
  transition: all 0.3s ease;
  max-height: 1000px;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
}
</style>
