# Session Tagging

Automatically categorize Goose sessions using an LLM, and support manual tagging via the desktop UI.

## Overview

Sessions are tagged and stored in a `session_tags` table in `sessions.db`. Tags can be:
- **Auto-generated** by the `tagger.py` CLI, which reads session messages and asks an LLM to pick relevant categories
- **Manually added/removed** through the Goose desktop app

Both sources coexist in the same table — each tag row tracks its `source` (`'auto'` or `'manual'`).

## Setup

```bash
cd features/session_tagging
python -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

No extra configuration is needed if Goose is already configured — the script reads the same provider/API key that Goose uses.

## Usage

```bash
# Auto-tag all untagged sessions
python tagger.py

# Re-tag all sessions (overwrites previous auto tags)
python tagger.py --force

# Tag a specific session
python tagger.py --session-id 20260329_1

# List all sessions and their current tags
python tagger.py --list

# Override provider or model
python tagger.py --provider anthropic --model claude-haiku-4-5
python tagger.py --provider openai --model gpt-4o-mini
```

## Predefined Tags

The LLM chooses from this fixed set (defined in `categories.py`):

`python`, `javascript`, `typescript`, `rust`, `html-css`, `frontend`, `backend`, `fullstack`, `api`, `database`, `debugging`, `refactoring`, `testing`, `devops`, `deployment`, `data-analysis`, `machine-learning`, `automation`, `scripting`, `documentation`, `code-review`, `git`, `setup`, `configuration`, `web-scraping`, `game-dev`, `cli-tool`, `file-management`, `research`, `writing`, `general`

## Database Schema

```sql
CREATE TABLE session_tags (
    session_id TEXT NOT NULL,
    tag        TEXT NOT NULL,
    source     TEXT NOT NULL DEFAULT 'manual',  -- 'manual' or 'auto'
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (session_id, tag),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
```

## Files

| File | Purpose |
|------|---------|
| `tagger.py` | Main CLI — reads sessions, calls LLM, writes tags |
| `db.py` | SQLite migration + tag CRUD functions |
| `config.py` | Reads Goose config.yaml / keychain / secrets.yaml for API keys |
| `categories.py` | Predefined tag list |
| `requirements.txt` | Python dependencies |
