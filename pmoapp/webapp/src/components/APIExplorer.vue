<template>
  <div class="api-explorer">
    <h1>API Explorer</h1>

    <div v-if="loading" class="loading">Chargement de la documentation API...</div>
    <div v-else-if="error" class="error">{{ error }}</div>

    <div v-else class="explorer-content">
      <!-- API Info Section -->
      <section class="api-info" v-if="apiDoc">
        <h2>{{ apiDoc.info?.title || 'API Documentation' }}</h2>
        <p v-if="apiDoc.info?.description" class="description">
          {{ apiDoc.info.description }}
        </p>
        <div class="metadata">
          <span v-if="apiDoc.info?.version" class="version">Version: {{ apiDoc.info.version }}</span>
        </div>
      </section>

      <!-- Endpoints List -->
      <section class="endpoints" v-if="endpoints.length > 0">
        <h2>Endpoints ({{ endpoints.length }})</h2>

        <div
          v-for="endpoint in endpoints"
          :key="endpoint.path + endpoint.method"
          class="endpoint-card"
          :class="{ expanded: expandedEndpoint === endpoint.path + endpoint.method }"
        >
          <div
            class="endpoint-header"
            @click="toggleEndpoint(endpoint)"
          >
            <span class="method" :class="endpoint.method.toLowerCase()">
              {{ endpoint.method.toUpperCase() }}
            </span>
            <span class="path">{{ endpoint.path }}</span>
            <span class="summary" v-if="endpoint.summary">{{ endpoint.summary }}</span>
            <span class="arrow">{{ expandedEndpoint === endpoint.path + endpoint.method ? '▼' : '▶' }}</span>
          </div>

          <div v-show="expandedEndpoint === endpoint.path + endpoint.method" class="endpoint-details">
            <!-- Description -->
            <div v-if="endpoint.description" class="description">
              {{ endpoint.description }}
            </div>

            <!-- Tags -->
            <div v-if="endpoint.tags && endpoint.tags.length > 0" class="tags">
              <span v-for="tag in endpoint.tags" :key="tag" class="tag">{{ tag }}</span>
            </div>

            <!-- Parameters -->
            <div v-if="endpoint.parameters && endpoint.parameters.length > 0" class="parameters">
              <h4>Paramètres</h4>
              <table>
                <thead>
                  <tr>
                    <th>Nom</th>
                    <th>Type</th>
                    <th>Localisation</th>
                    <th>Requis</th>
                    <th>Description</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="param in endpoint.parameters" :key="param.name">
                    <td><code>{{ param.name }}</code></td>
                    <td>{{ param.schema?.type || 'unknown' }}</td>
                    <td><span class="param-in">{{ param.in }}</span></td>
                    <td>{{ param.required ? '✓' : '' }}</td>
                    <td>{{ param.description || '-' }}</td>
                  </tr>
                </tbody>
              </table>
            </div>

            <!-- Request Body -->
            <div v-if="endpoint.requestBody" class="request-body">
              <h4>Corps de la requête</h4>
              <div class="schema-viewer">
                <pre>{{ formatSchema(endpoint.requestBody) }}</pre>
              </div>
            </div>

            <!-- Responses -->
            <div v-if="endpoint.responses" class="responses">
              <h4>Réponses</h4>
              <div v-for="(response, status) in endpoint.responses" :key="status" class="response-item">
                <div class="response-status" :class="getStatusClass(status)">
                  {{ status }} - {{ response.description }}
                </div>
                <div v-if="response.content" class="response-schema">
                  <pre>{{ formatResponseSchema(response) }}</pre>
                </div>
              </div>
            </div>

            <!-- Test Section -->
            <div class="test-section">
              <h4>Tester cet endpoint</h4>

              <!-- Path Parameters -->
              <div v-if="endpoint.parameters && endpoint.parameters.some(p => p.in === 'path')" class="test-params">
                <label>Paramètres de chemin:</label>
                <div v-for="param in endpoint.parameters.filter(p => p.in === 'path')" :key="param.name" class="param-input">
                  <label>{{ param.name }}:</label>
                  <input
                    v-model="testParams[endpoint.path + endpoint.method]![param.name]"
                    type="text"
                    :placeholder="param.description || param.name"
                  />
                </div>
              </div>

              <!-- Query Parameters -->
              <div v-if="endpoint.parameters && endpoint.parameters.some(p => p.in === 'query')" class="test-params">
                <label>Paramètres de requête:</label>
                <div v-for="param in endpoint.parameters.filter(p => p.in === 'query')" :key="param.name" class="param-input">
                  <label>{{ param.name }}:</label>
                  <input
                    v-model="testParams[endpoint.path + endpoint.method]![param.name]"
                    type="text"
                    :placeholder="param.description || param.name"
                  />
                </div>
              </div>

              <!-- Request Body -->
              <div v-if="endpoint.requestBody" class="test-body">
                <label>Corps de la requête (JSON):</label>
                <textarea
                  v-model="testBodies[endpoint.path + endpoint.method]"
                  rows="6"
                  placeholder='{"key": "value"}'
                ></textarea>
              </div>

              <div class="test-actions">
                <button @click="executeRequest(endpoint)" class="btn-test" :disabled="testing">
                  {{ testing ? 'Envoi...' : 'Envoyer la requête' }}
                </button>
                <button @click="copyCurl(endpoint)" class="btn-copy">
                  Copier cURL
                </button>
              </div>

              <!-- Test Result -->
              <div v-if="testResults[endpoint.path + endpoint.method]" class="test-result">
                <div class="result-header">
                  <strong>Résultat:</strong>
                  <span
                    class="result-status"
                    :class="getStatusClass(testResults[endpoint.path + endpoint.method]!.status)"
                  >
                    {{ testResults[endpoint.path + endpoint.method]!.status }}
                  </span>
                </div>
                <pre class="result-body">{{ testResults[endpoint.path + endpoint.method]!.body }}</pre>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Schemas Section -->
      <section class="schemas" v-if="schemas && Object.keys(schemas).length > 0">
        <h2>Schémas ({{ Object.keys(schemas).length }})</h2>
        <div
          v-for="(schema, name) in schemas"
          :key="name"
          class="schema-card"
          :class="{ expanded: expandedSchema === name }"
        >
          <div
            class="schema-header"
            @click="toggleSchema(name)"
          >
            <span class="schema-name">{{ name }}</span>
            <span class="arrow">{{ expandedSchema === name ? '▼' : '▶' }}</span>
          </div>
          <div v-show="expandedSchema === name" class="schema-details">
            <pre>{{ JSON.stringify(schema, null, 2) }}</pre>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';

interface OpenAPIDoc {
  openapi?: string;
  info?: {
    title?: string;
    description?: string;
    version?: string;
  };
  paths?: Record<string, any>;
  components?: {
    schemas?: Record<string, any>;
  };
}

interface Endpoint {
  path: string;
  method: string;
  summary?: string;
  description?: string;
  tags?: string[];
  parameters?: any[];
  requestBody?: any;
  responses?: Record<string, any>;
}

const loading = ref(true);
const error = ref<string | null>(null);
const apiDoc = ref<OpenAPIDoc | null>(null);
const expandedEndpoint = ref<string | null>(null);
const expandedSchema = ref<string | null>(null);
const testing = ref(false);
const testParams = ref<Record<string, Record<string, string>>>({});
const testBodies = ref<Record<string, string>>({});
const testResults = ref<Record<string, { status: number; body: string }>>({});

const endpoints = computed<Endpoint[]>(() => {
  if (!apiDoc.value?.paths) return [];

  const result: Endpoint[] = [];
  for (const [path, pathItem] of Object.entries(apiDoc.value.paths)) {
    for (const [method, operation] of Object.entries(pathItem)) {
      if (['get', 'post', 'put', 'delete', 'patch'].includes(method)) {
        const endpointKey = path + method;
        if (!testParams.value[endpointKey]) {
          testParams.value[endpointKey] = {};
        }
        if (!testBodies.value[endpointKey]) {
          testBodies.value[endpointKey] = '';
        }

        result.push({
          path,
          method,
          ...(operation as any),
        });
      }
    }
  }
  return result;
});

const schemas = computed(() => {
  return apiDoc.value?.components?.schemas || {};
});

async function fetchAPIDoc() {
  try {
    loading.value = true;
    error.value = null;

    // Fetch the OpenAPI documentation from the server
    const response = await fetch('/api-docs/sources.json');

    if (!response.ok) {
      throw new Error(`Failed to fetch API documentation: ${response.statusText}`);
    }

    apiDoc.value = await response.json();
  } catch (e: any) {
    error.value = e.message || 'Failed to load API documentation';
    console.error('Error fetching API doc:', e);
  } finally {
    loading.value = false;
  }
}

function toggleEndpoint(endpoint: Endpoint) {
  const key = endpoint.path + endpoint.method;
  expandedEndpoint.value = expandedEndpoint.value === key ? null : key;
}

function toggleSchema(name: string) {
  expandedSchema.value = expandedSchema.value === name ? null : name;
}

function formatSchema(requestBody: any): string {
  try {
    if (requestBody.content) {
      const content = requestBody.content['application/json'];
      if (content?.schema) {
        return JSON.stringify(content.schema, null, 2);
      }
    }
    return JSON.stringify(requestBody, null, 2);
  } catch {
    return String(requestBody);
  }
}

function formatResponseSchema(response: any): string {
  try {
    if (response.content) {
      const content = response.content['application/json'];
      if (content?.schema) {
        return JSON.stringify(content.schema, null, 2);
      }
    }
    return JSON.stringify(response, null, 2);
  } catch {
    return String(response);
  }
}

function getStatusClass(status: string | number): string {
  const code = typeof status === 'string' ? parseInt(status) : status;
  if (code >= 200 && code < 300) return 'success';
  if (code >= 300 && code < 400) return 'redirect';
  if (code >= 400 && code < 500) return 'client-error';
  if (code >= 500) return 'server-error';
  return '';
}

function buildRequestUrl(endpoint: Endpoint): string {
  let url = endpoint.path;
  const key = endpoint.path + endpoint.method;
  const params = testParams.value[key] || {};

  // Replace path parameters
  for (const [name, value] of Object.entries(params)) {
    url = url.replace(`{${name}}`, encodeURIComponent(value));
  }

  // Add query parameters
  const queryParams = endpoint.parameters
    ?.filter(p => p.in === 'query' && params?.[p.name])
    .map(p => `${encodeURIComponent(p.name)}=${encodeURIComponent(params?.[p.name] || '')}`)
    .join('&');

  if (queryParams) {
    url += '?' + queryParams;
  }

  return url;
}

async function executeRequest(endpoint: Endpoint) {
  const key = endpoint.path + endpoint.method;
  testing.value = true;

  try {
    const url = buildRequestUrl(endpoint);
    const options: RequestInit = {
      method: endpoint.method.toUpperCase(),
      headers: {
        'Content-Type': 'application/json',
      },
    };

    if (endpoint.requestBody && testBodies.value[key]) {
      options.body = testBodies.value[key];
    }

    const response = await fetch(url, options);
    const contentType = response.headers.get('content-type');

    let body: any;
    if (contentType?.includes('application/json')) {
      body = JSON.stringify(await response.json(), null, 2);
    } else {
      body = await response.text();
    }

    testResults.value[key] = {
      status: response.status,
      body,
    };
  } catch (e: any) {
    testResults.value[key] = {
      status: 0,
      body: `Error: ${e.message}`,
    };
  } finally {
    testing.value = false;
  }
}

function copyCurl(endpoint: Endpoint) {
  const key = endpoint.path + endpoint.method;
  const url = window.location.origin + buildRequestUrl(endpoint);

  let curl = `curl -X ${endpoint.method.toUpperCase()} '${url}'`;

  if (endpoint.requestBody && testBodies.value[key]) {
    curl += ` -H 'Content-Type: application/json'`;
    curl += ` -d '${testBodies.value[key]}'`;
  }

  navigator.clipboard.writeText(curl).then(() => {
    alert('Commande cURL copiée dans le presse-papiers !');
  });
}

onMounted(() => {
  fetchAPIDoc();
});
</script>

<style scoped>
.api-explorer {
  padding: 2rem;
  max-width: 1400px;
  margin: 0 auto;
}

h1 {
  font-size: 2rem;
  margin-bottom: 1.5rem;
  color: #333;
}

h2 {
  font-size: 1.5rem;
  margin-bottom: 1rem;
  color: #444;
  border-bottom: 2px solid #ddd;
  padding-bottom: 0.5rem;
}

h4 {
  font-size: 1.1rem;
  margin: 1rem 0 0.5rem;
  color: #555;
}

.loading, .error {
  padding: 2rem;
  text-align: center;
  font-size: 1.1rem;
}

.error {
  color: #d32f2f;
  background: #ffebee;
  border-radius: 4px;
}

/* API Info Section */
.api-info {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  padding: 2rem;
  border-radius: 8px;
  margin-bottom: 2rem;
}

.api-info h2 {
  color: white;
  border: none;
  margin-bottom: 0.5rem;
}

.api-info .description {
  margin: 1rem 0;
  opacity: 0.95;
}

.api-info .metadata {
  display: flex;
  gap: 1rem;
  font-size: 0.9rem;
  opacity: 0.9;
}

.api-info .version {
  background: rgba(255, 255, 255, 0.2);
  padding: 0.25rem 0.75rem;
  border-radius: 4px;
}

/* Endpoints Section */
.endpoints {
  margin-bottom: 2rem;
}

.endpoint-card {
  background: white;
  border: 1px solid #ddd;
  border-radius: 8px;
  margin-bottom: 1rem;
  overflow: hidden;
  transition: box-shadow 0.2s;
}

.endpoint-card:hover {
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.endpoint-card.expanded {
  border-color: #667eea;
}

.endpoint-header {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1rem;
  cursor: pointer;
  background: #f8f9fa;
  transition: background 0.2s;
}

.endpoint-header:hover {
  background: #e9ecef;
}

.method {
  font-weight: bold;
  padding: 0.25rem 0.75rem;
  border-radius: 4px;
  font-size: 0.85rem;
  min-width: 70px;
  text-align: center;
}

.method.get { background: #4caf50; color: white; }
.method.post { background: #2196f3; color: white; }
.method.put { background: #ff9800; color: white; }
.method.delete { background: #f44336; color: white; }
.method.patch { background: #9c27b0; color: white; }

.path {
  font-family: 'Courier New', monospace;
  font-weight: 600;
  color: #333;
}

.summary {
  color: #666;
  flex: 1;
}

.arrow {
  margin-left: auto;
  color: #999;
}

.endpoint-details {
  padding: 1.5rem;
  background: white;
  border-top: 1px solid #eee;
}

.description {
  color: #666;
  margin-bottom: 1rem;
  line-height: 1.6;
}

.tags {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.tag {
  background: #e3f2fd;
  color: #1976d2;
  padding: 0.25rem 0.75rem;
  border-radius: 4px;
  font-size: 0.85rem;
}

/* Parameters Table */
.parameters table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 0.5rem;
}

.parameters th,
.parameters td {
  padding: 0.75rem;
  text-align: left;
  border-bottom: 1px solid #eee;
}

.parameters th {
  background: #f8f9fa;
  font-weight: 600;
  color: #555;
}

.parameters code {
  background: #f8f9fa;
  padding: 0.2rem 0.4rem;
  border-radius: 3px;
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
}

.param-in {
  display: inline-block;
  background: #fff3e0;
  color: #e65100;
  padding: 0.2rem 0.5rem;
  border-radius: 3px;
  font-size: 0.85rem;
}

/* Schema Viewer */
.schema-viewer pre,
.response-schema pre,
.result-body {
  background: #f8f9fa;
  padding: 1rem;
  border-radius: 4px;
  overflow-x: auto;
  font-family: 'Courier New', monospace;
  font-size: 0.9rem;
  line-height: 1.5;
}

/* Responses */
.responses {
  margin-top: 1rem;
}

.response-item {
  margin-bottom: 1rem;
}

.response-status {
  padding: 0.5rem 1rem;
  border-radius: 4px;
  font-weight: 600;
  margin-bottom: 0.5rem;
}

.response-status.success { background: #e8f5e9; color: #2e7d32; }
.response-status.redirect { background: #fff3e0; color: #e65100; }
.response-status.client-error { background: #ffebee; color: #c62828; }
.response-status.server-error { background: #fce4ec; color: #880e4f; }

/* Test Section */
.test-section {
  margin-top: 2rem;
  padding-top: 2rem;
  border-top: 2px solid #eee;
}

.test-params,
.test-body {
  margin-bottom: 1rem;
}

.test-params label,
.test-body label {
  display: block;
  font-weight: 600;
  margin-bottom: 0.5rem;
  color: #555;
}

.param-input {
  display: flex;
  align-items: center;
  gap: 1rem;
  margin-bottom: 0.5rem;
}

.param-input label {
  min-width: 150px;
  margin: 0;
  font-weight: normal;
}

.param-input input,
.test-body textarea {
  flex: 1;
  padding: 0.5rem;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-family: 'Courier New', monospace;
}

.test-body textarea {
  width: 100%;
  font-size: 0.9rem;
  resize: vertical;
}

.test-actions {
  display: flex;
  gap: 1rem;
  margin-top: 1rem;
}

.btn-test,
.btn-copy {
  padding: 0.75rem 1.5rem;
  border: none;
  border-radius: 4px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-test {
  background: #667eea;
  color: white;
}

.btn-test:hover:not(:disabled) {
  background: #5568d3;
}

.btn-test:disabled {
  background: #ccc;
  cursor: not-allowed;
}

.btn-copy {
  background: #f8f9fa;
  color: #333;
  border: 1px solid #ddd;
}

.btn-copy:hover {
  background: #e9ecef;
}

/* Test Result */
.test-result {
  margin-top: 1rem;
  padding: 1rem;
  background: #f8f9fa;
  border-radius: 4px;
}

.result-header {
  display: flex;
  align-items: center;
  gap: 1rem;
  margin-bottom: 1rem;
}

.result-status {
  padding: 0.25rem 0.75rem;
  border-radius: 4px;
  font-weight: 600;
  font-size: 0.9rem;
}

/* Schemas Section */
.schemas {
  margin-top: 2rem;
}

.schema-card {
  background: white;
  border: 1px solid #ddd;
  border-radius: 8px;
  margin-bottom: 1rem;
  overflow: hidden;
}

.schema-card.expanded {
  border-color: #667eea;
}

.schema-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem;
  cursor: pointer;
  background: #f8f9fa;
  transition: background 0.2s;
}

.schema-header:hover {
  background: #e9ecef;
}

.schema-name {
  font-family: 'Courier New', monospace;
  font-weight: 600;
  color: #333;
}

.schema-details {
  padding: 1.5rem;
  background: white;
  border-top: 1px solid #eee;
}

.schema-details pre {
  margin: 0;
}
</style>
