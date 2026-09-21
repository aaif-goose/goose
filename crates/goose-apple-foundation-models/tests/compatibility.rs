#![cfg(all(target_os = "macos", target_arch = "aarch64", feature = "native"))]

#[test]
fn native_linkage_preserves_the_deployment_target() {
    // Keep the bridge in this executable so we inspect the same imports as consumers.
    std::hint::black_box(goose_apple_foundation_models::is_supported());
    let executable = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let word = |offset| u32::from_le_bytes(executable[offset..offset + 4].try_into().unwrap());
    assert_eq!(
        word(0),
        0xfeedfacf,
        "Expected a little-endian Mach-O 64 binary"
    );
    let version: Vec<u32> = option_env!("MACOSX_DEPLOYMENT_TARGET")
        .unwrap_or("11.0")
        .split('.')
        .map(|part| part.parse().unwrap())
        .collect();
    let expected_minimum = (version[0] << 16)
        | (version.get(1).copied().unwrap_or(0) << 8)
        | version.get(2).copied().unwrap_or(0);
    let mut offset = 32;
    let mut saw_minimum = false;
    let mut saw_foundation_models = false;
    let mut saw_concurrency = false;
    for _ in 0..word(16) {
        let command = word(offset);
        let size = word(offset + 4) as usize;
        if command == 0x32 {
            assert_eq!(word(offset + 12), expected_minimum);
            saw_minimum = true;
        }
        if matches!(command, 0xc | 0x80000018) {
            let start = offset + word(offset + 8) as usize;
            let name = std::ffi::CStr::from_bytes_until_nul(&executable[start..offset + size])
                .unwrap()
                .to_str()
                .unwrap();
            if name.contains("FoundationModels.framework") {
                if expected_minimum < (26 << 16) {
                    assert_eq!(command, 0x80000018, "Foundation Models must be weak-linked");
                }
                saw_foundation_models = true;
            }
            if name.contains("libswift_Concurrency") {
                if expected_minimum < (12 << 16) {
                    assert_eq!(command, 0x80000018, "Concurrency must be weak-linked");
                }
                assert_eq!(name, "/usr/lib/swift/libswift_Concurrency.dylib");
                saw_concurrency = true;
            }
        }
        offset += size;
    }
    assert!(saw_minimum && saw_foundation_models && saw_concurrency);
}
