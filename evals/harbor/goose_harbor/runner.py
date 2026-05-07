from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

HARBOR_AGENT_IMPORT_PATH = "goose_harbor.goose_binary:GooseBinaryAgent"


def harbor_dir() -> Path:
    return Path(__file__).resolve().parents[1]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run a Harbor dataset with a caller-provided Goose binary.",
    )
    parser.add_argument("--goose-binary", required=True, type=Path)
    parser.add_argument(
        "--goose-profile",
        required=True,
        type=Path,
        help=(
            "Goose profile directory to copy into the benchmark container. "
            "Accepts either a GOOSE_PATH_ROOT-style directory or a config directory "
            "containing config.yaml."
        ),
    )
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--task", action="append", default=[], dest="tasks")
    parser.add_argument("--trials", type=int, default=1)
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--max-turns", type=int)
    parser.add_argument("--jobs-dir", type=Path, default=harbor_dir() / ".runs" / "jobs")
    parser.add_argument(
        "--config-dir", type=Path, default=harbor_dir() / ".runs" / "configs"
    )
    parser.add_argument("--job-name")
    parser.add_argument("--force-build", action="store_true")
    parser.add_argument(
        "--install-goose-runtime-deps",
        action="store_true",
        help=(
            "Install minimal OS runtime dependencies required by some Goose release "
            "binaries inside Debian/Ubuntu task containers."
        ),
    )
    parser.add_argument("--dry-run", action="store_true")
    return parser


def pythonpath_with_harbor() -> str:
    existing = os.environ.get("PYTHONPATH", "")
    return f"{harbor_dir()}{os.pathsep}{existing}" if existing else str(harbor_dir())


def dataset_config(dataset_ref: str, tasks: list[str]) -> dict[str, Any]:
    name, sep, ref = dataset_ref.rpartition("@")
    dataset: dict[str, Any] = {"name": name if sep else dataset_ref}
    if sep:
        dataset["ref" if "/" in name else "version"] = ref
    if tasks:
        dataset["task_names"] = tasks
    return dataset


def package_index_env() -> dict[str, str]:
    index_url = next(
        (
            os.environ[key]
            for key in ("UV_DEFAULT_INDEX", "PIP_INDEX_URL", "UV_INDEX_URL")
            if os.environ.get(key)
        ),
        None,
    )
    if index_url is None:
        return {}
    return {
        "PIP_INDEX_URL": index_url,
        "UV_DEFAULT_INDEX": index_url,
        "UV_INDEX_URL": index_url,
    }


def default_job_name(model: str, dataset: str) -> str:
    safe_model = re.sub(r"[^A-Za-z0-9._-]+", "-", model).strip("-")
    safe_dataset = re.sub(r"[^A-Za-z0-9._-]+", "-", dataset).strip("-")
    timestamp = datetime.now().strftime("%Y-%m-%d__%H-%M-%S")
    return f"goose-{safe_dataset}-{safe_model}-{timestamp}"


def validate_job_name(job_name: str) -> str:
    if not re.match(r"^[A-Za-z0-9][A-Za-z0-9._-]*$", job_name):
        raise ValueError(
            "Job name must start with a letter or number and contain only "
            "letters, numbers, dots, underscores, and hyphens"
        )
    return job_name


def build_harbor_config(args: argparse.Namespace) -> dict[str, Any]:
    goose_binary = args.goose_binary.expanduser().resolve()
    goose_profile = args.goose_profile.expanduser().resolve()

    if "/" not in args.model:
        raise ValueError(
            "Model must be in provider/model form, for example databricks/my-model"
        )
    if args.trials < 1:
        raise ValueError("--trials must be at least 1")
    if args.concurrency < 1:
        raise ValueError("--concurrency must be at least 1")
    if not goose_binary.is_file():
        raise ValueError(
            f"--goose-binary does not exist or is not a file: {args.goose_binary}"
        )
    if not goose_profile.is_dir():
        raise ValueError(
            "--goose-profile does not exist or is not a directory: "
            f"{args.goose_profile}"
        )

    agent_kwargs: dict[str, Any] = {
        "goose_binary": str(goose_binary),
        "goose_profile": str(goose_profile),
    }
    if args.install_goose_runtime_deps:
        agent_kwargs["install_goose_runtime_deps"] = True
    if args.max_turns is not None:
        agent_kwargs["max_turns"] = args.max_turns

    index_env = package_index_env()
    job_name = (
        validate_job_name(args.job_name)
        if args.job_name
        else default_job_name(args.model, args.dataset)
    )

    return {
        "job_name": job_name,
        "jobs_dir": str(args.jobs_dir.expanduser()),
        "n_attempts": args.trials,
        "n_concurrent_trials": args.concurrency,
        "environment": {
            "type": "docker",
            "force_build": args.force_build,
            "delete": True,
            "env": index_env,
        },
        "verifier": {"env": index_env},
        "agents": [
            {
                "import_path": HARBOR_AGENT_IMPORT_PATH,
                "model_name": args.model,
                "kwargs": agent_kwargs,
            }
        ],
        "datasets": [dataset_config(args.dataset, args.tasks)],
    }


def run_harbor(command: list[str]) -> int:
    env = os.environ.copy()
    env["PYTHONPATH"] = pythonpath_with_harbor()
    completed = subprocess.run(command, env=env, check=False)
    return completed.returncode


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        config = build_harbor_config(args)
        config_dir = args.config_dir.expanduser()
        config_dir.mkdir(parents=True, exist_ok=True)
        config_path = config_dir / f"{config['job_name']}.json"
        config_path.write_text(json.dumps(config, indent=2) + "\n")
        command = ["harbor", "run", "-c", str(config_path)]
    except Exception as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(f"Wrote Harbor config: {config_path}")
    print(f"Jobs directory: {config['jobs_dir']}")
    print(f"PYTHONPATH: {pythonpath_with_harbor()}")
    print(f"Command: {' '.join(command)}")

    if args.dry_run:
        return 0

    try:
        return run_harbor(command)
    except FileNotFoundError:
        print("error: `harbor` was not found on PATH", file=sys.stderr)
        return 127


if __name__ == "__main__":
    raise SystemExit(main())
