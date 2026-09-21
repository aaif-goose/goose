import Foundation
import FoundationModels

struct BridgeError: Error, Codable {
    let kind: String
    let message: String
}

struct Envelope<T: Encodable>: Encodable {
    var result: T? = nil
    var error: BridgeError? = nil
}

struct ModelInfo: Encodable {
    let context_size: Int
}

struct Request: Decodable, Sendable {
    let instructions: String
    let history: [Entry]
    let tools: [ToolDefinition]
    let temperature: Double?
    let max_tokens: Int?
}

struct ToolDefinition: Decodable, Sendable {
    let name: String
    let description: String
    let schema: String
}

struct ToolCall: Codable, Sendable {
    let id: String
    let name: String
    let arguments: String
}

struct Entry: Codable, Sendable {
    let type: String
    var text: String? = nil
    var calls: [ToolCall]? = nil
    var id: String? = nil
    var name: String? = nil
}

struct Response: Encodable {
    let entries: [Entry]
    let input_tokens: Int
    let output_tokens: Int
    let cached_tokens: Int
}

enum Handoff: Error { case tools }

@available(macOS 27.0, *)
struct DeferredTool: Tool {
    typealias Arguments = GeneratedContent
    typealias Output = String
    let name: String
    let description: String
    let parameters: GenerationSchema

    func call(arguments: GeneratedContent) async throws -> String {
        // This fallback also prevents execution if the profile hook's behavior changes.
        throw Handoff.tools
    }
}

@available(macOS 27.0, *)
func historyEntries(_ entries: [Entry]) throws -> [Transcript.Entry] {
    try entries.map { entry in
        switch entry.type {
        case "user":
            return .prompt(.init(segments: [.text(.init(content: entry.text ?? ""))]))
        case "assistant":
            return .response(.init(segments: [.text(.init(content: entry.text ?? ""))]))
        case "tool_calls":
            let calls = try (entry.calls ?? []).map { call in
                Transcript.ToolCall(id: call.id, toolName: call.name,
                                    arguments: try GeneratedContent(json: call.arguments))
            }
            return .toolCalls(.init(calls))
        case "tool_output":
            guard let id = entry.id, let name = entry.name else {
                throw BridgeError(kind: "invalid_request", message: "Tool output needs an ID and name")
            }
            return .toolOutput(.init(id: id, toolName: name,
                                    segments: [.text(.init(content: entry.text ?? ""))]))
        default:
            throw BridgeError(kind: "invalid_request", message: "Unknown transcript entry: \(entry.type)")
        }
    }
}

@available(macOS 27.0, *)
func availableModel() throws -> SystemLanguageModel {
    let model = SystemLanguageModel.default
    guard case .available = model.availability else {
        throw BridgeError(kind: "unavailable", message: "Apple Intelligence is unavailable: \(model.availability)")
    }
    return model
}

@available(macOS 27.0, *)
func generate(_ request: Request, model: some LanguageModel) async throws -> Response {
    let history = try historyEntries(request.history)
    let tools = try request.tools.map { tool in
        DeferredTool(name: tool.name, description: tool.description,
                     parameters: try JSONDecoder().decode(GenerationSchema.self, from: Data(tool.schema.utf8)))
    }
    let profile = LanguageModelSession.Profile {
        Instructions(request.instructions)
        tools as [any Tool]
    }
    .model(model)
    .historyTransform { entries in
        // The framework stores the profile's instructions AND tool definitions in
        // an instructions entry. Replace only conversation history, never that entry.
        entries.filter { if case .instructions = $0 { true } else { false } } + history
    }
    .transcriptErrorHandlingPolicy(.preserveTranscript)
    .onToolCall { _ in throw Handoff.tools }
    let session = LanguageModelSession(profile: profile, history: history)
    let originalIDs = Set(session.transcript.map(\.id))
    do {
        // The transform retains profile instructions/tools and the caller's exact
        // conversation, including a trailing tool output, without the trigger prompt.
        _ = try await session.respond(to: "", options: GenerationOptions(
            temperature: request.temperature, maximumResponseTokens: request.max_tokens))
    } catch Handoff.tools {
        // A completed tool-call batch is returned below, before any tool is executed.
    } catch let error as LanguageModelSession.ToolCallError where error.underlyingError is Handoff {
        // DeferredTool is the second boundary if a session bypasses onToolCall.
    }
    try Task.checkCancellation()
    var entries: [Entry] = []
    for entry in session.transcript where !originalIDs.contains(entry.id) {
        switch entry {
        case .response(let response):
            let text = response.segments.compactMap { segment -> String? in
                if case .text(let text) = segment { return text.content }
                return nil
            }.joined()
            if !text.isEmpty { entries.append(Entry(type: "assistant", text: text)) }
        case .toolCalls(let calls):
            entries.append(Entry(type: "tool_calls", calls: calls.map {
                ToolCall(id: $0.id, name: $0.toolName, arguments: $0.arguments.jsonString)
            }))
        default: break
        }
    }
    guard !entries.isEmpty else {
        throw BridgeError(kind: "generation", message: "Foundation Models returned no text or tool calls")
    }
    return Response(entries: entries, input_tokens: session.usage.input.totalTokenCount,
                    output_tokens: session.usage.output.totalTokenCount,
                    cached_tokens: session.usage.input.cachedTokenCount)
}

func bridgeError(_ error: Error) -> BridgeError {
    if let error = error as? BridgeError { return error }
    if #available(macOS 27.0, *), error is CancellationError {
        return BridgeError(kind: "cancelled", message: "Generation cancelled")
    }
    if error is DecodingError { return BridgeError(kind: "invalid_request", message: String(describing: error)) }
    if #available(macOS 27.0, *), let error = error as? LanguageModelError {
        switch error {
        case .contextSizeExceeded: return BridgeError(kind: "context_length_exceeded", message: error.localizedDescription)
        case .guardrailViolation, .refusal: return BridgeError(kind: "refusal", message: error.localizedDescription)
        default: break
        }
    }
    return BridgeError(kind: "generation", message: String(describing: error))
}

let unsupported = BridgeError(kind: "unavailable", message: "Apple Foundation Models provider requires macOS 27 or newer")

func json<T: Encodable>(_ value: T) -> String {
    // All bridge responses contain only strings, integers and arrays.
    String(decoding: try! JSONEncoder().encode(value), as: UTF8.self)
}

@_cdecl("goose_afm_info")
public func modelInfo() -> UnsafeMutablePointer<CChar> {
    let result: Envelope<ModelInfo>
    do {
        guard #available(macOS 27.0, *) else { throw unsupported }
        result = Envelope(result: ModelInfo(context_size: try availableModel().contextSize))
    } catch {
        result = Envelope(error: bridgeError(error))
    }
    return strdup(json(result))!
}

@_cdecl("goose_afm_is_supported")
public func isSupported() -> Bool {
    if #available(macOS 27.0, *) { return true }
    return false
}

@_cdecl("goose_afm_free")
public func freeString(_ pointer: UnsafeMutablePointer<CChar>) { free(pointer) }

// The callback context is opaque: Swift only passes it back once to its Rust owner.
struct Callback: @unchecked Sendable {
    let function: @convention(c) (UnsafePointer<CChar>, UnsafeMutableRawPointer) -> Void
    let context: UnsafeMutableRawPointer
    func complete(_ result: Envelope<Response>) {
        json(result).withCString { function($0, context) }
    }
}

@available(macOS 27.0, *)
final class Generation: Sendable {
    let task: Task<Void, Never>
    init(_ task: Task<Void, Never>) { self.task = task }
}

@_cdecl("goose_afm_start")
public func start(_ input: UnsafePointer<CChar>,
                  _ callback: @escaping @convention(c) (UnsafePointer<CChar>, UnsafeMutableRawPointer) -> Void,
                  _ context: UnsafeMutableRawPointer) -> UnsafeMutableRawPointer? {
    let reply = Callback(function: callback, context: context)
    guard #available(macOS 27.0, *) else {
        reply.complete(Envelope(error: unsupported))
        return nil
    }
    let data = Data(String(cString: input).utf8)
    let task = Task.detached {
        do {
            guard #available(macOS 27.0, *) else { throw unsupported }
            try Task.checkCancellation()
            let request = try JSONDecoder().decode(Request.self, from: data)
            let response = try await generate(request, model: availableModel())
            reply.complete(Envelope(result: response))
        } catch {
            reply.complete(Envelope(error: bridgeError(error)))
        }
    }
    return Unmanaged.passRetained(Generation(task)).toOpaque()
}

@_cdecl("goose_afm_cancel")
public func cancel(_ pointer: UnsafeMutableRawPointer?) {
    guard #available(macOS 27.0, *), let pointer else { return }
    let generation = Unmanaged<Generation>.fromOpaque(pointer).takeRetainedValue()
    generation.task.cancel()
}

@_cdecl("goose_afm_validate_schema")
public func validateSchema(_ input: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let result: Envelope<Bool>
    do {
        guard #available(macOS 27.0, *) else { throw unsupported }
        _ = try JSONDecoder().decode(GenerationSchema.self, from: Data(String(cString: input).utf8))
        result = Envelope(result: true)
    } catch {
        result = Envelope(error: BridgeError(kind: "invalid_request", message: String(describing: error)))
    }
    return strdup(json(result))!
}
