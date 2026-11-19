package org.oi_kit.providers.databricks

public fun provider(host: String, token: String): org.oi_kit.Provider =
    org.oi_kit.databricksProvider(host, token)

public fun defaultModel(): String = org.oi_kit.databricksDefaultModel()
