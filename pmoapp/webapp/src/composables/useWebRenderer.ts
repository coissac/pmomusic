/**
 * Composable pour gérer le WebRenderer navigateur.
 *
 * Utilise PMOPlayer pour le controle remote.
 * S'enregistre via POST /api/webrenderer/register
 * Utilise polling HTTP pour les commands
 */

import { ref, onMounted, onUnmounted, readonly } from "vue";
import { PMOPlayer } from "@/services/PMOPlayer";

function generateUUID(): string {
    if (typeof crypto.randomUUID === "function") {
        return crypto.randomUUID();
    }
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    bytes[6] = (bytes[6]! & 0x0f) | 0x40;
    bytes[8] = (bytes[8]! & 0x3f) | 0x80;
    const hex = Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

const INSTANCE_ID_KEY = "pmomusic_webrenderer_instance_id";

// Module-level singleton for PMOPlayer to prevent duplicate instances
let globalPlayer: PMOPlayer | null = null;
let globalInstanceId: string | null = null;
let registering = false;

// Module-level reactive state shared across all composable invocations
const sharedConnected = ref(false);
const sharedStreamUrl = ref<string | null>(null);
const sharedRendererUdn = ref<string | null>(null);

function getOrCreateInstanceId(): string {
    try {
        let id = sessionStorage.getItem(INSTANCE_ID_KEY);
        if (!id) {
            id = generateUUID();
            sessionStorage.setItem(INSTANCE_ID_KEY, id);
        }
        return id;
    } catch {
        return generateUUID();
    }
}

export function useWebRenderer() {
    const connected = sharedConnected;
    const streamUrl = sharedStreamUrl;
    const rendererUdn = sharedRendererUdn;

    let player: PMOPlayer | null = null;
    let onConnectedCallback: (() => void) | null = null;

    async function register(): Promise<void> {
        // Prevent concurrent registrations (race condition → double player)
        if (globalPlayer || registering) {
            if (globalPlayer) {
                player = globalPlayer;
                connected.value = true;
            }
            return;
        }
        registering = true;

        const instanceId = getOrCreateInstanceId();
        console.log('[WebRenderer] registering with instanceId:', instanceId);

        try {
            const resp = await fetch("/api/webrenderer/register", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    instance_id: instanceId,
                    user_agent: navigator.userAgent,
                }),
            });

            if (!resp.ok) {
                console.error("[WebRenderer] register failed:", resp.status);
                return;
            }

            const data = await resp.json();
            streamUrl.value = data.stream_url;
            rendererUdn.value = data.udn;

            player = new PMOPlayer(instanceId);
            globalPlayer = player;
            globalInstanceId = instanceId;
            player.setDebug(true);

            player.on('play', () => console.log('[WebRenderer] playing'));
            player.on('pause', () => console.log('[WebRenderer] paused'));

            // Si le backend est déjà en lecture (reconnexion après reload), démarrer immédiatement
            if (data.should_play && data.stream_url) {
                console.log('[WebRenderer] backend already playing, starting stream');
                player.playStream(data.stream_url);
            }

            connected.value = true;
            onConnectedCallback?.();
        } catch (e) {
            console.error("[WebRenderer] register error:", e);
        } finally {
            registering = false;
        }
    }

    async function unregister(): Promise<void> {
        // Only unregister if this is the global player - prevent duplicate unregister calls
        if (player !== globalPlayer || !player) {
            return;
        }
        
        const instanceId = globalInstanceId || getOrCreateInstanceId();
        
        // Clear global first to prevent other components from using it
        globalPlayer = null;
        globalInstanceId = null;
        
        try {
            await fetch(`/api/webrenderer/${instanceId}`, { method: "DELETE" });
        } catch {
            // Ignored
        }
        player?.destroy();
        player = null;
    }

    function setVolume(_v: number): void {
        // PMOPlayer doesn't control volume directly - handled by backend
    }

    function setMute(_m: boolean): void {
        // PMOPlayer doesn't control mute directly - handled by backend
    }

    const beforeUnloadHandler = () => void unregister();

    onMounted(() => {
        void register();
        window.addEventListener("beforeunload", beforeUnloadHandler);
    });

    onUnmounted(() => {
        void unregister();
        window.removeEventListener("beforeunload", beforeUnloadHandler);
    });

    return {
        connected: readonly(connected),
        streamUrl: readonly(streamUrl),
        rendererUdn: readonly(rendererUdn),
        onConnected(fn: () => void) {
            onConnectedCallback = fn;
        },
        setVolume,
        setMute,
    };
}