use goose::agents::StateMachineServices;

#[tokio::test]
async fn global_services_do_not_construct_scheduler() {
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

    let services = StateMachineServices::instance().await;

    assert!(services.config.scheduler_service.is_none());
    assert_eq!(std::fs::read(schedule_path).unwrap(), sentinel);
}
