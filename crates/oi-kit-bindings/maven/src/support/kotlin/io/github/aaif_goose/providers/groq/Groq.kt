package io.github.aaif_goose.providers.groq

public fun provider(apiKey: String): io.github.aaif_goose.Provider = io.github.aaif_goose.groqProvider(apiKey)

public fun defaultModel(): String = io.github.aaif_goose.groqDefaultModel()
