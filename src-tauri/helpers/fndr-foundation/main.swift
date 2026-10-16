import Foundation
import FoundationModels

struct Field: Decodable, Sendable {
    let name: String
    let description: String
    let type: String
    let choices: [String]?
}

struct Request: Decodable, Sendable {
    let id: UInt64
    let op: String
    let instructions: String?
    let prompt: String?
    let root_description: String?
    let fields: [Field]?
    let max_tokens: Int?
}

actor LineWriter {
    func send(_ value: [String: Any]) {
        guard let bytes = try? JSONSerialization.data(withJSONObject: value),
              let line = String(data: bytes, encoding: .utf8) else { return }
        FileHandle.standardOutput.write(Data((line + "\n").utf8))
    }
}

@available(macOS 26.0, *)
func availability() -> String {
    switch SystemLanguageModel.default.availability {
    case .available:
        return "available"
    case .unavailable(let reason):
        switch reason {
        case .deviceNotEligible: return "unsupported_device"
        case .appleIntelligenceNotEnabled: return "apple_intelligence_off"
        case .modelNotReady: return "model_not_ready"
        @unknown default: return "model_not_ready"
        }
    }
}

@available(macOS 26.0, *)
func schema(for fields: [Field], rootDescription: String) throws -> GenerationSchema {
    let properties: [DynamicGenerationSchema.Property] = fields.map { field in
        let value: DynamicGenerationSchema
        if let choices = field.choices, !choices.isEmpty {
            value = DynamicGenerationSchema(name: field.name + "Choice", description: field.description, anyOf: choices)
        } else if field.type == "strings" {
            value = DynamicGenerationSchema(arrayOf: DynamicGenerationSchema(type: String.self))
        } else {
            value = DynamicGenerationSchema(type: String.self)
        }
        return .init(name: field.name, description: field.description, schema: value)
    }
    let root = DynamicGenerationSchema(name: "FNDRResult", description: rootDescription, properties: properties)
    return try GenerationSchema(root: root, dependencies: [])
}

@available(macOS 26.0, *)
func errorCode(_ error: Error) -> String {
    if #available(macOS 27.0, *), let error = error as? LanguageModelError {
        switch error {
        case .contextSizeExceeded: return "context_size"
        case .guardrailViolation, .refusal: return "guardrail_refusal"
        case .unsupportedLanguageOrLocale: return "unsupported_language"
        default: return "generation_failed"
        }
    }
    if let error = error as? LanguageModelSession.GenerationError {
        switch error {
        case .exceededContextWindowSize: return "context_size"
        case .guardrailViolation, .refusal: return "guardrail_refusal"
        case .unsupportedLanguageOrLocale: return "unsupported_language"
        default: return "generation_failed"
        }
    }
    return "generation_failed"
}

@available(macOS 26.0, *)
func generate(_ request: Request, writer: LineWriter) async {
    guard availability() == "available" else {
        await writer.send(["id": request.id, "ok": false, "error": availability()])
        return
    }
    guard let instructions = request.instructions, let prompt = request.prompt,
          let fields = request.fields, !fields.isEmpty, fields.count <= 32,
          let maxTokens = request.max_tokens, (1...1024).contains(maxTokens) else {
        await writer.send(["id": request.id, "ok": false, "error": "invalid_request"])
        return
    }
    do {
        let generationSchema = try schema(for: fields, rootDescription: request.root_description ?? "")
        let session = LanguageModelSession(instructions: instructions)
        let output = try await session.respond(
            to: prompt,
            schema: generationSchema,
            options: GenerationOptions(maximumResponseTokens: maxTokens)
        )
        if Task.isCancelled {
            await writer.send(["id": request.id, "ok": false, "error": "cancelled"])
        } else {
            await writer.send(["id": request.id, "ok": true, "output": output.content.jsonString])
        }
    } catch {
        await writer.send(["id": request.id, "ok": false, "error": Task.isCancelled ? "cancelled" : errorCode(error)])
    }
}

@main
struct FoundationHelper {
    static func main() async {
        let writer = LineWriter()
        var active: Task<Void, Never>?
        while let line = readLine(strippingNewline: true) {
            guard let data = line.data(using: .utf8),
                  let request = try? JSONDecoder().decode(Request.self, from: data) else {
                await writer.send(["id": 0, "ok": false, "error": "invalid_request"])
                continue
            }
            switch request.op {
            case "probe":
                if #available(macOS 26.0, *) {
                    await writer.send(["id": request.id, "ok": true, "availability": availability()])
                } else {
                    await writer.send(["id": request.id, "ok": true, "availability": "unsupported_os"])
                }
            case "generate":
                if let previous = active {
                    await previous.value
                    active = nil
                }
                if #available(macOS 26.0, *) {
                    active = Task.detached {
                        await generate(request, writer: writer)
                    }
                } else {
                    await writer.send(["id": request.id, "ok": false, "error": "unsupported_os"])
                }
            case "cancel":
                active?.cancel()
                active = nil
                await writer.send(["id": request.id, "ok": true])
            case "exit":
                active?.cancel()
                await writer.send(["id": request.id, "ok": true])
                return
            default:
                await writer.send(["id": request.id, "ok": false, "error": "invalid_request"])
            }
        }
        active?.cancel()
    }
}
