<template>
  <div class="api-dashboard">
    <div class="header">
      <h1>API Dashboard</h1>
      <p class="subtitle">Vue d'ensemble des APIs disponibles dans PMOMusic</p>
    </div>

    <div v-if="loading" class="loading">Chargement des APIs...</div>
    <div v-else-if="error" class="error">{{ error }}</div>

    <div v-else class="dashboard-content">
      <!-- Stats globales -->
      <div class="stats-section">
        <div class="stat-card">
          <div class="stat-icon">🚀</div>
          <div class="stat-info">
            <div class="stat-value">{{ registry?.apis.length || 0 }}</div>
            <div class="stat-label">APIs disponibles</div>
          </div>
        </div>
        <div class="stat-card">
          <div class="stat-icon">🔌</div>
          <div class="stat-info">
            <div class="stat-value">{{ registry?.total_endpoints || 0 }}</div>
            <div class="stat-label">Endpoints totaux</div>
          </div>
        </div>
      </div>

      <!-- Liste des APIs -->
      <div class="apis-grid">
        <div
          v-for="api in registry?.apis"
          :key="api.name"
          class="api-card"
        >
          <div class="api-header">
            <div class="api-icon">{{ getApiIcon(api.name) }}</div>
            <div class="api-title-section">
              <h3>{{ api.title }}</h3>
              <p class="api-name">{{ api.name }}</p>
            </div>
            <div class="api-version">v{{ api.version }}</div>
          </div>

          <div class="api-body">
            <p v-if="api.description" class="api-description">
              {{ api.description }}
            </p>
            <p v-else class="api-description empty">Aucune description disponible</p>

            <div class="api-stats">
              <div class="api-stat">
                <span class="stat-icon">📍</span>
                <span class="stat-text">{{ api.endpoint_count }} endpoints</span>
              </div>
              <div class="api-stat">
                <span class="stat-icon">🔗</span>
                <span class="stat-text">{{ api.path }}</span>
              </div>
            </div>
          </div>

          <div class="api-footer">
            <a :href="api.swagger_ui_path" target="_blank" class="btn-swagger">
              <span class="btn-icon">📖</span>
              <span>Documentation Swagger</span>
            </a>
            <a :href="api.openapi_json_path" target="_blank" class="btn-json">
              <span class="btn-icon">📄</span>
              <span>Spec OpenAPI</span>
            </a>
          </div>
        </div>
      </div>

      <!-- Message si aucune API -->
      <div v-if="!registry?.apis || registry.apis.length === 0" class="empty-state">
        <div class="empty-icon">🔍</div>
        <h3>Aucune API enregistrée</h3>
        <p>Les APIs seront affichées ici au fur et à mesure de leur enregistrement.</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';

interface ApiRegistryEntry {
  name: string;
  path: string;
  swagger_ui_path: string;
  openapi_json_path: string;
  endpoint_count: number;
  version: string;
  description?: string;
  title: string;
}

interface ApiRegistry {
  apis: ApiRegistryEntry[];
  total_endpoints: number;
}

const loading = ref(true);
const error = ref<string | null>(null);
const registry = ref<ApiRegistry | null>(null);

async function fetchRegistry() {
  try {
    loading.value = true;
    error.value = null;

    const response = await fetch('/api/registry');

    if (!response.ok) {
      throw new Error(`Failed to fetch API registry: ${response.statusText}`);
    }

    registry.value = await response.json();
  } catch (e: any) {
    error.value = e.message || 'Failed to load API registry';
    console.error('Error fetching API registry:', e);
  } finally {
    loading.value = false;
  }
}

function getApiIcon(name: string): string {
  const icons: Record<string, string> = {
    covers: '🎨',
    audio: '🎵',
    sources: '📡',
    upnp: '🔌',
    devices: '📱',
    cache: '💾',
    mediaserver: '🎬',
    renderer: '🎭',
  };

  return icons[name.toLowerCase()] || '🔧';
}

onMounted(() => {
  fetchRegistry();
});
</script>

<style scoped>
.api-dashboard {
  padding: 2rem;
  max-width: 1400px;
  margin: 0 auto;
  min-height: 100vh;
  background: linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%);
}

.header {
  text-align: center;
  margin-bottom: 3rem;
}

.header h1 {
  font-size: 2.5rem;
  margin-bottom: 0.5rem;
  color: #2c3e50;
  font-weight: 700;
}

.subtitle {
  font-size: 1.1rem;
  color: #7f8c8d;
  margin: 0;
}

.loading,
.error {
  padding: 3rem;
  text-align: center;
  font-size: 1.2rem;
  background: white;
  border-radius: 12px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
}

.error {
  color: #e74c3c;
  background: #fff5f5;
  border: 2px solid #fc8181;
}

.dashboard-content {
  animation: fadeIn 0.5s ease-in;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* Stats Section */
.stats-section {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 1.5rem;
  margin-bottom: 2rem;
}

.stat-card {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  padding: 2rem;
  border-radius: 12px;
  display: flex;
  align-items: center;
  gap: 1.5rem;
  box-shadow: 0 8px 16px rgba(102, 126, 234, 0.3);
  transition: transform 0.3s ease, box-shadow 0.3s ease;
}

.stat-card:hover {
  transform: translateY(-5px);
  box-shadow: 0 12px 24px rgba(102, 126, 234, 0.4);
}

.stat-icon {
  font-size: 3rem;
}

.stat-info {
  flex: 1;
}

.stat-value {
  font-size: 2.5rem;
  font-weight: 700;
  line-height: 1;
  margin-bottom: 0.5rem;
}

.stat-label {
  font-size: 1rem;
  opacity: 0.9;
  font-weight: 500;
}

/* APIs Grid */
.apis-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
  gap: 1.5rem;
  margin-bottom: 2rem;
}

.api-card {
  background: white;
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  transition: transform 0.3s ease, box-shadow 0.3s ease;
  display: flex;
  flex-direction: column;
}

.api-card:hover {
  transform: translateY(-5px);
  box-shadow: 0 12px 24px rgba(0, 0, 0, 0.15);
}

.api-header {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  padding: 1.5rem;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.api-icon {
  font-size: 2.5rem;
  line-height: 1;
}

.api-title-section {
  flex: 1;
}

.api-title-section h3 {
  margin: 0 0 0.25rem 0;
  font-size: 1.3rem;
  font-weight: 600;
}

.api-name {
  margin: 0;
  font-size: 0.9rem;
  opacity: 0.9;
  font-family: 'Courier New', monospace;
}

.api-version {
  background: rgba(255, 255, 255, 0.2);
  padding: 0.25rem 0.75rem;
  border-radius: 20px;
  font-size: 0.85rem;
  font-weight: 600;
}

.api-body {
  padding: 1.5rem;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.api-description {
  color: #4a5568;
  line-height: 1.6;
  margin: 0;
}

.api-description.empty {
  color: #a0aec0;
  font-style: italic;
}

.api-stats {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  margin-top: auto;
}

.api-stat {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.95rem;
  color: #718096;
}

.api-stat .stat-icon {
  font-size: 1.2rem;
}

.api-stat .stat-text {
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
}

.api-footer {
  display: grid;
  grid-template-columns: 1fr 1fr;
  border-top: 1px solid #e2e8f0;
}

.btn-swagger,
.btn-json {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  padding: 1rem;
  text-decoration: none;
  font-weight: 600;
  transition: background 0.2s ease;
  color: #667eea;
  font-size: 0.9rem;
}

.btn-swagger {
  border-right: 1px solid #e2e8f0;
}

.btn-swagger:hover {
  background: #f7fafc;
  color: #5a67d8;
}

.btn-json {
  color: #48bb78;
}

.btn-json:hover {
  background: #f7fafc;
  color: #38a169;
}

.btn-icon {
  font-size: 1.2rem;
}

/* Empty State */
.empty-state {
  text-align: center;
  padding: 4rem 2rem;
  background: white;
  border-radius: 12px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
}

.empty-icon {
  font-size: 4rem;
  margin-bottom: 1rem;
}

.empty-state h3 {
  color: #2c3e50;
  font-size: 1.5rem;
  margin: 0 0 0.5rem 0;
}

.empty-state p {
  color: #7f8c8d;
  margin: 0;
}

/* Responsive */
@media (max-width: 768px) {
  .api-dashboard {
    padding: 1rem;
  }

  .header h1 {
    font-size: 2rem;
  }

  .apis-grid {
    grid-template-columns: 1fr;
  }

  .api-footer {
    grid-template-columns: 1fr;
  }

  .btn-swagger {
    border-right: none;
    border-bottom: 1px solid #e2e8f0;
  }
}
</style>
