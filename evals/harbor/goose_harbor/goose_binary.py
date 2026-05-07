from __future__ import annotations

import os
import shlex
from pathlib import Path
from tempfile import TemporaryDirectory

from harbor.agents.installed.base import with_prompt_template
from harbor.agents.installed.goose import Goose
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

CONTAINER_GOOSE_PATH_ROOT = "/installed-agent/goose-profile"
CONTAINER_RECIPE_PATH = "/installed-agent/harbor-recipe.yaml"
CONTAINER_CA_BUNDLE_PATH = "/installed-agent/ca-certificates.crt"


class GooseBinaryAgent(Goose):
    """Run a caller-provided Goose binary in the benchmark environment."""

    def __init__(
        self,
        *args,
        goose_binary: str,
        goose_profile: str,
        install_goose_runtime_deps: bool = False,
        **kwargs,
    ):
        super().__init__(*args, **kwargs)
        self.goose_binary = Path(goose_binary).expanduser().resolve()
        self.goose_profile = Path(goose_profile).expanduser().resolve()
        self.install_goose_runtime_deps = install_goose_runtime_deps
        self.ca_bundle_env_path: str | None = None

    @staticmethod
    def name() -> str:
        return "goose-binary"

    def get_version_command(self) -> str | None:
        return "/installed-agent/goose --version"

    def _profile_source_target(self) -> tuple[Path, str]:
        if not self.goose_profile.is_dir():
            raise FileNotFoundError(f"Goose profile does not exist: {self.goose_profile}")

        if (self.goose_profile / "config.yaml").is_file():
            return self.goose_profile, f"{CONTAINER_GOOSE_PATH_ROOT}/config"

        return self.goose_profile, CONTAINER_GOOSE_PATH_ROOT

    def _run_env(self) -> dict[str, str]:
        if not self.model_name or "/" not in self.model_name:
            raise ValueError("Model name must be in the format provider/model_name")

        provider, model = self.model_name.split("/", 1)
        env = {
            "GOOSE_MODEL": model,
            "GOOSE_PROVIDER": provider,
            "GOOSE_TELEMETRY_ENABLED": "false",
            "GOOSE_TELEMETRY_OFF": "true",
            "CONFIGURE": "false",
            "GOOSE_PATH_ROOT": CONTAINER_GOOSE_PATH_ROOT,
            "GOOSE_DISABLE_KEYRING": "true",
        }
        if self.ca_bundle_env_path:
            env["SSL_CERT_FILE"] = self.ca_bundle_env_path
        return env

    def _host_ca_bundle(self) -> Path:
        candidates = [
            "SSL_CERT_FILE",
            "REQUESTS_CA_BUNDLE",
            "CURL_CA_BUNDLE",
        ]
        for env_var in candidates:
            value = os.environ.get(env_var)
            if value and Path(value).expanduser().is_file():
                return Path(value).expanduser().resolve()

        for path in [
            Path("/etc/ssl/certs/ca-certificates.crt"),
            Path("/etc/ssl/cert.pem"),
            Path("/opt/homebrew/etc/ca-certificates/cert.pem"),
        ]:
            if path.is_file():
                return path.resolve()

        raise FileNotFoundError("Could not find a host CA bundle to copy into the task container")

    async def _ensure_ca_bundle(self, environment: BaseEnvironment) -> None:
        result = await self.exec_as_root(
            environment,
            command=(
                "if [ -r /etc/ssl/certs/ca-certificates.crt ]; "
                "then echo present; else echo missing; fi"
            ),
            timeout_sec=10,
        )
        if result.stdout.strip() != "missing":
            return

        await environment.upload_file(self._host_ca_bundle(), CONTAINER_CA_BUNDLE_PATH)
        await self.exec_as_root(
            environment,
            command=f"chmod 644 {shlex.quote(CONTAINER_CA_BUNDLE_PATH)}",
            timeout_sec=10,
        )
        self.ca_bundle_env_path = CONTAINER_CA_BUNDLE_PATH

    async def _install_goose_runtime_deps(self, environment: BaseEnvironment) -> None:
        await self.exec_as_root(
            environment,
            command=(
                "command -v apt-get >/dev/null 2>&1 || "
                "(echo 'install_goose_runtime_deps requires apt-get in the task container' >&2; exit 1); "
                "apt-get update && "
                "DEBIAN_FRONTEND=noninteractive apt-get install -y libgomp1"
            ),
            timeout_sec=300,
        )

    def _build_register_skills_command(self) -> str | None:
        if not self.skills_dir:
            return None
        skills_target = f"{CONTAINER_GOOSE_PATH_ROOT}/config/skills"
        return (
            f"mkdir -p {shlex.quote(skills_target)} && "
            f"cp -r {shlex.quote(self.skills_dir)}/* "
            f"{shlex.quote(skills_target)}/ 2>/dev/null || true"
        )

    async def _agent_uid_gid(self, environment: BaseEnvironment) -> tuple[str, str]:
        result = await self.exec_as_agent(
            environment,
            command="id -u && id -g",
            timeout_sec=10,
        )
        ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if len(ids) < 2:
            raise RuntimeError(f"Could not determine agent uid/gid: {result.stdout!r}")

        return ids[0], ids[1]

    async def _chown_to_agent_user(
        self,
        environment: BaseEnvironment,
        path: str,
        *,
        recursive: bool = False,
    ) -> None:
        uid, gid = await self._agent_uid_gid(environment)
        recursive_flag = "-R " if recursive else ""
        await self.exec_as_root(
            environment,
            command=(
                f"chown {recursive_flag}{shlex.quote(uid)}:{shlex.quote(gid)} "
                f"{shlex.quote(path)}"
            ),
        )

    async def install(self, environment: BaseEnvironment) -> None:
        if not self.goose_binary.is_file():
            raise FileNotFoundError(f"Goose binary does not exist: {self.goose_binary}")

        await environment.upload_file(self.goose_binary, "/installed-agent/goose")
        await self.exec_as_root(environment, command="chmod 755 /installed-agent/goose")
        if self.install_goose_runtime_deps:
            await self._install_goose_runtime_deps(environment)
        await self._ensure_ca_bundle(environment)

        source, target = self._profile_source_target()
        await self.exec_as_root(environment, command=f"mkdir -p {shlex.quote(target)}")
        await environment.upload_dir(source, target)
        await self._chown_to_agent_user(
            environment, CONTAINER_GOOSE_PATH_ROOT, recursive=True
        )

        await self.exec_as_agent(
            environment,
            command=(
                "mkdir -p ~/.local/bin && "
                "ln -sf /installed-agent/goose ~/.local/bin/goose && "
                "~/.local/bin/goose --version"
            ),
            env={
                "GOOSE_DISABLE_KEYRING": "true",
                "GOOSE_TELEMETRY_ENABLED": "false",
                "GOOSE_TELEMETRY_OFF": "true",
                "CONFIGURE": "false",
            },
            timeout_sec=30,
        )

    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        env = self._run_env()
        recipe_yaml = self._create_recipe_yaml(instruction)

        skills_command = self._build_register_skills_command()
        if skills_command:
            await self.exec_as_agent(
                environment,
                command=skills_command,
                env=env,
                timeout_sec=10,
            )

        with TemporaryDirectory() as tmp_dir:
            recipe_path = Path(tmp_dir) / "harbor-recipe.yaml"
            recipe_path.write_text(recipe_yaml)
            await environment.upload_file(recipe_path, CONTAINER_RECIPE_PATH)

        await self._chown_to_agent_user(environment, CONTAINER_RECIPE_PATH)

        cli_flags = self.build_cli_flags()
        await self.exec_as_agent(
            environment,
            command=(
                'export PATH="$HOME/.local/bin:$PATH" && '
                f"goose run --recipe {shlex.quote(CONTAINER_RECIPE_PATH)} "
                "--output-format stream-json "
                + ((cli_flags + " ") if cli_flags else "")
                + "2>&1 | stdbuf -oL tee /logs/agent/goose.txt"
            ),
            env=env,
        )
