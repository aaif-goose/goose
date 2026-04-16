# Step 10: Phase B — Migrate Config, Personas, Skills, Projects, Git, Doctor to `goose serve`

## Objective

Migrate each remaining Rust Tauri subsystem behind `goose serve` ACP extension methods, callable from TypeScript via `client.goose.<method>()`. This requires backend changes to the goose crate — adding new ACP extension methods to `goose serve`.

## Current State After Phase A (Steps 01–09)

| Module | Rust File(s) | Lines | Native Dependency |
|--------|-------------|-------|-------------------|
| Config (config.yaml, secrets, keyring) | `services/goose_config.rs`, `services/provider_defs.rs` | ~590 | Keyring, file system |
| Credentials commands | `commands/credentials.rs` | ~50 | GooseConfig |
| Personas | `services/personas.rs`, `types/agents.rs`, `types/builtin_personas.rs` | ~920 | File system |
| Persona commands | `commands/agents.rs` | ~210 | PersonaStore |
| Skills | `commands/skills.rs` | ~320 | File system |
| Projects | `commands/projects.rs` | ~495 | File system |
| Git operations | `commands/git.rs`, `commands/git_changes.rs` | ~570 | Shell commands |
| Doctor | `commands/doctor.rs` | ~15 | `doctor` crate |
| Agent setup | `commands/agent_setup.rs` | ~310 | Shell commands, streaming output |
| Model setup | `commands/model_setup.rs` | ~220 | Shell commands, streaming output |
| System utilities | `commands/system.rs` | ~360 | File system, dialog |
| **Total** | | **~4,060** | |

## Migration Pattern

For each subsystem:

1. **Backend**: Add ACP extension methods to `goose serve` (in `goose-acp` or `goose` crate)
2. **Schema**: Regenerate the ACP schema (`npm run build:schema` in `ui/acp/`)
3. **Client**: `GooseExtClient` auto-generates typed methods from the schema
4. **Frontend**: Replace `invoke("rust_command")` calls with `client.goose.<method>()` calls
5. **Cleanup**: Delete the Rust Tauri command and service code

## Subsystem Migration Details

### B1: Config Management

**Priority: High** — Config is needed for provider setup, part of the core onboarding flow.

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/config/get` | `{ key: string }` | `{ value: string \| null }` |
| `goose/config/set` | `{ key: string, value: string }` | `{}` |
| `goose/config/delete` | `{ key: string }` | `{ removed: boolean }` |
| `goose/secret/getMasked` | `{ key: string }` | `{ value: string \| null }` |
| `goose/secret/set` | `{ key: string, value: string }` | `{}` |
| `goose/secret/delete` | `{ key: string }` | `{ removed: boolean }` |
| `goose/provider/status` | `{ providerId: string }` | `{ providerId: string, isConfigured: boolean }` |
| `goose/provider/statusAll` | `{}` | `{ providers: [{ providerId: string, isConfigured: boolean }] }` |
| `goose/provider/fields` | `{ providerId: string }` | `{ fields: [{ key: string, value: string \| null, isSet: boolean, isSecret: boolean, required: boolean }] }` |
| `goose/provider/deleteConfig` | `{ providerId: string }` | `{}` |

#### Backend Notes

- The goose binary already has config management internally (`goose configure`). The extension methods expose the same logic over ACP.
- Keyring access happens in the `goose serve` process (which runs natively), so there is no loss of capability.
- Move `provider_defs.rs` static definitions to the goose crate.

#### Frontend Changes

- `invoke("get_provider_config")` → `client.goose.gooseProviderFields({ providerId })`
- `invoke("save_provider_field")` → `client.goose.gooseSecretSet({ key, value })` or `client.goose.gooseConfigSet({ key, value })`
- `invoke("delete_provider_config")` → `client.goose.gooseProviderDeleteConfig({ providerId })`
- `invoke("check_all_provider_status")` → `client.goose.gooseProviderStatusAll({})`
- `invoke("restart_app")` — remains in Rust (native window management)

#### Files Deleted

- `src-tauri/src/services/goose_config.rs`
- `src-tauri/src/services/provider_defs.rs`
- `src-tauri/src/commands/credentials.rs` (except `restart_app`)
- `keyring` dependency from `Cargo.toml` (all 3 platform variants)
- `etcetera` dependency

---

### B2: Personas

**Priority: Medium** — Used in the chat flow but not on the critical path.

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/personas/list` | `{}` | `{ personas: Persona[] }` |
| `goose/personas/create` | `CreatePersonaRequest` | `{ persona: Persona }` |
| `goose/personas/update` | `{ id: string, ...UpdatePersonaRequest }` | `{ persona: Persona }` |
| `goose/personas/delete` | `{ id: string }` | `{}` |
| `goose/personas/refresh` | `{}` | `{ personas: Persona[] }` |
| `goose/personas/export` | `{ id: string }` | `{ json: string, suggestedFilename: string }` |
| `goose/personas/import` | `{ fileBytes: number[], fileName: string }` | `{ personas: Persona[] }` |
| `goose/personas/saveAvatar` | `{ personaId: string, bytes: number[], extension: string }` | `{ filename: string }` |
| `goose/personas/avatarsDir` | `{}` | `{ path: string }` |

#### Backend Notes

- Persona storage (`~/.goose/personas.json`, `~/.goose/agents/*.md`) and avatar handling (`~/.goose/avatars/`) are file-based. The goose binary can read/write these directly.
- Move builtin persona definitions from `types/builtin_personas.rs` to the goose crate.

#### Files Deleted

- `src-tauri/src/services/personas.rs`
- `src-tauri/src/types/agents.rs`
- `src-tauri/src/types/builtin_personas.rs`
- `src-tauri/src/types/messages.rs`
- `src-tauri/src/types/mod.rs`
- `src-tauri/src/commands/agents.rs`

---

### B3: Skills

**Priority: Low**

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/skills/list` | `{}` | `{ skills: SkillInfo[] }` |
| `goose/skills/create` | `{ name, description, instructions }` | `{}` |
| `goose/skills/update` | `{ name, description, instructions }` | `{ skill: SkillInfo }` |
| `goose/skills/delete` | `{ name: string }` | `{}` |
| `goose/skills/export` | `{ name: string }` | `{ json: string, filename: string }` |
| `goose/skills/import` | `{ fileBytes: number[], fileName: string }` | `{ skills: SkillInfo[] }` |

#### Files Deleted

- `src-tauri/src/commands/skills.rs`

---

### B4: Projects

**Priority: Low**

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/projects/list` | `{}` | `{ projects: ProjectInfo[] }` |
| `goose/projects/create` | `{ name, description, prompt, icon, color, ... }` | `{ project: ProjectInfo }` |
| `goose/projects/update` | `{ id, name, description, prompt, icon, color, ... }` | `{ project: ProjectInfo }` |
| `goose/projects/delete` | `{ id: string }` | `{}` |
| `goose/projects/get` | `{ id: string }` | `{ project: ProjectInfo }` |
| `goose/projects/listArchived` | `{}` | `{ projects: ProjectInfo[] }` |
| `goose/projects/archive` | `{ id: string }` | `{}` |
| `goose/projects/restore` | `{ id: string }` | `{}` |

#### Files Deleted

- `src-tauri/src/commands/projects.rs`

---

### B5: Git Operations

**Priority: Medium** — Git state is shown in the workspace widget and context panel.

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/git/state` | `{ path: string }` | `GitState` |
| `goose/git/changedFiles` | `{ path: string }` | `{ files: ChangedFile[] }` |
| `goose/git/switchBranch` | `{ path, branch }` | `{}` |
| `goose/git/stash` | `{ path }` | `{}` |
| `goose/git/init` | `{ path }` | `{}` |
| `goose/git/fetch` | `{ path }` | `{}` |
| `goose/git/pull` | `{ path }` | `{}` |
| `goose/git/createBranch` | `{ path, name, baseBranch }` | `{}` |
| `goose/git/createWorktree` | `{ path, name, branch, createBranch, baseBranch? }` | `CreatedWorktree` |

#### Backend Notes

- Git operations run shell commands (`git status`, `git switch`, etc.). The goose binary runs these the same way.
- The `ignore` crate for `.gitignore`-aware file scanning in `list_files_for_mentions` moves to goose serve as well.

#### Files Deleted

- `src-tauri/src/commands/git.rs`
- `src-tauri/src/commands/git_changes.rs`

---

### B6: Doctor

**Priority: Low** — Diagnostic tool, not on the critical path.

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/doctor/run` | `{}` | `DoctorReport` |
| `goose/doctor/fix` | `{ checkId: string, fixType: string }` | `{}` |

#### Backend Notes

The `doctor` crate already exists in the goose ecosystem. The extension methods expose it over ACP.

#### Files Deleted

- `src-tauri/src/commands/doctor.rs`
- `doctor` dependency from `Cargo.toml`

---

### B7: Agent & Model Setup

**Priority: Medium** — Needed for onboarding third-party agents and OAuth flows.

This subsystem involves interactive shell commands with streaming output. The current Rust code spawns a child process and streams stdout/stderr lines as Tauri events (`agent-setup:output`, `model-setup:output`).

#### Recommendation: Keep in Rust

These commands remain as Tauri-native commands. They are inherently interactive (opening browsers for OAuth, waiting for user input), are rarely called (only during onboarding), and migrating them would require designing a new ACP streaming notification type. They stay as the last remaining Tauri commands.

---

### B8: System Utilities

**Priority: Low**

#### Extension Methods

| Method | Request | Response |
|--------|---------|----------|
| `goose/system/homeDir` | `{}` | `{ path: string }` |
| `goose/system/pathExists` | `{ path: string }` | `{ exists: boolean }` |
| `goose/system/listDir` | `{ path: string }` | `{ entries: FileTreeEntry[] }` |
| `goose/system/listFilesForMentions` | `{ roots: string[], maxResults?: number }` | `{ files: string[] }` |

#### Stays in Rust: `saveExportedSessionFile`

This command uses `tauri_plugin_dialog` to show a native save dialog. It cannot move to `goose serve`.

#### Files Deleted

- `src-tauri/src/commands/system.rs` (except `save_exported_session_file`)
- `ignore` dependency from `Cargo.toml`

---

## End State After Phase B

**Rust Tauri backend (~780 lines):**

```
src-tauri/src/
  lib.rs                    — ~40 lines: spawn goose serve, register ~3 commands
  main.rs                   — 6 lines (unchanged)
  commands/
    mod.rs                  — 3 modules
    acp.rs                  — get_goose_serve_url (~15 lines)
    system.rs               — save_exported_session_file (~40 lines)
    agent_setup.rs          — install/auth agents (~310 lines)
    model_setup.rs          — model provider auth (~220 lines)
  services/
    mod.rs                  — 1 module
    acp/
      mod.rs                — 1 module
      goose_serve.rs        — GooseServeProcess (~150 lines)
```

**Cargo.toml dependencies (minimal):**

```toml
tauri = "2"
tauri-plugin-opener = "2"
tauri-plugin-dialog = ">=2,<2.7"
tauri-plugin-window-state = "2"
tauri-plugin-log = "2"
serde = "1"
serde_json = "1"
tokio = "1"
dirs = "6"
log = "0.4"
```

## Migration Order

| Step | Effort | Value | Order |
|------|--------|-------|-------|
| B1 (Config) | Medium | High (removes keyring dep) | 1st |
| B5 (Git) | Medium | Medium | 2nd |
| B2 (Personas) | Medium | Medium | 3rd |
| B3 (Skills) | Small | Small | 4th |
| B4 (Projects) | Small | Small | 5th |
| B6 (Doctor) | Small | Small | 6th |
| B8 (System utils) | Small | Small | 7th |
| B7 (Agent/Model setup) | — | — | Keep in Rust |

All steps are blocked on implementing the corresponding backend ACP methods, except B7 which remains native.

## Workflow Per Subsystem

1. Design the ACP extension method schemas in `crates/goose-acp/`
2. Implement the handlers in the goose serve server
3. Regenerate the schema: `cd ui/acp && npm run build:schema`
4. Rebuild the TS client: `cd ui/acp && npm run build`
5. Update goose2: use the new `client.goose.<method>()` calls
6. Delete the Rust Tauri code

Each subsystem migrates independently. The frontend can use a mix of `invoke()` (not-yet-migrated) and `client.goose.*()` (migrated) during the transition.
