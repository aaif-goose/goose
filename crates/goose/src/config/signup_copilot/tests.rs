use super::CopilotInstallFlow;

#[test]
fn install_url_includes_oauth_client_and_state() {
    let flow = CopilotInstallFlow::new().with_oauth_client_id("Iv1.test-client".into());
    let url = flow.install_url();
    assert!(url.starts_with("https://github.com/login/oauth/authorize"));
    assert!(url.contains("client_id=Iv1.test-client"));
    assert!(url.contains("state="));
    assert!(url.contains("redirect_uri="));
}

#[test]
fn install_url_legacy_slug_when_no_oauth_client() {
    let flow = CopilotInstallFlow::new();
    let url = flow.install_url();
    assert!(url.contains("github.com/apps/goose-copilot/installations/new"));
    assert!(url.contains("state="));
}

#[test]
fn distinct_flows_have_distinct_state() {
    let a = CopilotInstallFlow::new();
    let b = CopilotInstallFlow::new();
    assert_ne!(a.install_url(), b.install_url());
}

#[test]
fn callback_url_is_localhost() {
    assert_eq!(CopilotInstallFlow::callback_url(), "http://localhost:3458/");
}
