import Foundation

@main
struct StreamingTranscriptTest {
    static func main() {
        let transcript = StreamingTranscriptBuffer()

        let intermediate = transcript.observe("Show my meetings", recognizerFinal: true)
        precondition(intermediate?.type == "partial")
        precondition(intermediate?.text == "Show my meetings")
        precondition(transcript.finish() == "Show my meetings")

        transcript.reset()
        precondition(transcript.finish() == nil)
    }
}
