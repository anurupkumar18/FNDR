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
