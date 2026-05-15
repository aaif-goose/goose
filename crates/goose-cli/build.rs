fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();

    if target == "aarch64-unknown-linux-musl" {
        println!("cargo:rustc-link-lib=atomic");
    }
}
