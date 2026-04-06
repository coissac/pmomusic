/**
 * PMOPlayer - Invisible remote-controlled audio player for browser
 *
 * Receives commands from Web Media Renderer (backend)
 * Reports position/state back to backend via HTTP
 *
 * Usage:
 *   const player = new PMOPlayer('my-instance-id');
 *   player.on('play', () => console.log('playing'));
 */

export type PlayerState = 'playing' | 'paused' | 'stopped' | 'buffering' | 'error';
export type ReadyState = 'have_nothing' | 'have_metadata' | 'have_current_data' | 'have_future_data' | 'can_play' | 'can_play_through';

export interface PlayerEvents {
    play: () => void;
    pause: () => void;
    stop: () => void;
    flush: () => void;
    positionchange: (position_sec: number) => void;
    durationchange: (duration_sec: number) => void;
    statechange: (state: PlayerState) => void;
    trackchange: (track: TrackInfo) => void;
    readychange: (ready: ReadyState) => void;
    error: (error: string) => void;
}

export interface TrackInfo {
    id: string;
    title: string;
    artist: string;
    album?: string;
    cover?: string;
}

// Types stricts pour les commandes reçues du backend (P6)
interface StreamCommand {
    type: 'stream';
    url: string;
}

interface PlayCommand {
    type: 'play';
}

interface PauseCommand {
    type: 'pause';
}

interface SeekCommand {
    type: 'seek';
    timestamp: number;
}

interface FlushCommand {
    type: 'flush';
}

interface StopCommand {
    type: 'stop';
}

type CommandMessage = StreamCommand | PlayCommand | PauseCommand | SeekCommand | FlushCommand | StopCommand;

function isValidCommand(msg: Record<string, unknown> | unknown): msg is CommandMessage {
    if (!msg || typeof msg !== 'object') return false;
    if (!('type' in msg)) return false;
    
    const type = (msg as Record<string, unknown>).type;
    if (typeof type !== 'string') return false;
    
    // Valider les champs selon le type
    switch (type) {
        case 'stream':
            return 'url' in msg && typeof (msg as StreamCommand).url === 'string';
        case 'seek':
            return 'timestamp' in msg && typeof (msg as SeekCommand).timestamp === 'number';
        case 'play':
        case 'pause':
        case 'flush':
        case 'stop':
            return true;
        default:
            return false;
    }
}

export class PMOPlayer {
    private audio: HTMLAudioElement;
    private instanceId: string;
    private ac: AudioContext | null = null;

    private state: PlayerState = 'stopped';
    private positionInterval: number | null = null;
    private commandInterval: number | null = null;
    private listeners: Partial<PlayerEvents> = {};
    private debug: boolean = false;
    private pendingPlay = false;
    private unlockListener: (() => void) | null = null;
    private reconnectAttempts = 0;
    private readonly MAX_RECONNECT_ATTEMPTS = 5;
    private reconnectTimeout: number | null = null;

    constructor(instanceId: string) {
        this.instanceId = instanceId;
        console.log('[PMOPlayer] constructor called for:', instanceId);
        this.audio = new Audio();

        this.audio.preload = 'auto';
        this.audio.style.display = 'none';
        this.audio.style.visibility = 'hidden';
        this.audio.style.position = 'absolute';
        this.audio.style.width = '0';
        this.audio.style.height = '0';
        this.audio.style.overflow = 'hidden';
        document.body.appendChild(this.audio);

        this.setupAudioListeners();
        this.startCommandPolling();
    }

    private ensureAudioContext(): AudioContext {
        if (!this.ac) {
            this.ac = new AudioContext();
            const source = this.ac.createMediaElementSource(this.audio);
            source.connect(this.ac.destination);
        }
        return this.ac;
    }

    private scheduleReconnect() {
        if (this.reconnectAttempts >= this.MAX_RECONNECT_ATTEMPTS) {
            this.log('max reconnect attempts reached, giving up');
            this.setState('error');
            return;
        }
        const delayMs = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 16000);
        this.reconnectAttempts++;
        this.log(`reconnect attempt ${this.reconnectAttempts} in ${delayMs}ms`);
        this.reconnectTimeout = window.setTimeout(() => {
            const url = this.audio.getAttribute('data-stream-url');
            if (url) {
                this.stream(url);
                this.play();
            }
        }, delayMs);
    }

    private log(...args: unknown[]) {
        if (this.debug) {
            console.log('[PMOPlayer]', ...args);
        }
    }

    private async fetchCommand() {
        try {
            const resp = await fetch(`/api/webrenderer/${this.instanceId}/command`);
            if (resp.status === 204) {
                // No command pending
                return;
            }
            if (resp.ok) {
                const text = await resp.text();
                if (text) {
                    const cmd = JSON.parse(text);
                    if (cmd && cmd.type) {
                        this.log('fetched command', cmd);
                        this.handleCommand(cmd);
                    }
                }
            }
        } catch (err) {
            this.log('fetch command error', err);
        }
    }

    private startCommandPolling() {
        this.commandInterval = window.setInterval(() => {
            // ATTENTION : ne pas conditionner ce poll à l'état courant.
            // L'état initial est 'stopped', et c'est dans cet état que le backend
            // envoie la première commande 'stream' pour démarrer la lecture.
            // Filtrer sur state !== 'stopped' casse le chemin nominal de démarrage.
            this.fetchCommand();
        }, 500);
    }

    private setupAudioListeners() {
        this.audio.addEventListener('play', () => {
            this.log('play event');
            this.setState('playing');
            this.startPositionReporting();
        });

        this.audio.addEventListener('pause', () => {
            this.log('pause event');
            this.setState('paused');
        });

        this.audio.addEventListener('ended', () => {
            this.log('ended event');
            this.setState('stopped');
            this.stopPositionReporting();
        });

        this.audio.addEventListener('error', () => {
            if (!this.audio.getAttribute('src')) return;
            const code = this.audio.error?.code;
            const isNetworkError = code === MediaError.MEDIA_ERR_NETWORK
                                || code === MediaError.MEDIA_ERR_DECODE;
            if (isNetworkError && this.state !== 'stopped') {
                this.log('network error, scheduling reconnect');
                this.scheduleReconnect();
            } else {
                this.log('error event', this.audio.error);
                this.setState('error');
                this.listeners.error?.(this.audio.error?.message || 'unknown error');
            }
        });

        this.audio.addEventListener('waiting', () => {
            this.log('waiting event');
            this.setState('buffering');
        });

        this.audio.addEventListener('canplay', () => {
            this.log('canplay event');
            this.reportReadyState('can_play');
        });

        this.audio.addEventListener('durationchange', () => {
            const dur = this.audio.duration;
            if (isFinite(dur)) {
                this.log('durationchange', dur);
                this.reportDuration(dur);
            }
        });

        this.audio.addEventListener('loadedmetadata', () => {
            this.log('loadedmetadata', this.audio.duration);
            this.reportReadyState('have_metadata');
        });

        this.audio.addEventListener('loadeddata', () => {
            this.log('loadeddata');
            this.reportReadyState('have_current_data');
        });
    }

    private handleCommand(msg: Record<string, unknown>) {
        // Validate command structure before processing (P6)
        if (!isValidCommand(msg)) {
            console.warn('[PMOPlayer] Invalid command received:', msg);
            return;
        }

        const type = msg.type;

        switch (type) {
            case 'stream': {
                this.playStream(msg.url);
                break;
            }
            case 'play': {
                const streamUrl = `/api/webrenderer/${this.instanceId}/stream`;
                this.playStream(streamUrl);
                break;
            }
            case 'pause':
                this.pause();
                break;
            case 'seek':
                this.seek(msg.timestamp);
                break;
            case 'flush':
                this.flush();
                break;
            case 'stop':
                this.stop();
                break;
        }
    }

    // ─── Commands from backend ─────────────────────────────────────────────

    stream(url: string) {
        this.log('stream:', url);
        this.audio.setAttribute('data-stream-url', url);
        this.reconnectAttempts = 0;
        this.audio.src = url;
        this.audio.load();
    }

    playStream(url: string) {
        this.stream(url);
        this.play();
    }

    play() {
        this.log('play()');
        const ac = this.ensureAudioContext();
        if (ac.state === 'suspended') {
            ac.resume().catch(err => this.log('AudioContext resume error', err));
        }
        this.audio.play().catch(err => {
            if ((err as DOMException).name === 'NotAllowedError') {
                this.log('autoplay blocked, will retry on user interaction');
                this.pendingPlay = true;
                this.setupAutoplayUnlock();
            } else {
                this.log('play error', err);
            }
        });
    }

    private setupAutoplayUnlock() {
        if (this.unlockListener) return;
        this.unlockListener = () => {
            if (this.pendingPlay) {
                this.pendingPlay = false;
                this.log('retrying play after user interaction');
                this.audio.play().catch(err => this.log('play retry error', err));
            }
            document.removeEventListener('click', this.unlockListener!);
            this.unlockListener = null;
        };
        document.addEventListener('click', this.unlockListener);
    }

    pause() {
        this.log('pause()');
        this.audio.pause();
    }

    seek(timestamp: number) {
        this.log('seek:', timestamp);
        this.audio.currentTime = timestamp;
    }

    flush() {
        this.log('flush()');
        this.audio.pause();
        this.audio.removeAttribute('src');
        this.audio.load();
        this.ac?.suspend();
        this.listeners.flush?.();
    }

    stop() {
        this.log('stop()');
        this.flush();
        this.setState('stopped');
    }

    // ─── Reports to backend ──────────────────────────────────────────

    private setState(state: PlayerState) {
        if (this.state !== state) {
            this.state = state;
            this.log('state:', state);
            this.listeners.statechange?.(state);

            this.httpReport('report', {
                state: state,
            });
        }
    }

    private reportPosition() {
        const pos = this.audio.currentTime;
        const dur = isFinite(this.audio.duration) ? this.audio.duration : null;

        this.listeners.positionchange?.(pos);

        this.httpReport('report', {
            position_sec: pos,
            duration_sec: dur,
            state: this.state,
        });
    }

    private reportDuration(dur: number) {
        this.listeners.durationchange?.(dur);
    }

    private reportReadyState(ready: ReadyState) {
        this.log('ready_state:', ready);
        this.listeners.readychange?.(ready);

        this.httpReport('report', {
            ready_state: ready,
        });
    }

    private async httpReport(endpoint: string, data: Record<string, unknown>) {
        try {
            await fetch(`/api/webrenderer/${this.instanceId}/${endpoint}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(data),
            });
        } catch (err) {
            this.log('http report error', err);
        }
    }

    private startPositionReporting() {
        this.stopPositionReporting();
        this.positionInterval = window.setInterval(() => {
            if (this.state === 'playing') {
                this.reportPosition();
            }
        }, 1000);
    }

    private stopPositionReporting() {
        if (this.positionInterval !== null) {
            clearInterval(this.positionInterval);
            this.positionInterval = null;
        }
    }

    // ─── Public API ───────────────────────────────────────────────────────────

    on<K extends keyof PlayerEvents>(event: K, fn: PlayerEvents[K]) {
        this.listeners[event] = fn;
    }

    off<K extends keyof PlayerEvents>(event: K) {
        delete this.listeners[event];
    }

    getState(): PlayerState {
        return this.state;
    }

    getPosition(): number {
        return this.audio.currentTime;
    }

    getDuration(): number | null {
        const dur = this.audio.duration;
        return isFinite(dur) ? dur : null;
    }

    // Track info from backend - not implemented yet
    getTrack(): null {
        return null;
    }

    setDebug(enabled: boolean) {
        this.debug = enabled;
    }

    destroy() {
        this.log('destroy()');
        this.stopPositionReporting();
        if (this.commandInterval !== null) {
            clearInterval(this.commandInterval);
            this.commandInterval = null;
        }
        if (this.unlockListener) {
            document.removeEventListener('click', this.unlockListener);
            this.unlockListener = null;
        }
        if (this.reconnectTimeout !== null) {
            clearTimeout(this.reconnectTimeout);
            this.reconnectTimeout = null;
        }
        this.ac?.close();
        this.ac = null;
        // Rapport final garanti via sendBeacon (fonctionne pendant beforeunload)
        navigator.sendBeacon(
            `/api/webrenderer/${this.instanceId}/report`,
            JSON.stringify({ state: 'stopped' })
        );
        this.audio.pause();
        this.audio.removeAttribute('src');
        this.audio.load();
        this.audio.remove();
    }
}
