from __future__ import annotations

import asyncio
import contextlib
import json
import os
import signal
import shlex
import subprocess
from collections import deque
from pathlib import Path

from harbor.agents.installed.base import NonZeroAgentExitCodeError, with_prompt_template
from harbor.agents.installed.goose import Goose
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

from goose_terminal_bench.docker_util import (
    docker_container_workdir,
    find_docker_compose_main_container,
)


class GooseExternalAgent(Goose):
    """Run host Goose against a Harbor Docker task container."""

    def __init__(
        self,
        *args,
        goose_command: str | None = None,
        goose_binary: str | None = None,
        goose_cwd: str | None = None,
        workdir: str | None = None,
        **kwargs,
    ):
        if goose_command and goose_binary:
            raise ValueError("Set either goose_command or goose_binary, not both")
        super().__init__(*args, **kwargs)
        self.goose_command = goose_command
        self.goose_binary = goose_binary
        self.goose_cwd = goose_cwd
        self.workdir = workdir

    @staticmethod
    def name() -> str:
        return "goose-external"

    def get_version_command(self) -> str | None:
        return None

    async def install(self, environment: BaseEnvironment) -> None:
        return None

    def _create_recipe_yaml(self, instruction: str, workdir: str) -> str:
        return json.dumps(
            {
                "version": "1.0.0",
                "title": "harbor-task",
                "description": "Harbor task recipe for external Goose benchmark runs",
                "instructions": (
                    "You are given a Terminal Bench task and need to complete it "
                    "autonomously. Use the developer tools to inspect and change the "
                    f"task container. Relative paths resolve under {workdir}, and "
                    "absolute paths refer to paths inside the task container."
                ),
                "prompt": instruction,
            },
            indent=2,
        )

    def _goose_base_command(self) -> list[str]:
        if self.goose_binary:
            return [self.goose_binary]
        if self.goose_command:
            return shlex.split(self.goose_command)
        return ["goose"]

    def _wrapper_path(self) -> Path:
        if os.name == "nt":
            return self.logs_dir / "developer.cmd"
        return self.logs_dir / "developer"

    def _extension_command(self, wrapper_path: Path) -> str:
        value = str(wrapper_path)
        if any(ch.isspace() for ch in value):
            return f'"{value}"'
        return value

    def _create_developer_wrapper(self, *, container_id: str, workdir: str) -> Path:
        wrapper_path = self._wrapper_path()
        command = [
            *self._goose_base_command(),
            "bench",
            "tool-server",
            "--profile",
            "developer",
            "--target",
            "docker",
            "--container",
            container_id,
            "--workdir",
            workdir,
        ]
        if os.name == "nt":
            lines = ["@echo off", "setlocal"]
            if self.goose_cwd:
                lines.append(f"cd /d {subprocess.list2cmdline([self.goose_cwd])}")
                lines.append("if errorlevel 1 exit /b %errorlevel%")
            lines.append(subprocess.list2cmdline(command))
            lines.append("exit /b %errorlevel%")
        else:
            lines = ["#!/usr/bin/env sh", "set -eu"]
            if self.goose_cwd:
                lines.append(f"cd {shlex.quote(self.goose_cwd)}")
            lines.append(f"exec {shlex.join(command)}")
        wrapper_path.write_text("\n".join(lines) + "\n")
        if os.name != "nt":
            wrapper_path.chmod(0o755)
        return wrapper_path

    def _build_goose_command(self, *, wrapper_path: Path, recipe_path: Path) -> list[str]:
        if not self.model_name or "/" not in self.model_name:
            raise ValueError("Model name must be in the format provider/model_name")

        provider, model = self.model_name.split("/", 1)
        command = [
            *self._goose_base_command(),
            "run",
            "--no-profile",
            "--provider",
            provider,
            "--model",
            model,
            "--recipe",
            str(recipe_path),
            "--output-format",
            "stream-json",
            "--with-extension",
            self._extension_command(wrapper_path),
        ]
        cli_flags = self.build_cli_flags()
        if cli_flags:
            command.extend(shlex.split(cli_flags))
        return command

    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        self.logs_dir.mkdir(parents=True, exist_ok=True)
        container_id = find_docker_compose_main_container(environment.session_id)
        workdir = self.workdir or docker_container_workdir(container_id)
        recipe_path = self.logs_dir / "harbor-recipe.yaml"
        recipe_path.write_text(self._create_recipe_yaml(instruction, workdir))
        wrapper_path = self._create_developer_wrapper(
            container_id=container_id,
            workdir=workdir,
        )

        command = self._build_goose_command(
            wrapper_path=wrapper_path,
            recipe_path=recipe_path,
        )
        await self._run_host_goose(command)

    async def _run_host_goose(self, command: list[str]) -> None:
        self.logs_dir.mkdir(parents=True, exist_ok=True)
        log_path = self.logs_dir / "goose.txt"
        env = os.environ.copy()
        env["GOOSE_TELEMETRY_ENABLED"] = "false"
        env["GOOSE_TELEMETRY_OFF"] = "true"

        tail: deque[str] = deque(maxlen=40)
        process = None
        try:
            process_kwargs = {}
            if os.name == "posix":
                process_kwargs["start_new_session"] = True

            process = await asyncio.create_subprocess_exec(
                *command,
                env=env,
                cwd=self.goose_cwd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,
                **process_kwargs,
            )

            with log_path.open("w") as log_file:
                if process.stdout is not None:
                    while True:
                        line = await process.stdout.readline()
                        if not line:
                            break
                        text = line.decode(errors="replace")
                        tail.append(text)
                        log_file.write(text)
                        log_file.flush()

                return_code = await process.wait()
        except asyncio.CancelledError:
            await self._terminate_host_goose(process)
            raise
        except Exception:
            await self._terminate_host_goose(process)
            raise

        if return_code != 0:
            output_tail = "".join(tail).strip()
            raise NonZeroAgentExitCodeError(
                f"Host Goose exited with code {return_code}.\n{output_tail}"
            )

    async def _terminate_host_goose(
        self,
        process: asyncio.subprocess.Process | None,
    ) -> None:
        if process is None or process.returncode is not None:
            return

        if os.name == "posix":
            with contextlib.suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
        else:
            process.terminate()

        try:
            await asyncio.wait_for(process.wait(), timeout=5)
            return
        except asyncio.TimeoutError:
            pass

        if os.name == "posix":
            with contextlib.suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()

        await process.wait()
