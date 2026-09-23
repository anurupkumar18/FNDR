import { emit } from "@tauri-apps/api/event";
import { CAPTURE_STATUS_EVENT, SCREEN_GUIDE_STATE_EVENT } from "@/shared/ipc/tauri";
import type {
    CaptureStatus,
    ComposedAnswer,
    MemoryCard,
    MemoryReviewWorkerStatus,
    NeedsSignalCard,
    PrivacyAlert,
    PrivacyProof,
    RuntimeMetricsSnapshot,
    ScreenGuideSettings,
    Stats,
    Task,
    WeeklyWrapped,
} from "@/shared/ipc/tauri";
import type { ModelInfo, OnboardingState } from "@/shared/ipc/onboarding";

type PreviewIpcHandler = (command: string, payload?: unknown) => Promise<unknown>;

const previewNow = Date.parse("2026-09-22T22:00:00.000Z");

const previewMemoryCards: MemoryCard[] = [
    {
        id: "memory-design-review",
        title: "Reviewed the FNDR home experience",
        summary: "Compared the home hierarchy, search prominence, and wallpaper contrast before the UI overhaul.",
        display_summary: "The search action should stay primary while the wallpaper supports it quietly.",
        action: "Reviewed a product design",
        context: ["Home", "visual hierarchy", "accessibility"],
        timestamp: previewNow - 24 * 60 * 1000,
        app_name: "Figma",
        window_title: "FNDR — Home concepts",
        score: 0.96,
        source_count: 7,
        continuity: true,
        raw_snippets: [],
        confidence: 0.94,
        activity_type: "docs",
        session_duration_mins: 38,
        insight_what_happened: "Reviewed the landing screen and clarified the search-first hierarchy.",
        insight_why_mattered: "A stable foreground keeps every palette readable and makes the next action obvious.",
        insight_what_changed: "Removed a false scroll cue and moved copy onto semantic contrast tokens.",
        topic_categories: ["product design", "accessibility"],
        enrichment_status: "reviewed_local",
        storage_outcome: "enriched_memory_card",
    },
    {
        id: "memory-preview-harness",
        title: "Built a safe browser preview for FNDR",
        summary: "Mounted the real React shell behind strict synthetic Tauri IPC fixtures for repeatable UI review.",
        display_summary: "The browser can now exercise the real interface without requesting capture or biometric permissions.",
        action: "Implemented a development workflow",
        context: ["React", "Tauri mocks", "browser QA"],
        timestamp: previewNow - 76 * 60 * 1000,
        app_name: "Visual Studio Code",
        window_title: "previewIpc.ts — FNDR",
        score: 0.93,
        source_count: 5,
        raw_snippets: [],
        confidence: 0.92,
        activity_type: "coding",
        files_touched: ["src/dev/previewIpc.ts", "src/dev/uiPreview.tsx"],
        session_duration_mins: 51,
        topic_categories: ["frontend", "quality"],
        enrichment_status: "reviewed_local",
        storage_outcome: "enriched_memory_card",
    },
    {
        id: "memory-accessibility-notes",
        title: "Collected accessibility acceptance criteria",
        summary: "Outlined keyboard, focus, contrast, reduced-motion, and 200% text checks for every major surface.",
        display_summary: "Every feature needs usable empty, loading, error, permission-denied, and long-content states.",
        action: "Researched inclusive interaction patterns",
        context: ["WCAG", "keyboard", "reduced motion"],
        timestamp: previewNow - 3 * 60 * 60 * 1000,
        app_name: "Google Chrome",
        window_title: "Inclusive product interface checklist",
        url: "https://www.w3.org/WAI/WCAG22/quickref/",
        reopen_target: "https://www.w3.org/WAI/WCAG22/quickref/",
        score: 0.9,
        source_count: 4,
        raw_snippets: [],
        confidence: 0.9,
        activity_type: "browsing",
        session_duration_mins: 27,
        topic_categories: ["accessibility"],
        enrichment_status: "reviewed_local",
        storage_outcome: "enriched_memory_card",
    },
    {
        id: "memory-roadmap",
        title: "Sequenced the UI overhaul into vertical slices",
        summary: "Prioritized trust and accessibility, then shell, search, vault, utility panels, and native verification.",
        display_summary: "Each slice should ship with browser evidence and a narrow regression test.",
        action: "Planned the product roadmap",
        context: ["priorities", "acceptance criteria", "verification"],
        timestamp: previewNow - 22 * 60 * 60 * 1000,
        app_name: "Terminal",
        window_title: "FNDR UI audit notes",
        score: 0.88,
        source_count: 3,
        raw_snippets: [],
        confidence: 0.91,
        activity_type: "docs",
        session_duration_mins: 19,
        topic_categories: ["roadmap", "product"],
        enrichment_status: "reviewed_daily",
        storage_outcome: "enriched_memory_card",
    },
];

const previewNeedsSignalCards: NeedsSignalCard[] = [
    {
        card: {
            id: "memory-needs-review",
            title: "Unclear browser transition",
            summary: "A brief transition was captured without enough readable task context.",
            action: "Awaiting review",
            context: [],
            timestamp: previewNow - 11 * 60 * 1000,
            app_name: "Google Chrome",
            window_title: "New Tab",
            score: 0.31,
            source_count: 1,
            raw_snippets: [],
            confidence: 0.28,
            activity_type: "browsing",
            enrichment_status: "pending",
            storage_outcome: "metadata_only",
        },
        reason_code: "low_readable_signal",
        reason: "FNDR found too little readable task context to make this memory searchable.",
    },
];

const previewSimilarResults = [
    {
        id: "similar-home-review",
        timestamp: previewNow - 2 * 60 * 60 * 1000,
        app_name: "Figma",
        window_title: "FNDR — Contrast concepts",
        session_id: "preview-design-session",
        text: "Compared a stable foreground card against the ambient Film wallpaper.",
        snippet: "Compared foreground contrast options for the FNDR home experience.",
        display_summary: "A related visual review of the Home foreground and background layers.",
        score: 0.89,
    },
];

const captureStatus: CaptureStatus = {
    is_capturing: true,
    is_paused: false,
    is_incognito: false,
    frames_captured: 1842,
    frames_dropped: 317,
    last_capture_time: previewNow,
    ai_model_available: true,
    ai_model_loaded: true,
    loaded_model_id: "qwen3-vl-2b",
    embedding_backend: "ONNX Runtime",
    embedding_degraded: false,
    embedding_detail: "Local MiniLM embedder ready",
    embedding_model_name: "all-MiniLM-L6-v2",
    embedding_dimension: 384,
    pipeline: {
        evaluated: 2159,
        stored_ocr_path: 1694,
        stored_visual_path: 102,
        stored_url_only: 46,
        stored_total: 1842,
        skipped_blocklist: 42,
        skipped_self_app: 61,
        skipped_surface_policy: 18,
        skipped_perceptual_dup: 93,
        skipped_semantic_dup: 54,
        skipped_ocr_failed: 12,
        skipped_low_signal_text: 21,
        skipped_noise: 8,
        skipped_grounding: 3,
        skipped_stacked_extraction: 0,
        skipped_visual_small: 2,
        skipped_visual_novelty: 1,
        skipped_visual_compose_failed: 2,
        skipped_screen_capture_failed: 2,
        skipped_embedder_unavailable: 0,
        skipped_total: 317,
        last_skip_reason: "semantic duplicate",
        last_skip_app: "Visual Studio Code",
        last_skip_timestamp_ms: previewNow - 18_000,
    },
};

const previewModels: ModelInfo[] = [
    {
        id: "qwen3-vl-2b",
        name: "Qwen3-VL 2B",
        description: "Local visual-language understanding for richer memory context.",
        size_bytes: 2_100_000_000,
        size_label: "2.1 GB",
        quality_label: "Balanced",
        speed_label: "Fast on Apple silicon",
        ram_gb: 4,
        recommended: true,
        required: false,
        filename: "qwen3-vl-2b.gguf",
        download_url: "already_downloaded",
    },
    {
        id: "all-minilm-l6-v2",
        name: "MiniLM embeddings",
        description: "Local semantic search embeddings.",
        size_bytes: 90_000_000,
        size_label: "90 MB",
        quality_label: "Search",
        speed_label: "Fast",
        ram_gb: 0.25,
        recommended: true,
        required: true,
        filename: "all-MiniLM-L6-v2.onnx",
        download_url: "already_downloaded",
    },
];

const previewTasks: Task[] = [
    {
        id: "task-share-audit",
        title: "Share the UI audit with the capstone team",
        description: "Confirm the browser walkthrough findings and assign the native-only checks.",
        source_app: "memory: FNDR UI audit",
        source_memory_id: "memory-roadmap",
        created_at: previewNow - 52 * 60 * 1000,
        due_date: previewNow + 24 * 60 * 60 * 1000,
        is_completed: false,
        is_dismissed: false,
        task_type: "Followup",
        linked_urls: [],
        linked_memory_ids: ["memory-roadmap"],
    },
    {
        id: "task-keyboard-pass",
        title: "Run the keyboard-only panel pass",
        description: "Verify focus entry, focus return, Escape, and visible focus for every mounted panel.",
        source_app: "manual",
        source_memory_id: "memory-accessibility-notes",
        created_at: previewNow - 29 * 60 * 1000,
        due_date: null,
        is_completed: false,
        is_dismissed: false,
        task_type: "Todo",
        linked_urls: [],
        linked_memory_ids: ["memory-accessibility-notes"],
    },
    {
        id: "task-native-permissions",
        title: "Verify capture permissions in the native app",
        description: "Browser preview cannot prove macOS Screen Recording, Accessibility, or microphone behavior.",
        source_app: "manual",
        source_memory_id: null,
        created_at: previewNow - 12 * 60 * 1000,
        due_date: previewNow + 2 * 24 * 60 * 60 * 1000,
        is_completed: false,
        is_dismissed: false,
        task_type: "Reminder",
        linked_urls: [],
        linked_memory_ids: [],
    },
];

const previewStats: Stats = {
    total_records: 1842,
    total_days: 18,
    apps: [
        { name: "Visual Studio Code", count: 782 },
        { name: "Google Chrome", count: 526 },
        { name: "Figma", count: 318 },
        { name: "Terminal", count: 216 },
    ],
    today_count: 126,
    unique_apps: 9,
    unique_sessions: 41,
    unique_window_titles: 287,
    unique_urls: 164,
    unique_domains: 32,
    records_with_url: 502,
    records_with_screenshot: 1796,
    records_with_clean_text: 1714,
    records_last_hour: 23,
    records_last_24h: 126,
    records_last_7d: 836,
    avg_records_per_active_day: 102.3,
    avg_records_per_hour: 14.8,
    focus_app_share_pct: 42.5,
    app_switches: 97,
    app_switch_rate_per_hour: 7.4,
    avg_gap_minutes: 4.2,
    longest_gap_minutes: 84,
    first_capture_ts: previewNow - 18 * 24 * 60 * 60 * 1000,
    last_capture_ts: previewNow - 18_000,
    capture_span_hours: 432,
    current_streak_days: 6,
    longest_streak_days: 11,
    avg_ocr_confidence: 0.91,
    low_confidence_records: 38,
    avg_noise_score: 0.08,
    high_noise_records: 17,
    avg_ocr_blocks: 11.6,
    llm_count: 402,
    vlm_count: 102,
    fallback_count: 71,
    other_summary_count: 1267,
    top_domains: [
        { domain: "capstone.cs.utah.edu", count: 94 },
        { domain: "developer.mozilla.org", count: 61 },
        { domain: "w3.org", count: 37 },
    ],
    busiest_day: { day: "2026-09-22", count: 126 },
    quietest_day: { day: "2026-09-18", count: 34 },
    busiest_hour: { hour: 16, count: 48 },
    hourly_distribution: [
        { hour: 9, count: 12 },
        { hour: 10, count: 21 },
        { hour: 11, count: 18 },
        { hour: 13, count: 27 },
        { hour: 14, count: 32 },
        { hour: 15, count: 39 },
        { hour: 16, count: 48 },
        { hour: 17, count: 26 },
    ],
    weekday_distribution: [
        { weekday: "Monday", count: 311 },
        { weekday: "Tuesday", count: 426 },
        { weekday: "Wednesday", count: 344 },
        { weekday: "Thursday", count: 296 },
        { weekday: "Friday", count: 284 },
        { weekday: "Saturday", count: 103 },
        { weekday: "Sunday", count: 78 },
    ],
    daypart_distribution: [
        { daypart: "Morning", count: 472 },
        { daypart: "Afternoon", count: 934 },
        { daypart: "Evening", count: 401 },
        { daypart: "Night", count: 35 },
    ],
};

const previewRuntimeMetrics: RuntimeMetricsSnapshot = {
    generated_at_ms: previewNow,
    process_rss_bytes: 284_164_096,
    capture: {
        frames_captured: captureStatus.frames_captured,
        frames_dropped: captureStatus.frames_dropped,
        last_capture_time_ms: previewNow - 18_000,
    },
    embedding: {
        backend: "ONNX Runtime",
        degraded: false,
        detail: "Local MiniLM embedder ready",
        model_name: "all-MiniLM-L6-v2",
        dimension: 384,
        clip_session_loaded: true,
        last_clip_infer_ms: 41,
    },
    inference: {
        ai_model_available: true,
        ai_model_loaded: true,
        loaded_model_id: "qwen3-vl-2b",
    },
    aggregates: {
        search: {
            n: 24,
            sum_ms: 696,
            max_ms: 61,
            avg_ms: 29,
            ewma_ms: 27.4,
            p50_ms: 25,
            p95_ms: 52,
        },
        capture_flush: {
            n: 48,
            sum_ms: 576,
            max_ms: 28,
            avg_ms: 12,
            ewma_ms: 11.7,
            p50_ms: 10,
            p95_ms: 21,
        },
    },
    counters: {
        "capture.skipped_sensitive": 14,
        "search.timeout": 0,
    },
    recent: [
        { ts_ms: previewNow - 14_000, op: "search", ms: 26, meta: "4 cards" },
        { ts_ms: previewNow - 33_000, op: "capture_flush", ms: 11, meta: null },
    ],
    system: {
        generated_at_ms: previewNow,
        sample_interval_ms: 3000,
        process_cpu: {
            cpu_percent: 7.8,
            user_time_ms: 184_000,
            system_time_ms: 31_000,
            threads: 18,
        },
        process_memory: {
            rss_bytes: 284_164_096,
            virtual_bytes: 3_221_225_472,
            phys_footprint_bytes: 301_989_888,
            lifetime_max_phys_footprint_bytes: 356_515_840,
        },
        process_io: {
            disk_bytes_read: 82_575_360,
            disk_bytes_written: 19_922_944,
            disk_read_rate_bps: 32_768,
            disk_write_rate_bps: 12_288,
        },
        process_energy: {
            idle_wakeups: 14,
            interrupt_wakeups: 3,
            billed_system_time_ns: 9_400_000,
            label: "low",
        },
        host_cpu: {
            cpu_percent_total: 21.6,
            cpu_percent_per_core: [34, 29, 18, 14, 11, 9, 8, 7],
        },
        host_memory: {
            page_size_bytes: 16_384,
            free_bytes: 5_368_709_120,
            active_bytes: 9_663_676_416,
            inactive_bytes: 3_221_225_472,
            wired_bytes: 2_147_483_648,
            compressed_bytes: 1_073_741_824,
            total_bytes: 24_000_000_000,
            pressure_label: "low",
        },
        gpu: {
            device_utilization_percent: 11,
            renderer_utilization_percent: 7,
            in_use_system_memory_bytes: 536_870_912,
            recovery_count: 0,
        },
        model_memory: [
            { id: "qwen3-vl-2b", kind: "vlm", estimated_bytes: 2_100_000_000, loaded: true },
            { id: "all-MiniLM-L6-v2", kind: "embedding", estimated_bytes: 90_000_000, loaded: true },
        ],
    },
};

const previewReviewStatus: MemoryReviewWorkerStatus = {
    queue_depth: 3,
    last_review_at_ms: previewNow - 45_000,
    last_error_kind: null,
    worker_enabled: true,
    pressure_blocked: false,
};

const previewPrivacyProof: PrivacyProof = {
    evaluated: 2159,
    stored: 1842,
    skipped_by_reason: {
        sensitive_context: 14,
        blocklist: 42,
        self_app: 61,
        perceptual_dup: 93,
        semantic_dup: 54,
        low_signal_text: 21,
        screen_capture_failed: 2,
    },
    egress_requests: 0,
    egress_hosts: [],
};

function clonePreview<T>(value: T): T {
    return structuredClone(value);
}

async function emitPreviewEventIfAvailable(event: string, payload: unknown): Promise<void> {
    if (typeof window === "undefined") {
        return;
    }
    const previewWindow = window as Window & {
        __TAURI_INTERNALS__?: { invoke?: unknown };
    };
    if (typeof previewWindow.__TAURI_INTERNALS__?.invoke !== "function") return;
    await emit(event, clonePreview(payload));
}

function payloadRecord(payload: unknown): Record<string, unknown> | null {
    return typeof payload === "object" && payload !== null
        ? payload as Record<string, unknown>
        : null;
}

function requiredString(
    payload: unknown,
    field: string,
    command: string,
): string {
    const value = payloadRecord(payload)?.[field];
    if (typeof value !== "string" || !value.trim()) {
        throw new Error(`Preview ${command} requires ${field}.`);
    }
    return value.trim();
}

function validDate(value: string): boolean {
    return /^\d{4}-\d{2}-\d{2}$/.test(value)
        && Number.isFinite(Date.parse(`${value}T12:00:00.000Z`));
}

function displayPreviewDate(dateStr: string): string {
    return new Intl.DateTimeFormat("en-US", {
        month: "long",
        day: "numeric",
        year: "numeric",
        timeZone: "UTC",
    }).format(new Date(`${dateStr}T12:00:00.000Z`));
}

function searchPreviewCards(
    cards: MemoryCard[],
    query: string,
    appFilter: unknown,
    timeFilter: unknown,
    requestedLimit: unknown,
): MemoryCard[] {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return [];

    const stopWords = new Set(["a", "about", "and", "for", "how", "is", "of", "the", "to", "what"]);
    const tokens = normalized.split(/[^a-z0-9]+/).filter((token) => token && !stopWords.has(token));
    const limit = typeof requestedLimit === "number"
        ? Math.max(0, Math.floor(requestedLimit))
        : 20;

    return cards
        .filter((card) => typeof appFilter !== "string" || !appFilter.trim() || card.app_name === appFilter.trim())
        .filter((card) => {
            if (typeof timeFilter !== "string" || !timeFilter) return true;
            const age = previewNow - card.timestamp;
            if (timeFilter === "last_hour") return age <= 60 * 60 * 1000;
            if (timeFilter === "today" || timeFilter === "last_24h") return age <= 24 * 60 * 60 * 1000;
            if (timeFilter === "last_7d") return age <= 7 * 24 * 60 * 60 * 1000;
            return true;
        })
        .filter((card) => {
            if (normalized === "today" || normalized.includes("happened today")) {
                return previewNow - card.timestamp <= 24 * 60 * 60 * 1000;
            }
            const haystack = [
                card.title,
                card.summary,
                card.display_summary,
                card.action,
                card.app_name,
                card.window_title,
                ...(card.context ?? []),
                ...(card.topic_categories ?? []),
            ].filter(Boolean).join(" ").toLowerCase();
            return tokens.some((token) => haystack.includes(token));
        })
        .slice(0, limit)
        .map((card) => clonePreview(card));
}

/**
 * Synthetic IPC used only by ui-preview.html on Vite's development server.
 * Unknown commands fail intentionally so browser audits never mistake an
 * incomplete fixture for a working product flow.
 */
export function createPreviewIpcHandler(): PreviewIpcHandler {
    let previewCaptureStatus: CaptureStatus = {
        ...captureStatus,
        pipeline: { ...captureStatus.pipeline },
    };
    let previewCards = clonePreview(previewMemoryCards);
    let tasks = clonePreview(previewTasks);
    let nextTaskId = 1;
    let screenGuideSettings: ScreenGuideSettings = {
        enabled: true,
        shortcut: "Control+Alt+Space",
        speak_responses: false,
        show_cursor: true,
    };
    let screenGuideGeneration = 0;
    const releasedScreenGuideGenerations = new Set<number>();
    let previewOnboardingState: OnboardingState = {
        step: "complete",
        biometric_enabled: false,
        screen_permission: true,
        accessibility_permission: true,
        model_downloaded: true,
        model_id: "qwen3-vl-2b",
        display_name: "Anurup",
    };
    let previewBlocklist = ["1Password", "bank.example"];
    let previewPrivacyAlerts: PrivacyAlert[] = [];

    return async (command: string, payload?: unknown) => {
        switch (command) {
            case "get_onboarding_state":
                return { ...previewOnboardingState };
            case "save_onboarding_state": {
                const state =
                    typeof payload === "object" && payload !== null && "state" in payload
                        ? (payload as { state?: unknown }).state
                        : null;
                if (!state || typeof state !== "object") {
                    throw new Error("Preview save_onboarding_state requires a state payload.");
                }
                previewOnboardingState = { ...previewOnboardingState, ...(state as OnboardingState) };
                return undefined;
            }
            case "get_status":
                return { ...previewCaptureStatus, pipeline: { ...previewCaptureStatus.pipeline } };
            case "pause_capture":
                previewCaptureStatus = { ...previewCaptureStatus, is_paused: true };
                await emitPreviewEventIfAvailable(CAPTURE_STATUS_EVENT, previewCaptureStatus);
                return undefined;
            case "resume_capture":
                previewCaptureStatus = { ...previewCaptureStatus, is_paused: false };
                await emitPreviewEventIfAvailable(CAPTURE_STATUS_EVENT, previewCaptureStatus);
                return undefined;
            case "get_fun_greeting":
                return "Good evening, Anurup.";
            case "get_app_names":
                return ["Visual Studio Code", "Google Chrome", "Terminal", "Figma"];
            case "get_privacy_alerts":
                return previewPrivacyAlerts.map((alert) => ({ ...alert }));
            case "get_blocklist":
                return [...previewBlocklist];
            case "set_blocklist": {
                const apps =
                    typeof payload === "object" && payload !== null && "apps" in payload
                        ? (payload as { apps?: unknown }).apps
                        : null;
                if (!Array.isArray(apps) || !apps.every((app) => typeof app === "string")) {
                    throw new Error("Preview set_blocklist requires a string array.");
                }
                previewBlocklist = [...apps];
                return undefined;
            }
            case "add_to_blocklist": {
                const site =
                    typeof payload === "object" && payload !== null && "site" in payload
                        ? (payload as { site?: unknown }).site
                        : null;
                if (typeof site !== "string" || !site.trim()) {
                    throw new Error("Preview add_to_blocklist requires a site.");
                }
                if (!previewBlocklist.includes(site.trim())) previewBlocklist.push(site.trim());
                previewPrivacyAlerts = previewPrivacyAlerts.filter(
                    (alert) => alert.domain_or_title !== site,
                );
                return undefined;
            }
            case "dismiss_privacy_alert": {
                const site =
                    typeof payload === "object" && payload !== null && "site" in payload
                        ? (payload as { site?: unknown }).site
                        : null;
                if (typeof site !== "string") {
                    throw new Error("Preview dismiss_privacy_alert requires a site.");
                }
                previewPrivacyAlerts = previewPrivacyAlerts.filter(
                    (alert) => alert.domain_or_title !== site,
                );
                return undefined;
            }
            case "list_available_models":
                return previewModels.map((model) => ({ ...model }));
            case "fndr_quality_status":
                return {
                    stored_count: previewCaptureStatus.pipeline.stored_total,
                    dropped_count: previewCaptureStatus.pipeline.skipped_total,
                    flagged_count: 0,
                };
            case "search_memory_cards": {
                const input = payloadRecord(payload);
                const query = input?.query;
                if (typeof query !== "string") {
                    throw new Error("Preview search_memory_cards requires query.");
                }
                return searchPreviewCards(
                    previewCards,
                    query,
                    input?.appFilter,
                    input?.timeFilter,
                    input?.limit,
                );
            }
            case "summarize_search": {
                const query = requiredString(payload, "query", command);
                const snippets = payloadRecord(payload)?.resultsSnippets;
                if (!Array.isArray(snippets) || !snippets.every((snippet) => typeof snippet === "string")) {
                    throw new Error("Preview summarize_search requires resultsSnippets.");
                }
                return `Preview-only summary for “${query}”: ${snippets.slice(0, 2).join(" ")}`;
            }
            case "transcribe_voice_input": {
                const audioBytes = payloadRecord(payload)?.audioBytes;
                if (!Array.isArray(audioBytes) || !audioBytes.every((byte) => typeof byte === "number")) {
                    throw new Error("Preview transcribe_voice_input requires audioBytes.");
                }
                return {
                    text: "Search for the FNDR browser preview",
                    backend: "preview-synthetic-audio",
                };
            }
            case "fndr_answer": {
                const query = requiredString(payload, "query", command);
                const hasPreviewEvidence = /accessibility|browser|contrast|design|fndr|home|keyboard|permission|preview|roadmap|ui|ux/i.test(query);
                const matchingCards = hasPreviewEvidence
                    ? searchPreviewCards(previewCards, query, null, null, 5)
                    : [];
                const previewHarness = previewCards.find((card) => card.id === "memory-preview-harness");
                const browserQuestion = /browser|permission|preview/i.test(query);
                const cards = browserQuestion && previewHarness
                    ? [clonePreview(previewHarness)]
                    : matchingCards;
                if (cards.length === 0) {
                    const refusal: ComposedAnswer = {
                        query,
                        answer: "I could not find enough evidence in the synthetic preview memories to answer that.",
                        evidence: {
                            files: [], commands: [], decisions: [], errors: [], todos: [], urls: [],
                        },
                        cards: [],
                        verify_outcome: {
                            kind: "not_enough_evidence",
                            reason: "No preview memory matched this question.",
                        },
                        surfacing_reasons: [],
                    };
                    return refusal;
                }
                const answer: ComposedAnswer = {
                    query,
                    answer: browserQuestion
                        ? "The browser preview mounts FNDR's real React shell behind synthetic, local-only IPC fixtures, so it does not request capture, biometric, microphone, file, or external-app permissions."
                        : `The synthetic preview memories point to ${cards[0].title.toLowerCase()}.`,
                    evidence: {
                        files: [{ path: "src/dev/previewIpc.ts", memory_ids: cards.map((card) => card.id) }],
                        commands: [],
                        decisions: [{ decision: "Keep native capabilities outside browser QA.", memory_ids: cards.map((card) => card.id) }],
                        errors: [],
                        todos: [],
                        urls: [],
                    },
                    cards,
                    verify_outcome: { kind: "grounded", confidence: 0.94 },
                    surfacing_reasons: cards.map((card) => ({
                        headline: `Matched ${card.title}`,
                        routes: ["synthetic preview memory"],
                    })),
                };
                return answer;
            }
            case "list_memory_cards": {
                const appFilter =
                    typeof payload === "object" && payload !== null && "appFilter" in payload
                        ? (payload as { appFilter?: unknown }).appFilter
                        : null;
                return typeof appFilter === "string" && appFilter.trim()
                    ? clonePreview(previewCards.filter((card) => card.app_name === appFilter.trim()))
                    : clonePreview(previewCards);
            }
            case "list_needs_signal_memory_cards":
                return clonePreview(previewNeedsSignalCards);
            case "fndr_get_related_memories": {
                const memoryId =
                    typeof payload === "object" && payload !== null && "memoryId" in payload
                        ? (payload as { memoryId?: unknown }).memoryId
                        : null;
                const requestedLimit =
                    typeof payload === "object" && payload !== null && "limit" in payload
                        ? (payload as { limit?: unknown }).limit
                        : 4;
                const limit = typeof requestedLimit === "number" ? requestedLimit : 4;
                return previewCards
                    .filter((card) => card.id !== memoryId)
                    .slice(0, Math.max(0, limit))
                    .map((card) => clonePreview(card));
            }
            case "fndr_get_memory_subgraph": {
                const seedIds =
                    typeof payload === "object" && payload !== null && "seedIds" in payload
                        ? (payload as { seedIds?: unknown }).seedIds
                        : [];
                return {
                    seed_ids: Array.isArray(seedIds)
                        ? seedIds.filter((id): id is string => typeof id === "string")
                        : [],
                    node_count: 3,
                    edge_count: 2,
                };
            }
            case "find_visually_similar_memories":
                return clonePreview(previewSimilarResults);
            case "fndr_build_context_pack": {
                const query =
                    typeof payload === "object" && payload !== null && "query" in payload
                        ? (payload as { query?: unknown }).query
                        : "";
                return {
                    query: typeof query === "string" ? query : "",
                    summary:
                        "A synthetic, local-only context pack for validating the preview copy flow.",
                    relevant_files: [
                        { path: "src/app/HomeHero.tsx" },
                        { path: "src/app/HomeHero.css" },
                    ],
                    recent_decisions: [
                        { summary: "Keep the wallpaper ambient and the task surface readable." },
                    ],
                };
            }
            case "reopen_memory": {
                const memoryId =
                    typeof payload === "object" && payload !== null && "memoryId" in payload
                        ? (payload as { memoryId?: unknown }).memoryId
                        : null;
                // Preview confirms that a target exists but never launches a URL or native app.
                return previewCards.some(
                    (card) => card.id === memoryId && Boolean(card.reopen_target),
                );
            }
            case "delete_memory": {
                const memoryId =
                    typeof payload === "object" && payload !== null && "memoryId" in payload
                        ? (payload as { memoryId?: unknown }).memoryId
                        : null;
                const cardIndex = previewCards.findIndex((card) => card.id === memoryId);
                if (cardIndex < 0) return false;
                previewCards = previewCards.filter((card) => card.id !== memoryId);
                return true;
            }
            case "generate_daily_summary_for_date": {
                const dateStr = requiredString(payload, "dateStr", command);
                if (!validDate(dateStr)) {
                    throw new Error("Preview generate_daily_summary_for_date requires YYYY-MM-DD.");
                }
                return [
                    `On ${displayPreviewDate(dateStr)}, you focused on making FNDR's browser-previewed interface consistent, readable, and permission-free.`,
                    "You reviewed the Home hierarchy, documented accessibility acceptance criteria, and sequenced the remaining work into testable vertical slices.",
                    "The native permission and relaunch checks remain separate operator tasks because a browser cannot prove macOS integration behavior.",
                ].join("\n\n");
            }
            case "get_daily_summary_overview": {
                const dateStr = requiredString(payload, "dateStr", command);
                if (!validDate(dateStr)) {
                    throw new Error("Preview get_daily_summary_overview requires YYYY-MM-DD.");
                }
                const openFollowups = tasks.filter((task) => (
                    task.task_type === "Followup" && !task.is_completed && !task.is_dismissed
                )).length;
                return `FNDR previewed 126 local captures on ${displayPreviewDate(dateStr)}. Visual Studio Code was the leading app. You have ${openFollowups} open follow-up${openFollowups === 1 ? "" : "s"}.`;
            }
            case "get_daily_summary_followups":
                return clonePreview(tasks.filter((task) => (
                    task.task_type === "Followup" && !task.is_completed && !task.is_dismissed
                )));
            case "generate_daily_briefing":
                return "Your preview briefing: finish the keyboard walkthrough, share the UI audit, and run native permission checks in the Tauri app.";
            case "get_todos":
                return clonePreview(tasks.filter((task) => !task.is_completed && !task.is_dismissed));
            case "add_todo": {
                const title = requiredString(payload, "title", command);
                const requestedType = payloadRecord(payload)?.taskType;
                const taskType = requestedType === "Reminder" || requestedType === "Followup"
                    ? requestedType
                    : "Todo";
                const task: Task = {
                    id: `preview-task-${nextTaskId++}`,
                    title,
                    description: "Added in the permission-free browser preview. This task is not persisted to disk.",
                    source_app: "manual",
                    source_memory_id: null,
                    created_at: previewNow + nextTaskId,
                    due_date: null,
                    is_completed: false,
                    is_dismissed: false,
                    task_type: taskType,
                    linked_urls: [],
                    linked_memory_ids: [],
                };
                tasks = [task, ...tasks];
                return clonePreview(task);
            }
            case "update_todo": {
                const taskId = requiredString(payload, "taskId", command);
                const title = requiredString(payload, "title", command);
                const requestedType = payloadRecord(payload)?.taskType;
                const index = tasks.findIndex((task) => task.id === taskId);
                if (index < 0) {
                    throw new Error("Preview task no longer exists.");
                }
                const taskType = requestedType === "Todo"
                    || requestedType === "Reminder"
                    || requestedType === "Followup"
                    ? requestedType
                    : tasks[index].task_type;
                const updated: Task = { ...tasks[index], title, task_type: taskType };
                tasks = tasks.map((task) => task.id === taskId ? updated : task);
                return clonePreview(updated);
            }
            case "complete_todo":
            case "dismiss_todo": {
                const taskId = requiredString(payload, "taskId", command);
                const index = tasks.findIndex((task) => task.id === taskId);
                if (index < 0) return false;
                tasks = tasks.map((task) => task.id === taskId
                    ? {
                        ...task,
                        is_completed: command === "complete_todo" ? true : task.is_completed,
                        is_dismissed: command === "dismiss_todo" ? true : task.is_dismissed,
                    }
                    : task);
                return true;
            }
            case "set_todo_completed": {
                const taskId = requiredString(payload, "taskId", command);
                const isCompleted = payloadRecord(payload)?.isCompleted;
                if (typeof isCompleted !== "boolean") {
                    throw new Error("Preview set_todo_completed requires isCompleted.");
                }
                if (!tasks.some((task) => task.id === taskId)) return false;
                tasks = tasks.map((task) => task.id === taskId
                    ? { ...task, is_completed: isCompleted }
                    : task);
                return true;
            }
            case "get_stats":
                return clonePreview(previewStats);
            case "get_weekly_wrapped": {
                const input = payloadRecord(payload);
                const startDate = typeof input?.startDate === "string" && validDate(input.startDate)
                    ? input.startDate
                    : "2026-09-21";
                const endDate = typeof input?.endDate === "string" && validDate(input.endDate)
                    ? input.endDate
                    : "2026-09-22";
                const wrapped: WeeklyWrapped = {
                    start_date: startDate,
                    end_date: endDate,
                    generated_at_ms: previewNow,
                    total_captures: 1842,
                    active_days: 6,
                    total_minutes: 2310,
                    apps: [
                        { name: "Visual Studio Code", captures: 782, duration_minutes: 986 },
                        { name: "Google Chrome", captures: 526, duration_minutes: 612 },
                        { name: "Figma", captures: 318, duration_minutes: 438 },
                    ],
                    websites: [
                        { name: "capstone.cs.utah.edu", captures: 94, duration_minutes: 173 },
                        { name: "developer.mozilla.org", captures: 61, duration_minutes: 119 },
                    ],
                    projects_and_topics: [
                        { name: "FNDR UI overhaul", count: 28 },
                        { name: "Accessibility", count: 17 },
                        { name: "Browser QA", count: 14 },
                    ],
                    meeting_count: 2,
                    document_count: 19,
                    open_followups: tasks.filter((task) => task.task_type === "Followup" && !task.is_completed).length,
                    open_tasks: tasks.filter((task) => !task.is_completed && !task.is_dismissed).length,
                    busiest_day: { day: "Tuesday", count: 426 },
                    busiest_hour: 16,
                    most_revisited_file: "src/dev/previewIpc.ts",
                };
                return wrapped;
            }
            case "export_daily_summary_pdf": {
                const dateStr = requiredString(payload, "dateStr", command);
                requiredString(payload, "summaryText", command);
                if (!validDate(dateStr)) {
                    throw new Error("Preview export_daily_summary_pdf requires YYYY-MM-DD.");
                }
                // Virtual scheme: the browser preview never writes a file.
                return `preview-only://exports/daily-summary-${dateStr}.pdf`;
            }
            case "export_weekly_wrapped_pdf": {
                const startDate = requiredString(payload, "startDate", command);
                const endDate = requiredString(payload, "endDate", command);
                requiredString(payload, "recapText", command);
                if (!validDate(startDate) || !validDate(endDate)) {
                    throw new Error("Preview export_weekly_wrapped_pdf requires YYYY-MM-DD dates.");
                }
                // Virtual scheme: the browser preview never writes a file.
                return `preview-only://exports/fndr-wrapped-${startDate}-to-${endDate}.pdf`;
            }
            case "open_exported_pdf": {
                const path = requiredString(payload, "path", command);
                if (!path.startsWith("preview-only://exports/") || !path.endsWith(".pdf")) {
                    throw new Error("Preview open_exported_pdf requires a preview-only export path.");
                }
                // Deliberate no-op: never hand a path to the OS from the browser preview.
                return undefined;
            }
            case "get_screen_guide_settings":
                return { ...screenGuideSettings };
            case "set_screen_guide_settings": {
                const settings = payloadRecord(payload)?.settings;
                if (
                    typeof settings !== "object"
                    || settings === null
                    || typeof (settings as Record<string, unknown>).enabled !== "boolean"
                    || typeof (settings as Record<string, unknown>).shortcut !== "string"
                    || !(settings as Record<string, unknown>).shortcut
                    || typeof (settings as Record<string, unknown>).speak_responses !== "boolean"
                    || typeof (settings as Record<string, unknown>).show_cursor !== "boolean"
                ) {
                    throw new Error("Preview set_screen_guide_settings requires complete settings.");
                }
                screenGuideSettings = { ...(settings as ScreenGuideSettings) };
                return { ...screenGuideSettings };
            }
            case "screen_guide_press":
                screenGuideGeneration += 1;
                await emitPreviewEventIfAvailable(SCREEN_GUIDE_STATE_EVENT, {
                    phase: "listening",
                    message: "Preview-only listening simulation; no microphone was opened.",
                });
                return screenGuideGeneration;
            case "screen_guide_release": {
                const generation = payloadRecord(payload)?.generation;
                if (
                    typeof generation !== "number"
                    || generation <= 0
                    || generation > screenGuideGeneration
                ) {
                    throw new Error("Preview screen_guide_release requires an active generation.");
                }
                releasedScreenGuideGenerations.add(generation);
                await emitPreviewEventIfAvailable(SCREEN_GUIDE_STATE_EVENT, {
                    phase: "answer",
                    message: "Preview-only response ready; no screen was captured.",
                });
                return undefined;
            }
            case "submit_screen_guide_text":
                requiredString(payload, "text", command);
                await emitPreviewEventIfAvailable(SCREEN_GUIDE_STATE_EVENT, {
                    phase: "answer",
                    message: "Preview-only response ready; no screen was captured.",
                });
                // No display capture, OCR, or native overlay is started.
                return undefined;
            case "get_runtime_metrics":
                return clonePreview(previewRuntimeMetrics);
            case "get_memory_review_status":
                return { ...previewReviewStatus };
            case "get_privacy_proof":
                return clonePreview(previewPrivacyProof);
            case "get_meeting_status":
                return {
                    is_recording: false,
                    current_meeting_id: null,
                    current_title: null,
                    model: null,
                    started_at: null,
                    ffmpeg_available: true,
                    transcription_backend: "local Whisper",
                    is_analyzing: false,
                    last_error: null,
                };
            case "get_model_download_status":
                return {
                    state: "idle",
                    model_id: null,
                    filename: null,
                    download_url: null,
                    destination_path: null,
                    temp_path: null,
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    percent: 0,
                    done: false,
                    error: null,
                    logs: [],
                    updated_at_ms: previewNow,
                };
            default:
                throw new Error(`Unhandled UI preview command: ${command}`);
        }
    };
}
