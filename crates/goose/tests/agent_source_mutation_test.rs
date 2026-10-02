use agent_client_protocol::ErrorCode;
use goose::config::paths::Paths;
use goose::sources::{create_source, import_sources, list_sources};
use goose_sdk_types::custom_requests::SourceType;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

fn assert_project_mutations_rejected(project: &Path) {
    let create = create_source(
        SourceType::Agent,
        "Project Agent",
        "An agent",
        "Instructions",
        false,
        project.to_str(),
        HashMap::new(),
    );
    let import = import_sources(
        &json!({
            "version": 1,
            "type": "agent",
            "name": "Imported Agent",
            "instructions": "Imported instructions",
            "metadata": { "model": "test-model" }
        })
        .to_string(),
        false,
        project.to_str(),
    );
    for error in [create.unwrap_err(), import.unwrap_err()] {
        assert_eq!(error.code, ErrorCode::InvalidParams);
        assert!(error.to_string().contains("Project-scoped Agent"));
    }
}

#[test]
fn project_agent_create_and_import_do_not_create_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    assert_project_mutations_rejected(&project);
    assert!(!project.exists());

    std::fs::create_dir(&project).unwrap();
    assert_project_mutations_rejected(&project);
    assert_eq!(std::fs::read_dir(project).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn project_agent_create_and_import_do_not_follow_linked_roots_or_ancestors() {
    use std::os::unix::fs::symlink;

    for linked_component in [".agents", ".agents/agents"] {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let external = tmp.path().join("external");
        std::fs::create_dir(&external).unwrap();
        let link = project.join(linked_component);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&external, &link).unwrap();

        assert_project_mutations_rejected(&project);
        assert_eq!(std::fs::read_dir(&external).unwrap().count(), 0);
        assert!(link.is_symlink());
    }
}

#[test]
fn existing_project_agents_remain_discoverable() {
    let tmp = tempfile::tempdir().unwrap();
    let _env = env_lock::lock_env([("GOOSE_PATH_ROOT", tmp.path().to_str())]);
    let project = tmp.path().join("project");
    let agent_dir = project.join(".agents/agents");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let agent_path = agent_dir.join("existing.md");
    let contents = "---\nname: Existing Project Agent\ndescription: Existing\n---\n\nInstructions";
    std::fs::write(&agent_path, contents).unwrap();

    let sources = list_sources(Some(SourceType::Agent), project.to_str(), false).unwrap();
    let agent = sources
        .iter()
        .find(|source| source.name == "Existing Project Agent")
        .unwrap();
    assert!(!agent.global);
    assert_eq!(agent.content, "Instructions");
    assert_eq!(std::fs::read_to_string(agent_path).unwrap(), contents);
    assert_eq!(std::fs::read_dir(agent_dir).unwrap().count(), 1);
}

#[test]
fn global_agent_create_and_import_preserve_content_and_collision_naming() {
    let tmp = tempfile::tempdir().unwrap();
    let _env = env_lock::lock_env([("GOOSE_PATH_ROOT", tmp.path().to_str())]);
    let properties = HashMap::from([("model".to_owned(), json!("test-model"))]);
    let created = create_source(
        SourceType::Agent,
        "Global Agent",
        "Global description",
        "Global instructions",
        true,
        None,
        properties.clone(),
    )
    .unwrap();
    assert!(created.global);
    assert!(created.writable);
    assert_eq!(created.properties, properties);
    assert_eq!(
        Path::new(&created.path),
        Paths::agents_dir().join("global-agent.md")
    );

    let imported = import_sources(
        &json!({
            "version": 1,
            "type": "agent",
            "name": created.name,
            "description": created.description,
            "content": created.content,
            "properties": properties
        })
        .to_string(),
        true,
        None,
    )
    .unwrap()
    .remove(0);
    assert!(imported.global);
    assert!(imported.writable);
    assert_eq!(imported.name, created.name);
    assert_eq!(imported.description, created.description);
    assert_eq!(imported.content, created.content);
    assert_eq!(imported.properties, properties);
    assert_eq!(
        Path::new(&imported.path),
        Paths::agents_dir().join("global-agent-2.md")
    );
    assert!(Path::new(&created.path).is_file());
}
