# Kotlin/JVM GDK smoke test

This is a small downstream Kotlin/JVM app that consumes the Maven artifact
`io.github.aaif-goose:gdk` from `mavenLocal()`.

From the repository root, first build and publish the Maven artifact locally:

```bash
source bin/activate-hermit
just --justfile crates/goose-sdk/justfile maven-package
```

Then run the smoke test:

```bash
cd crates/goose-sdk/examples/uniffi/kotlin
gradle --no-daemon run
```

Set `DATABRICKS_HOST` and `DATABRICKS_TOKEN` before running the example.
`DATABRICKS_HOST` should be the Databricks workspace URL, for example
`https://dbc-xxxxxxxx-xxxx.cloud.databricks.com`. The example uses the native
GDK `DatabricksProvider`, not the declarative JSON provider. The expected output
is a streamed completion from Databricks followed by optional usage metadata.
The important failure to watch for is `UnsatisfiedLinkError` or a missing native
library resource, which would mean the bundled native library was not loaded
correctly.

The example sets `--enable-native-access=ALL-UNNAMED` because JNA loads the
bundled Goose native library. Newer JDKs warn when native access is not enabled
explicitly, and future JDKs may require it.
