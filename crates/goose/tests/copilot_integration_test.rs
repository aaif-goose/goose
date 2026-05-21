use goose::config::Config;
use goose::copilot::{
    cached_installation_id, clear_install, extract_agent_id, load_prefs_from, save_prefs_to,
    CopilotCommentRequest, CopilotPrefs, ReviewSeverity, RoutingPrefs, TriggerPreference,
    INSTALLATION_ID_CONFIG_KEY,
};
use tempfile::NamedTempFile;

fn isolated_config() -> Config {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
}

#[test]
fn prefs_roundtrip_through_isolated_config() {
    let config = isolated_config();
    let prefs = CopilotPrefs {
        auto_review_on_pr_open: false,
        trigger_preference: TriggerPreference::ManualOnly,
        review_severity: ReviewSeverity::High,
        custom_instructions: "be thorough".into(),
        ..Default::default()
    };
    prefs.validate().unwrap();
    save_prefs_to(&config, &prefs).unwrap();
    assert_eq!(load_prefs_from(&config), prefs);
}

#[test]
fn clear_install_removes_id_and_prefs() {
    let config = isolated_config();
    config
        .set_param(INSTALLATION_ID_CONFIG_KEY, serde_json::json!(99_u64))
        .unwrap();
    save_prefs_to(
        &config,
        &CopilotPrefs {
            custom_instructions: "gone".into(),
            ..Default::default()
        },
    )
    .unwrap();
    clear_install(&config);
    assert_eq!(cached_installation_id(&config), None);
    assert_eq!(load_prefs_from(&config), CopilotPrefs::default());
}

#[test]
fn routing_default_matches_switchboard_fixture() {
    let routing = RoutingPrefs::default();
    let actual = serde_json::to_string(&routing).unwrap();
    let expected =
        include_str!("../../../services/copilot-switchboard/fixtures/routing_prefs_default.json")
            .trim();
    assert_eq!(actual, expected);
}

#[test]
fn routing_subset_strips_non_routing_fields() {
    let prefs = CopilotPrefs {
        custom_instructions: "full prefs".into(),
        review_severity: ReviewSeverity::Critical,
        specific_users_allowlist: vec!["octocat".into()],
        ..Default::default()
    };
    prefs.validate().unwrap();
    let routing = prefs.routing_subset();
    assert_eq!(
        routing.specific_users_allowlist,
        vec!["octocat".to_string()]
    );
    assert_eq!(routing.auto_review_on_pr_open, prefs.auto_review_on_pr_open);
    let json = serde_json::to_value(&routing).unwrap();
    assert!(json.get("custom_instructions").is_none());
    assert!(json.get("review_severity").is_none());
}

#[test]
fn extract_agent_id_parses_tunnel_url() {
    assert_eq!(
        extract_agent_id("https://tunnel-proxy.example/tunnel/abc123"),
        Some("abc123".to_string())
    );
    assert_eq!(
        extract_agent_id("https://tunnel-proxy.example/tunnel/abc123/extra?q=1"),
        Some("abc123".to_string())
    );
    assert_eq!(extract_agent_id("https://example.com/no-tunnel-path"), None);
}

#[test]
fn comment_request_is_pr_explicit_false() {
    let req: CopilotCommentRequest = serde_json::from_str(
        r#"{
            "github_token": "t",
            "repo": "o/r",
            "pr_number": 1,
            "pr_url": "https://github.com/o/r/issues/1",
            "comment_body": "hi",
            "commenter": "u",
            "is_pr": false
        }"#,
    )
    .unwrap();
    assert!(!req.is_pr);
}

#[test]
fn comment_request_is_pr_omitted_defaults_legacy_true() {
    let req: CopilotCommentRequest = serde_json::from_str(
        r#"{
            "github_token": "t",
            "repo": "o/r",
            "pr_number": 1,
            "pr_url": "https://github.com/o/r/pull/1",
            "comment_body": "hi",
            "commenter": "u"
        }"#,
    )
    .unwrap();
    assert!(req.is_pr);
}
