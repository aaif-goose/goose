import streamlit as st
import sqlite3
import os
import uuid
import json
from datetime import datetime

# ── Config ──────────────────────────────────────────────
DB_PATH = os.path.expanduser("~/.local/share/goose/sessions/sessions.db")

st.set_page_config(
    page_title="Goose Conversation Branching",
    page_icon="🪿",
    layout="wide"
)

# ── Custom CSS ──────────────────────────────────────────
st.markdown("""
<style>
    .main-header {
        font-size: 2rem;
        font-weight: 700;
        color: #FFFFFF;
        margin-bottom: 0.2rem;
        text-align: center;
    }
    .sub-header {
        font-size: 0.95rem;
        color: #AAAAAA;
        margin-bottom: 2rem;
        text-align: center;
    }
    .message-user {
        background: #1A3A2A;
        border-left: 4px solid #2ECC71;
        border-radius: 6px;
        padding: 10px 14px;
        margin-bottom: 8px;
        color: #FFFFFF;
    }
    .message-assistant {
        background: #1A2A3A;
        border-left: 4px solid #3498DB;
        border-radius: 6px;
        padding: 10px 14px;
        margin-bottom: 8px;
        color: #FFFFFF;
    }
    .success-box {
        background: #1A3A2A;
        border: 1px solid #2ECC71;
        border-radius: 8px;
        padding: 16px;
        margin-top: 16px;
        color: #FFFFFF;
    }
</style>
""", unsafe_allow_html=True)

# ── DB Helpers ──────────────────────────────────────────
def get_connection():
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn

@st.cache_data(ttl=30)
def load_sessions():
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, name, working_dir, provider_name,
               total_tokens, created_at, session_type
        FROM sessions
        WHERE session_type = 'user'
        ORDER BY created_at DESC
    """)
    sessions = [dict(row) for row in cursor.fetchall()]
    conn.close()
    return sessions

def load_messages(session_id):
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, message_id, role, content_json,
               created_timestamp, tokens
        FROM messages
        WHERE session_id = ?
        ORDER BY created_timestamp ASC
    """, (session_id,))
    messages = [dict(row) for row in cursor.fetchall()]
    conn.close()
    return messages

def extract_text(content_json):
    try:
        content = json.loads(content_json)
        if isinstance(content, list) and len(content) > 0:
            return content[0].get('text', '') or str(content)
        return str(content)
    except:
        return content_json

def create_branch(session_id, from_message, original_name, all_messages):
    conn = get_connection()
    cursor = conn.cursor()

    cursor.execute("SELECT * FROM sessions WHERE id = ?", (session_id,))
    original = cursor.fetchone()

    now = datetime.now()
    new_session_id = now.strftime("%Y%m%d_%H%M%S") + "_branch"
    branch_name = f"Branch of [{original_name}] from msg {from_message}"

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
        new_session_id, branch_name,
        f"Branched from session {session_id} at message {from_message}",
        True, original['session_type'], original['working_dir'],
        now.strftime("%Y-%m-%d %H:%M:%S"),
        now.strftime("%Y-%m-%d %H:%M:%S"),
        original['extension_data'],
        None, None, None, None, None, None,
        None, None, None,
        original['provider_name'], original['model_config_json'],
        original['goose_mode'], None
    ))

    messages_to_copy = all_messages[:from_message]
    for msg in messages_to_copy:
        new_message_id = str(uuid.uuid4())[:8]
        cursor.execute("""
            INSERT INTO messages (
                message_id, session_id, role,
                content_json, created_timestamp,
                timestamp, tokens, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            new_message_id, new_session_id, msg['role'],
            msg['content_json'], msg['created_timestamp'],
            msg.get('timestamp'), msg.get('tokens'),
            msg.get('metadata_json')
        ))

    conn.commit()
    conn.close()
    return new_session_id, branch_name

# ── UI ──────────────────────────────────────────────────
st.markdown('<div class="main-header">🪿 Goose Conversation Branching</div>', unsafe_allow_html=True)
st.markdown('<div class="sub-header">Fork any session at a specific message and continue independently in Goose</div>', unsafe_allow_html=True)

sessions = load_sessions()

if not sessions:
    st.warning("No sessions found in the Goose database.")
    st.stop()

col1, col2 = st.columns([1, 2])

with col1:
    st.subheader("📋 Select Session")

    session_options = {
        f"{s['name'] or 'Unnamed'} ({s['id']})": s
        for s in sessions
    }

    selected_label = st.selectbox(
        "Choose a session to branch:",
        options=list(session_options.keys()),
        label_visibility="collapsed"
    )

    selected_session = session_options[selected_label]

    st.markdown("---")
    st.markdown("**Session Info**")
    st.markdown(f"**ID:** `{selected_session['id']}`")
    st.markdown(f"**Provider:** {selected_session['provider_name'] or 'N/A'}")
    st.markdown(f"**Tokens:** {selected_session['total_tokens'] or 0:,}")
    st.markdown(f"**Created:** {selected_session['created_at']}")
    st.markdown(f"**Working Dir:** `{selected_session['working_dir']}`")

with col2:
    st.subheader("💬 Conversation Timeline")

    messages = load_messages(selected_session['id'])

    if not messages:
        st.info("No messages found in this session.")
    else:
        st.markdown(f"*{len(messages)} messages — click Branch after any message*")
        st.markdown("")

        for i, msg in enumerate(messages, start=1):
            text = extract_text(msg['content_json'])
            role = msg['role'].upper()
            css_class = "message-user" if msg['role'] == 'user' else "message-assistant"
            icon = "👤" if msg['role'] == 'user' else "🪿"

            st.markdown(
                f'<div class="{css_class}">'
                f'<strong>{icon} [{i}] {role}</strong><br>'
                f'{text[:300]}{"..." if len(text) > 300 else ""}'
                f'</div>',
                unsafe_allow_html=True
            )

            if st.button(
                f"🔀 Branch from message {i}",
                key=f"branch_{i}",
                use_container_width=True
            ):
                with st.spinner("Creating branch..."):
                    new_id, branch_name = create_branch(
                        selected_session['id'],
                        i,
                        selected_session['name'] or selected_session['id'],
                        messages
                    )

                st.success("✅ Branch created successfully!")
                st.markdown(
                    f'<div class="success-box">'
                    f'<strong>New Session ID:</strong> {new_id}<br>'
                    f'<strong>Branch Name:</strong> {branch_name}<br>'
                    f'<strong>Messages Copied:</strong> {i} of {len(messages)}<br><br>'
                    f'🪿 Open Goose to see the branched session in the sidebar!'
                    f'</div>',
                    unsafe_allow_html=True
                )
                st.cache_data.clear()