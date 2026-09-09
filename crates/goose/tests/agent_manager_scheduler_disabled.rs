use goose::execution::manager::AgentManager;

#[tokio::test]
async fn global_agent_manager_does_not_construct_scheduler() {
    let root = tempfile::tempdir().unwrap();
    let _guard = env_lock::lock_env([
        ("GOOSE_DISABLE_KEYRING", Some("true")),
        ("GOOSE_PATH_ROOT", root.path().to_str()),
    ]);
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let schedule_path = data_dir.join("schedule.json");
    let sentinel = b"do not touch";
    std::fs::write(&schedule_path, sentinel).unwrap();

    let manager = AgentManager::instance().await.unwrap();

    assert!(manager.scheduler().is_none());
    assert_eq!(std::fs::read(schedule_path).unwrap(), sentinel);
}
