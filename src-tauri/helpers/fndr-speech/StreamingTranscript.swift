import Foundation

final class StreamingTranscriptBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var latestText = ""

    func reset() {
        lock.lock()
        latestText = ""
        lock.unlock()
    }

    func observe(_ text: String, recognizerFinal _: Bool) -> (type: String, text: String)? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        lock.lock()
        latestText = trimmed
        lock.unlock()
        // Apple's streaming recognizer may finalize a phrase while the audio
        // stream remains open. The voice protocol reserves `final` for the
        // person's explicit Stop, so every in-stream result stays partial.
        return (type: "partial", text: trimmed)
    }

    func finish() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return latestText.isEmpty ? nil : latestText
    }
}

/// The stop vocabulary of docs/product/voice-ux.md ("Matching rule"):
/// `[lead-in] (stop | cancel) [it | that | now] [please]`, at most four words.
/// Kept in step with `is_stop_word` in `src-tauri/src/voice/mod.rs` and
/// `isStopWord` in `src/domains/notch/doRun.ts`.
enum StopWordMatcher {
    private static let leadIns: [[String]] = [["hey", "fndr"], ["fndr"], ["please"], ["okay"], ["ok"], ["no"]]
    private static let stopWords: Set<String> = ["stop", "cancel"]
    private static let tails: Set<String> = ["it", "that", "now"]

    static func words(_ text: String) -> [String] {
        let scalars = text.lowercased().unicodeScalars.map { scalar -> Character in
            CharacterSet.alphanumerics.contains(scalar) || scalar == "'" ? Character(scalar) : " "
        }
        return String(scalars).split(separator: " ").map(String.init)
    }

    static func matches(_ text: String) -> Bool {
        var rest = words(text)[...]
        guard !rest.isEmpty, rest.count <= 4 else { return false }
        if let lead = leadIns.first(where: { rest.starts(with: $0) }) {
            rest = rest.dropFirst(lead.count)
        }
        guard let first = rest.first, stopWords.contains(first) else { return false }
        rest = rest.dropFirst()
        if let tail = rest.first, tails.contains(tail) { rest = rest.dropFirst() }
        if rest.first == "please" { rest = rest.dropFirst() }
        return rest.isEmpty
    }
}

/// Splits what the recognizer hears into segments bounded by a short quiet,
/// and decides each one: `stop_word` or `speech_ignored`. The words themselves
/// never leave this object, so in spotting mode no text leaves the helper.
final class StopWordSpotter: @unchecked Sendable {
    static let quietSeconds: TimeInterval = 0.3

    private let lock = NSLock()
    /// Text already decided, for recognizers whose results repeat earlier words.
    private var consumed = ""
    private var segment = ""
    private var lastChange: TimeInterval = 0

    func reset() {
        lock.lock()
        consumed = ""
        segment = ""
        lastChange = 0
        lock.unlock()
    }

    /// `cumulative` is true when `text` still holds earlier segments
    /// (`SFSpeechRecognizer`); false when each result is its own range
    /// (`SpeechAnalyzer`). A recognizer-final segment is decided at once.
    func observe(_ text: String, segmentFinal: Bool, cumulative: Bool, at now: TimeInterval) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        lock.lock()
        defer { lock.unlock() }
        let fresh: String
        if cumulative, trimmed.hasPrefix(consumed) {
            fresh = String(trimmed.dropFirst(consumed.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        } else {
            fresh = trimmed
        }
        if fresh != segment {
            segment = fresh
            lastChange = now
        }
        if segmentFinal {
            return decide(full: trimmed, cumulative: cumulative)
        }
        return nil
    }

    /// Decides the open segment once it has been quiet long enough.
    func tick(at now: TimeInterval, fullText: String? = nil) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard !segment.isEmpty, now - lastChange >= Self.quietSeconds else { return nil }
        return decide(full: fullText, cumulative: fullText != nil)
    }

    private func decide(full: String?, cumulative: Bool) -> String? {
        guard !segment.isEmpty else { return nil }
        let verdict = StopWordMatcher.matches(segment) ? "stop_word" : "speech_ignored"
        if cumulative, let full { consumed = full } else { consumed = "" }
        segment = ""
        return verdict
    }
}
