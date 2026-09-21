import Foundation
import FoundationModels

@available(macOS 27.0, *)
actor Calls {
    var transcripts: [Transcript] = []
    func record(_ transcript: Transcript) { transcripts.append(transcript) }
}

@available(macOS 27.0, *)
struct ScriptedModel: LanguageModel {
    let calls: Calls
    let arguments: String
    var capabilities: LanguageModelCapabilities { .init([.toolCalling]) }
    var executorConfiguration: Int { 0 }

    struct Executor: LanguageModelExecutor {
        typealias Model = ScriptedModel
        init(configuration: Int) {}
        func respond(to request: LanguageModelExecutorGenerationRequest, model: Model,
                     streamingInto channel: LanguageModelExecutorGenerationChannel) async throws {
            await model.calls.record(request.transcript)
            let instructions = request.transcript.compactMap { entry -> Transcript.Instructions? in
                if case .instructions(let instructions) = entry { return instructions }
                return nil
            }
            precondition(instructions.count == 1, "The model must receive the profile instructions")
            let text = instructions[0].segments.compactMap { segment -> String? in
                if case .text(let text) = segment { return text.content }
                return nil
            }.joined()
            precondition(text == "Use tools", "The caller's system instructions must reach the model")
            precondition(instructions[0].toolDefinitions.map(\.name) == ["echo"],
                         "The model must receive the actual tool definitions")
            precondition(instructions[0].toolDefinitions[0].description == "Echo a value")
            let hasResults = request.transcript.contains { if case .toolOutput = $0 { true } else { false } }
            if hasResults {
                await channel.send(.response(action: .appendText("Both results received", tokenCount: 3)))
            } else {
                for id in ["a", "b"] {
                    await channel.send(.toolCalls(entryID: "batch", action: .toolCall(
                        id: id, name: "echo", action: .appendArguments(model.arguments, tokenCount: 5))))
                }
            }
        }
    }
}

@main
struct BridgeTests {
    static func main() async throws {
        guard #available(macOS 27.0, *) else {
            print("SKIPPED: native bridge contract tests require macOS 27")
            return
        }
        let schema = #"{"title":"Args","type":"object","properties":{"value":{"type":"string"}},"required":["value"],"x-order":["value"],"additionalProperties":false}"#
        try await verify(schema: schema, arguments: #"{"value":"hello"}"#)
        let envelopeSchema = #"{"title":"Args","type":"object","properties":{"arguments_json":{"type":"string"}},"required":["arguments_json"],"x-order":["arguments_json"],"additionalProperties":false}"#
        let arguments = json(["arguments_json": #"{"parameters":{"arbitrary/key":{"nested":[true,null,42]}}}"#])
        try await verify(schema: envelopeSchema, arguments: arguments)
        print("PASS: native and JSON-envelope tools retain single-turn handoff and exact continuation")
    }

    @available(macOS 27.0, *)
    static func verify(schema: String, arguments: String) async throws {
        let calls = Calls()
        let model = ScriptedModel(calls: calls, arguments: arguments)
        let tools = [ToolDefinition(name: "echo", description: "Echo a value", schema: schema)]
        let first = Request(instructions: "Use tools", history: [Entry(type: "user", text: "Echo twice")],
                            tools: tools, temperature: nil, max_tokens: nil)
        let result = try await generate(first, model: model)
        precondition(result.entries.count == 1)
        let batch = result.entries[0].calls!
        precondition(batch.map(\.id) == ["a", "b"], "Return the whole parallel batch")
        let expected = try JSONDecoder().decode([String: String].self, from: Data(arguments.utf8))
        for call in batch {
            let actual = try JSONDecoder().decode([String: String].self, from: Data(call.arguments.utf8))
            precondition(actual == expected, "Tool arguments must survive the native handoff")
        }
        let recorded = await calls.transcripts
        precondition(recorded.count == 1, "The bridge must perform only one model generation")
        let prompts = recorded[0].compactMap { entry -> Transcript.Prompt? in
            if case .prompt(let prompt) = entry { return prompt }; return nil
        }
        precondition(prompts.count == 1, "The trigger prompt must not reach the model")
        precondition(prompts[0].segments == [.text(.init(id: prompts[0].segments[0].id, content: "Echo twice"))])
        var history = first.history + result.entries
        history += batch.map { Entry(type: "tool_output", text: "hello", id: $0.id, name: $0.name) }
        let second = try await generate(Request(instructions: first.instructions, history: history,
            tools: tools, temperature: nil, max_tokens: nil), model: model)
        precondition(second.entries.first?.text == "Both results received")
        let replay = await calls.transcripts
        precondition(replay.count == 2)
        guard case .toolOutput(let output) = replay[1].last else { fatalError("Expected trailing tool output, without a synthetic prompt") }
        precondition(output.id == "b")
    }
}
