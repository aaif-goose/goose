import sqlite3
import argparse
import os
import uuid
from datetime import datetime

# ── Config ──────────────────────────────────────────────
DB_PATH = os.path.expanduser("~/.local/share/goose/sessions/sessions.db")

def get_connection():
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn

def list_sessions():
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, name, working_dir, provider_name,
               total_tokens, created_at
        FROM sessions
        ORDER BY created_at DESC
    """)
    sessions = cursor.fetchall()
    conn.close()

    print("\n" + "="*65)
    print("  AVAILABLE SESSIONS")
    print("="*65)
    for s in sessions:
        print(f"  ID       : {s['id']}")
        print(f"  Name     : {s['name'] or 'Unnamed'}")
        print(f"  Provider : {s['provider_name'] or 'N/A'}")
        print(f"  Created  : {s['created_at']}")
        print(f"  Tokens   : {s['total_tokens'] or 0}")
        print("-"*65)
    print()
    return sessions

def get_messages(session_id):
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, message_id, role, content_json,
               created_timestamp
        FROM messages
        WHERE session_id = ?
        ORDER BY created_timestamp ASC
    """, (session_id,))
    messages = cursor.fetchall()
    conn.close()
    return messages

def list_messages(session_id):
    messages = get_messages(session_id)

    if not messages:
        print(f"\nNo messages found for session {session_id}")
        return messages

    print("\n" + "="*65)
    print(f"  MESSAGES IN SESSION: {session_id}")
    print("="*65)
    for i, msg in enumerate(messages, start=1):
        import json
        try:
            content = json.loads(msg['content_json'])
            if isinstance(content, list) and len(content) > 0:
                text = content[0].get('text', '')[:80]
            else:
                text = str(content)[:80]
        except:
            text = msg['content_json'][:80]
        print(f"  [{i}] {msg['role'].upper()}: {text}...")
    print()
    return messages

def branch_session(session_id, from_message):
    conn = get_connection()
    cursor = conn.cursor()

    # Get original session
    cursor.execute("SELECT * FROM sessions WHERE id = ?", (session_id,))
    original = cursor.fetchone()
    if not original:
        print(f"Error: Session '{session_id}' not found.")
        conn.close()
        return

    # Get all messages
    cursor.execute("""
        SELECT * FROM messages
        WHERE session_id = ?
        ORDER BY created_timestamp ASC
    """, (session_id,))
    all_messages = cursor.fetchall()

    if from_message > len(all_messages):
        print(f"Error: Session only has {len(all_messages)} messages.")
        conn.close()
        return

    messages_to_copy = all_messages[:from_message]

    # Generate new session ID
    now = datetime.now()
    new_session_id = now.strftime("%Y%m%d_%H%M%S") + "_branch"
    branch_name = f"Branch of [{original['name'] or session_id}] from msg {from_message}"

    # Insert new session
    cursor.execute("""
        INSERT INTO sessions (
            id, name, description, user_set_name,
            session_type, working_dir, created_at, updated_at,
            extension_data, total_tokens, input_tokens, output_tokens,
            accumulated_total_tokens, accumulated_input_tokens,
            accumulated_output_tokens, schedule_id, recipe_json,
            user_recipe_values_json, provider_name, model_config_json,
            goose_mode, thread_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, (
        new_session_id,
        branch_name,
        f"Branched from session {session_id} at message {from_message}",
        True,
        original['session_type'],
        original['working_dir'],
        now.strftime("%Y-%m-%d %H:%M:%S"),
        now.strftime("%Y-%m-%d %H:%M:%S"),
        original['extension_data'],
        None, None, None, None, None, None,
        None, None, None,
        original['provider_name'],
        original['model_config_json'],
        original['goose_mode'],
        None
    ))

    # Copy messages
    for msg in messages_to_copy:
        new_message_id = str(uuid.uuid4())[:8]
        cursor.execute("""
            INSERT INTO messages (
                message_id, session_id, role,
                content_json, created_timestamp,
                timestamp, tokens, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            new_message_id,
            new_session_id,
            msg['role'],
            msg['content_json'],
            msg['created_timestamp'],
            msg['timestamp'],
            msg['tokens'],
            msg['metadata_json']
        ))

    conn.commit()
    conn.close()

    print("\n" + "="*65)
    print("  BRANCH CREATED SUCCESSFULLY")
    print("="*65)
    print(f"  Original Session : {session_id}")
    print(f"  New Session ID   : {new_session_id}")
    print(f"  Branch Name      : {branch_name}")
    print(f"  Messages Copied  : {from_message} of {len(all_messages)}")
    print(f"  Status           : Open Goose to see the new session")
    print("="*65 + "\n")
    return new_session_id

def interactive_mode():
    print("\n" + "="*65)
    print("  GOOSE CONVERSATION BRANCHING — INTERACTIVE MODE")
    print("="*65)

    # Step 1 — show sessions
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, name, working_dir, provider_name,
               total_tokens, created_at
        FROM sessions
        ORDER BY created_at DESC
    """)
    sessions = cursor.fetchall()
    conn.close()

    if not sessions:
        print("No sessions found in database.")
        return

    print("\n  Available Sessions:\n")
    for i, s in enumerate(sessions, start=1):
        msg_count = get_messages(s['id'])
        name = s['name'] or 'Unnamed'
        print(f"  [{i}] {name} ({s['id']}) — {len(msg_count)} messages")

    # Step 2 — pick session
    print()
    while True:
        try:
            choice = int(input("  Select a session (enter number): "))
            if 1 <= choice <= len(sessions):
                selected_session = sessions[choice - 1]
                break
            else:
                print(f"  Please enter a number between 1 and {len(sessions)}")
        except ValueError:
            print("  Please enter a valid number")

    # Step 3 — show messages
    session_id = selected_session['id']
    session_name = selected_session['name'] or session_id
    messages = get_messages(session_id)

    if not messages:
        print(f"\n  No messages found in session '{session_name}'")
        return

    import json
    print(f"\n  Messages in \"{session_name}\":\n")
    for i, msg in enumerate(messages, start=1):
        try:
            content = json.loads(msg['content_json'])
            if isinstance(content, list) and len(content) > 0:
                text = content[0].get('text', '')[:70]
            else:
                text = str(content)[:70]
        except:
            text = msg['content_json'][:70]
        print(f"  [{i}] {msg['role'].upper()}: {text}...")

    # Step 4 — pick message
    print()
    while True:
        try:
            msg_choice = int(input(f"  Branch from which message? (1-{len(messages)}): "))
            if 1 <= msg_choice <= len(messages):
                break
            else:
                print(f"  Please enter a number between 1 and {len(messages)}")
        except ValueError:
            print("  Please enter a valid number")

    # Step 5 — confirm
    print(f"\n  You are about to branch \"{session_name}\"")
    print(f"  Copying messages 1 through {msg_choice} into a new session.")
    confirm = input("\n  Confirm? (yes/no): ").strip().lower()

    if confirm not in ["yes", "y"]:
        print("\n  Branching cancelled.\n")
        return

    # Step 6 — create branch
    new_id = branch_session(session_id, msg_choice)
    if new_id:
        print(f"  Open Goose and look for:")
        print(f"  \"Branch of [{session_name}] from msg {msg_choice}\"")
        print(f"  in the sidebar.\n")

def main():
    parser = argparse.ArgumentParser(
        description="Goose Conversation Branching Tool"
    )
    parser.add_argument(
        "--interactive",
        action="store_true",
        help="Launch interactive mode — guided step by step branching"
    )
    parser.add_argument(
        "--list-sessions",
        action="store_true",
        help="List all available sessions"
    )
    parser.add_argument(
        "--list-messages",
        type=str,
        metavar="SESSION_ID",
        help="List all messages in a session"
    )
    parser.add_argument(
        "--session",
        type=str,
        help="Session ID to branch from"
    )
    parser.add_argument(
        "--from-message",
        type=int,
        help="Message index to branch from (inclusive)"
    )

    args = parser.parse_args()

    if args.interactive:
        interactive_mode()
    elif args.list_sessions:
        list_sessions()
    elif args.list_messages:
        list_messages(args.list_messages)
    elif args.session and args.from_message:
        branch_session(args.session, args.from_message)
    else:
        print("\n  Tip: Run with --interactive for guided mode")
        print("  Example: python3 branch_session.py --interactive\n")
        parser.print_help()

if __name__ == "__main__":
    main()