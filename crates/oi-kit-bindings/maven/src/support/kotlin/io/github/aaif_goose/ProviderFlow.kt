package io.github.aaif_goose

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

public fun Provider.streamFlow(
    model: ProviderModelConfig,
    system: String,
    messages: List<ProviderMessage>,
    tools: List<ProviderTool> = emptyList(),
): Flow<StreamChunk> = flow {
    val stream = stream(model, system, messages, tools)
    while (true) {
        emit(stream.nextChunk() ?: break)
    }
}

public suspend fun Provider.complete(
    model: ProviderModelConfig,
    system: String,
    messages: List<ProviderMessage>,
): ProviderCompletion = complete(model, system, messages, emptyList())
