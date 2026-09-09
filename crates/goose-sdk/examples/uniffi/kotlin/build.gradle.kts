plugins {
    kotlin("jvm") version "2.2.21"
    application
}

fun gooseSdkVersion(): String {
    val cargoToml = file("../../../Cargo.toml").readText()
    return Regex("(?m)^version\\s*=\\s*\"([^\"]+)\"")
        .find(cargoToml)
        ?.groupValues
        ?.get(1)
        ?: error("Could not find goose-sdk version in ../../../Cargo.toml")
}

dependencies {
    implementation("io.github.aaif-goose:gdk:${gooseSdkVersion()}")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11)
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

application {
    mainClass.set("MainKt")
    applicationDefaultJvmArgs = listOf("--enable-native-access=ALL-UNNAMED")
}
