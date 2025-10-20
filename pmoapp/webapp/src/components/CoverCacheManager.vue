<template>
  <div class="cover-cache-manager">
    <div class="header">
      <h2>🖼️ Cover Cache Manager</h2>
      <div class="stats">
        <span>{{ images.length }} images</span>
        <span v-if="totalHits > 0">{{ totalHits }} hits</span>
      </div>
    </div>

    <!-- Formulaire d'ajout -->
    <div class="add-form">
      <h3>➕ Add New Cover</h3>
      <form @submit.prevent="handleAddImage">
        <div class="form-group">
          <input
            v-model="newImageUrl"
            type="url"
            placeholder="https://example.com/cover.jpg"
            required
            :disabled="isAdding"
          />
          <button type="submit" :disabled="isAdding || !newImageUrl">
            {{ isAdding ? "Adding..." : "Add Image" }}
          </button>
        </div>
        <p v-if="addError" class="error">❌ {{ addError }}</p>
        <p v-if="addSuccess" class="success">✅ {{ addSuccess }}</p>
      </form>
    </div>

    <!-- Contrôles -->
    <div class="controls">
      <div class="sort-controls">
        <label>Sort by:</label>
        <select v-model="sortBy">
          <option value="hits">Most Used</option>
          <option value="last_used">Recently Used</option>
          <option value="recent">Recently Added</option>
        </select>
      </div>
      <div class="actions">
        <button @click="refreshImages" :disabled="isLoading">
          🔄 {{ isLoading ? "Loading..." : "Refresh" }}
        </button>
        <button @click="handleConsolidate" :disabled="isConsolidating" class="btn-secondary">
          🔧 {{ isConsolidating ? "Consolidating..." : "Consolidate" }}
        </button>
        <button @click="handlePurge" class="btn-danger" :disabled="isPurging">
          🗑️ {{ isPurging ? "Purging..." : "Purge All" }}
        </button>
      </div>
    </div>

    <!-- Galerie d'images -->
    <div v-if="isLoading && images.length === 0" class="loading-state">
      ⏳ Loading images...
    </div>

    <div v-else-if="images.length === 0" class="empty-state">
      📭 No images in cache. Add one using the form above!
    </div>

    <div v-else class="image-grid">
      <div
        v-for="image in sortedImages"
        :key="image.pk"
        class="image-card"
        @click="selectedImage = image"
      >
        <div class="image-wrapper">
          <img
            :src="getImageUrl(image.pk, 256)"
            :alt="image.source_url"
            loading="lazy"
            @error="handleImageError"
          />
          <div class="image-overlay">
            <span class="hits">👁️ {{ image.hits }}</span>
          </div>
        </div>
        <div class="image-info">
          <div class="pk">{{ image.pk }}</div>
          <div class="url" :title="image.source_url">
            {{ truncateUrl(image.source_url) }}
          </div>
          <div class="meta">
            <span v-if="image.last_used" class="last-used">
              🕐 {{ formatDate(image.last_used) }}
            </span>
          </div>
        </div>
        <div class="image-actions">
          <button
            @click.stop="handleDeleteImage(image.pk)"
            class="btn-delete"
            :disabled="deletingImages.has(image.pk)"
          >
            {{ deletingImages.has(image.pk) ? "..." : "🗑️" }}
          </button>
        </div>
      </div>
    </div>

    <!-- Modal de détails -->
    <div v-if="selectedImage" class="modal" @click="selectedImage = null">
      <div class="modal-content" @click.stop>
        <button class="modal-close" @click="selectedImage = null">✕</button>
        <img
          :src="getImageUrl(selectedImage.pk)"
          :alt="selectedImage.source_url"
          class="modal-image"
        />
        <div class="modal-info">
          <h3>Image Details</h3>
          <p><strong>PK:</strong> {{ selectedImage.pk }}</p>
          <p><strong>Source URL:</strong> <a :href="selectedImage.source_url" target="_blank">{{ selectedImage.source_url }}</a></p>
          <p><strong>Hits:</strong> {{ selectedImage.hits }}</p>
          <p v-if="selectedImage.last_used"><strong>Last Used:</strong> {{ formatDate(selectedImage.last_used) }}</p>
          <div class="modal-actions">
            <button @click="copyImageUrl(selectedImage.pk)" class="btn-secondary">
              📋 Copy URL
            </button>
            <button @click="handleDeleteImage(selectedImage.pk); selectedImage = null" class="btn-danger">
              🗑️ Delete
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import type { CacheEntry } from "../services/coverCache";
import {
  listImages,
  addImage,
  deleteImage,
  purgeCache,
  consolidateCache,
  getImageUrl,
} from "../services/coverCache";

// --- États ---
const images = ref<CacheEntry[]>([]);
const selectedImage = ref<CacheEntry | null>(null);
const isLoading = ref(false);
const sortBy = ref<"hits" | "last_used" | "recent">("hits");

// Formulaire d'ajout
const newImageUrl = ref("");
const isAdding = ref(false);
const addError = ref("");
const addSuccess = ref("");

// Contrôles
const isConsolidating = ref(false);
const isPurging = ref(false);
const deletingImages = ref(new Set<string>());

// --- Computed ---
const totalHits = computed(() => images.value.reduce((sum, i) => sum + i.hits, 0));

const sortedImages = computed(() => {
  const arr = [...images.value];
  switch (sortBy.value) {
    case "hits": return arr.sort((a,b)=>b.hits-a.hits);
    case "last_used":
      return arr.sort((a,b)=>{
        if(!a.last_used) return 1;
        if(!b.last_used) return -1;
        return new Date(b.last_used).getTime()-new Date(a.last_used).getTime();
      });
    case "recent": return arr.reverse();
    default: return arr;
  }
});

// --- Fonctions ---
async function refreshImages() {
  isLoading.value = true;
  try { images.value = await listImages(); }
  finally { isLoading.value = false; }
}

async function handleAddImage() {
  if(!newImageUrl.value) return;
  isAdding.value = true; addError.value=""; addSuccess.value="";
  try {
    const result = await addImage(newImageUrl.value);
    addSuccess.value = `Image added! PK: ${result.pk}`;
    newImageUrl.value = "";
    await refreshImages();
  } catch(e:any) { addError.value = e.message ?? "Failed to add image"; }
  finally { isAdding.value=false; setTimeout(()=>addSuccess.value="",1500); }
}

async function handleDeleteImage(pk:string){
  if(!confirm(`Delete image ${pk}?`)) return;
  deletingImages.value.add(pk);
  try{ await deleteImage(pk); await refreshImages(); }
  finally{ deletingImages.value.delete(pk); }
}

async function handlePurge(){
  if(!confirm("⚠️ Delete ALL images?")) return;
  isPurging.value = true;
  try{ await purgeCache(); await refreshImages(); }
  finally{ isPurging.value=false; }
}

async function handleConsolidate(){
  if(!confirm("Consolidate cache?")) return;
  isConsolidating.value=true;
  try{ await consolidateCache(); await refreshImages(); }
  finally{ isConsolidating.value=false; }
}

function copyImageUrl(pk:string){
  navigator.clipboard.writeText(window.location.origin + getImageUrl(pk));
  alert("✅ URL copied!");
}

function truncateUrl(url:string,maxLength=40){ return url.length<=maxLength?url:url.slice(0,maxLength-3)+"..."; }
function formatDate(dateString:string){
  const d=new Date(dateString), diff=Date.now()-d.getTime(), days=Math.floor(diff/(1000*60*60*24));
  if(days===0)return"Today"; if(days===1)return"Yesterday"; if(days<7)return`${days} days ago`; return d.toLocaleDateString();
}
function handleImageError(e:Event){(e.target as HTMLImageElement).src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='256' height='256'%3E%3Crect fill='%23333' width='256' height='256'/%3E%3Ctext x='50%25' y='50%25' dominant-baseline='middle' text-anchor='middle' fill='%23999' font-size='20'%3EError%3C/text%3E%3C/svg%3E";}

onMounted(()=>refreshImages());
</script>

<style scoped>
.cover-cache-manager {
  padding: 1rem;
  width: 100%;
  max-width: 100%;
  margin: 0;
  box-sizing: border-box;
}

@media (min-width: 1400px) {
  .cover-cache-manager {
    padding: 2rem;
    max-width: 1400px;
    margin: 0 auto;
  }
}

@media (max-width: 768px) {
  .cover-cache-manager {
    padding: 0.5rem;
  }
}
.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
  padding-bottom: 1rem;
  border-bottom: 2px solid #444;
}
.header h2 {
  margin: 0;
  color: #61dafb;
}
.stats {
  display: flex;
  gap: 1rem;
  font-size: 0.9rem;
  color: #999;
} /* Formulaire d'ajout */
.add-form {
  background: #2a2a2a;
  padding: 1.5rem;
  border-radius: 8px;
  margin-bottom: 1.5rem;
}
.add-form h3 {
  margin-top: 0;
  color: #61dafb;
}
.form-group {
  display: flex;
  gap: 0.5rem;
}
.form-group input {
  flex: 1;
  padding: 0.75rem;
  border: 1px solid #444;
  border-radius: 4px;
  background: #1a1a1a;
  color: #fff;
  font-size: 1rem;
}
.form-group button {
  padding: 0.75rem 1.5rem;
  background: #61dafb;
  color: #000;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-weight: bold;
  transition: all 0.2s;
}
.form-group button:hover:not(:disabled) {
  background: #4fa8c5;
}
.form-group button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.error {
  color: #ff6b6b;
  margin-top: 0.5rem;
}
.success {
  color: #51cf66;
  margin-top: 0.5rem;
} /* Contrôles */
.controls {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
  padding: 1rem;
  background: #2a2a2a;
  border-radius: 8px;
}
.sort-controls {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}
.sort-controls label {
  color: #999;
}
.sort-controls select {
  padding: 0.5rem;
  border: 1px solid #444;
  border-radius: 4px;
  background: #1a1a1a;
  color: #fff;
}
.actions {
  display: flex;
  gap: 0.5rem;
}
button {
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
  transition: all 0.2s;
}
button:not(.btn-danger):not(.btn-secondary) {
  background: #61dafb;
  color: #000;
}
button:not(.btn-danger):not(.btn-secondary):hover:not(:disabled) {
  background: #4fa8c5;
}
.btn-secondary {
  background: #555;
  color: #fff;
}
.btn-secondary:hover:not(:disabled) {
  background: #666;
}
.btn-danger {
  background: #ff6b6b;
  color: #fff;
}
.btn-danger:hover:not(:disabled) {
  background: #ee5a52;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
} /* États */
.loading-state,
.empty-state {
  text-align: center;
  padding: 3rem;
  color: #999;
  font-size: 1.2rem;
} /* Grille d'images */
.image-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 1.5rem;
}
.image-card {
  background: #2a2a2a;
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}
.image-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 8px 16px rgba(0, 0, 0, 0.3);
}
.image-wrapper {
  position: relative;
  width: 100%;
  padding-top: 100%; /* Ratio 1:1 */
  background: #1a1a1a;
  overflow: hidden;
}
.image-wrapper img {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.image-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: linear-gradient(to top, rgba(0, 0, 0, 0.8), transparent);
  padding: 0.5rem;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.hits {
  color: #fff;
  font-size: 0.9rem;
}
.image-info {
  padding: 1rem;
}
.pk {
  font-family: monospace;
  color: #61dafb;
  font-size: 0.9rem;
  margin-bottom: 0.25rem;
}
.url {
  color: #999;
  font-size: 0.85rem;
  margin-bottom: 0.5rem;
}
.meta {
  display: flex;
  gap: 0.5rem;
  font-size: 0.8rem;
  color: #777;
}
.image-actions {
  padding: 0 1rem 1rem;
}
.btn-delete {
  width: 100%;
  background: #555;
  color: #fff;
  padding: 0.5rem;
}
.btn-delete:hover:not(:disabled) {
  background: #ff6b6b;
} /* Modal */
.modal {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.9);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 2rem;
}
.modal-content {
  background: #2a2a2a;
  border-radius: 12px;
  max-width: 800px;
  max-height: 90vh;
  overflow: auto;
  position: relative;
}
.modal-close {
  position: absolute;
  top: 1rem;
  right: 1rem;
  background: rgba(0, 0, 0, 0.5);
  color: #fff;
  border: none;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.2rem;
  z-index: 1;
}
.modal-close:hover {
  background: rgba(0, 0, 0, 0.8);
}
.modal-image {
  width: 100%;
  display: block;
}
.modal-info {
  padding: 1.5rem;
}
.modal-info h3 {
  margin-top: 0;
  color: #61dafb;
}
.modal-info p {
  margin: 0.5rem 0;
}
.modal-info a {
  color: #61dafb;
  text-decoration: none;
}
.modal-info a:hover {
  text-decoration: underline;
}
.modal-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1rem;
}
</style>
