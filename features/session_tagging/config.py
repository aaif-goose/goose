"""Read Goose config, resolve API keys, and locate sessions.db.

Reuses the same lookup strategy as enhancements/session-summarizer/session_summarizer.py
so no extra configuration is needed when Goose is already set up.
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

try:
    import yaml
    HAS_YAML = True
except ImportError:
    HAS_YAML = False

try:
    import keyring
    HAS_KEYRING = True
except ImportError:
    HAS_KEYRING = False

CONFIG_PATH  = Path.home() / ".config" / "goose" / "config.yaml"
SECRETS_PATH = Path.home() / ".config" / "goose" / "secrets.yaml"

KEYRING_SERVICE  = "goose"
KEYRING_USERNAME = "secrets"

TETRATE_BASE_URL = "https://api.router.tetrate.ai/v1"

ENV_VAR = "GOOSE_SESSIONS_DB"


def read_goose_config() -> dict:
    if not CONFIG_PATH.exists():
        return {}
    if not HAS_YAML:
        print("Warning: pyyaml not installed. Using defaults.", file=sys.stderr)
        return {}
    with open(CONFIG_PATH) as f:
        return yaml.safe_load(f) or {}


def get_api_key(provider: str) -> str | None:
    p = provider.lower()

    env_var_map = {
        "anthropic": "ANTHROPIC_API_KEY",
        "openai":    "OPENAI_API_KEY",
        "google":    "GOOGLE_API_KEY",
        "gemini":    "GOOGLE_API_KEY",
        "tetrate":   "TETRATE_API_KEY",
        "azure":     "AZURE_OPENAI_API_KEY",
        "databricks":"DATABRICKS_TOKEN",
    }
    secret_key_map = {
        "anthropic": "ANTHROPIC_API_KEY",
        "openai":    "OPENAI_API_KEY",
        "google":    "GOOGLE_API_KEY",
        "gemini":    "GOOGLE_API_KEY",
        "tetrate":   "TETRATE_API_KEY",
    }

    env_var    = env_var_map.get(p)
    secret_key = secret_key_map.get(p)

    if env_var:
        val = os.environ.get(env_var)
        if val:
            return val

    if HAS_KEYRING and secret_key:
        try:
            raw = keyring.get_password(KEYRING_SERVICE, KEYRING_USERNAME)
            if raw:
                secrets = json.loads(raw)
                if secret_key in secrets:
                    return secrets[secret_key]
        except Exception:
            pass

    if SECRETS_PATH.exists() and secret_key and HAS_YAML:
        try:
            with open(SECRETS_PATH) as f:
                file_secrets = yaml.safe_load(f) or {}
            if secret_key in file_secrets:
                return file_secrets[secret_key]
        except Exception:
            pass

    return None


def _candidate_db_paths() -> list[Path]:
    home = Path.home()
    paths: list[Path] = []

    if sys.platform.startswith("win"):
        appdata = os.environ.get("APPDATA")
        if appdata:
            paths.append(Path(appdata) / "Block" / "goose" / "data" / "sessions" / "sessions.db")
        local = os.environ.get("LOCALAPPDATA")
        if local:
            paths.append(Path(local) / "Block" / "goose" / "data" / "sessions" / "sessions.db")
    elif sys.platform == "darwin":
        paths.append(home / "Library" / "Application Support" / "Block" / "goose" / "data" / "sessions" / "sessions.db")
        paths.append(home / ".local" / "share" / "goose" / "sessions" / "sessions.db")
    else:
        xdg = os.environ.get("XDG_DATA_HOME")
        base = Path(xdg) if xdg else home / ".local" / "share"
        paths.append(base / "goose" / "sessions" / "sessions.db")

    return paths


def resolve_db_path() -> Path:
    env_val = os.environ.get(ENV_VAR)
    if env_val:
        p = Path(env_val).expanduser()
        if p.exists():
            return p

    for candidate in _candidate_db_paths():
        if candidate.exists():
            return candidate

    print(
        "Error: sessions.db not found.\n"
        f"Set the {ENV_VAR} environment variable to point to your sessions.db file,\n"
        "or make sure Goose has been run at least once.",
        file=sys.stderr,
    )
    sys.exit(1)
