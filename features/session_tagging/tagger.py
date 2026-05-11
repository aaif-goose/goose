#!/usr/bin/env python3
"""
tagger.py — Goose Session Auto-Tagger
======================================
Reads Goose session messages from the SQLite DB, sends them to the configured
LLM, and writes category tags back to the session_tags table.

Works with any LLM provider Goose is configured for (Tetrate, Anthropic,
OpenAI, Google). Zero extra configuration needed when Goose is already set up.

Usage:
    python tagger.py                         # auto-tag all untagged sessions
    python tagger.py --force                 # re-tag all sessions
    python tagger.py --session-id 20260329_1 # tag a specific session
    python tagger.py --list                  # list all tagged sessions
    python tagger.py --provider anthropic --model claude-haiku-4-5
"""

import argparse
import json
import sys

from categories import PREDEFINED_TAGS
from config import TETRATE_BASE_URL, get_api_key, read_goose_config, resolve_db_path
from db import (
    add_tags,
    ensure_tags_table,
    get_all_sessions,
    get_messages_for_session,
    get_tags_for_session,
    get_untagged_sessions,
    remove_tag,
)

# ─── Optional LLM imports ─────────────────────────────────────────────────────

try:
    import anthropic
    HAS_ANTHROPIC = True
except ImportError:
    HAS_ANTHROPIC = False

try:
    import openai
    HAS_OPENAI = True
except ImportError:
    HAS_OPENAI = False

try:
    from google import genai as genai_client
    HAS_GOOGLE = True
except ImportError:
    HAS_GOOGLE = False


# ─── Transcript helpers (same as session_summarizer.py) ───────────────────────

def extract_text_from_content(content_json: str) -> str:
    try:
        blocks = json.loads(content_json)
    except (json.JSONDecodeError, TypeError):
        return str(content_json)

    parts = []
    for block in blocks:
        btype = block.get("type", "")
        if btype == "text":
            text = block.get("text", "").strip()
            if text:
                parts.append(text)
        elif btype == "toolRequest":
            tool_call = block.get("toolCall", {})
            value     = tool_call.get("value", {})
            name      = value.get("name", "unknown_tool")
            parts.append(f"[TOOL: {name}]")
    return "\n".join(parts)


def build_short_transcript(messages: list[dict], max_chars: int = 3000) -> str:
    lines = []
    for msg in messages:
        role = msg["role"].upper()
        text = extract_text_from_content(msg.get("content_json", ""))
        if text.strip():
            lines.append(f"[{role}] {text[:500]}")
    full = "\n".join(lines)
    return full[:max_chars]


# ─── LLM tag extraction ────────────────────────────────────────────────────────

TAG_PROMPT = """\
You are categorizing a session from the Goose AI coding agent.
Given the transcript below, return 2-3 tags from EXACTLY this list:

{tags}

Return ONLY a JSON array of strings. Example: ["python", "debugging", "api"]
No explanation, no extra text.

TRANSCRIPT:
{transcript}
"""


def parse_tags_from_response(text: str, strict: bool = False) -> list[str]:
    text = text.strip()
    # Try to extract JSON array from the response
    start = text.find("[")
    end   = text.rfind("]")
    if start == -1 or end == -1:
        return []
    try:
        tags = json.loads(text[start:end + 1])
    except json.JSONDecodeError:
        return []
    result = [t.lower().strip() for t in tags if isinstance(t, str)]
    if strict:
        result = [t for t in result if t in PREDEFINED_TAGS]
    return result[:5]  # cap at 5 tags


def call_llm_for_tags(
    transcript: str,
    provider: str,
    model: str,
    api_key: str,
) -> list[str]:
    prompt = TAG_PROMPT.format(
        tags=", ".join(PREDEFINED_TAGS),
        transcript=transcript,
    )
    p = provider.lower()

    if "tetrate" in p:
        raw = _call_openai_compat(prompt, api_key, model, base_url=TETRATE_BASE_URL)
    elif "openai" in p or "azure" in p or "gpt" in p:
        raw = _call_openai_compat(prompt, api_key, model)
    elif "anthropic" in p or "claude" in p:
        raw = _call_anthropic(prompt, api_key, model)
    elif "google" in p or "gemini" in p or "vertex" in p:
        raw = _call_google(prompt, api_key, model)
    else:
        print(f"Warning: unknown provider '{provider}', trying OpenAI-compatible API.")
        raw = _call_openai_compat(prompt, api_key, model)

    return parse_tags_from_response(raw)


def _call_openai_compat(
    prompt: str,
    api_key: str,
    model: str,
    base_url: str | None = None,
) -> str:
    if not HAS_OPENAI:
        raise RuntimeError("openai package not installed. Run: pip install openai")
    kwargs = {"api_key": api_key}
    if base_url:
        kwargs["base_url"] = base_url
    client   = openai.OpenAI(**kwargs)
    response = client.chat.completions.create(
        model    = model or "gpt-4o-mini",
        messages = [{"role": "user", "content": prompt}],
        max_tokens = 64,
    )
    return response.choices[0].message.content


def _call_anthropic(prompt: str, api_key: str, model: str) -> str:
    if not HAS_ANTHROPIC:
        raise RuntimeError("anthropic package not installed. Run: pip install anthropic")
    client   = anthropic.Anthropic(api_key=api_key)
    response = client.messages.create(
        model      = model or "claude-haiku-4-5",
        max_tokens = 64,
        messages   = [{"role": "user", "content": prompt}],
    )
    return response.content[0].text


def _call_google(prompt: str, api_key: str, model: str) -> str:
    if not HAS_GOOGLE:
        raise RuntimeError("google-genai package not installed. Run: pip install google-genai")
    client   = genai_client.Client(api_key=api_key)
    response = client.models.generate_content(
        model    = model or "gemini-2.0-flash",
        contents = prompt,
    )
    return response.text


# ─── CLI ──────────────────────────────────────────────────────────────────────

def print_tagged_sessions(db_path: str) -> None:
    sessions = get_all_sessions(db_path)
    print(f"\n{'Session ID':<22} {'Name':<35} {'Tags'}")
    print("─" * 80)
    for s in sessions:
        tags = get_tags_for_session(db_path, s["id"])
        tag_str = ", ".join(t["tag"] for t in tags) if tags else "(none)"
        name    = (s["name"] or "Unnamed")[:34]
        print(f"{s['id']:<22} {name:<35} {tag_str}")
    print()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Auto-tag Goose sessions using an LLM.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python tagger.py                         # auto-tag all untagged sessions
  python tagger.py --force                 # re-tag all sessions (overwrite auto tags)
  python tagger.py --session-id 20260329_1 # tag a specific session
  python tagger.py --list                  # list all sessions with their tags
  python tagger.py --provider anthropic --model claude-haiku-4-5
        """,
    )
    parser.add_argument("--list",       action="store_true", help="List sessions and their tags")
    parser.add_argument("--force",      action="store_true", help="Re-tag sessions that already have auto tags")
    parser.add_argument("--session-id", help="Tag only this specific session")
    parser.add_argument("--provider",   help="Override LLM provider")
    parser.add_argument("--model",      help="Override model name")
    parser.add_argument("--api-key",    help="Override API key")
    args = parser.parse_args()

    db_path  = str(resolve_db_path())
    ensure_tags_table(db_path)

    if args.list:
        print_tagged_sessions(db_path)
        return

    config   = read_goose_config()
    provider = args.provider or config.get("GOOSE_PROVIDER") or "tetrate"
    model    = args.model    or config.get("GOOSE_MODEL")    or ""
    api_key  = args.api_key  or get_api_key(provider)

    if not api_key:
        print(
            f"\nNo API key found for provider '{provider}'.\n"
            f"Set env var, run 'goose configure', or pass --api-key.",
            file=sys.stderr,
        )
        sys.exit(1)

    if args.session_id:
        sessions_to_tag = [{"id": args.session_id, "name": ""}]
        # Remove existing auto tags if re-tagging
        for t in get_tags_for_session(db_path, args.session_id):
            if t["source"] == "auto":
                remove_tag(db_path, args.session_id, t["tag"])
    elif args.force:
        sessions_to_tag = get_all_sessions(db_path)
        for s in sessions_to_tag:
            for t in get_tags_for_session(db_path, s["id"]):
                if t["source"] == "auto":
                    remove_tag(db_path, s["id"], t["tag"])
    else:
        sessions_to_tag = get_untagged_sessions(db_path)

    if not sessions_to_tag:
        print("No sessions to tag. Use --force to re-tag existing sessions.")
        return

    print(f"Tagging {len(sessions_to_tag)} session(s) using {provider} ({model or 'default'})...\n")

    tagged   = 0
    skipped  = 0
    failed   = 0

    for session in sessions_to_tag:
        sid  = session["id"]
        name = (session.get("name") or "Unnamed")[:40]
        msgs = get_messages_for_session(db_path, sid, limit=10)

        user_msgs = [m for m in msgs if m["role"] == "user"]
        if not user_msgs:
            print(f"  SKIP  {sid:<22} {name} — no user messages")
            skipped += 1
            continue

        transcript = build_short_transcript(msgs)

        try:
            tags = call_llm_for_tags(transcript, provider, model, api_key)
        except Exception as e:
            print(f"  FAIL  {sid:<22} {name} — LLM error: {e}")
            failed += 1
            continue

        if not tags:
            # Retry with stricter prompt
            try:
                tags = call_llm_for_tags(
                    "Respond with ONLY a JSON array of tags.\n" + transcript,
                    provider, model, api_key,
                )
            except Exception:
                pass

        if tags:
            add_tags(db_path, sid, tags, source="auto")
            print(f"  OK    {sid:<22} {name} → {', '.join(tags)}")
            tagged += 1
        else:
            print(f"  SKIP  {sid:<22} {name} — could not parse tags from LLM response")
            skipped += 1

    print(f"\nDone: {tagged} tagged, {skipped} skipped, {failed} failed.")


if __name__ == "__main__":
    main()
