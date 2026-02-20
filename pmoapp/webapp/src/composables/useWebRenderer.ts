/**
 * Composable pour gérer le WebRenderer navigateur.
 *
 * Se connecte automatiquement au WebSocket /api/webrenderer/ws au montage
 * et se déconnecte proprement au démontage ou à la fermeture de la page.
 *
 * Le navigateur est ainsi vu comme un renderer UPnP par le ControlPoint.
 * Un élément <audio> headless exécute les commandes de transport reçues.
 */

import { ref, onMounted, onUnmounted, readonly } from "vue";

// ─── Types (miroir de messages.rs) ────────────────────────────────────────────

interface BrowserCapabilities {
    user_agent: string;
    supported_formats: string[];
}

interface RendererInfo {
    udn: string;
    friendly_name: string;
    model_name: string;
    description_url: string;
}

type TransportAction = "play" | "pause" | "stop" | "seek" | "set_uri";

interface CommandParams {
    uri?: string;
    metadata?: string;
    position?: string;
}

type ServerMessage =
    | { type: "session_created"; token: string; renderer_info: RendererInfo }
    | { type: "command"; action: TransportAction; params?: CommandParams }
    | { type: "set_volume"; volume: number }
    | { type: "set_mute"; mute: boolean }
    | { type: "ping" };

type PlaybackState = "PLAYING" | "PAUSED" | "STOPPED" | "TRANSITIONING";

type ClientMessage =
    | { type: "init"; capabilities: BrowserCapabilities }
    | { type: "state_update"; state: PlaybackState }
    | { type: "position_update"; position: string; duration: string }
    | { type: "volume_update"; volume: number; mute: boolean }
    | { type: "pong" };

// ─── Détection des formats supportés ─────────────────────────────────────────

function getSupportedFormats(): string[] {
    const audio = document.createElement("audio");
    const formats: Array<[string, string]> = [
        ["mp3", "audio/mpeg"],
        ["flac", "audio/flac"],
        ["ogg", "audio/ogg; codecs=vorbis"],
        ["opus", "audio/ogg; codecs=opus"],
        ["aac", "audio/aac"],
        ["wav", "audio/wav"],
        ["m4a", 'audio/mp4; codecs="mp4a.40.2"'],
        ["webm", "audio/webm"],
    ];
    return formats
        .filter(([, mime]) => audio.canPlayType(mime) !== "")
        .map(([fmt]) => fmt);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secondsToUpnpTime(s: number): string {
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = Math.floor(s % 60);
    return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

function upnpTimeToSeconds(t: string): number {
    const parts = t.split(":").map(Number);
    if (parts.length !== 3) return 0;
    const [h, m, s] = parts;
    return (h ?? 0) * 3600 + (m ?? 0) * 60 + (s ?? 0);
}

// ─── Composable ───────────────────────────────────────────────────────────────

export function useWebRenderer() {
    const connected = ref(false);
    const rendererInfo = ref<RendererInfo | null>(null);

    let ws: WebSocket | null = null;
    let audio: HTMLAudioElement | null = null;
    let positionTimer: ReturnType<typeof setInterval> | null = null;
    let onConnectedCallback: (() => void) | null = null;

    // ── Envoi d'un message au backend ────────────────────────────────────────

    function send(msg: ClientMessage) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(msg));
        }
    }

    // ── Player audio headless ─────────────────────────────────────────────────

    function sendPosition() {
        if (!audio) return;
        const pos = isFinite(audio.currentTime) ? audio.currentTime : 0;
        const dur = isFinite(audio.duration) ? audio.duration : 0;
        send({
            type: "position_update",
            position: secondsToUpnpTime(pos),
            duration: secondsToUpnpTime(dur),
        });
    }

    function startPositionTimer() {
        if (positionTimer !== null) return;
        positionTimer = setInterval(sendPosition, 1000);
    }

    function stopPositionTimer() {
        if (positionTimer !== null) {
            clearInterval(positionTimer);
            positionTimer = null;
        }
    }

    function createAudio(): HTMLAudioElement {
        const el = new Audio();
        el.preload = "auto";

        el.addEventListener("play", () => {
            send({ type: "state_update", state: "PLAYING" });
            startPositionTimer();
        });
        el.addEventListener("pause", () => {
            send({ type: "state_update", state: "PAUSED" });
            stopPositionTimer();
            sendPosition();
        });
        el.addEventListener("ended", () => {
            send({ type: "state_update", state: "STOPPED" });
            stopPositionTimer();
        });
        el.addEventListener("waiting", () => {
            send({ type: "state_update", state: "TRANSITIONING" });
        });
        el.addEventListener("canplay", () => {
            if (!el.paused) send({ type: "state_update", state: "PLAYING" });
        });
        el.addEventListener("volumechange", () => {
            const vol = Math.round(el.volume * 100);
            send({ type: "volume_update", volume: vol, mute: el.muted });
        });
        el.addEventListener("error", () => {
            console.error("[WebRenderer] Erreur audio :", el.error);
            send({ type: "state_update", state: "STOPPED" });
            stopPositionTimer();
        });

        return el;
    }

    // ── Exécution des commandes UPnP ──────────────────────────────────────────

    async function execCommand(action: TransportAction, params?: CommandParams) {
        if (!audio) return;
        console.debug(`[WebRenderer] Commande: ${action}`, params);

        switch (action) {
            case "set_uri":
                if (params?.uri) {
                    audio.pause();
                    audio.src = params.uri;
                    audio.load();
                    send({ type: "state_update", state: "STOPPED" });
                }
                break;

            case "play":
                if (audio.src) {
                    try {
                        await audio.play();
                    } catch (e) {
                        console.error("[WebRenderer] play() refusé :", e);
                    }
                }
                break;

            case "pause":
                audio.pause();
                break;

            case "stop":
                audio.pause();
                audio.currentTime = 0;
                send({ type: "state_update", state: "STOPPED" });
                break;

            case "seek":
                if (params?.position) {
                    audio.currentTime = upnpTimeToSeconds(params.position);
                }
                break;
        }
    }

    // ── Gestion des messages entrants ─────────────────────────────────────────

    function handleMessage(event: MessageEvent) {
        let msg: ServerMessage;
        try {
            msg = JSON.parse(event.data as string) as ServerMessage;
        } catch {
            console.warn("[WebRenderer] Message non-JSON reçu :", event.data);
            return;
        }

        switch (msg.type) {
            case "session_created":
                rendererInfo.value = msg.renderer_info;
                connected.value = true;
                console.info(
                    `[WebRenderer] Session créée — UDN: ${msg.renderer_info.udn}`,
                );
                // Notifier le parent pour qu'il rafraîchisse la liste des renderers
                // L'événement SSE peut arriver avant que le subscriber soit prêt
                onConnectedCallback?.();
                break;

            case "command":
                void execCommand(msg.action, msg.params);
                break;

            case "set_volume":
                if (audio) {
                    audio.volume = Math.max(0, Math.min(1, msg.volume / 100));
                }
                break;

            case "set_mute":
                if (audio) {
                    audio.muted = msg.mute;
                }
                break;

            case "ping":
                send({ type: "pong" });
                break;
        }
    }

    // ── Connexion ─────────────────────────────────────────────────────────────

    function connect() {
        if (ws) return;

        const protocol = location.protocol === "https:" ? "wss:" : "ws:";
        const url = `${protocol}//${location.host}/api/webrenderer/ws`;
        console.info(`[WebRenderer] Connexion à : ${url}`);

        ws = new WebSocket(url);

        ws.onopen = () => {
            console.info("[WebRenderer] WebSocket ouvert, envoi Init");
            send({
                type: "init",
                capabilities: {
                    user_agent: navigator.userAgent,
                    supported_formats: getSupportedFormats(),
                },
            });
        };

        ws.onmessage = handleMessage;

        ws.onclose = () => {
            connected.value = false;
            rendererInfo.value = null;
            ws = null;
            console.info("[WebRenderer] Déconnecté");
        };

        ws.onerror = (err) => {
            console.error("[WebRenderer] Erreur WebSocket :", err);
        };
    }

    // ── Déconnexion ───────────────────────────────────────────────────────────

    function disconnect() {
        stopPositionTimer();
        if (audio) {
            audio.pause();
            audio.src = "";
        }
        if (ws) {
            ws.close(1000, "Page unloaded");
            ws = null;
        }
        connected.value = false;
    }

    // ── Cycle de vie ──────────────────────────────────────────────────────────

    onMounted(() => {
        audio = createAudio();
        connect();
        window.addEventListener("beforeunload", disconnect);
    });

    onUnmounted(() => {
        disconnect();
        audio = null;
        window.removeEventListener("beforeunload", disconnect);
    });

    // ── API publique ──────────────────────────────────────────────────────────

    return {
        /** true quand la session WebRenderer est établie */
        connected: readonly(connected),
        /** Infos du renderer UPnP créé pour ce navigateur */
        rendererInfo: readonly(rendererInfo),
        /** Callback appelé quand la session est créée (pour rafraîchir la liste des renderers) */
        onConnected(fn: () => void) {
            onConnectedCallback = fn;
        },
    };
}
