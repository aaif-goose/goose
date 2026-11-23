package org.oi_kit.providers.openai

public fun provider(
    apiKey: String,
    baseUrl: String? = null,
): org.oi_kit.Provider = org.oi_kit.openaiProvider(apiKey, baseUrl)

public fun defaultModel(): String = org.oi_kit.openaiDefaultModel()
