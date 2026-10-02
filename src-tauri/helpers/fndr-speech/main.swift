import AVFoundation
import Darwin
import Foundation
import Speech

private final class LineWriter: @unchecked Sendable {
    private let lock = NSLock()

    func emit(_ type: String, _ values: [String: Any] = [:]) {
        var event = values
        event["type"] = type
        guard let data = try? JSONSerialization.data(withJSONObject: event),
              let line = String(data: data, encoding: .utf8)
        else {
            return
        }

        lock.lock()
        FileHandle.standardOutput.write(Data((line + "\n").utf8))
        lock.unlock()
    }
}

@available(macOS 26.0, *)
private final class AnalyzerSpeechSession: @unchecked Sendable {
    private let writer: LineWriter
    private let engine = AVAudioEngine()
    private var analyzer: SpeechAnalyzer?
    private var continuation: AsyncStream<AnalyzerInput>.Continuation?
    private var analysisTask: Task<Void, Never>?
    private var resultTask: Task<Void, Never>?
    private var tapInstalled = false
    private var active = false

    init(writer: LineWriter) {
        self.writer = writer
    }

    func start() {
        guard !active, analysisTask == nil else { return }
        active = true
        analysisTask = Task { [weak self] in
            await self?.startAnalysis()
        }
    }

    func stop() {
        guard active else { return }
        stopAudio()
        continuation?.finish()
    }

    func cancel() {
        guard active || analysisTask != nil else { return }
        stopAudio()
        continuation?.finish()
        continuation = nil
        resultTask?.cancel()
        analysisTask?.cancel()
        if let analyzer {
            Task { await analyzer.cancelAndFinishNow() }
        }
        finish()
    }

    private func startAnalysis() async {
        do {
            guard SpeechTranscriber.isAvailable,
                  let locale = await SpeechTranscriber.supportedLocale(
                      equivalentTo: Locale(identifier: "en-US")
                  )
            else {
                writer.emit("error", [
                    "code": "recognition_unavailable",
                    "message": "On-device English speech recognition is unavailable.",
                ])
                finish()
                return
            }

            let transcriber = SpeechTranscriber(locale: locale, preset: .progressiveTranscription)
            let modules: [any SpeechModule] = [transcriber]
            let assetStatus = await AssetInventory.status(forModules: modules)
            if assetStatus != .installed {
                writer.emit("preparing_model")
                guard let installation = try await AssetInventory.assetInstallationRequest(
                    supporting: modules
                ) else {
                    writer.emit("error", [
                        "code": "language_asset_missing",
                        "message": "The on-device English speech model could not be prepared.",
                    ])
                    finish()
                    return
                }
                try await installation.downloadAndInstall()
            }

            guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(
                compatibleWith: modules
            ) else {
                writer.emit("error", [
                    "code": "recognition_unavailable",
                    "message": "No compatible on-device speech audio format is available.",
                ])
                finish()
                return
            }

            let analyzer = SpeechAnalyzer(modules: modules)
            try await analyzer.prepareToAnalyze(in: format)
            self.analyzer = analyzer

            let (inputs, continuation) = AsyncStream.makeStream(of: AnalyzerInput.self)
            self.continuation = continuation
            resultTask = Task { [weak self] in
                do {
                    for try await result in transcriber.results {
                        guard let self else { return }
                        let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                        if !text.isEmpty {
                            writer.emit(result.isFinal ? "final" : "partial", ["text": text])
                        }
                    }
                } catch is CancellationError {
                    return
                } catch {
                    self?.writer.emit("error", [
                        "code": "recognition_failed",
                        "message": error.localizedDescription,
                    ])
                }
            }

            let input = engine.inputNode
            let microphoneFormat = input.outputFormat(forBus: 0)
            guard let converter = AVAudioConverter(from: microphoneFormat, to: format) else {
                writer.emit("error", [
                    "code": "recording_failed",
                    "message": "The microphone audio format cannot be converted for speech recognition.",
                ])
                finish()
                return
            }
            input.installTap(onBus: 0, bufferSize: 1_024, format: microphoneFormat) {
                [weak self] buffer, _ in
                guard let self else { return }
                emitLevel(buffer)
                do {
                    if let converted = try convertAudioBuffer(
                        buffer,
                        with: converter,
                        to: format
                    ) {
                        continuation.yield(AnalyzerInput(buffer: converted))
                    }
                } catch {
                    writer.emit("error", [
                        "code": "recording_failed",
                        "message": error.localizedDescription,
                    ])
                }
            }
            tapInstalled = true

            engine.prepare()
            try engine.start()
            writer.emit("listening", ["level": 0.0])

            if let lastSampleTime = try await analyzer.analyzeSequence(inputs) {
                try await analyzer.finalizeAndFinish(through: lastSampleTime)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            finish()
        } catch is CancellationError {
            finish()
        } catch {
            writer.emit("error", [
                "code": "recognition_failed",
                "message": error.localizedDescription,
            ])
            finish()
        }
    }

    private func emitLevel(_ buffer: AVAudioPCMBuffer) {
        writer.emit("level", ["level": normalizedLevel(buffer)])
    }

    private func stopAudio() {
        engine.stop()
        if tapInstalled {
            engine.inputNode.removeTap(onBus: 0)
            tapInstalled = false
        }
    }

    private func finish() {
        stopAudio()
        continuation = nil
        analyzer = nil
        resultTask = nil
        analysisTask = nil
        active = false
    }
}

private final class SpeechHelper: @unchecked Sendable {
    private let writer = LineWriter()
    private let engine = AVAudioEngine()
    private let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var analyzerSession: AnyObject?
    private var active = false

    func run() {
        writer.emit("ready")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            while let command = readLine() {
                self?.handle(command.trimmingCharacters(in: .whitespacesAndNewlines))
            }
            self?.cancel()
        }
        dispatchMain()
    }

    private func handle(_ command: String) {
        switch command {
        case "":
            return
        case "start":
            start()
        case "stop":
            stop()
        case "cancel":
            cancel()
        case "quit":
            cancel()
            exit(EXIT_SUCCESS)
        default:
            writer.emit("error", ["code": "invalid_command", "message": "Unknown command: \(command)"])
        }
    }

    private func start() {
        guard !active else { return }

        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            startForCurrentMacOS()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
                guard let self else { return }
                granted ? self.startForCurrentMacOS() : self.permissionDenied("microphone")
            }
        default:
            permissionDenied("microphone")
        }
    }

    private func startForCurrentMacOS() {
        if #available(macOS 26.0, *) {
            let session: AnalyzerSpeechSession
            if let existing = analyzerSession as? AnalyzerSpeechSession {
                session = existing
            } else {
                session = AnalyzerSpeechSession(writer: writer)
                analyzerSession = session
            }
            session.start()
        } else {
            requestSpeechAuthorization()
        }
    }

    private func requestSpeechAuthorization() {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            startRecognition()
        case .notDetermined:
            SFSpeechRecognizer.requestAuthorization { [weak self] status in
                guard let self else { return }
                status == .authorized ? self.startRecognition() : self.permissionDenied("speech_recognition")
            }
        default:
            permissionDenied("speech_recognition")
        }
    }

    private func startRecognition() {
        guard !active else { return }
        guard let recognizer, recognizer.isAvailable else {
            writer.emit("error", ["code": "recognition_unavailable", "message": "Speech recognition is unavailable."])
            return
        }
        guard recognizer.supportsOnDeviceRecognition else {
            writer.emit("error", ["code": "language_asset_missing", "message": "On-device English speech recognition is not installed."])
            return
        }

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        self.request = request

        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1_024, format: format) { [weak self] buffer, _ in
            self?.request?.append(buffer)
            self?.emitLevel(buffer)
        }

        do {
            engine.prepare()
            try engine.start()
            active = true
            writer.emit("listening", ["level": 0.0])
            task = recognizer.recognitionTask(with: request) { [weak self] result, error in
                guard let self else { return }
                if let result {
                    let text = result.bestTranscription.formattedString
                    if !text.isEmpty {
                        self.writer.emit(result.isFinal ? "final" : "partial", ["text": text])
                    }
                    if result.isFinal {
                        self.cleanup()
                    }
                }
                if let error, self.active {
                    self.writer.emit("error", ["code": "recognition_failed", "message": error.localizedDescription])
                    self.cleanup()
                }
            }
        } catch {
            writer.emit("error", ["code": "recording_failed", "message": error.localizedDescription])
            cleanup()
        }
    }

    private func stop() {
        if #available(macOS 26.0, *),
           let session = analyzerSession as? AnalyzerSpeechSession
        {
            session.stop()
            return
        }
        guard active else { return }
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
    }

    private func cancel() {
        if #available(macOS 26.0, *),
           let session = analyzerSession as? AnalyzerSpeechSession
        {
            session.cancel()
        }
        guard active || request != nil || task != nil else { return }
        task?.cancel()
        cleanup()
    }

    private func cleanup() {
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
        request = nil
        task = nil
        active = false
    }

    private func permissionDenied(_ permission: String) {
        writer.emit("error", ["code": "permission_denied", "message": "\(permission) permission was not granted."])
    }

    private func emitLevel(_ buffer: AVAudioPCMBuffer) {
        writer.emit("level", ["level": normalizedLevel(buffer)])
    }
}

private func normalizedLevel(_ buffer: AVAudioPCMBuffer) -> Double {
    guard let channel = buffer.floatChannelData?.pointee else { return 0 }
    let frames = Int(buffer.frameLength)
    guard frames > 0 else { return 0 }
    var squared = 0.0
    for index in 0 ..< frames {
        let sample = Double(channel[index])
        squared += sample * sample
    }
    let rms = sqrt(squared / Double(frames))
    return min(1.0, max(0.0, rms * 8.0))
}

private func convertAudioBuffer(
    _ input: AVAudioPCMBuffer,
    with converter: AVAudioConverter,
    to outputFormat: AVAudioFormat
) throws -> AVAudioPCMBuffer? {
    let sampleRateRatio = outputFormat.sampleRate / input.format.sampleRate
    let requiredFrames = max(
        1,
        AVAudioFrameCount(ceil(Double(input.frameLength) * sampleRateRatio)) + 32
    )
    guard let output = AVAudioPCMBuffer(
        pcmFormat: outputFormat,
        frameCapacity: requiredFrames
    ) else {
        return nil
    }

    var suppliedInput = false
    var conversionError: NSError?
    let status = converter.convert(to: output, error: &conversionError) { _, inputStatus in
        if suppliedInput {
            inputStatus.pointee = .noDataNow
            return nil
        }
        suppliedInput = true
        inputStatus.pointee = .haveData
        return input
    }

    if let conversionError {
        throw conversionError
    }
    switch status {
    case .haveData, .inputRanDry:
        return output.frameLength > 0 ? output : nil
    case .endOfStream, .error:
        return nil
    @unknown default:
        return nil
    }
}

private let helper = SpeechHelper()
helper.run()
