package io.github.aaif_goose.providers.databricks

public fun provider(host: String, token: String): io.github.aaif_goose.Provider =
    io.github.aaif_goose.databricksProvider(host, token)

public fun defaultModel(): String = io.github.aaif_goose.databricksDefaultModel()
