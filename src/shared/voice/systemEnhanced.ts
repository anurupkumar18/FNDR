/**
 * The best macOS voice installed, through the webview's speech synthesis.
 *
 * WKWebView lists the same AVSpeechSynthesizer voices a native helper would
 * see, and each voiceURI carries Apple's quality tier
 * (`com.apple.voice.premium.en-US.Zoe`, `...enhanced...`, `...compact...`), so
 * ranking here needs no native code. It lists no voices until its synthesizer
 * has warmed up, which `available()` waits for. Measured on the owner's Mac on
 * 2026-10-09: 69 voices listed after warm-up, none Premium or Enhanced, so the
 * voice sounded robotic because only compact voices were installed.
 */

import type { SpeakOptions, SpeechProvider } from "./speechProvider";

export type VoiceQuality = "siri" | "premium" | "enhanced" | "compact";

export interface VoiceLike {
    name: string;
    voiceURI: string;
    lang: string;
    localService: boolean;
    default: boolean;
}

export interface RankedVoice {
    id: string;
    name: string;
    lang: string;
    quality: VoiceQuality;
    voice: VoiceLike;
}

export interface SynthLike {
    speak(utterance: never): void;
    cancel(): void;
    readonly speaking: boolean;
    getVoices(): VoiceLike[];
    addEventListener?(type: "voiceschanged", listener: () => void): void;
    removeEventListener?(type: "voiceschanged", listener: () => void): void;
}

export interface UtteranceLike {
    text: string;
    voice: VoiceLike | null;
    rate: number;
    pitch: number;
    lang: string;
    onend: (() => void) | null;
    onerror: ((event: { error: string }) => void) | null;
}

export type MakeUtterance = new (text: string) => UtteranceLike;

export interface SystemSpeechDeps {
    synth?: SynthLike;
    Utterance?: MakeUtterance;
    lang?: string;
    maxChunk?: number;
    voiceWaitMs?: number;
}

/** Where a person downloads a natural voice; FNDR cannot do it for them. */
export const PREMIUM_VOICE_GUIDE =
    "For a natural voice, open System Settings, Accessibility, Spoken Content, choose System Voice, then Manage Voices, and download a Premium or Enhanced voice such as Zoe or Ava. Reopen this window once it has downloaded.";

/** The system voices' own pace and pitch, which Apple tunes per voice. Not judged by ear; change only after listening. */
export const NATURAL_RATE = 1;
export const NATURAL_PITCH = 1;
const MAX_CHUNK = 200;
const PREVIEW_LINE = "This is how FNDR sounds when it talks you through a task.";

const TIER: Record<VoiceQuality, number> = { siri: 4, premium: 3, enhanced: 2, compact: 1 };

function qualityOf(voice: VoiceLike): { quality: VoiceQuality; detail: number } | null {
    const key = `${voice.voiceURI} ${voice.name}`.toLowerCase();
    if (key.includes("com.apple.speech.synthesis.voice.") || key.includes("com.apple.eloquence")) return null;
    if (key.includes("siri")) return { quality: "siri", detail: 0 };
    if (key.includes("premium")) return { quality: "premium", detail: 0 };
    if (key.includes("enhanced")) return { quality: "enhanced", detail: 0 };
    return { quality: "compact", detail: key.includes("super-compact") ? 0 : 1 };
}

function displayName(name: string): string {
    return name.replace(/\s*\((premium|enhanced)\)\s*$/i, "").trim();
}

function baseLang(lang: string): string {
    return lang.toLowerCase().split(/[-_]/)[0];
}

/** Usable voices, best first: quality tier, then the person's exact region, then the system default. */
export function rankVoices(voices: VoiceLike[], lang: string): RankedVoice[] {
    const wanted = lang.toLowerCase().replace("_", "-");
    const scored = voices
        .filter((v) => v.localService)
        .map((v) => ({ v, q: qualityOf(v) }))
        .filter((x): x is { v: VoiceLike; q: { quality: VoiceQuality; detail: number } } => x.q !== null);
    const sameLanguage = scored.filter((x) => baseLang(x.v.lang) === baseLang(wanted));
    const pool = sameLanguage.length > 0 ? sameLanguage : scored;
    const score = (x: (typeof pool)[number]) => [
        TIER[x.q.quality],
        x.v.lang.toLowerCase().replace("_", "-") === wanted ? 1 : 0,
        x.q.detail,
        x.v.default ? 1 : 0,
    ];
    pool.sort((a, b) => {
        const sa = score(a);
        const sb = score(b);
        for (let i = 0; i < sa.length; i += 1) if (sa[i] !== sb[i]) return sb[i] - sa[i];
        return a.v.name.localeCompare(b.v.name);
    });
    const seen = new Set<string>();
    const ranked: RankedVoice[] = [];
    for (const { v, q } of pool) {
        const name = displayName(v.name);
        const key = `${name}|${v.lang}`;
        if (seen.has(key)) continue;
        seen.add(key);
        ranked.push({ id: v.voiceURI, name, lang: v.lang, quality: q.quality, voice: v });
    }
    return ranked;
}

function splitLong(sentence: string, max: number): string[] {
    const pieces: string[] = [];
    let rest = sentence;
    while (rest.length > max) {
        const window = rest.slice(0, max + 1);
        const comma = window.lastIndexOf(", ");
        const space = window.lastIndexOf(" ");
        const cut = comma > 0 && comma + 1 <= max ? comma + 1 : space > 0 ? space : max;
        pieces.push(rest.slice(0, cut).trim());
        rest = rest.slice(cut).trim();
    }
    if (rest) pieces.push(rest);
    return pieces;
}

/** Sentence-sized pieces so a long answer does not stall or stutter in the synthesizer. */
export function chunkForSpeech(text: string, max = MAX_CHUNK): string[] {
    const clean = text.replace(/\s+/g, " ").trim();
    if (!clean) return [];
    const sentences = (clean.match(/[^.!?;]+[.!?;]+["')\]]*|[^.!?;]+$/g) ?? [clean]).map((s) => s.trim()).filter(Boolean);
    const chunks: string[] = [];
    let current = "";
    for (const sentence of sentences.flatMap((s) => splitLong(s, max))) {
        if (current && current.length + 1 + sentence.length <= max) current = `${current} ${sentence}`;
        else {
            if (current) chunks.push(current);
            current = sentence;
        }
    }
    if (current) chunks.push(current);
    return chunks;
}

/**
 * Plays utterances in order on one synthesizer. Shared by the system voice and
 * the last-resort webview voice so both cancel and finish the same way.
 */
export function createUtterancePlayer(synth: SynthLike, Utterance: MakeUtterance) {
    let turn = 0;
    let active = false;
    return {
        play(texts: string[], configure: (u: UtteranceLike) => void): Promise<void> {
            if (active) synth.cancel();
            const mine = ++turn;
            if (texts.length === 0) return Promise.resolve();
            active = true;
            return new Promise<void>((resolve, reject) => {
                let settled = false;
                const settle = (error?: string) => {
                    if (settled) return;
                    settled = true;
                    if (mine === turn) active = false;
                    if (error) reject(new Error(`Speech failed: ${error}`));
                    else resolve();
                };
                texts.forEach((text, index) => {
                    const utterance = new Utterance(text);
                    configure(utterance);
                    utterance.onend = index === texts.length - 1 ? () => settle() : null;
                    utterance.onerror = (event) => {
                        const benign = mine !== turn || event.error === "canceled" || event.error === "interrupted";
                        settle(benign ? undefined : event.error);
                    };
                    synth.speak(utterance as never);
                });
            });
        },
        cancel() {
            turn += 1;
            active = false;
            synth.cancel();
        },
        speaking: () => active,
    };
}

function defaultSynth(): SynthLike | undefined {
    return typeof window === "undefined" || !window.speechSynthesis ? undefined : (window.speechSynthesis as unknown as SynthLike);
}

function defaultUtterance(): MakeUtterance | undefined {
    return typeof SpeechSynthesisUtterance === "undefined" ? undefined : (SpeechSynthesisUtterance as unknown as MakeUtterance);
}

function defaultLang(): string {
    return typeof navigator === "undefined" ? "en-US" : navigator.language || "en-US";
}

/** Voices are listed only once the synthesizer has warmed up. */
export function loadVoices(synth: SynthLike, waitMs = 1000): Promise<VoiceLike[]> {
    const now = synth.getVoices();
    if (now.length > 0 || waitMs <= 0 || !synth.addEventListener) return Promise.resolve(now);
    return new Promise((resolve) => {
        const done = () => {
            clearTimeout(timer);
            synth.removeEventListener?.("voiceschanged", done);
            resolve(synth.getVoices());
        };
        const timer = setTimeout(done, waitMs);
        synth.addEventListener!("voiceschanged", done);
    });
}

export interface SystemEnhancedProvider extends SpeechProvider {
    /** The voices a Settings picker offers, best first. */
    voices(): Promise<RankedVoice[]>;
}

export function createSystemEnhancedProvider(deps: SystemSpeechDeps = {}): SystemEnhancedProvider {
    const synth = "synth" in deps ? deps.synth : defaultSynth();
    const Utterance = "Utterance" in deps ? deps.Utterance : defaultUtterance();
    const lang = deps.lang ?? defaultLang();
    const maxChunk = deps.maxChunk ?? MAX_CHUNK;
    const player = synth && Utterance ? createUtterancePlayer(synth, Utterance) : null;

    const voices = async () => (synth ? rankVoices(await loadVoices(synth, deps.voiceWaitMs), lang) : []);

    return {
        id: "system_enhanced",
        label: "Mac voice",
        voices,
        async available() {
            if (!player) return { ok: false, reason: "This window has no speech synthesis." };
            const ranked = await voices();
            if (ranked.length === 0) return { ok: false, reason: "No system voice is installed." };
            if (ranked[0].quality === "compact") return { ok: true, reason: `Only compact voices are installed. ${PREMIUM_VOICE_GUIDE}` };
            return { ok: true };
        },
        speak(text: string, opts: SpeakOptions = {}) {
            if (!player || !synth) return Promise.reject(new Error("This window has no speech synthesis."));
            const ranked = rankVoices(synth.getVoices(), lang);
            const chosen = ranked.find((v) => v.id === opts.voice) ?? ranked[0];
            return player.play(chunkForSpeech(text, maxChunk), (u) => {
                if (chosen) {
                    u.voice = chosen.voice;
                    u.lang = chosen.lang;
                }
                u.rate = opts.rate ?? NATURAL_RATE;
                u.pitch = NATURAL_PITCH;
            });
        },
        cancel() {
            player?.cancel();
        },
        speaking() {
            return player?.speaking() ?? false;
        },
    };
}

/** Speaks a sample line in one voice so a person can compare before choosing. */
export function previewVoice(voiceId: string, deps: SystemSpeechDeps & { rate?: number } = {}): Promise<void> {
    return createSystemEnhancedProvider(deps).speak(PREVIEW_LINE, { voice: voiceId, rate: deps.rate });
}
