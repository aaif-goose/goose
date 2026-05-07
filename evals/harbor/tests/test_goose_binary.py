from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from goose_harbor.goose_binary import GooseBinaryAgent
from goose_harbor.goose_binary import CONTAINER_CA_BUNDLE_PATH
from goose_harbor.goose_binary import CONTAINER_RECIPE_PATH
from goose_harbor.goose_binary import CONTAINER_GOOSE_PATH_ROOT


class ExecResult:
    def __init__(self, stdout: str = "goose 1.0.0") -> None:
        self.return_code = 0
        self.stdout = stdout
        self.stderr = ""


class FakeEnvironment:
    def __init__(self) -> None:
        self.uploads: list[tuple[Path, str]] = []
        self.dir_uploads: list[tuple[Path, str]] = []
        self.commands: list[dict[str, object]] = []
        self.default_user: str | int | None = None
        self.has_system_ca = True

    async def upload_file(self, source_path: Path | str, target_path: str) -> None:
        self.uploads.append((Path(source_path), target_path))

    async def upload_dir(self, source_dir: Path | str, target_dir: str) -> None:
        self.dir_uploads.append((Path(source_dir), target_dir))

    async def exec(
        self,
        command: str,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
        timeout_sec: int | None = None,
        user: str | int | None = None,
    ) -> ExecResult:
        self.commands.append(
            {
                "command": command,
                "cwd": cwd,
                "env": env,
                "timeout_sec": timeout_sec,
                "user": user,
            }
        )
        if "id -u && id -g" in command:
            return ExecResult("1000\n1000\n")
        if "ca-certificates.crt" in command and "echo present" in command:
            return ExecResult("present\n" if self.has_system_ca else "missing\n")
        return ExecResult()


@pytest.fixture
def goose_binary(tmp_path: Path) -> Path:
    path = tmp_path / "goose"
    path.write_text("#!/bin/sh\n")
    return path


@pytest.fixture
def goose_profile(tmp_path: Path) -> Path:
    path = tmp_path / "profile"
    (path / "config").mkdir(parents=True)
    (path / "config" / "config.yaml").write_text("GOOSE_PROVIDER: databricks\n")
    return path


def test_install_uploads_binary_and_profile(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
        )
        environment = FakeEnvironment()

        await agent.install(environment)
        return environment

    environment = asyncio.run(run_test())

    assert environment.uploads == [(goose_binary.resolve(), "/installed-agent/goose")]
    commands = "\n".join(str(item["command"]) for item in environment.commands)
    assert "chmod 755 /installed-agent/goose" in commands
    assert "ln -sf /installed-agent/goose ~/.local/bin/goose" in commands
    assert environment.dir_uploads == [(goose_profile.resolve(), "/installed-agent/goose-profile")]


def test_install_uploads_config_directory_profile(
    goose_binary: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        config_dir = tmp_path / "config"
        config_dir.mkdir()
        (config_dir / "config.yaml").write_text("GOOSE_PROVIDER: databricks\n")
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(config_dir),
        )
        environment = FakeEnvironment()

        await agent.install(environment)
        return environment

    environment = asyncio.run(run_test())

    assert environment.dir_uploads == [(tmp_path / "config", "/installed-agent/goose-profile/config")]


def test_install_chowns_uploaded_profile_when_agent_user_is_image_default(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
        )
        environment = FakeEnvironment()

        await agent.install(environment)
        return environment

    environment = asyncio.run(run_test())

    commands = [str(item["command"]) for item in environment.commands]
    assert any("id -u && id -g" in command for command in commands)
    assert any(
        "chown -R 1000:1000 /installed-agent/goose-profile" in command
        for command in commands
    )


def test_install_can_install_goose_runtime_deps(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
            install_goose_runtime_deps=True,
        )
        environment = FakeEnvironment()

        await agent.install(environment)
        return environment

    environment = asyncio.run(run_test())

    commands = [str(item["command"]) for item in environment.commands]
    assert any("apt-get install -y libgomp1" in command for command in commands)


def test_missing_container_ca_bundle_is_uploaded_and_used(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    async def run_test() -> FakeEnvironment:
        host_ca_bundle = tmp_path / "cert.pem"
        host_ca_bundle.write_text("test cert\n")
        monkeypatch.setenv("SSL_CERT_FILE", str(host_ca_bundle))
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
        )
        environment = FakeEnvironment()
        environment.has_system_ca = False

        await agent.install(environment)
        await agent.run("fix the repo", environment, object())
        return environment

    environment = asyncio.run(run_test())

    assert any(target == CONTAINER_CA_BUNDLE_PATH for _, target in environment.uploads)
    assert environment.commands[-1]["env"]["SSL_CERT_FILE"] == CONTAINER_CA_BUNDLE_PATH


def test_run_uses_profile_without_keyring_or_provider_env_forwarding(
    goose_binary: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        profile_root = tmp_path / "profile"
        (profile_root / "config").mkdir(parents=True)
        (profile_root / "config" / "config.yaml").write_text("GOOSE_PROVIDER: databricks\n")
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(profile_root),
        )
        environment = FakeEnvironment()

        await agent.run("fix the repo", environment, object())
        return environment

    environment = asyncio.run(run_test())

    run_command = environment.commands[-1]
    env = run_command["env"]
    assert isinstance(env, dict)
    assert env["GOOSE_PATH_ROOT"] == "/installed-agent/goose-profile"
    assert env["GOOSE_DISABLE_KEYRING"] == "true"
    assert "DATABRICKS_TOKEN" not in env


def test_run_uploads_recipe_file_instead_of_heredoc(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
        )
        environment = FakeEnvironment()

        await agent.run("line before\nEOF\nline after", environment, object())
        return environment

    environment = asyncio.run(run_test())

    commands = [str(item["command"]) for item in environment.commands]
    assert all("<< 'EOF'" not in command for command in commands)
    assert any(target == CONTAINER_RECIPE_PATH for _, target in environment.uploads)
    assert any(
        f"goose run --recipe {CONTAINER_RECIPE_PATH}" in command
        for command in commands
    )


def test_run_copies_skills_into_isolated_profile(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        skills_dir = tmp_path / "skills"
        skills_dir.mkdir()
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
            skills_dir=str(skills_dir),
        )
        environment = FakeEnvironment()

        await agent.run("fix the repo", environment, object())
        return environment

    environment = asyncio.run(run_test())

    commands = [str(item["command"]) for item in environment.commands]
    assert any(
        f"{CONTAINER_GOOSE_PATH_ROOT}/config/skills" in command
        and "~/.config/goose/skills" not in command
        for command in commands
    )


def test_run_chowns_uploaded_recipe_for_image_default_agent_user(
    goose_binary: Path,
    goose_profile: Path,
    tmp_path: Path,
) -> None:
    async def run_test() -> FakeEnvironment:
        agent = GooseBinaryAgent(
            logs_dir=tmp_path,
            model_name="databricks/model",
            goose_binary=str(goose_binary),
            goose_profile=str(goose_profile),
        )
        environment = FakeEnvironment()

        await agent.run("fix the repo", environment, object())
        return environment

    environment = asyncio.run(run_test())

    commands = [str(item["command"]) for item in environment.commands]
    assert any("id -u && id -g" in command for command in commands)
    assert any(
        f"chown 1000:1000 {CONTAINER_RECIPE_PATH}" in command
        for command in commands
    )
