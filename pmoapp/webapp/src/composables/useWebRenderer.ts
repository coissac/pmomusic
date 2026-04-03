/**
 * Composable pour gérer le WebRenderer navigateur.
 *
 * S'enregistre via POST /api/webrenderer/register au montage
 * et diffuse l'audio via un élément <audio> pointant sur
 * /api/webrenderer/{id}/stream (flux FLAC encodé côté serveur).
 *
 * Le navigateur est vu comme un renderer UPnP par le ControlPoint.
 * Le gapless est géré côté serveur : le navigateur lit un flux continu.
 *
 * Cycle de vie du flux audio :
 * - PLAYING/TRANSITIONING → src = stream_url + play()  (nouvelle connexion HTTP)
 * - PAUSED/STOPPED        → pause() + src = ""          (déconnexion HTTP)
 */

import { ref, onMounted, onUnmounted, readonly } from "vue";
import { useSSE } from "./useSSE";

// ─── Identifiant stable de l'instance navigateur ─────────────────────────────

const INSTANCE_ID_KEY = "pmomusic_webrenderer_instance_id";

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

// ─── Types ────────────────────────────────────────────────────────────────────

interface RegisterRequest {
    instance_id: string;
    user_agent: string;
}

interface RegisterResponse {
    stream_url: string;
    udn: string;
}

// ─── Composable ───────────────────────────────────────────────────────────────

export function useWebRenderer() {
    const connected = ref(false);
    const streamUrl = ref<string | null>(null);
    /** UDN du device UPnP créé côté serveur pour ce navigateur */
    const rendererUdn = ref<string | null>(null);

    let audioEl: HTMLAudioElement | null = null;
    let instanceId: string | null = null;
    let currentStreamUrl: string | null = null;
    let onConnectedCallback: (() => void) | null = null;
    let sseUnsubscribe: (() => void) | null = null;
    let pendingCanPlay: (() => void) | null = null;
    let positionInterval: ReturnType<typeof setInterval> | null = null;

    // ── Reporting de position ─────────────────────────────────────────────────

    function startPositionReporting(): void {
        stopPositionReporting();
        positionInterval = setInterval(() => {
            if (!audioEl || !instanceId) return;
            const pos = audioEl.currentTime;
            const dur = isFinite(audioEl.duration) ? audioEl.duration : null;
            void fetch(`/api/webrenderer/${instanceId}/position`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ position_sec: pos, duration_sec: dur }),
            });
        }, 1000);
    }

    function stopPositionReporting(): void {
        if (positionInterval !== null) {
            clearInterval(positionInterval);
            positionInterval = null;
        }
    }

    // ── Connexion / déconnexion du flux audio ─────────────────────────────────

    function startStream(): void {
        console.log("[WebRenderer] startStream called, audioEl=", !!audioEl, "currentStreamUrl=", currentStreamUrl);
        if (!audioEl || !currentStreamUrl) return;
        const el = audioEl;
        // Si le stream est en erreur (networkState=3), réinitialiser avant de réessayer
        if (el.networkState === 3 /* NETWORK_NO_SOURCE */ && !pendingCanPlay) {
            console.log("[WebRenderer] startStream: networkState=3, resetting before retry");
            el.removeAttribute("src");
            el.load();
        }
        // Si le stream est déjà chargé/en cours, ne pas réouvrir la connexion HTTP.
        // (évite que PLAYING après TRANSITIONING ne crée un nouveau pipe)
        if (el.hasAttribute("src") && (el.readyState > 0 || pendingCanPlay)) {
            console.log("[WebRenderer] startStream: stream already open, ignoring (readyState=", el.readyState, ")");
            return;
        }
        // Sur Safari, play() échoue avec NotSupportedError si appelé avant que
        // l'élément audio ait reçu assez de données (readyState < HAVE_FUTURE_DATA).
        // On attend canplay avant d'appeler play().
        if (pendingCanPlay) {
            el.removeEventListener("canplay", pendingCanPlay);
        }
        el.src = currentStreamUrl;
        console.log("[WebRenderer] src set, readyState=", el.readyState, "networkState=", el.networkState);

        el.addEventListener("error", () => {
            console.error("[WebRenderer] event:error code=", el.error?.code, el.error?.message,
                "readyState=", el.readyState, "networkState=", el.networkState);
            // Nettoyer pendingCanPlay pour permettre un retry au prochain PLAYING/TRANSITIONING
            if (pendingCanPlay) {
                el.removeEventListener("canplay", pendingCanPlay);
                pendingCanPlay = null;
            }
        }, { once: true });
        el.addEventListener("loadstart",      () => console.debug("[WebRenderer] event:loadstart readyState=", el.readyState), { once: true });
        el.addEventListener("loadedmetadata", () => console.debug("[WebRenderer] event:loadedmetadata readyState=", el.readyState), { once: true });
        el.addEventListener("loadeddata",     () => console.debug("[WebRenderer] event:loadeddata readyState=", el.readyState), { once: true });
        el.addEventListener("progress",       () => console.debug("[WebRenderer] event:progress readyState=", el.readyState), { once: true });
        el.addEventListener("stalled",        () => console.warn("[WebRenderer] event:stalled readyState=", el.readyState, "networkState=", el.networkState));
        el.addEventListener("waiting",        () => console.warn("[WebRenderer] event:waiting readyState=", el.readyState));
        el.addEventListener("suspend",        () => console.debug("[WebRenderer] event:suspend readyState=", el.readyState, "networkState=", el.networkState), { once: true });
        el.addEventListener("abort",          () => console.warn("[WebRenderer] event:abort"), { once: true });
        el.addEventListener("emptied",        () => console.warn("[WebRenderer] event:emptied"), { once: true });

        const onCanPlay = () => {
            console.log("[WebRenderer] event:canplay readyState=", el.readyState, "calling play()");
            pendingCanPlay = null;
            el.removeEventListener("canplay", onCanPlay);
            el.play().then(() => {
                console.log("[WebRenderer] play() resolved OK");
                startPositionReporting();
            }).catch((e: unknown) => {
                console.warn("[WebRenderer] play() rejected:", e);
            });
        };
        pendingCanPlay = onCanPlay;
        el.addEventListener("canplay", onCanPlay);
    }

    function stopStream(): void {
        console.log("[WebRenderer] stopStream called, readyState=", audioEl?.readyState);
        stopPositionReporting();
        if (!audioEl) return;
        if (pendingCanPlay) {
            audioEl.removeEventListener("canplay", pendingCanPlay);
            pendingCanPlay = null;
        }
        audioEl.pause();
        audioEl.removeAttribute("src");  // ferme la connexion HTTP (src="" résolu comme URL de page sur Safari)
        audioEl.load();  // force le reset de l'état réseau
    }

    // ── Enregistrement ────────────────────────────────────────────────────────

    async function register(): Promise<void> {
        instanceId = getOrCreateInstanceId();

        const body: RegisterRequest = {
            instance_id: instanceId,
            user_agent: navigator.userAgent,
        };

        try {
            const resp = await fetch("/api/webrenderer/register", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body),
            });

            if (!resp.ok) {
                console.error("[WebRenderer] register failed:", resp.status);
                return;
            }

            const data = (await resp.json()) as RegisterResponse;
            streamUrl.value = data.stream_url;
            currentStreamUrl = data.stream_url;
            rendererUdn.value = data.udn;
            connected.value = true;
            onConnectedCallback?.();

            // S'abonner aux événements SSE du renderer pour piloter la lecture
            const { connect, onRendererEvent } = useSSE();
            connect();
            const udn = data.udn;
            sseUnsubscribe?.();
            sseUnsubscribe = onRendererEvent((event) => {
                if (event.renderer_id !== udn) return;
                if (event.type !== "state_changed") return;

                const state = event.state;
                console.log("[WebRenderer] SSE state_changed →", state, "| event.renderer_id=", event.renderer_id, "udn=", udn, "| audioEl.src=", audioEl?.src, "readyState=", audioEl?.readyState, "networkState=", audioEl?.networkState, "pendingCanPlay=", !!pendingCanPlay);
                if (state === "PLAYING" || state === "TRANSITIONING") {
                    startStream();
                } else if (state === "PAUSED" || state === "STOPPED") {
                    stopStream();
                }
            });
        } catch (e) {
            console.error("[WebRenderer] register error:", e);
        }
    }

    // ── Désenregistrement ─────────────────────────────────────────────────────

    async function unregister(): Promise<void> {
        if (!instanceId) return;
        try {
            await fetch(`/api/webrenderer/${instanceId}`, { method: "DELETE" });
        } catch {
            // Ignoré lors du déchargement de page
        }
        instanceId = null;
    }

    // ── Volume / Mute ─────────────────────────────────────────────────────────

    function setVolume(v: number): void {
        if (audioEl) audioEl.volume = v;
    }

    function setMute(m: boolean): void {
        if (audioEl) audioEl.muted = m;
    }

    // ── Cycle de vie ──────────────────────────────────────────────────────────

    onMounted(() => {
        audioEl = document.createElement("audio");
        audioEl.preload = "auto";
        document.body.appendChild(audioEl);

        void register();
        window.addEventListener("beforeunload", () => void unregister());
    });

    onUnmounted(() => {
        sseUnsubscribe?.();
        sseUnsubscribe = null;
        stopPositionReporting();
        stopStream();
        void unregister();
        if (audioEl) {
            audioEl.remove();
            audioEl = null;
        }
        connected.value = false;
        streamUrl.value = null;
        rendererUdn.value = null;
        window.removeEventListener("beforeunload", () => void unregister());
    });

    // ── API publique ──────────────────────────────────────────────────────────

    return {
        /** true quand l'instance est enregistrée sur le serveur */
        connected: readonly(connected),
        /** URL du flux FLAC servi par le serveur */
        streamUrl: readonly(streamUrl),
        /** UDN du device UPnP créé pour ce navigateur (null avant enregistrement) */
        rendererUdn: readonly(rendererUdn),
        /** Callback appelé quand l'enregistrement est confirmé */
        onConnected(fn: () => void) {
            onConnectedCallback = fn;
        },
        setVolume,
        setMute,
    };
}
