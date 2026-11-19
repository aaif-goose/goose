import org.oi_kit.MessageContent
import org.oi_kit.MessageRole
import org.oi_kit.ProviderMessage
import org.oi_kit.ProviderModelConfig
import org.oi_kit.StreamChunk
import org.oi_kit.streamFlow
import org.oi_kit.providers.openai.defaultModel
import org.oi_kit.providers.openai.provider as openAiProvider
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    val apiKey = System.getenv("OPENAI_API_KEY")
    require(!apiKey.isNullOrBlank()) {
        "Set OPENAI_API_KEY before running this example."
    }

    val provider = openAiProvider(apiKey)
    val model = ProviderModelConfig(modelName = defaultModel())
    val messages = listOf(
        ProviderMessage(
            role = MessageRole.USER,
            content = listOf(
                MessageContent.Text(
                    text = "What is the capital of France? Answer in one sentence.",
                ),
            ),
        ),
    )

    provider
        .streamFlow(
            model,
            "You are a knowledgeable geography expert.",
            messages,
        )
        .collect { chunk ->
            when (chunk) {
                is StreamChunk.TextChunk -> print(chunk.text)
                is StreamChunk.EndChunk -> chunk.usage?.let { println("\nusage: $it") }
                is StreamChunk.ErrorChunk -> System.err.println("\nerror: ${chunk.error.message}")
                is StreamChunk.ToolChunk -> Unit
                is StreamChunk.ThinkingChunk -> Unit
                is StreamChunk.RedactedThinkingChunk -> Unit
            }
        }
    println()
}
