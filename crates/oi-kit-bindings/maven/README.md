# oi-kit Maven package

This project packages the UniFFI-generated Kotlin/JVM bindings for `oi-kit-bindings`
as the Maven artifact `io.github.aaif-goose:oi-kit`.

The artifact version is read from `crates/oi-kit-bindings/Cargo.toml`, so it stays in
lockstep with the Rust crate version. The jar includes the generated Kotlin API
and native libraries under JNA platform resource directories. Packaging supports
`darwin-aarch64`, `darwin-x86-64`, `linux-x86-64`, `linux-aarch64`, and
`win32-x86-64` resource prefixes; CI is responsible for assembling every native
library into the final published jar.

Build locally from the repository root:

```bash
just --justfile crates/oi-kit-bindings/justfile maven-package
```

Publish to Maven Central from the repository root:

```bash
just --justfile crates/oi-kit-bindings/justfile maven-publish
```

Publishing requires the standard Gradle properties used by
`com.vanniktech.maven.publish` for Maven Central credentials and in-memory PGP
signing, for example via environment variables:

- `ORG_GRADLE_PROJECT_mavenCentralUsername`
- `ORG_GRADLE_PROJECT_mavenCentralPassword`
- `ORG_GRADLE_PROJECT_signingInMemoryKey`
- `ORG_GRADLE_PROJECT_signingInMemoryKeyPassword`
