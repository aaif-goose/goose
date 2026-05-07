from __future__ import annotations

import json
from pathlib import Path

import pytest

from goose_harbor import runner


@pytest.fixture(autouse=True)
def clear_package_index_env(monkeypatch: pytest.MonkeyPatch) -> None:
    for key in ("UV_DEFAULT_INDEX", "PIP_INDEX_URL", "UV_INDEX_URL"):
        monkeypatch.delenv(key, raising=False)


def test_dry_run_writes_config_without_running_harbor(tmp_path: Path) -> None:
    goose_binary = tmp_path / "goose"
    goose_binary.write_text("#!/bin/sh\n")
    goose_profile = tmp_path / "goose-profile"
    goose_profile.mkdir()
    config_dir = tmp_path / "configs"

    result = runner.main(
        [
            "--goose-binary",
            str(goose_binary),
            "--goose-profile",
            str(goose_profile),
            "--dataset",
            "terminal-bench/terminal-bench-2",
            "--model",
            "databricks/model",
            "--task",
            "terminal-bench/fix-git",
            "--install-goose-runtime-deps",
            "--config-dir",
            str(config_dir),
            "--dry-run",
        ]
    )

    assert result == 0
    config_path = next(config_dir.glob("*.json"))
    config = json.loads(config_path.read_text())
    assert config["datasets"] == [
        {
            "name": "terminal-bench/terminal-bench-2",
            "task_names": ["terminal-bench/fix-git"],
        }
    ]
    assert config["agents"][0]["kwargs"]["install_goose_runtime_deps"] is True


def test_package_dataset_suffix_uses_ref(tmp_path: Path) -> None:
    goose_binary = tmp_path / "goose"
    goose_binary.write_text("#!/bin/sh\n")
    goose_profile = tmp_path / "goose-profile"
    goose_profile.mkdir()
    config_dir = tmp_path / "configs"

    result = runner.main(
        [
            "--goose-binary",
            str(goose_binary),
            "--goose-profile",
            str(goose_profile),
            "--dataset",
            "terminal-bench/terminal-bench-2@v1",
            "--model",
            "databricks/model",
            "--config-dir",
            str(config_dir),
            "--dry-run",
        ]
    )

    assert result == 0
    config = json.loads(next(config_dir.glob("*.json")).read_text())
    assert config["datasets"] == [
        {"name": "terminal-bench/terminal-bench-2", "ref": "v1"}
    ]


def test_registry_dataset_suffix_uses_version(tmp_path: Path) -> None:
    goose_binary = tmp_path / "goose"
    goose_binary.write_text("#!/bin/sh\n")
    goose_profile = tmp_path / "goose-profile"
    goose_profile.mkdir()
    config_dir = tmp_path / "configs"

    result = runner.main(
        [
            "--goose-binary",
            str(goose_binary),
            "--goose-profile",
            str(goose_profile),
            "--dataset",
            "terminal-bench@2.0",
            "--model",
            "databricks/model",
            "--config-dir",
            str(config_dir),
            "--dry-run",
        ]
    )

    assert result == 0
    config = json.loads(next(config_dir.glob("*.json")).read_text())
    assert config["datasets"] == [{"name": "terminal-bench", "version": "2.0"}]


def test_dry_run_accepts_unexpanded_home_paths(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    home = tmp_path / "home"
    goose_binary = home / "bin" / "goose"
    goose_binary.parent.mkdir(parents=True)
    goose_binary.write_text("#!/bin/sh\n")
    goose_profile = home / "goose-profile"
    goose_profile.mkdir()
    config_dir = tmp_path / "configs"
    monkeypatch.setenv("HOME", str(home))

    result = runner.main(
        [
            "--goose-binary",
            "~/bin/goose",
            "--goose-profile",
            "~/goose-profile",
            "--dataset",
            "terminal-bench/terminal-bench-2",
            "--model",
            "databricks/model",
            "--config-dir",
            str(config_dir),
            "--dry-run",
        ]
    )

    assert result == 0
    config = json.loads(next(config_dir.glob("*.json")).read_text())
    assert config["agents"][0]["kwargs"]["goose_binary"] == str(goose_binary)
    assert config["agents"][0]["kwargs"]["goose_profile"] == str(goose_profile)
