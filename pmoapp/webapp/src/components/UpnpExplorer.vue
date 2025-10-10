<template>
  <div class="upnp-explorer">
    <div class="header">
      <h2>🎵 UPnP Device Explorer</h2>
      <div class="controls">
        <button @click="refreshDevices" :disabled="isLoading" class="refresh-btn">
          {{ isLoading ? '⏳ Loading...' : '🔄 Refresh' }}
        </button>
        <span class="device-count">{{ devices.length }} device(s)</span>
      </div>
    </div>

    <!-- État de chargement -->
    <div v-if="isLoading && devices.length === 0" class="loading-state">
      ⏳ Loading UPnP devices...
    </div>

    <!-- État vide -->
    <div v-else-if="!isLoading && devices.length === 0" class="empty-state">
      <div class="empty-icon">📡</div>
      <p>No UPnP devices found</p>
      <p class="hint">Devices will appear here once registered</p>
    </div>

    <!-- Liste des devices avec leurs services intégrés -->
    <div v-else class="devices-list">
      <div
        v-for="device in devicesWithDetails"
        :key="device.udn"
        class="device-section"
      >
        <!-- En-tête du device -->
        <div class="device-header" @click="toggleDevice(device.udn)">
          <div class="device-title">
            <span class="device-icon">{{ getDeviceIcon(device.device_type) }}</span>
            <div class="device-names">
              <span class="device-name">{{ device.friendly_name }}</span>
              <span class="device-type">{{ device.name }}</span>
            </div>
          </div>
          <div class="device-meta">
            <span v-if="device.services" class="service-count">
              {{ device.services.length }} service(s)
            </span>
            <span class="expand-icon">{{ expandedDevices.has(device.udn) ? '▼' : '▶' }}</span>
          </div>
        </div>

        <!-- Détails du device (expandable) -->
        <transition name="expand">
          <div v-if="expandedDevices.has(device.udn)" class="device-details">
            <!-- Chargement des détails -->
            <div v-if="!device.services" class="loading-services">
              ⏳ Loading services...
            </div>

            <!-- Services -->
            <div v-else class="services-list">
              <ServicePanel
                v-for="service in device.services"
                :key="service.name"
                :service="service"
                :device-udn="device.udn"
              />
            </div>
          </div>
        </transition>
      </div>
    </div>

    <!-- Toast de notification d'erreur -->
    <transition name="fade">
      <div v-if="error" class="error-toast" @click="error = null">
        ❌ {{ error }}
      </div>
    </transition>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import ServicePanel from './upnp/ServicePanel.vue'

const devices = ref([])
const deviceDetails = ref(new Map()) // UDN -> détails complets
const isLoading = ref(false)
const error = ref(null)
const expandedDevices = ref(new Set())
const refreshInterval = ref(null)

// Devices avec leurs détails fusionnés
const devicesWithDetails = computed(() => {
  return devices.value.map(device => {
    const details = deviceDetails.value.get(device.udn)
    return details ? { ...device, ...details } : device
  })
})

function getDeviceIcon(deviceType) {
  if (deviceType?.includes('MediaRenderer')) return '🎵'
  if (deviceType?.includes('MediaServer')) return '💿'
  return '📱'
}

async function loadDevices() {
  try {
    const response = await fetch('/api/upnp/devices')
    if (!response.ok) throw new Error(`HTTP ${response.status}`)

    const data = await response.json()
    devices.value = data.devices || []
  } catch (err) {
    console.error('Failed to load devices:', err)
    error.value = `Failed to load devices: ${err.message}`
    setTimeout(() => error.value = null, 5000)
  }
}

async function loadDeviceDetails(udn) {
  try {
    const response = await fetch(`/api/upnp/devices/${encodeURIComponent(udn)}`)
    if (!response.ok) throw new Error(`HTTP ${response.status}`)

    const details = await response.json()
    deviceDetails.value.set(udn, details)
  } catch (err) {
    console.error('Failed to load device details:', err)
    error.value = `Failed to load device details: ${err.message}`
    setTimeout(() => error.value = null, 5000)
  }
}

function toggleDevice(udn) {
  if (expandedDevices.value.has(udn)) {
    expandedDevices.value.delete(udn)
  } else {
    expandedDevices.value.add(udn)
    // Charger les détails si pas encore fait
    if (!deviceDetails.value.has(udn)) {
      loadDeviceDetails(udn)
    }
  }
}

async function refreshDevices() {
  isLoading.value = true
  await loadDevices()
  isLoading.value = false
}

// Auto-refresh toutes les 30 secondes
onMounted(() => {
  refreshDevices()
  refreshInterval.value = setInterval(loadDevices, 30000)
})

onUnmounted(() => {
  if (refreshInterval.value) {
    clearInterval(refreshInterval.value)
  }
})
</script>

<style scoped>
.upnp-explorer {
  padding: 1rem;
  width: 100%;
  max-width: 100%;
  margin: 0;
  box-sizing: border-box;
}

@media (min-width: 1400px) {
  .upnp-explorer {
    padding: 2rem;
    max-width: 1400px;
    margin: 0 auto;
  }
}

@media (max-width: 768px) {
  .upnp-explorer {
    padding: 0.5rem;
  }
}

/* Header */
.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 2rem;
  padding-bottom: 1rem;
  border-bottom: 2px solid rgba(52, 152, 219, 0.3);
}

.header h2 {
  margin: 0;
  color: #ecf0f1;
  font-size: 1.8rem;
}

.controls {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.refresh-btn {
  padding: 0.6rem 1.2rem;
  background: linear-gradient(135deg, #3498db, #2980b9);
  color: white;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 1rem;
  font-weight: 500;
  transition: all 0.3s;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
}

.refresh-btn:hover:not(:disabled) {
  background: linear-gradient(135deg, #5dade2, #3498db);
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
}

.refresh-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.device-count {
  padding: 0.5rem 1rem;
  background: rgba(52, 152, 219, 0.2);
  border: 1px solid rgba(52, 152, 219, 0.4);
  border-radius: 20px;
  color: #3498db;
  font-size: 0.9rem;
  font-weight: 600;
}

/* États */
.loading-state,
.empty-state {
  text-align: center;
  padding: 4rem 2rem;
  color: #95a5a6;
}

.empty-icon {
  font-size: 4rem;
  margin-bottom: 1rem;
  opacity: 0.5;
}

.hint {
  font-size: 0.9rem;
  color: #7f8c8d;
}

/* Devices list */
.devices-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.device-section {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(52, 152, 219, 0.3);
  border-radius: 12px;
  overflow: hidden;
  transition: all 0.3s;
}

.device-section:hover {
  border-color: rgba(52, 152, 219, 0.6);
  box-shadow: 0 4px 12px rgba(52, 152, 219, 0.2);
}

.device-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1.2rem 1.5rem;
  cursor: pointer;
  transition: background 0.2s;
}

.device-header:hover {
  background: rgba(52, 152, 219, 0.05);
}

.device-title {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.device-icon {
  font-size: 2rem;
}

.device-names {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.device-name {
  font-size: 1.2rem;
  font-weight: 600;
  color: #ecf0f1;
}

.device-type {
  font-size: 0.85rem;
  color: #95a5a6;
}

.device-meta {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.service-count {
  padding: 0.3rem 0.8rem;
  background: rgba(46, 204, 113, 0.2);
  border: 1px solid rgba(46, 204, 113, 0.3);
  border-radius: 12px;
  color: #2ecc71;
  font-size: 0.85rem;
  font-weight: 600;
}

.expand-icon {
  color: #3498db;
  font-size: 1rem;
  transition: transform 0.3s;
}

/* Device details */
.device-details {
  padding: 0 1.5rem 1.5rem 1.5rem;
  border-top: 1px solid rgba(52, 152, 219, 0.2);
}

.loading-services {
  padding: 2rem;
  text-align: center;
  color: #95a5a6;
}

.services-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-top: 1rem;
}

/* Transitions */
.expand-enter-active,
.expand-leave-active {
  transition: all 0.3s ease;
  max-height: 5000px;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* Error toast */
.error-toast {
  position: fixed;
  bottom: 2rem;
  right: 2rem;
  background: linear-gradient(135deg, #e74c3c, #c0392b);
  color: white;
  padding: 1rem 1.5rem;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  cursor: pointer;
  z-index: 1000;
  max-width: 400px;
  animation: slideIn 0.3s ease;
}

@keyframes slideIn {
  from {
    transform: translateX(100%);
    opacity: 0;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

/* Responsive */
@media (max-width: 768px) {
  .upnp-explorer {
    padding: 1rem;
  }

  .header {
    flex-direction: column;
    align-items: flex-start;
    gap: 1rem;
  }

  .device-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .device-meta {
    width: 100%;
    justify-content: space-between;
  }
}
</style>
