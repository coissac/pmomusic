<script setup lang="ts">
import { useRouter } from "vue-router";
import {
    Music,
    ScrollText,
    Image,
    Database,
    ListMusic,
    Network,
    LayoutDashboard,
    Radio,
    ArrowLeft,
} from "lucide-vue-next";

const router = useRouter();

const tools = [
    {
        path: "/debug/covers-cache",
        name: "Cache des couvertures",
        description: "Visualiser et gérer le cache d'images de couverture",
        icon: Image,
    },
    {
        path: "/debug/audio-cache",
        name: "Cache audio",
        description: "Gérer le cache des fichiers audio",
        icon: Database,
    },
    {
        path: "/debug/playlists",
        name: "Playlists",
        description: "Gérer et inspecter les playlists en cache",
        icon: ListMusic,
    },
    {
        path: "/debug/logs",
        name: "Logs",
        description: "Consulter les logs du serveur en temps réel",
        icon: ScrollText,
    },
    {
        path: "/debug/upnp",
        name: "Explorateur UPnP",
        description: "Parcourir les dispositifs et services UPnP",
        icon: Network,
    },
    {
        path: "/debug/api-dashboard",
        name: "API Dashboard",
        description: "Documentation et test des API disponibles",
        icon: LayoutDashboard,
    },
    {
        path: "/debug/generic-player",
        name: "Lecteur générique",
        description: "Lecteur de test pour les flux audio",
        icon: Music,
    },
    {
        path: "/debug/radio-paradise",
        name: "Radio Paradise",
        description: "Explorateur Radio Paradise",
        icon: Radio,
    },
];
</script>

<template>
    <div class="debug-view">
        <header class="debug-header">
            <button class="back-btn" @click="router.push('/')" title="Retour">
                <ArrowLeft :size="20" />
            </button>
            <h1 class="debug-title">Outils de debug</h1>
        </header>

        <div class="debug-grid">
            <button
                v-for="tool in tools"
                :key="tool.path"
                class="tool-card"
                @click="router.push(tool.path)"
            >
                <div class="tool-icon">
                    <component :is="tool.icon" :size="28" />
                </div>
                <div class="tool-info">
                    <div class="tool-name">{{ tool.name }}</div>
                    <div class="tool-desc">{{ tool.description }}</div>
                </div>
            </button>
        </div>
    </div>
</template>

<style scoped>
.debug-view {
    min-height: 100vh;
    padding: var(--spacing-lg);
    box-sizing: border-box;
}

.debug-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    margin-bottom: var(--spacing-xl);
}

.back-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    padding: 0;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    cursor: pointer;
    color: var(--color-text);
    transition: background var(--transition-fast) ease;
    flex-shrink: 0;
}

.back-btn:hover {
    background: rgba(255, 255, 255, 0.2);
}

.debug-title {
    font-size: var(--text-2xl);
    font-weight: 700;
    margin: 0;
    color: var(--color-text);
}

.debug-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--spacing-md);
}

.tool-card {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-lg);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius-lg);
    cursor: pointer;
    text-align: left;
    color: var(--color-text);
    transition: all var(--transition-fast) ease;
    font-family: inherit;
}

.tool-card:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
    transform: translateY(-2px);
}

.tool-card:active {
    transform: translateY(0);
}

.tool-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 52px;
    height: 52px;
    flex-shrink: 0;
    border-radius: var(--radius-md);
    background: rgba(102, 126, 234, 0.15);
    color: var(--color-primary);
}

.tool-info {
    flex: 1;
    min-width: 0;
}

.tool-name {
    font-size: var(--text-base);
    font-weight: 600;
    margin-bottom: 4px;
}

.tool-desc {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    line-height: 1.4;
}
</style>
