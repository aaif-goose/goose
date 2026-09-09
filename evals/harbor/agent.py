"""Harbor agent that runs a caller-provided Goose binary inside the task container."""

from __future__ import annotations

import json
import os
import shlex
from pathlib import Path
from tempfile import TemporaryDirectory

import yaml

from harbor.agents.installed.base import NonZeroAgentExitCodeError, with_prompt_template
from harbor.agents.installed.goose import Goose
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext


PROVIDER_SECRETS = {
    "anthropic": ["ANTHROPIC_API_KEY"],
    "openai": ["OPENAI_API_KEY"],
    "databricks": ["DATABRICKS_HOST", "DATABRICKS_TOKEN"],
    "google": ["GOOGLE_API_KEY"],
    "gemini": ["GEMINI_API_KEY"],
    "openrouter": ["OPENROUTER_API_KEY"],
}

CONTAINER_GOOSE_PATH_ROOT = "/installed-agent/goose-profile"
CONTAINER_CONFIG_PATH = f"{CONTAINER_GOOSE_PATH_ROOT}/config/config.yaml"
CONTAINER_RECIPE_PATH = "/installed-agent/harbor-recipe.yaml"
CONTAINER_CA_BUNDLE_PATH = "/installed-agent/ca-certificates.crt"

FATAL_GOOSE_NOTIFICATIONS = ("creditsExhausted",)


class GooseBinaryAgent(Goose):
    """Run a caller-provided Goose binary in the benchmark environment.

    Differs from harbor's vanilla ``Goose``:
      * Uses a pre-built binary uploaded into the container (no curl install).
      * Generates ``config.yaml`` from ``config_template.yaml`` with a
        caller-specified set of enabled extensions.
      * Reads provider secrets from the harbor host env, not from a profile file.
    """

    def __init__(
        self,
        *args,
        goose_binary: str,
        config_yaml: str,
        extension_entries: list[dict[str, str]],
        install_goose_runtime_deps: bool = False,
        **kwargs,
    ):
        super().__init__(*args, **kwargs)
        self.goose_binary = Path(goose_binary).expanduser().resolve()
        self.config_yaml = config_yaml
        self.extension_entries = extension_entries
        self.install_goose_runtime_deps = install_goose_runtime_deps
        self.ca_bundle_env_path: str | None = None

    @staticmethod
    def name() -> str:
        return "goose-binary"

    def get_version_command(self) -> str | None:
        return "/installed-agent/goose --version"

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
        for key in PROVIDER_SECRETS.get(provider, []):
            value = os.environ.get(key)
            if value:
                env[key] = value
        if self.ca_bundle_env_path:
            env["SSL_CERT_FILE"] = self.ca_bundle_env_path
        return env

    def _host_ca_bundle(self) -> Path:
        for env_var in ("SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"):
            value = os.environ.get(env_var)
            if value and Path(value).expanduser().is_file():
                return Path(value).expanduser().resolve()
        for path in (
            Path("/etc/ssl/certs/ca-certificates.crt"),
            Path("/etc/ssl/cert.pem"),
            Path("/opt/homebrew/etc/ca-certificates/cert.pem"),
        ):
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

    async def _agent_uid_gid(self, environment: BaseEnvironment) -> tuple[str, str]:
        result = await self.exec_as_agent(environment, command="id -u && id -g", timeout_sec=10)
        ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if len(ids) < 2:
            raise RuntimeError(f"Could not determine agent uid/gid: {result.stdout!r}")
        return ids[0], ids[1]

    async def _chown_to_agent_user(
        self, environment: BaseEnvironment, path: str, *, recursive: bool = False
    ) -> None:
        uid, gid = await self._agent_uid_gid(environment)
        flag = "-R " if recursive else ""
        await self.exec_as_root(
            environment,
            command=f"chown {flag}{shlex.quote(uid)}:{shlex.quote(gid)} {shlex.quote(path)}",
        )

    async def install(self, environment: BaseEnvironment) -> None:
        if not self.goose_binary.is_file():
            raise FileNotFoundError(f"Goose binary does not exist: {self.goose_binary}")

        await environment.upload_file(self.goose_binary, "/installed-agent/goose")
        await self.exec_as_root(environment, command="chmod 755 /installed-agent/goose")
        if self.install_goose_runtime_deps:
            await self._install_goose_runtime_deps(environment)
        await self._ensure_ca_bundle(environment)

        config_dir = f"{CONTAINER_GOOSE_PATH_ROOT}/config"
        await self.exec_as_root(
            environment, command=f"mkdir -p {shlex.quote(config_dir)}"
        )
        with TemporaryDirectory() as tmp:
            config_path = Path(tmp) / "config.yaml"
            config_path.write_text(self.config_yaml)
            await environment.upload_file(config_path, CONTAINER_CONFIG_PATH)
        await self._chown_to_agent_user(environment, CONTAINER_GOOSE_PATH_ROOT, recursive=True)

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

    def _create_recipe_yaml(self, instruction: str) -> str:
        return yaml.dump(
            {
                "version": "1.0.0",
                "title": "harbor-task",
                "description": "harbor task recipe",
                "instructions": (
                    "You are given a task and you need to complete it. "
                    "You are currently executing in a docker container where you are "
                    "being evaluated on a benchmark for LLM agents. Act autonomously. "
                    "You will not receive any feedback on your progress, so you must "
                    "use your own tools to complete the task without any intervention."
                ),
                "prompt": instruction,
                "extensions": self.extension_entries,
            }
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

        with TemporaryDirectory() as tmp:
            recipe_path = Path(tmp) / "harbor-recipe.yaml"
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
        self._raise_on_fatal_goose_notification()

    def _raise_on_fatal_goose_notification(self) -> None:
        log_path = self.logs_dir / "goose.txt"
        if not log_path.is_file():
            return
        log_text = log_path.read_text(errors="replace")
        for notification in FATAL_GOOSE_NOTIFICATIONS:
            if f'"notificationType":"{notification}"' in log_text:
                raise NonZeroAgentExitCodeError(
                    f"Goose exited without running the task: {notification}. "
                    f"See {log_path} for details."
                )

    @staticmethod
    def _extract_complete_event(log_text: str) -> dict | None:
        complete_event = None
        for line in log_text.strip().split("\n"):
            line = line.strip()
            if not line or '"complete"' not in line:
                continue
            event = json.loads(line)
            if event.get("type") == "complete":
                complete_event = event
        return complete_event

    def _compute_cost_from_pricing(
        self,
        prompt_tokens: int | None,
        completion_tokens: int | None,
        cache_read_tokens: int | None,
        cache_write_tokens: int | None,
    ) -> float | None:
        if not self.model_name or not (prompt_tokens or completion_tokens):
            return None
        try:
            import litellm
        except ImportError:
            return None
        pricing = None
        for key in (self.model_name, self.model_name.split("/", 1)[-1]):
            entry = litellm.model_cost.get(key)
            if entry:
                pricing = entry
                break
        if pricing is None:
            return None
        input_cost = pricing.get("input_cost_per_token") or 0.0
        cache_read = cache_read_tokens or 0
        cache_write = cache_write_tokens or 0
        uncached = max((prompt_tokens or 0) - cache_read - cache_write, 0)
        return (
            uncached * input_cost
            + cache_read * (pricing.get("cache_read_input_token_cost") or input_cost)
            + cache_write
            * (pricing.get("cache_creation_input_token_cost") or input_cost)
            + (completion_tokens or 0) * (pricing.get("output_cost_per_token") or 0.0)
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        super().populate_context_post_run(context)
        txt_path = self.logs_dir / "goose.txt"
        if not txt_path.exists():
            return
        log_text = txt_path.read_text()
        complete_event = self._extract_complete_event(log_text)
        if complete_event is None:
            return

        inp = complete_event.get("input_tokens")
        out = complete_event.get("output_tokens")
        cache_read = complete_event.get("cache_read_input_tokens")
        cache_write = complete_event.get("cache_write_input_tokens")
        if inp is not None:
            context.n_input_tokens = inp
        if out is not None:
            context.n_output_tokens = out
        if cache_read is not None:
            context.n_cache_tokens = cache_read
        if cache_read is not None or cache_write is not None:
            context.metadata = {
                **(context.metadata or {}),
                "cache_read_input_tokens": cache_read or 0,
                "cache_write_input_tokens": cache_write or 0,
            }
        cost = complete_event.get("cost_usd")
        if cost is None:
            cost = self._compute_cost_from_pricing(inp, out, cache_read, cache_write)
            if cost is not None:
                context.metadata = {
                    **(context.metadata or {}),
                    "cost_source": "litellm_estimate",
                }
        if cost is not None:
            context.cost_usd = cost
