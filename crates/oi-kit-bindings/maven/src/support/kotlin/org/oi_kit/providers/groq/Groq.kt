package org.oi_kit.providers.groq

public fun provider(apiKey: String): org.oi_kit.Provider = org.oi_kit.groqProvider(apiKey)

public fun defaultModel(): String = org.oi_kit.groqDefaultModel()
