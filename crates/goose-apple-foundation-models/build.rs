use std::{env, path::PathBuf, process::Command};

fn output(command: &mut Command) -> String {
    let result = command
        .output()
        .expect("Xcode 27 is required to build Apple Foundation Models bindings");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

fn main() {
    println!("cargo:rerun-if-changed=swift/Bridge.swift");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("aarch64")
        || env::var_os("CARGO_FEATURE_NATIVE").is_none()
    {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let deployment_target = output(
        Command::new(env::var_os("RUSTC").unwrap())
            .args(["--print", "deployment-target", "--target"])
            .arg(env::var_os("TARGET").unwrap()),
    );
    let deployment_target = deployment_target.split_once('=').unwrap().1;
    let sdk = output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
    output(
        Command::new("xcrun")
            .args([
                "swiftc",
                "-parse-as-library",
                "-emit-library",
                "-static",
                "-O",
                "-swift-version",
                "6",
                "-module-name",
                "GooseAppleFoundationModels",
                "-target",
                &format!("arm64-apple-macos{deployment_target}"),
                "-sdk",
                &sdk,
                "-module-cache-path",
            ])
            .arg(out.join("module-cache"))
            .arg("swift/Bridge.swift")
            .arg("-o")
            .arg(out.join("libgoose_afm.a")),
    );
    // Rust's older deployment target otherwise selects an @rpath back-deployment
    // runtime. We only use concurrency on macOS 27+, where it is in the dyld cache.
    // Use a local SDK stub so downstream binaries need no special linker flags.
    let concurrency_stub = PathBuf::from(&sdk).join("usr/lib/swift/libswift_Concurrency.tbd");
    println!("cargo:rerun-if-changed={}", concurrency_stub.display());
    let stub = std::fs::read_to_string(concurrency_stub).unwrap().replace(
        "$ld$previous$@rpath/libswift_Concurrency.dylib",
        "goose_unused_swift_concurrency_backdeployment",
    );
    std::fs::write(out.join("libswift_Concurrency.tbd"), stub).unwrap();
    let swiftc = PathBuf::from(output(Command::new("xcrun").args(["--find", "swiftc"])));
    let toolchain = swiftc.parent().unwrap().parent().unwrap();
    println!("cargo:rustc-link-search=native={}", out.display());
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain.join("lib/swift/macosx").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain.join("lib/swift/compatibility/macosx").display()
    );
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-lib=static=goose_afm");
    println!("cargo:rustc-link-lib=framework=Foundation");
    // Swift's availability-annotated imports produce weak load commands for the
    // newer frameworks and runtimes. Explicit -l/-framework flags defeat that.
    println!("cargo:rustc-link-lib=dylib=swiftCore");
}
