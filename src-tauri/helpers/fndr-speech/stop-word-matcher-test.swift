import Foundation

@main
struct StopWordMatcherTest {
    static func main() {
        let accepted = [
            "stop", "Stop!", "cancel", "stop it", "stop that", "stop now", "stop please",
            "cancel that please", "please stop", "okay stop", "ok, cancel", "no stop",
            "no, stop it", "FNDR stop", "hey FNDR, cancel", "hey fndr stop now", "fndr cancel please",
        ]
        for phrase in accepted {
            precondition(StopWordMatcher.matches(phrase), "should match: \(phrase)")
        }
        let rejected = [
            "don't stop", "don't stop the music", "stop sign", "unstoppable", "stopped", "stops",
            "cancelled", "stop at the second tab and open settings", "please stop the timer now",
            "okay so stop it now please", "hey fndr stop it please", "wait", "pause", "hold on",
            "never mind", "nevermind", "abort", "just stop", "stop stop stop stop stop", "go",
            "yes", "no", "", "  ...  ",
        ]
        for phrase in rejected {
            precondition(!StopWordMatcher.matches(phrase), "should not match: \(phrase)")
        }

        // SpeechAnalyzer: each result is its own range; a final decides it at once.
        let analyzer = StopWordSpotter()
        precondition(analyzer.observe("open the", segmentFinal: false, cumulative: false, at: 0) == nil)
        precondition(analyzer.tick(at: 0.1) == nil)
        precondition(analyzer.observe("open the music", segmentFinal: true, cumulative: false, at: 0.2) == "speech_ignored")
        precondition(analyzer.observe("stop", segmentFinal: false, cumulative: false, at: 1.0) == nil)
        precondition(analyzer.tick(at: 1.2) == nil)
        precondition(analyzer.tick(at: 1.31) == "stop_word")
        precondition(analyzer.tick(at: 2.0) == nil)

        // SFSpeechRecognizer: results repeat earlier words; only the new tail is a segment.
        let legacy = StopWordSpotter()
        let first = "turn the music up"
        _ = legacy.observe(first, segmentFinal: false, cumulative: true, at: 0)
        precondition(legacy.tick(at: 0.4, fullText: first) == "speech_ignored")
        let second = "turn the music up please stop"
        _ = legacy.observe(second, segmentFinal: false, cumulative: true, at: 1.0)
        precondition(legacy.tick(at: 1.4, fullText: second) == "stop_word")
        print("stop-word matcher ok")
    }
}
