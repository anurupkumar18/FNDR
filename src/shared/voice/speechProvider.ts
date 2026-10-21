/**
 * The contract every way FNDR can speak implements. The registry
 * (`speechRegistry.ts`) picks one in preference order and falls back to the
 * next when one is unavailable, fails, or does not start in time.
 */

export type SpeechProviderId = "codex_realtime" | "system_enhanced" | "local_neural" | "webview_basic";

export interface SpeakOptions {
    /** A provider-specific voice id; a provider ignores one it does not know. */
    voice?: string;
    /** 1 is the provider's natural pace. */
    rate?: number;
    /** Cuts in front of anything queued or playing. */
    urgent?: boolean;
}

export interface SpeechProvider {
    id: SpeechProviderId;
    label: string;
    available(): Promise<{ ok: boolean; reason?: string }>;
    /** Resolves when finished or cancelled; rejects when speech failed. */
    speak(text: string, opts?: SpeakOptions): Promise<void>;
    cancel(): void;
    speaking(): boolean;
}
