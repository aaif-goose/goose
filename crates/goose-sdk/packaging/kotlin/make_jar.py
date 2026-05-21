#!/usr/bin/env python3
"""Assemble the goose-sdk Kotlin/JVM JAR (debug profile).

Builds the cargo dylib + uniffi bindings, downloads JNA if missing, compiles
the Kotlin sources, and bundles classes + the native library into a JAR with
a minimal POM. Output: packaging/kotlin/dist/goose-sdk-<version>-<os>-<arch>.jar

Consumers also need net.java.dev.jna:jna and kotlin-stdlib on the classpath.
"""

from __future__ import annotations

import platform
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
PROFILE_DIR = REPO_ROOT / "target" / "debug"

GROUP, ARTIFACT, VERSION = "io.aaif.goose", "goose-sdk", "0.1.0"
JNA_VERSION = "5.14.0"
JNA_URL = f"https://repo1.maven.org/maven2/net/java/dev/jna/jna/{JNA_VERSION}/jna-{JNA_VERSION}.jar"

NATIVE_LOADER_KT = r"""
package io.aaif.goose.sdk

import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption

/** Extracts the bundled native lib from the JAR and points JNA at it. */
object NativeLoader {
    private var loaded = false

    @Synchronized fun ensureLoaded() {
        if (loaded) return
        val (osDir, libName) = platform()
        val res = "/native/$osDir/$libName"
        val stream = NativeLoader::class.java.getResourceAsStream(res)
            ?: throw UnsatisfiedLinkError("goose-sdk: no bundled native lib at $res")
        val tmp = Files.createTempDirectory("goose-sdk-").toFile().apply { deleteOnExit() }
        val out = File(tmp, libName).apply { deleteOnExit() }
        stream.use { Files.copy(it, out.toPath(), StandardCopyOption.REPLACE_EXISTING) }
        val prev = System.getProperty("jna.library.path")
        System.setProperty("jna.library.path",
            if (prev.isNullOrEmpty()) tmp.absolutePath
            else "${tmp.absolutePath}${File.pathSeparator}$prev")
        loaded = true
    }

    private fun platform(): Pair<String, String> {
        val os = System.getProperty("os.name").lowercase()
        val arch = when (System.getProperty("os.arch").lowercase()) {
            "amd64", "x86_64" -> "x86_64"
            "aarch64", "arm64" -> "aarch64"
            else -> System.getProperty("os.arch").lowercase()
        }
        return when {
            "mac" in os || "darwin" in os -> "darwin-$arch" to "libgoose_sdk.dylib"
            "win" in os -> "windows-$arch" to "goose_sdk.dll"
            else -> "linux-$arch" to "libgoose_sdk.so"
        }
    }
}
"""

POM_XML = f"""<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>{GROUP}</groupId>
  <artifactId>{ARTIFACT}</artifactId>
  <version>{VERSION}</version>
  <dependencies>
    <dependency><groupId>net.java.dev.jna</groupId><artifactId>jna</artifactId><version>{JNA_VERSION}</version></dependency>
    <dependency><groupId>org.jetbrains.kotlin</groupId><artifactId>kotlin-stdlib</artifactId><version>1.9.0</version></dependency>
  </dependencies>
</project>
"""


def lib_info() -> tuple[str, str]:
    system = platform.system()
    arch = {"arm64": "aarch64", "aarch64": "aarch64",
            "x86_64": "x86_64", "amd64": "x86_64"}.get(
        platform.machine().lower(), platform.machine().lower())
    if system == "Darwin": return f"darwin-{arch}", "libgoose_sdk.dylib"
    if system == "Windows": return f"windows-{arch}", "goose_sdk.dll"
    return f"linux-{arch}", "libgoose_sdk.so"


def run(cmd: list[str], **kw) -> None:
    print("$", *cmd, flush=True)
    subprocess.run(cmd, check=True, **kw)


def main() -> int:
    run(["cargo", "build", "-p", "goose-sdk", "--features", "uniffi"], cwd=REPO_ROOT)

    os_arch, lib_name = lib_info()
    lib = PROFILE_DIR / lib_name
    bindgen = PROFILE_DIR / "goose-uniffi-bindgen"
    for p in (lib, bindgen):
        if not p.exists():
            sys.exit(f"missing: {p}")

    build = HERE / ".build"
    shutil.rmtree(build, ignore_errors=True)
    bindings, classes, staging = build / "bindings", build / "classes", build / "staging"
    for d in (bindings, classes, staging):
        d.mkdir(parents=True)

    run([str(bindgen), "generate", "--library", str(lib), "--language", "kotlin",
         "--no-format", "--out-dir", str(bindings)])

    jna_jar = build / f"jna-{JNA_VERSION}.jar"
    if not jna_jar.exists():
        print("$ download", JNA_URL, flush=True)
        urllib.request.urlretrieve(JNA_URL, jna_jar)

    loader_kt = build / "NativeLoader.kt"
    loader_kt.write_text(NATIVE_LOADER_KT)
    sources = [
        str(bindings / "io" / "aaif" / "goose" / "sdk_types" / "goose_sdk_types.kt"),
        str(bindings / "io" / "aaif" / "goose" / "sdk" / "goose_sdk.kt"),
        str(loader_kt),
    ]
    run(["kotlinc", "-cp", str(jna_jar), "-nowarn", "-d", str(classes), *sources])

    for sub in classes.iterdir():
        dest = staging / sub.name
        (shutil.copytree if sub.is_dir() else shutil.copy)(sub, dest)

    native = staging / "native" / os_arch
    native.mkdir(parents=True)
    shutil.copy(lib, native / lib_name)

    pom_dir = staging / "META-INF" / "maven" / GROUP / ARTIFACT
    pom_dir.mkdir(parents=True)
    (pom_dir / "pom.xml").write_text(POM_XML)

    dist = HERE / "dist"
    shutil.rmtree(dist, ignore_errors=True)
    dist.mkdir()
    jar = dist / f"{ARTIFACT}-{VERSION}-{os_arch}.jar"
    run(["jar", "--create", "--file", str(jar), "-C", str(staging), "."])

    print(f"\nBuilt JAR: {jar}")
    print(f"Runtime deps: net.java.dev.jna:jna:{JNA_VERSION}, kotlin-stdlib")
    return 0


if __name__ == "__main__":
    sys.exit(main())
