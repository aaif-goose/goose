from __future__ import annotations

import re
import subprocess


def sanitize_docker_compose_project_name(name: str) -> str:
    name = name.lower()
    if not re.match(r"^[a-z0-9]", name):
        name = "0" + name
    return re.sub(r"[^a-z0-9_-]", "-", name)


def find_docker_compose_main_container(session_id: str) -> str:
    project_name = sanitize_docker_compose_project_name(session_id)
    completed = subprocess.run(
        [
            "docker",
            "ps",
            "--filter",
            f"label=com.docker.compose.project={project_name}",
            "--filter",
            "label=com.docker.compose.service=main",
            "--format",
            "{{.ID}}",
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "Failed to find the Harbor task container with docker ps: "
            f"{completed.stderr.strip()}"
        )

    container_ids = [
        line.strip() for line in completed.stdout.splitlines() if line.strip()
    ]
    if len(container_ids) != 1:
        raise RuntimeError(
            "Expected exactly one Harbor main task container for session "
            f"{session_id!r}, found {len(container_ids)}."
        )
    return container_ids[0]


def docker_container_workdir(container_id: str) -> str:
    completed = subprocess.run(
        [
            "docker",
            "inspect",
            container_id,
            "--format",
            "{{.Config.WorkingDir}}",
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "Failed to inspect the Harbor task container working directory: "
            f"{completed.stderr.strip()}"
        )

    return completed.stdout.strip() or "/"
