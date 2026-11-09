fn main() {
    println!("cargo:rerun-if-changed=memory.x");
    if std::env::var("TARGET").unwrap().starts_with("thumb")
        && std::env::var_os("CARGO_FEATURE_BARE_METAL").is_some()
    {
        let script = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("memory.x");
        println!(
            "cargo:rustc-link-arg-bin=alloc-agent=-T{}",
            script.display()
        );
    }
}
