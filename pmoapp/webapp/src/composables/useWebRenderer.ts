/**
 * Composable pour gérer le WebRenderer navigateur.
 *
 * Se connecte automatiquement au WebSocket /api/webrenderer/ws au montage
 * et se déconnecte proprement au démontage ou à la fermeture de la page.
 *
 * Le navigateur est ainsi vu comme un renderer UPnP par le ControlPoint.
 * L'audio est géré via deux éléments <audio> en ping-pong connectés à un
 * AudioContext (MediaElementSourceNode) pour le contrôle du volume/mute.
 *
 * ## Flux gapless
 * 1. SetAVTransportURI(N)     → slotA.src = N, slotA.load()
 * 2. Play                     → slotA.play()
 * 3. SetNextAVTransportURI(N+1) → slotB.src = N+1, slotB.load() (préchargement)
 * 4. slotA "ended"            → currentSlot = B, slotB.play() immédiat
 *                               → TrackEnded envoyé au backend
 * 5. Cycle recommence depuis 3 (slotA redevient le "next")
 */

import { ref, onMounted, onUnmounted, readonly } from "vue";

// ─── Types (miroir de messages.rs) ────────────────────────────────────────────

interface BrowserCapabilities {
    instance_id: string;
    user_agent: string;
    supported_formats: string[];
}

interface RendererInfo {
    udn: string;
    friendly_name: string;
    model_name: string;
    description_url: string;
}

type TransportAction = "play" | "pause" | "stop" | "seek" | "set_uri" | "set_next_uri";

interface CommandParams {
    uri?: string;
    metadata?: string;
    position?: string;
}

interface StateSyncMessage {
    type: "state_sync";
    current_uri?: string;
    current_metadata?: string;
    next_uri?: string;
    next_metadata?: string;
    playback_state: PlaybackState;
    position?: string;
    volume: number;
    mute: boolean;
}

type ServerMessage =
    | { type: "session_created"; token: string; renderer_info: RendererInfo }
    | StateSyncMessage
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
    | { type: "track_ended" }
    | { type: "pong" };

// ─── Identifiant stable de l'instance navigateur ─────────────────────────────

const INSTANCE_ID_KEY = "pmomusic_webrenderer_instance_id";

/**
 * Génère un UUID v4. Utilise crypto.randomUUID() si disponible (HTTPS/localhost),
 * sinon fallback sur crypto.getRandomValues() (disponible partout, y compris HTTP).
 */
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

/**
 * Retourne un UUID stable pour cette instance navigateur.
 * Généré une fois, persisté en localStorage, réutilisé entre les reloads.
 */
function getOrCreateInstanceId(): string {
    try {
        let id = localStorage.getItem(INSTANCE_ID_KEY);
        if (!id) {
            id = generateUUID();
            localStorage.setItem(INSTANCE_ID_KEY, id);
        }
        return id;
    } catch {
        // localStorage unavailable (private mode, etc.) → use a session-scoped UUID
        return generateUUID();
    }
}

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

// ─── Moteur Audio (HTMLAudioElement + MediaElementSourceNode) ─────────────────

class GaplessEngine {
    /** Les deux éléments <audio> fixes (ping-pong) */
    private readonly slots: [HTMLAudioElement, HTMLAudioElement];

    /** Index du slot actuellement en lecture (0 ou 1) */
    private currentSlot: 0 | 1 = 0;
    /** URI préchargée dans le slot "next" (l'autre) */
    private nextUri: string | null = null;

    private volume = 1.0;
    private muted = false;

    /** Durée de la piste courante (secondes), lue via loadedmetadata */
    private _duration = 0;

    /**
     * Indique qu'un play() a été reçu avant set_uri (race condition).
     * setCurrent() le détectera et lancera la lecture automatiquement.
     */
    private playPending = false;

    onStateChange: (state: PlaybackState) => void = () => {};
    onPosition: (pos: number, dur: number) => void = () => {};
    onTrackEnded: () => void = () => {};

    private positionInterval: ReturnType<typeof setInterval> | null = null;

    constructor() {
        this.slots = [
            this.makeAudioElement(),
            this.makeAudioElement(),
        ];
    }

    // ── Volume / Mute ────────────────────────────────────────────────────────

    setVolume(v: number) {
        this.volume = v;
        for (const el of this.slots) {
            el.volume = this.muted ? 0 : v;
        }
    }

    setMute(m: boolean) {
        this.muted = m;
        for (const el of this.slots) {
            el.volume = m ? 0 : this.volume;
        }
    }

    // ── Transport ────────────────────────────────────────────────────────────

    /** Charge la piste courante (sans la jouer). */
    setCurrent(uri: string): void {
        this.nextUri = null;
        this.onStateChange("TRANSITIONING");
        this._loadCurrent(uri);

        // Si play() est arrivé avant set_uri (race condition), lancer la lecture maintenant
        if (this.playPending) {
            this.playPending = false;
            this.play().catch((e) =>
                console.error("[GaplessEngine] deferred play() failed:", e),
            );
        }
    }

    /**
     * Restaure l'état audio après un reload sans notifier le serveur de TRANSITIONING.
     * Le serveur connaît déjà l'état ; on recharge juste l'audio localement.
     * Si shouldPlay=true mais que l'autoplay est bloqué, on notifie PAUSED
     * pour que l'interface puisse proposer un bouton Play fonctionnel.
     */
    async syncRestore(currentUri: string, nextUri: string | undefined, shouldPlay: boolean): Promise<void> {
        this._loadCurrent(currentUri);
        if (nextUri) {
            this.setNext(nextUri);
        }
        if (shouldPlay) {
            try {
                await this.play();
                // play() a réussi : onStateChange("PLAYING") a déjà été appelé dans play()
            } catch {
                // Autoplay bloqué par le navigateur : signaler PAUSED au serveur
                // L'audio est chargé, un clic Play suffira à démarrer
                this.onStateChange("PAUSED");
            }
        }
    }

    private _loadCurrent(uri: string): void {
        const el = this.slots[this.currentSlot];
        // Retirer l'écouteur "ended" de l'autre slot si présent
        const otherSlot = (1 - this.currentSlot) as 0 | 1;
        this.slots[otherSlot].onended = null;
        this.slots[otherSlot].pause();

        el.onended = null;
        el.pause();
        el.src = uri;
        el.load();

        // Récupérer la durée dès que les métadonnées sont disponibles
        el.onloadedmetadata = () => {
            this._duration = el.duration || 0;
        };
    }

    /** Précharge la piste suivante dans l'autre slot. */
    setNext(uri: string): void {
        this.nextUri = uri;
        const nextSlot = (1 - this.currentSlot) as 0 | 1;
        const el = this.slots[nextSlot];
        el.src = uri;
        el.preload = "auto";
        el.load();
    }

    async play(): Promise<void> {
        const el = this.slots[this.currentSlot];

        // Si pas de source : play() est arrivé avant set_uri (race condition).
        // On mémorise et setCurrent() déclenchera la lecture dès qu'il sera appelé.
        if (!el.src || el.src === window.location.href) {
            this.playPending = true;
            return;
        }

        this.playPending = false;
        el.onended = () => this.onCurrentEnded();

        // Si l'élément n'a pas encore de données, attendre canplay.
        if (el.readyState < HTMLMediaElement.HAVE_FUTURE_DATA) {
            await new Promise<void>((resolve) => {
                const onCanPlay = () => {
                    el.removeEventListener("canplay", onCanPlay);
                    resolve();
                };
                el.addEventListener("canplay", onCanPlay);
            });
        }

        try {
            await el.play();
        } catch (e) {
            console.warn("[GaplessEngine] play() failed (autoplay blocked?):", e);
            throw e;
        }

        this.startPositionTimer();
        this.onStateChange("PLAYING");
    }

    pause(): void {
        const el = this.slots[this.currentSlot];
        el.pause();
        this.stopPositionTimer();
        this.onStateChange("PAUSED");
        this.sendPosition();
    }

    stop(): void {
        this.playPending = false;
        const el = this.slots[this.currentSlot];
        el.onended = null;
        el.pause();
        el.currentTime = 0;
        this.nextUri = null;
        this.stopPositionTimer();
        this.onStateChange("STOPPED");
    }

    seek(toSeconds: number): void {
        const el = this.slots[this.currentSlot];
        el.currentTime = toSeconds;
    }

    destroy(): void {
        this.stopPositionTimer();
        for (const el of this.slots) {
            el.onended = null;
            el.pause();
            el.src = "";
        }
    }

    // ── Privé ────────────────────────────────────────────────────────────────

    private makeAudioElement(): HTMLAudioElement {
        const el = document.createElement("audio");
        el.preload = "auto";
        return el;
    }

    private onCurrentEnded(): void {
        const nextSlot = (1 - this.currentSlot) as 0 | 1;

        if (this.nextUri !== null) {
            // Le slot suivant est préchargé, on bascule
            this.currentSlot = nextSlot;
            this.nextUri = null;
            this._duration = 0;

            const nextEl = this.slots[this.currentSlot];
            nextEl.onended = () => this.onCurrentEnded();

            // Récupérer la durée de la nouvelle piste courante
            if (nextEl.duration && isFinite(nextEl.duration)) {
                this._duration = nextEl.duration;
            } else {
                nextEl.onloadedmetadata = () => {
                    this._duration = nextEl.duration || 0;
                };
            }

            // Informer le backend (qui fera le swap current←next côté serveur)
            this.onTrackEnded();

            // Démarrer immédiatement (le préchargement a eu lieu)
            nextEl.play().catch((e) =>
                console.error("[GaplessEngine] next.play() failed:", e),
            );
            // L'état reste PLAYING
        } else {
            // Pas de suivant : fin de lecture
            this.stopPositionTimer();
            this.onStateChange("STOPPED");
            this.onTrackEnded();
        }
    }

    private startPositionTimer(): void {
        if (this.positionInterval !== null) return;
        this.positionInterval = setInterval(() => this.sendPosition(), 1000);
    }

    private stopPositionTimer(): void {
        if (this.positionInterval !== null) {
            clearInterval(this.positionInterval);
            this.positionInterval = null;
        }
    }

    private sendPosition(): void {
        const el = this.slots[this.currentSlot];
        const pos = el.currentTime || 0;
        const dur = (el.duration && isFinite(el.duration)) ? el.duration : this._duration;
        this.onPosition(pos, dur);
    }
}

// ─── Composable ───────────────────────────────────────────────────────────────

export function useWebRenderer() {
    const connected = ref(false);
    const rendererInfo = ref<RendererInfo | null>(null);

    let ws: WebSocket | null = null;
    let engine: GaplessEngine | null = null;
    let onConnectedCallback: (() => void) | null = null;

    // ── Envoi d'un message au backend ────────────────────────────────────────

    function send(msg: ClientMessage) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(msg));
        }
    }

    // ── Initialisation du moteur ──────────────────────────────────────────────

    function initEngine(): GaplessEngine {
        const e = new GaplessEngine();

        e.onStateChange = (state) => {
            send({ type: "state_update", state });
        };

        e.onPosition = (pos, dur) => {
            send({
                type: "position_update",
                position: secondsToUpnpTime(pos),
                duration: secondsToUpnpTime(dur),
            });
        };

        e.onTrackEnded = () => {
            send({ type: "track_ended" });
        };

        return e;
    }

    // ── Exécution des commandes UPnP ──────────────────────────────────────────

    async function execCommand(action: TransportAction, params?: CommandParams) {
        if (!engine) return;

        switch (action) {
            case "set_uri":
                if (params?.uri) {
                    engine.setCurrent(params.uri);
                }
                break;

            case "set_next_uri":
                if (params?.uri) {
                    engine.setNext(params.uri);
                }
                break;

            case "play":
                await engine.play().catch((e) =>
                    console.warn("[WebRenderer] play() failed:", e),
                );
                break;

            case "pause":
                engine.pause();
                break;

            case "stop":
                engine.stop();
                break;

            case "seek":
                if (params?.position) {
                    engine.seek(upnpTimeToSeconds(params.position));
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
                onConnectedCallback?.();
                break;

            case "state_sync":
                if (engine && msg.current_uri) {
                    engine.setVolume(msg.volume / 100);
                    engine.setMute(msg.mute);
                    const shouldPlay = msg.playback_state === "PLAYING" || msg.playback_state === "TRANSITIONING";
                    // syncRestore recharge l'audio sans notifier le serveur de l'état
                    // (le serveur connaît déjà l'état ; on évite un aller-retour TRANSITIONING)
                    engine.syncRestore(msg.current_uri, msg.next_uri, shouldPlay);
                }
                break;

            case "command":
                void execCommand(msg.action, msg.params);
                break;

            case "set_volume":
                engine?.setVolume(msg.volume / 100);
                break;

            case "set_mute":
                engine?.setMute(msg.mute);
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
        ws = new WebSocket(url);

        ws.onopen = () => {
            send({
                type: "init",
                capabilities: {
                    instance_id: getOrCreateInstanceId(),
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
        };

        ws.onerror = (err) => {
            console.error("[WebRenderer] Erreur WebSocket :", err);
        };
    }

    // ── Déconnexion ───────────────────────────────────────────────────────────

    function disconnect() {
        engine?.destroy();
        if (ws) {
            ws.close(1000, "Page unloaded");
            ws = null;
        }
        connected.value = false;
    }

    // ── Cycle de vie ──────────────────────────────────────────────────────────

    onMounted(() => {
        engine = initEngine();
        connect();
        window.addEventListener("beforeunload", disconnect);
    });

    onUnmounted(() => {
        disconnect();
        engine = null;
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
