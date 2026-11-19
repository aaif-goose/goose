package org.oi_kit.providers.anthropic

public fun provider(
    apiKey: String,
    baseUrl: String? = null,
    betaHeaders: List<String> = emptyList(),
): org.oi_kit.Provider = org.oi_kit.anthropicProvider(apiKey, baseUrl, betaHeaders)

public fun defaultModel(): String = org.oi_kit.anthropicDefaultModel()
