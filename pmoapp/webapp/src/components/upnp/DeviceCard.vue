<template>
  <div
    class="device-card"
    :class="{ expanded: isExpanded }"
    @click="handleClick"
  >
    <div class="card-header">
      <div class="device-icon">
        {{ getDeviceIcon(device.device_type) }}
      </div>
      <div class="device-info">
        <h3 class="device-name">{{ device.friendly_name }}</h3>
        <p class="device-type">{{ formatDeviceType(device.device_type) }}</p>
      </div>
      <div class="expand-icon">
        {{ isExpanded ? '▼' : '▶' }}
      </div>
    </div>

    <div class="card-body">
      <div class="device-details">
        <div class="detail-item">
          <span class="detail-icon">🏷️</span>
          <span class="detail-label">Name:</span>
          <span class="detail-value">{{ device.name }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-icon">🏭</span>
          <span class="detail-label">Manufacturer:</span>
          <span class="detail-value">{{ device.manufacturer }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-icon">📦</span>
          <span class="detail-label">Model:</span>
          <span class="detail-value">{{ device.model_name }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-icon">🔗</span>
          <span class="detail-label">Base URL:</span>
          <a :href="device.base_url" target="_blank" class="detail-value link">
            {{ device.base_url }}
          </a>
        </div>
        <div class="detail-item udn">
          <span class="detail-icon">🆔</span>
          <span class="detail-label">UDN:</span>
          <code class="detail-value monospace">{{ device.udn }}</code>
        </div>
      </div>

      <div class="card-actions">
        <button
          @click.stop="$emit('load-details')"
          class="details-btn"
        >
          📋 View Services
        </button>
        <a
          :href="device.description_url"
          target="_blank"
          class="xml-btn"
          @click.stop
        >
          📄 Device XML
        </a>
      </div>
    </div>
  </div>
</template>

<script setup>
import { defineProps, defineEmits } from 'vue'

const props = defineProps({
  device: {
    type: Object,
    required: true
  },
  isExpanded: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['toggle', 'load-details'])

function handleClick() {
  emit('toggle')
}

function getDeviceIcon(deviceType) {
  if (deviceType.includes('MediaRenderer')) return '🎵'
  if (deviceType.includes('MediaServer')) return '💿'
  if (deviceType.includes('Display')) return '🖥️'
  return '📱'
}

function formatDeviceType(deviceType) {
  // Extraire le type simple depuis l'URN
  const match = deviceType.match(/device:([^:]+)/)
  return match ? match[1] : deviceType
}
</script>

<style scoped>
.device-card {
  background: linear-gradient(135deg, #2c3e50 0%, #34495e 100%);
  border-radius: 12px;
  border: 2px solid #3498db;
  overflow: hidden;
  transition: all 0.3s ease;
  cursor: pointer;
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
}

.device-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 8px 16px rgba(52, 152, 219, 0.4);
  border-color: #5dade2;
}

.device-card.expanded {
  border-color: #2ecc71;
}

.card-header {
  display: flex;
  align-items: center;
  padding: 1.25rem;
  gap: 1rem;
  background: rgba(0, 0, 0, 0.2);
}

.device-icon {
  font-size: 2.5rem;
  flex-shrink: 0;
  filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.3));
}

.device-info {
  flex: 1;
  min-width: 0;
}

.device-name {
  margin: 0 0 0.25rem 0;
  font-size: 1.2rem;
  font-weight: 600;
  color: #ecf0f1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.device-type {
  margin: 0;
  font-size: 0.85rem;
  color: #3498db;
  font-weight: 500;
}

.expand-icon {
  font-size: 1.2rem;
  color: #3498db;
  transition: transform 0.3s;
  flex-shrink: 0;
}

.device-card.expanded .expand-icon {
  transform: rotate(0deg);
}

.card-body {
  max-height: 0;
  overflow: hidden;
  transition: max-height 0.3s ease;
}

.device-card.expanded .card-body {
  max-height: 500px;
}

.device-details {
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.detail-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 6px;
  transition: background 0.2s;
}

.detail-item:hover {
  background: rgba(52, 152, 219, 0.1);
}

.detail-item.udn {
  flex-wrap: wrap;
}

.detail-icon {
  font-size: 1.1rem;
  flex-shrink: 0;
}

.detail-label {
  font-weight: 600;
  color: #95a5a6;
  min-width: 100px;
  flex-shrink: 0;
}

.detail-value {
  color: #ecf0f1;
  flex: 1;
  word-break: break-word;
}

.monospace {
  font-family: 'Courier New', monospace;
  font-size: 0.8rem;
  background: rgba(0, 0, 0, 0.3);
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
}

.link {
  color: #3498db;
  text-decoration: none;
  transition: color 0.2s;
}

.link:hover {
  color: #5dade2;
  text-decoration: underline;
}

.card-actions {
  display: flex;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
  background: rgba(0, 0, 0, 0.3);
  border-top: 1px solid rgba(52, 152, 219, 0.3);
}

.details-btn,
.xml-btn {
  flex: 1;
  padding: 0.75rem 1rem;
  border: none;
  border-radius: 6px;
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  text-decoration: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
}

.details-btn {
  background: #3498db;
  color: white;
}

.details-btn:hover {
  background: #2980b9;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(52, 152, 219, 0.3);
}

.xml-btn {
  background: #2ecc71;
  color: white;
}

.xml-btn:hover {
  background: #27ae60;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(46, 204, 113, 0.3);
}

/* Animation d'entrée */
@keyframes slideIn {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.device-card {
  animation: slideIn 0.3s ease-out;
}
</style>
