import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SpeechProvider, SpeechProviderId } from "./speechProvider";
import { createSpeechRegistry, type SpeechRegistry } from "./speechRegistry";
import type { RankedVoice } from "./systemEnhanced";
import type { TtsStatus } from "@/shared/ipc/tauri";
import { VoiceOutputSection, type NeuralVoiceControls } from "./VoiceOutputSection";

const mocks = vi.hoisted(() => ({
    get: vi.fn(),
    set: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    getVoiceOutputSettings: mocks.get,
    setVoiceOutputSettings: mocks.set,
    ttsStatus: vi.fn(),
    ttsVoices: vi.fn(),
    ttsInstall: vi.fn(),
    ttsRemove: vi.fn(),
    ttsCancel: vi.fn(),
    ttsSynthesize: vi.fn(),
}));

function provider(id: SpeechProviderId, available: { ok: boolean; reason?: string }): SpeechProvider {
    return {
        id,
        label: id,
        available: () => Promise.resolve(available),
        speak: () => Promise.resolve(),
        cancel: () => undefined,
        speaking: () => false,
    };
}

function ranked(name: string, quality: RankedVoice["quality"]): RankedVoice {
    const id = `com.apple.voice.${quality}.en-US.${name}`;
    return { id, name, lang: "en-US", quality, voice: { name, voiceURI: id, lang: "en-US", localService: true, default: false } };
}

const PREMIUM = [ranked("Zoe", "premium"), ranked("Samantha", "compact")];
const COMPACT = [ranked("Samantha", "compact")];

describe("VoiceOutputSection", () => {
    let registry: SpeechRegistry;

    beforeEach(() => {
        registry = createSpeechRegistry({ storage: null });
        mocks.get.mockResolvedValue({ provider: "auto", system_voice: "", rate: 1 });
        mocks.set.mockImplementation((settings) => Promise.resolve(settings));
    });

    afterEach(() => {
        cleanup();
        vi.clearAllMocks();
    });

    function renderSection(voices = PREMIUM, preview = vi.fn(() => Promise.resolve())) {
        render(<VoiceOutputSection registry={registry} listVoices={() => Promise.resolve(voices)} preview={preview} />);
        return preview;
    }

    it("says the ChatGPT plan voice is in use when it is available", async () => {
        registry.registerProvider(provider("codex_realtime", { ok: true }));
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection();
        expect(await screen.findByText("Using your ChatGPT plan")).toBeInTheDocument();
    });

    it("names the on-device voice and why the ChatGPT plan voice is not used", async () => {
        registry.registerProvider(provider("codex_realtime", { ok: false, reason: "Sign in to ChatGPT in Settings." }));
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection();
        expect(await screen.findByText("Using an on-device voice (Sign in to ChatGPT in Settings.)")).toBeInTheDocument();
    });

    it("says when the ChatGPT plan voice is not built into this version", async () => {
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection();
        expect(await screen.findByText(/ChatGPT plan voice is not available in this version/)).toBeInTheDocument();
    });

    it("lists the best voices first, previews one and saves the choice", async () => {
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        const preview = renderSection();
        const picker = await screen.findByLabelText("Mac voice");
        const options = Array.from((picker as HTMLSelectElement).options).map((o) => o.textContent);
        expect(options).toEqual(["Best installed (Zoe)", "Zoe, Premium", "Samantha, Compact"]);
        fireEvent.change(picker, { target: { value: PREMIUM[1].id } });
        await waitFor(() => expect(mocks.set).toHaveBeenCalledWith({ provider: "auto", system_voice: PREMIUM[1].id, rate: 1 }));
        expect(registry.settings().voice).toBe(PREMIUM[1].id);
        fireEvent.click(screen.getByRole("button", { name: "Preview voice" }));
        expect(preview).toHaveBeenCalledWith(PREMIUM[1].id, 1);
    });

    it("saves the speed and the provider choice", async () => {
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        registry.registerProvider(provider("webview_basic", { ok: true }));
        renderSection();
        const speed = await screen.findByLabelText("Speed");
        fireEvent.change(speed, { target: { value: "1.2" } });
        await waitFor(() => expect(mocks.set).toHaveBeenLastCalledWith(expect.objectContaining({ rate: 1.2 })));
        expect(registry.settings().rate).toBe(1.2);
        fireEvent.change(screen.getByLabelText("Speak with"), { target: { value: "webview_basic" } });
        await waitFor(() => expect(mocks.set).toHaveBeenLastCalledWith(expect.objectContaining({ provider: "webview_basic" })));
        expect(registry.order()[0]).toBe("webview_basic");
    });

    it("applies the saved settings to the registry when it opens", async () => {
        mocks.get.mockResolvedValue({ provider: "system_enhanced", system_voice: PREMIUM[0].id, rate: 0.9 });
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection();
        await waitFor(() => expect(registry.settings()).toEqual({ preferred: "system_enhanced", voice: PREMIUM[0].id, rate: 0.9 }));
        expect(((await screen.findByLabelText("Speed")) as HTMLInputElement).value).toBe("0.9");
    });

    it("explains how to download a Premium voice when only compact voices exist", async () => {
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection(COMPACT);
        expect(await screen.findByText(/Spoken Content/)).toBeInTheDocument();
    });

    it("shows a save failure", async () => {
        mocks.set.mockRejectedValue(new Error("disk full"));
        registry.registerProvider(provider("system_enhanced", { ok: true }));
        renderSection();
        const speed = await screen.findByLabelText("Speed");
        await act(async () => {
            fireEvent.change(speed, { target: { value: "1.5" } });
        });
        expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    });

    describe("on-device neural voice", () => {
        const status = (state: TtsStatus["state"], downloadedBytes = 0): TtsStatus => ({
            state,
            detail: null,
            downloadedBytes,
            totalBytes: 134_000_000,
            loaded: false,
        });

        function neural(initial: TtsStatus): NeuralVoiceControls & { current: TtsStatus } {
            const controls = {
                current: initial,
                status: vi.fn(async () => controls.current),
                voices: vi.fn(async () => ({
                    voices: [
                        { id: "af_heart", name: "Heart", accent: "us" as const, gender: "female" as const, grade: "A" },
                        { id: "bf_emma", name: "Emma", accent: "gb" as const, gender: "female" as const, grade: "B-" },
                    ],
                    defaultVoice: "af_heart",
                })),
                install: vi.fn(async () => {
                    controls.current = status("downloading", 67_000_000);
                    return controls.current;
                }),
                remove: vi.fn(async () => {
                    controls.current = status("not_installed");
                    return controls.current;
                }),
                preview: vi.fn(async () => undefined),
            };
            return controls;
        }

        function renderNeural(controls: NeuralVoiceControls) {
            registry.registerProvider(provider("local_neural", { ok: false, reason: "not installed" }));
            registry.registerProvider(provider("system_enhanced", { ok: true }));
            render(<VoiceOutputSection registry={registry} listVoices={() => Promise.resolve(PREMIUM)} preview={vi.fn()} neural={controls} />);
        }

        it("asks before downloading, names the size and says it runs on this Mac", async () => {
            const controls = neural(status("not_installed"));
            renderNeural(controls);
            expect(await screen.findByText(/runs fully on this Mac/)).toBeInTheDocument();
            fireEvent.click(await screen.findByRole("button", { name: "Install (134 MB)" }));
            expect(controls.install).not.toHaveBeenCalled();
            expect(screen.getByRole("note")).toHaveTextContent("Hugging Face");
            fireEvent.click(screen.getByRole("button", { name: "Not now" }));
            expect(screen.queryByRole("note")).toBeNull();
            fireEvent.click(screen.getByRole("button", { name: "Install (134 MB)" }));
            fireEvent.click(screen.getByRole("button", { name: "Download 134 MB" }));
            await waitFor(() => expect(controls.install).toHaveBeenCalledWith(true));
            expect(await screen.findByText("50% of 134 MB")).toBeInTheDocument();
        });

        it("shows the voices once ready, saves the choice, previews and removes", async () => {
            const controls = neural(status("ready"));
            renderNeural(controls);
            const picker = (await screen.findByLabelText("On-device voice")) as HTMLSelectElement;
            await waitFor(() => expect(picker.options).toHaveLength(2));
            expect(picker.options[1].textContent).toBe("Emma, British female");
            fireEvent.change(picker, { target: { value: "bf_emma" } });
            await waitFor(() => expect(mocks.set).toHaveBeenLastCalledWith(expect.objectContaining({ neural_voice: "bf_emma" })));
            fireEvent.click(screen.getByRole("button", { name: "Preview" }));
            expect(controls.preview).toHaveBeenCalledWith("bf_emma", 1);
            fireEvent.click(screen.getByRole("button", { name: "Remove (134 MB)" }));
            await waitFor(() => expect(controls.remove).toHaveBeenCalled());
            expect(await screen.findByRole("button", { name: "Install (134 MB)" })).toBeInTheDocument();
        });

        it("is hidden where the neural voice is not registered", async () => {
            registry.registerProvider(provider("system_enhanced", { ok: true }));
            render(<VoiceOutputSection registry={registry} listVoices={() => Promise.resolve(PREMIUM)} preview={vi.fn()} neural={neural(status("ready"))} />);
            await screen.findByLabelText("Mac voice");
            expect(screen.queryByText("On-device neural voice")).toBeNull();
        });
    });
});
