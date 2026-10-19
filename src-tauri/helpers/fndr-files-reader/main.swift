// fndr-files-reader: Files search's macOS readers (ADR 033). Turns a file into
// what the index and the EmbeddingGemma 2 helper take: page text, pictures
// sized for the model, the text in a screenshot, 30 s windows of 16 kHz
// sound, a frame per shot of a video. Ported from DigUp's Apple extractors
// (github.com/ARahim3/DigUp, MIT): ImageIO, Vision, PDFKit and AVFoundation.
//
// One JSON request per stdin line, one JSON reply per stdout line. Files are
// only read; everything written goes under the request's out_dir, which the
// app owns and clears. Quits when stdin closes.

import AppKit
@preconcurrency import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
import PDFKit
import UniformTypeIdentifiers
import Vision

struct ReadError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

// MARK: Pictures

/// A downscaled copy for the model, decoded straight to that size (a 48 MP
/// photo is never decoded whole). HEIC, RAW, WebP and friends; SVG is drawn.
func preparePicture(_ url: URL, maxPixel: Int, minSide: Int, folder: URL, png: Bool) throws -> [String: Any] {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
          CGImageSourceGetType(source) != nil
    else { return try drawPicture(url, maxPixel: maxPixel, minSide: minSide, folder: folder, png: png) }
    let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any] ?? [:]
    let width = properties[kCGImagePropertyPixelWidth] as? Int ?? 0
    let height = properties[kCGImagePropertyPixelHeight] as? Int ?? 0
    guard max(width, height) >= minSide else { return ["too_small": true, "width": width, "height": height] }
    let options: [CFString: Any] = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
    ]
    guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    else { throw ReadError("can't decode image") }
    let out = folder.appendingPathComponent(UUID().uuidString + (png ? ".png" : ".jpg"))
    try write(thumbnail, to: out, png: png)
    let exif = properties[kCGImagePropertyExifDictionary] as? [CFString: Any]
    var reply: [String: Any] = ["path": out.path, "width": width, "height": height]
    if let taken = exif?[kCGImagePropertyExifDateTimeOriginal] as? String { reply["taken"] = taken }
    return reply
}

/// A picture ImageIO can't read (an SVG): AppKit draws it at the size the model needs, on white.
func drawPicture(_ url: URL, maxPixel: Int, minSide: Int, folder: URL, png: Bool) throws -> [String: Any] {
    guard let image = NSImage(contentsOf: url), image.size.width > 0, image.size.height > 0
    else { throw ReadError("can't read this picture") }
    let (width, height) = (Int(image.size.width.rounded()), Int(image.size.height.rounded()))
    guard max(width, height) >= minSide else { return ["too_small": true, "width": width, "height": height] }
    let scale = CGFloat(maxPixel) / max(image.size.width, image.size.height)
    let side = (max(1, Int((image.size.width * scale).rounded())), max(1, Int((image.size.height * scale).rounded())))
    guard let context = CGContext(data: nil, width: side.0, height: side.1, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { throw ReadError("can't draw this picture") }
    context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: side.0, height: side.1))
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    image.draw(in: NSRect(x: 0, y: 0, width: side.0, height: side.1))
    NSGraphicsContext.restoreGraphicsState()
    guard let bitmap = context.makeImage() else { throw ReadError("can't draw this picture") }
    let out = folder.appendingPathComponent(UUID().uuidString + (png ? ".png" : ".jpg"))
    try write(bitmap, to: out, png: png)
    return ["path": out.path, "width": width, "height": height]
}

func fullImage(_ url: URL) -> CGImage? {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(source, 0, nil)
}

func write(_ image: CGImage, to url: URL, png: Bool) throws {
    let type = (png ? UTType.png : UTType.jpeg).identifier as CFString
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, type, 1, nil)
    else { throw ReadError("can't write \(url.lastPathComponent)") }
    let properties = png ? nil : [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary
    CGImageDestinationAddImage(destination, image, properties)
    guard CGImageDestinationFinalize(destination) else { throw ReadError("can't encode image") }
}

// MARK: Text in pictures

/// Apple Vision, accurate: small UI text is what screenshots are full of, and
/// fast lost an exact lookup on DigUp's evals.
/// A picture with see-through parts is read on white first: Vision takes them
/// as black, and dark text on a clear background then reads as nothing.
func recognizeText(in image: CGImage) -> String {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true
    request.automaticallyDetectsLanguage = true
    try? VNImageRequestHandler(cgImage: onWhite(image)).perform([request])
    return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
}

let readsArabic: Bool = {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    return (try? request.supportedRecognitionLanguages())?.contains { $0.hasPrefix("ar") } ?? false
}()

func onWhite(_ image: CGImage) -> CGImage {
    switch image.alphaInfo {
    case .none, .noneSkipFirst, .noneSkipLast: return image
    default: break
    }
    guard let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
    else { return image }
    let whole = CGRect(x: 0, y: 0, width: image.width, height: image.height)
    context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(whole)
    context.draw(image, in: whole)
    return context.makeImage() ?? image
}

// MARK: PDF pages

let arabic: ClosedRange<UInt32> = 0x0600...0x06FF

/// What is wrong with the text PDFKit read from a page. Bengali and Hindi
/// read in glyph order (vowel signs ahead of their consonants) are used as a
/// picture; Arabic that lost lam ligatures is read again by OCR.
enum PageTextProblem {
    case glyphOrder, lostLetters

    static func check(_ text: String) -> PageTextProblem? {
        let text = text.precomposedStringWithCanonicalMapping
        var vowelSigns = 0, misplaced = 0, lostLetters = 0
        var previous: Unicode.Scalar = " ", beforePrevious: Unicode.Scalar = " "
        for scalar in text.unicodeScalars {
            if isIndicVowelSign(scalar.value) {
                vowelSigns += 1
                if !isIndicConsonantOrNukta(previous.value) { misplaced += 1 }
            }
            if beforePrevious == "\u{0627}", "?.\u{FFFD}".unicodeScalars.contains(previous),
               arabic.contains(scalar.value), scalar.properties.isAlphabetic {
                lostLetters += 1
            }
            beforePrevious = previous
            previous = scalar
        }
        if vowelSigns >= 10, misplaced * 20 >= vowelSigns { return .glyphOrder }
        if lostLetters >= 2 { return .lostLetters }
        return nil
    }

    static func isIndicVowelSign(_ value: UInt32) -> Bool {
        switch value {
        case 0x09BE...0x09C4, 0x09C7, 0x09C8, 0x09CB, 0x09CC, 0x09D7, 0x09E2, 0x09E3: return true
        case 0x093A, 0x093B, 0x093E...0x094C, 0x094E, 0x094F, 0x0955...0x0957, 0x0962, 0x0963: return true
        default: return false
        }
    }

    static func isIndicConsonantOrNukta(_ value: UInt32) -> Bool {
        switch value {
        case 0x0995...0x09B9, 0x09BC, 0x09CE, 0x09DC, 0x09DD, 0x09DF, 0x09F0, 0x09F1: return true
        case 0x0915...0x0939, 0x093C, 0x0958...0x095F, 0x0979...0x097F: return true
        default: return false
        }
    }
}

func render(_ page: PDFPage, pixels: Int) -> CGImage? {
    let side = CGFloat(pixels)
    return page.thumbnail(of: NSSize(width: side, height: side), for: .mediaBox)
        .cgImage(forProposedRect: nil, context: nil, hints: nil)
}

/// Pages from `first` (0-based) up to `end` of a PDF. Without `first`, a
/// short PDF is read whole and a long one (more than `big` pages) its first
/// `step`. Pages with real text become text; scans and slides are rendered,
/// with OCR's words for keyword search.
func pdfPages(_ url: URL, request: [String: Any], folder: URL) throws -> [String: Any] {
    guard let document = PDFDocument(url: url) else { throw ReadError("can't open PDF") }
    if document.isLocked { throw ReadError("password-protected") }
    let total = document.pageCount
    let maxPages = request["max_pages"] as? Int ?? 5000
    let minText = request["min_text"] as? Int ?? 200
    let renderPixels = request["render_pixels"] as? Int ?? 1400
    let limit = min(total, maxPages)
    let first = min(request["first"] as? Int ?? 0, limit)
    let end: Int
    if let requested = request["end"] as? Int {
        end = min(requested, limit)
    } else if request["first"] == nil, total > (request["big"] as? Int ?? 50) {
        end = min(request["step"] as? Int ?? 30, limit)
    } else {
        end = limit
    }
    var pages: [[String: Any]] = []
    for index in first..<max(first, end) {
        guard let page = document.page(at: index) else { continue }
        var text = (page.string ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        var garbled: String?
        switch PageTextProblem.check(text) {
        case .lostLetters:
            if readsArabic, let image = render(page, pixels: 2000) {
                let read = recognizeText(in: image).trimmingCharacters(in: .whitespacesAndNewlines)
                if read.count >= minText, PageTextProblem.check(read) == nil,
                   read.unicodeScalars.contains(where: { arabic.contains($0.value) }) {
                    pages.append(["number": index + 1, "text": read, "ocr": true])
                    continue
                }
            }
            if text.count >= minText {
                pages.append(["number": index + 1, "text": text])
                continue
            }
        case .glyphOrder:
            (text, garbled) = ("", text)
        case nil:
            if text.count >= minText {
                pages.append(["number": index + 1, "text": text])
                continue
            }
        }
        guard let image = render(page, pixels: renderPixels) else { continue }
        let out = folder.appendingPathComponent(UUID().uuidString + ".png")
        try write(image, to: out, png: true)
        var ocr = false
        if garbled == nil {
            let read = recognizeText(in: image).trimmingCharacters(in: .whitespacesAndNewlines)
            if read.count > text.count { (text, ocr) = (read, true) }
        }
        var entry: [String: Any] = ["number": index + 1, "text": text, "image": out.path, "ocr": ocr]
        if let garbled { entry["garbled"] = garbled }
        pages.append(entry)
    }
    var reply: [String: Any] = ["total": total, "first": first, "end": max(first, end), "pages": pages]
    if let title = document.documentAttributes?[PDFDocumentAttribute.titleAttribute] as? String { reply["title"] = title }
    return reply
}

// MARK: Sound and video

let sampleRate = 16_000.0

/// Runs async AVFoundation work from this one-thread loop.
final class Box<T>: @unchecked Sendable { var result: Result<T, Error>? }

func runBlocking<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) throws -> T {
    let box = Box<T>()
    let done = DispatchSemaphore(value: 0)
    Task.detached {
        do { box.result = .success(try await operation()) } catch { box.result = .failure(error) }
        done.signal()
    }
    done.wait()
    return try box.result!.get()
}

func rms(_ samples: ArraySlice<Int16>) -> Double {
    guard !samples.isEmpty else { return 0 }
    var sum = 0.0
    for sample in samples { let value = Double(sample) / 32768; sum += value * value }
    return (sum / Double(samples.count)).squareRoot()
}

func writeWAV(_ samples: ArraySlice<Int16>, to url: URL) throws {
    var data = Data(capacity: 44 + samples.count * 2)
    func append(_ text: String) { data.append(contentsOf: Array(text.utf8)) }
    func append32(_ value: UInt32) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
    func append16(_ value: UInt16) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
    let rate = UInt32(sampleRate)
    append("RIFF"); append32(UInt32(36 + samples.count * 2)); append("WAVE")
    append("fmt "); append32(16); append16(1); append16(1); append32(rate); append32(rate * 2); append16(2); append16(16)
    append("data"); append32(UInt32(samples.count * 2))
    samples.withUnsafeBytes { data.append(contentsOf: $0) }   // Int16 is little-endian on Apple silicon
    try data.write(to: url)
}

/// 16 kHz mono in `window`-second WAV pieces `hop` seconds apart, silent ones
/// skipped. One model input ignores anything past 30 s. Streams: memory stays
/// at one window, even for a 3-hour podcast.
func soundWindows(_ url: URL, window: Double, hop: Double, minimum: Double = 2, folder: URL) async throws
    -> (pieces: [[String: Any]], duration: Double) {
    let asset = AVURLAsset(url: url)
    guard let track = try await asset.loadTracks(withMediaType: .audio).first else { return ([], 0) }
    let duration = try await asset.load(.duration).seconds
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(track: track, outputSettings: [
        AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: sampleRate, AVNumberOfChannelsKey: 1,
        AVLinearPCMBitDepthKey: 16, AVLinearPCMIsFloatKey: false, AVLinearPCMIsBigEndianKey: false,
        AVLinearPCMIsNonInterleaved: false,
    ])
    reader.add(output)
    guard reader.startReading() else { throw reader.error ?? ReadError("can't read audio") }
    let windowSamples = Int(window * sampleRate), hopSamples = Int(hop * sampleRate)
    var buffer: [Int16] = []
    var offset = 0
    var pieces: [[String: Any]] = []
    func emit(_ count: Int) throws {
        let slice = buffer[0..<count]
        guard rms(slice) > 0.003 else { return }   // about -50 dBFS: silence is not worth a vector
        let out = folder.appendingPathComponent(UUID().uuidString + ".wav")
        try writeWAV(slice, to: out)
        pieces.append(["path": out.path, "start": Double(offset) / sampleRate,
                       "end": Double(offset + count) / sampleRate])
    }
    while let sampleBuffer = output.copyNextSampleBuffer() {
        guard let block = CMSampleBufferGetDataBuffer(sampleBuffer) else { continue }
        let length = CMBlockBufferGetDataLength(block)
        var chunk = [Int16](repeating: 0, count: length / 2)
        _ = chunk.withUnsafeMutableBytes {
            CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: length, destination: $0.baseAddress!)
        }
        buffer.append(contentsOf: chunk)
        while buffer.count >= windowSamples {
            try emit(windowSamples)
            buffer.removeFirst(hopSamples)
            offset += hopSamples
        }
    }
    if reader.status == .failed { throw reader.error ?? ReadError("audio decoding failed") }
    let alreadyCovered = offset == 0 ? 0 : windowSamples - hopSamples
    if buffer.count - alreadyCovered >= Int(minimum * sampleRate) { try emit(buffer.count) }
    return (pieces, duration.isFinite ? duration : Double(offset + buffer.count) / sampleRate)
}

func averageHash(_ image: CGImage) -> UInt64 {
    let side = 8
    var pixels = [UInt8](repeating: 0, count: side * side)
    let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
        guard let context = CGContext(data: buffer.baseAddress, width: side, height: side, bitsPerComponent: 8,
                                      bytesPerRow: side, space: CGColorSpaceCreateDeviceGray(),
                                      bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return false }
        context.interpolationQuality = .low
        context.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
        return true
    }
    guard drawn else { return 0 }
    let mean = pixels.reduce(0) { $0 + Int($1) } / pixels.count
    var hash: UInt64 = 0
    for (index, pixel) in pixels.enumerated() where Int(pixel) > mean { hash |= 1 << UInt64(index) }
    return hash
}

/// A frame every `interval` seconds, one kept per shot: a frame within 4 bits
/// of the last kept one's 8x8 average hash only extends it.
func keyframes(_ url: URL, every interval: Double, maxPixel: Int, maxFrames: Int = 1200, folder: URL) async throws
    -> (frames: [[String: Any]], duration: Double, hasAudio: Bool) {
    let asset = AVURLAsset(url: url)
    let duration = try await asset.load(.duration).seconds
    let hasAudio = !(try await asset.loadTracks(withMediaType: .audio)).isEmpty
    guard duration.isFinite, duration > 0, !(try await asset.loadTracks(withMediaType: .video)).isEmpty
    else { return ([], duration.isFinite ? duration : 0, hasAudio) }
    let step = max(interval, duration / Double(maxFrames))
    let generator = AVAssetImageGenerator(asset: asset)
    generator.appliesPreferredTrackTransform = true
    generator.maximumSize = CGSize(width: maxPixel, height: maxPixel)
    let tolerance = CMTime(seconds: step / 2, preferredTimescale: 600)
    generator.requestedTimeToleranceBefore = tolerance
    generator.requestedTimeToleranceAfter = tolerance
    let times = stride(from: min(0.5, duration / 2), to: duration, by: step).map {
        CMTime(seconds: $0, preferredTimescale: 600)
    }
    var frames: [[String: Any]] = []
    var lastHash: UInt64?
    for await result in generator.images(for: times) {
        guard case let .success(_, image, actualTime) = result else { continue }
        let hash = averageHash(image)
        if let lastHash, (hash ^ lastHash).nonzeroBitCount <= 4 { continue }
        lastHash = hash
        let out = folder.appendingPathComponent(UUID().uuidString + ".jpg")
        try write(image, to: out, png: false)
        frames.append(["path": out.path, "start": max(0, actualTime.seconds), "end": duration])
    }
    for index in frames.indices.dropLast() { frames[index]["end"] = frames[index + 1]["start"] }
    return (frames, duration, hasAudio)
}

func previewFrame(_ url: URL, at seconds: Double, pixels: Int, out: URL) async throws {
    let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
    generator.appliesPreferredTrackTransform = true
    generator.maximumSize = CGSize(width: pixels, height: pixels)
    generator.requestedTimeToleranceBefore = .zero
    generator.requestedTimeToleranceAfter = .zero
    let (image, _) = try await generator.image(at: CMTime(seconds: seconds, preferredTimescale: 600))
    try write(image, to: out, png: false)
}

// MARK: Opening at the place

/// A word from page `page` (from 1) of a PDF that Preview will find there,
/// spelled as the page spells it, preferring one no earlier page has: a PDF
/// opens with it in Preview's search, which lists matches by page. Nil for
/// page 1 or when none of the words is on the page.
func landmark(_ document: PDFDocument, page: Int, words: [String]) -> String? {
    guard page > 1, document.pageCount >= page, let target = document.page(at: page - 1)?.string else { return nil }
    let options: String.CompareOptions = [.caseInsensitive, .diacriticInsensitive]
    let before = (0..<(page - 1)).map { document.page(at: $0)?.string ?? "" }
    let onPage = words.filter { $0.count >= 3 }.compactMap { word in
        target.range(of: word, options: options).map { String(target[$0]) }
    }
    return onPage.first { word in !before.contains { $0.range(of: word, options: options) != nil } } ?? onPage.first
}

/// The Open Documents event with text to search for (keyAESearchText), as Spotlight opens documents.
func openEvent(_ url: URL, searching text: String) -> NSAppleEventDescriptor {
    let event = NSAppleEventDescriptor(eventClass: AEEventClass(kCoreEventClass), eventID: AEEventID(kAEOpenDocuments),
                                       targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                       transactionID: AETransactionID(kAnyTransactionID))
    let files = NSAppleEventDescriptor.list()
    files.insert(NSAppleEventDescriptor(fileURL: url), at: 0)
    event.setParam(files, forKeyword: keyDirectObject)
    event.setParam(NSAppleEventDescriptor(string: text), forKeyword: AEKeyword(keyAESearchText))
    return event
}

let quickTime = "com.apple.QuickTimePlayerX"
let iina = "com.colliderli.iina"

/// Opens a file in its app: a PDF page with a word from it in Preview's
/// search, a moment in QuickTime Player (macOS asks once whether FNDR may
/// control it) or IINA, anything else plainly. Returns how it opened.
func open(_ url: URL, page: Int?, seconds: Double?, words: [String]) -> String {
    guard let app = NSWorkspace.shared.urlForApplication(toOpen: url) else {
        NSWorkspace.shared.open(url)
        return "default"
    }
    let appID = Bundle(url: app)?.bundleIdentifier
    if let page, let document = PDFDocument(url: url), let word = landmark(document, page: page, words: words) {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.appleEvent = openEvent(url, searching: word)
        NSWorkspace.shared.openApplication(at: app, configuration: configuration)
        return "searching \(word)"
    }
    if let seconds, seconds >= 1, appID == iina {
        var components = URLComponents(string: "iina://open")
        components?.queryItems = [URLQueryItem(name: "url", value: url.absoluteString),
                                  URLQueryItem(name: "mpv_start", value: String(format: "%.1f", seconds))]
        if let link = components?.url {
            NSWorkspace.shared.open(link)
            return "iina"
        }
    }
    if let seconds, seconds >= 1, appID == quickTime {
        // osascript, so macOS's "control QuickTime Player?" question never blocks this loop.
        // The time goes over as whole milliseconds: a decimal point depends on locale.
        let script = """
            on run argv
                set theFile to POSIX file (item 1 of argv)
                set theTime to (item 2 of argv as integer) / 1000
                tell application "QuickTime Player"
                    activate
                    set theDocument to open theFile
                    set current time of theDocument to theTime
                    play theDocument
                end tell
            end run
            """
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", script, url.path, String(Int(seconds * 1000))]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        process.terminationHandler = { finished in
            // Not allowed to control it, or no answer: open it the plain way.
            if finished.terminationStatus != 0 { NSWorkspace.shared.open(url) }
        }
        if (try? process.run()) != nil { return "quicktime" }
    }
    NSWorkspace.shared.open(url)
    return "default"
}

// MARK: Code editors

/// How an editor is told the line: a URL, or a command its app bundles.
enum EditorMethod {
    case fileURL(String)      // "vscode://file/<path>:<line>:1"
    case command(String)      // a command inside the bundle taking "<path>:<line>"
    case xed                  // Xcode's `xed --line <line> <path>`
    case jetBrains            // the launcher takes `--line <line> <path>`
    case openURL(String)      // "txmt://open?url=file://<path>&line=<line>"
}

let editors: [(id: String, name: String, method: EditorMethod)] = [
    ("dev.zed.Zed", "Zed", .command("Contents/MacOS/cli")),
    ("dev.zed.Zed-Preview", "Zed Preview", .command("Contents/MacOS/cli")),
    ("com.todesktop.230313mzl4w4u92", "Cursor", .fileURL("cursor")),
    ("com.microsoft.VSCode", "Visual Studio Code", .fileURL("vscode")),
    ("com.microsoft.VSCodeInsiders", "VS Code Insiders", .fileURL("vscode-insiders")),
    ("com.exafunction.windsurf", "Windsurf", .fileURL("windsurf")),
    ("com.vscodium", "VSCodium", .fileURL("vscodium")),
    ("com.sublimetext.4", "Sublime Text", .command("Contents/SharedSupport/bin/subl")),
    ("com.sublimetext.3", "Sublime Text 3", .command("Contents/SharedSupport/bin/subl")),
    ("com.jetbrains.pycharm", "PyCharm", .jetBrains),
    ("com.jetbrains.pycharm.ce", "PyCharm CE", .jetBrains),
    ("com.jetbrains.intellij", "IntelliJ IDEA", .jetBrains),
    ("com.jetbrains.intellij.ce", "IntelliJ IDEA CE", .jetBrains),
    ("com.jetbrains.WebStorm", "WebStorm", .jetBrains),
    ("com.jetbrains.CLion", "CLion", .jetBrains),
    ("com.jetbrains.goland", "GoLand", .jetBrains),
    ("com.jetbrains.rustrover", "RustRover", .jetBrains),
    ("com.jetbrains.PhpStorm", "PhpStorm", .jetBrains),
    ("com.jetbrains.rubymine", "RubyMine", .jetBrains),
    ("com.google.android.studio", "Android Studio", .jetBrains),
    ("com.apple.dt.Xcode", "Xcode", .xed),
    ("com.barebones.bbedit", "BBEdit", .openURL("x-bbedit")),
    ("com.macromates.TextMate", "TextMate", .openURL("txmt")),
]

func installedEditors() -> [(id: String, name: String, method: EditorMethod)] {
    editors.filter { NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0.id) != nil }
}

/// Opens code at a line in `editorID` ("" for the file's default app; nil for
/// the first installed editor, preferring one that is running). Returns how.
func openCode(_ url: URL, line: Int, editorID: String?) -> String {
    let installed = installedEditors()
    let running = Set(NSWorkspace.shared.runningApplications.compactMap(\.bundleIdentifier))
    let editor: (id: String, name: String, method: EditorMethod)? = editorID == "" ? nil
        : installed.first { $0.id == editorID } ?? installed.first { running.contains($0.id) } ?? installed.first
    guard let editor, let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: editor.id) else {
        NSWorkspace.shared.open(url)
        return "default"
    }
    var link: URL?
    var command: (URL, [String])?
    switch editor.method {
    case .fileURL(let scheme):
        let path = url.path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? url.path
        link = URL(string: "\(scheme)://file\(path):\(line):1")
    case .openURL(let scheme):
        var components = URLComponents(string: "\(scheme)://open")
        components?.queryItems = [URLQueryItem(name: "url", value: url.absoluteString),
                                  URLQueryItem(name: "line", value: "\(line)")]
        link = components?.url
    case .command(let path):
        command = (app.appendingPathComponent(path), ["\(url.path):\(line)"])
    case .xed:
        command = (app.appendingPathComponent("Contents/Developer/usr/bin/xed"), ["--line", "\(line)", url.path])
    case .jetBrains:
        let executable = Bundle(url: app)?.executableURL ?? app.appendingPathComponent("Contents/MacOS/idea")
        command = (executable, ["--line", "\(line)", url.path])
    }
    if let link {
        NSWorkspace.shared.open(link)
    } else if let (executable, arguments) = command, FileManager.default.isExecutableFile(atPath: executable.path) {
        let process = Process()
        process.executableURL = executable
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        if (try? process.run()) == nil {
            NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
        }
    } else {
        NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
    }
    return editor.name
}

// MARK: Requests

func handle(_ request: [String: Any]) throws -> [String: Any] {
    guard let op = request["op"] as? String else { throw ReadError("no op") }
    if op == "editors" {
        return ["editors": installedEditors().map { ["id": $0.id, "name": $0.name] }]
    }
    guard let path = request["path"] as? String else { throw ReadError("no path") }
    let url = URL(fileURLWithPath: path)
    let folder = URL(fileURLWithPath: request["out_dir"] as? String ?? NSTemporaryDirectory())
    if request["out_dir"] != nil {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }
    switch op {
    case "picture":
        let screenshot = request["screenshot"] as? Bool ?? false
        var reply = try preparePicture(url, maxPixel: request["max_pixel"] as? Int ?? (screenshot ? 1536 : 1024),
                                       minSide: request["min_side"] as? Int ?? 256, folder: folder, png: screenshot)
        if screenshot, reply["too_small"] == nil {
            reply["ocr"] = fullImage(url).map(recognizeText(in:)) ?? ""
        }
        return reply
    case "ocr":
        return ["text": fullImage(url).map(recognizeText(in:)) ?? ""]
    case "pdf":
        return try pdfPages(url, request: request, folder: folder)
    case "audio":
        let (window, hop) = (request["window"] as? Double ?? 30, request["hop"] as? Double ?? 25)
        let read = try runBlocking { try await soundWindows(url, window: window, hop: hop, folder: folder) }
        return ["pieces": read.pieces, "duration": read.duration]
    case "video":
        let interval = request["interval"] as? Double ?? 3
        let (window, hop) = (request["window"] as? Double ?? 30, request["hop"] as? Double ?? 25)
        let video = try runBlocking { try await keyframes(url, every: interval, maxPixel: 1024, folder: folder) }
        let sound = video.hasAudio
            ? try runBlocking { try await soundWindows(url, window: window, hop: hop, folder: folder) }.pieces
            : []
        return ["frames": video.frames, "sound": sound, "duration": video.duration]
    case "open":
        let how = open(url, page: request["page"] as? Int, seconds: request["seconds"] as? Double,
                       words: request["words"] as? [String] ?? [])
        return ["how": how]
    case "open_code":
        let how = openCode(url, line: max(1, request["line"] as? Int ?? 1), editorID: request["editor"] as? String)
        return ["how": how]
    case "copy":
        // The file itself, to paste in Finder, Mail or a chat.
        NSPasteboard.general.clearContents()
        return ["copied": NSPasteboard.general.writeObjects([url as NSURL])]
    case "preview_page":
        guard let document = PDFDocument(url: url) else { throw ReadError("can't open PDF") }
        if document.isLocked { throw ReadError("password-protected") }
        let number = request["page"] as? Int ?? 1
        guard let page = document.page(at: number - 1),
              let image = render(page, pixels: request["pixels"] as? Int ?? 1200)
        else { throw ReadError("no page \(number)") }
        let out = folder.appendingPathComponent(UUID().uuidString + ".jpg")
        try write(image, to: out, png: false)
        return ["path": out.path]
    case "preview_frame":
        let seconds = request["seconds"] as? Double ?? 0
        let pixels = request["pixels"] as? Int ?? 1200
        let out = folder.appendingPathComponent(UUID().uuidString + ".jpg")
        try runBlocking { try await previewFrame(url, at: seconds, pixels: pixels, out: out) }
        return ["path": out.path]
    default:
        throw ReadError("unknown op \(op)")
    }
}

func reply(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object),
          let line = String(data: data, encoding: .utf8) else {
        print(#"{"ok":false,"error":"can't encode the reply"}"#)
        fflush(stdout)
        return
    }
    print(line)
    fflush(stdout)
}

print(#"{"ready":true}"#)
fflush(stdout)
while let line = readLine(strippingNewline: true) {
    if line.isEmpty { continue }
    autoreleasepool {
        guard let data = line.data(using: .utf8),
              let request = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else {
            reply(["ok": false, "error": "bad request"])
            return
        }
        let id = request["id"] ?? NSNull()
        do {
            var result = try handle(request)
            result["ok"] = true
            result["id"] = id
            reply(result)
        } catch {
            reply(["ok": false, "id": id, "error": String("\(error)".prefix(300))])
        }
    }
}
