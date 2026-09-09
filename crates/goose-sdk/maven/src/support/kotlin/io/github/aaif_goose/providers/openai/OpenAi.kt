package io.github.aaif_goose.providers.openai

public fun provider(apiKey: String): io.github.aaif_goose.Provider = io.github.aaif_goose.openaiProvider(apiKey)

public fun defaultModel(): String = io.github.aaif_goose.openaiDefaultModel()
