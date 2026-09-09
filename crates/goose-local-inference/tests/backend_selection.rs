use goose_local_inference::selection::select_backend;

#[test]
fn defaults_follow_artifact_format() {
    assert_eq!(select_backend("gguf", None, None).unwrap(), "llamacpp");
    assert_eq!(select_backend("safetensors", None, None).unwrap(), "eredu");
}

#[test]
fn eredu_can_be_selected_for_gguf_globally_or_per_model() {
    assert_eq!(
        select_backend("gguf", None, Some("eredu")).unwrap(),
        "eredu"
    );
    assert_eq!(
        select_backend("gguf", Some("eredu"), None).unwrap(),
        "eredu"
    );
    assert_eq!(
        select_backend("gguf", Some("llamacpp"), Some("eredu")).unwrap(),
        "llamacpp"
    );
    assert_eq!(
        select_backend("safetensors", Some("eredu"), Some("llamacpp")).unwrap(),
        "eredu"
    );
}

#[test]
fn eredu_admits_future_formats_without_a_goose_allowlist() {
    assert_eq!(
        select_backend("future-format", Some("eredu"), None).unwrap(),
        "eredu"
    );
}

#[test]
fn invalid_explicit_backends_never_fall_back_silently() {
    for backend in ["mlx", "unknown"] {
        assert!(select_backend("gguf", Some(backend), None).is_err());
        assert!(select_backend("gguf", None, Some(backend)).is_err());
    }
    assert!(select_backend("safetensors", Some("llamacpp"), None).is_err());
}
