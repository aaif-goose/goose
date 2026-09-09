package io.github.aaif_goose.providers.anthropic

public fun provider(
    apiKey: String,
    baseUrl: String? = null,
    betaHeaders: List<String> = emptyList(),
): io.github.aaif_goose.Provider = io.github.aaif_goose.anthropicProvider(apiKey, baseUrl, betaHeaders)

public fun defaultModel(): String = io.github.aaif_goose.anthropicDefaultModel()
