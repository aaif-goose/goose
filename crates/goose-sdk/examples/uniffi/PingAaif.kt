package examples

import io.aaif.goose.sdk.Agent
import io.aaif.goose.sdk.EventSink
import io.aaif.goose.sdk_types.AgentEvent
import io.aaif.goose.sdk_types.ExtensionSpec
import io.aaif.goose.sdk_types.ProviderSpec

private object Style {
    const val DIM = "\u001B[2m"
    const val CYAN = "\u001B[36m"
    const val GREEN = "\u001B[32m"
    const val RED = "\u001B[31m"
    const val RESET = "\u001B[0m"
}

private fun String.paint(color: String) = "$color$this${Style.RESET}"

private fun String.preview(maxLines: Int = 3, maxWidth: Int = 100): String =
    lineSequence()
        .filter { it.isNotBlank() }
        .map { it.take(maxWidth) }
        .take(maxLines)
        .joinToString("\n  ")

private class Printer : EventSink {
    private var midText = false

    override fun onEvent(event: AgentEvent) {
        when (event) {
            is AgentEvent.AssistantText -> {
                print(event.text)
                midText = true
            }
            is AgentEvent.ToolRequest -> {
                endTextLine()
                val args = event.arguments.replace("\n", " ").take(120)
                println("→ ${event.name}".paint(Style.CYAN) + " " + args.paint(Style.DIM))
            }
            is AgentEvent.ToolResponse -> {
                endTextLine()
                val color = if (event.isError) Style.RED else Style.GREEN
                val marker = if (event.isError) "✗" else "✓"
                println(marker.paint(color) + " " + event.output.preview().paint(Style.DIM))
                println()
            }
            is AgentEvent.Thinking -> Unit
        }
        System.out.flush()
    }

    override fun onError(error: String) {
        System.err.println("\n${"error:".paint(Style.RED)} $error")
    }

    override fun onDone() = endTextLine()

    private fun endTextLine() {
        if (midText) {
            println()
            midText = false
        }
    }
}

fun main() {
    System.err.println("configuring agent…".paint(Style.DIM))

    val agent = Agent().apply {
        configure(
            ProviderSpec(
                name = System.getenv("GOOSE_PROVIDER"),
                model = System.getenv("GOOSE_MODEL"),
            ),
            listOf(ExtensionSpec.Builtin(name = "developer")),
        )
    }

    System.err.println("> ping aaif.io".paint(Style.DIM) + "\n")
    agent.reply("ping aaif.io", Printer())
}
