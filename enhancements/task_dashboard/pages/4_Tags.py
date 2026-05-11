"""Tags overview: tag cloud, top-tags bar chart, and per-tag session breakdown."""
from __future__ import annotations

import pandas as pd
import streamlit as st

from config import resolve_db_path
from db import load_all_tags, load_sessions, mtime_for

st.set_page_config(page_title="Tags — Goose", page_icon="🏷️", layout="wide")
st.title("🏷️ Session Tags")

db_path, _ = resolve_db_path()
mtime = mtime_for(db_path)

tags_df = load_all_tags(str(db_path), mtime)

if tags_df.empty:
    st.info(
        "No tags found yet.  "
        "Run `python features/session_tagging/tagger.py` to auto-tag sessions, "
        "or add tags manually in the Goose desktop app."
    )
    st.stop()

sessions_df = load_sessions(str(db_path), mtime)

# ---------- Tag counts ----------
tag_counts = (
    tags_df.groupby("tag")
    .agg(count=("session_id", "nunique"), sources=("source", lambda x: ", ".join(sorted(x.unique()))))
    .reset_index()
    .sort_values("count", ascending=False)
)

# ---------- Summary metrics ----------
col1, col2, col3 = st.columns(3)
col1.metric("Unique tags", len(tag_counts))
col2.metric("Tagged sessions", tags_df["session_id"].nunique())
col3.metric("Total tag assignments", len(tags_df))

st.divider()

# ---------- Top-tags bar chart ----------
st.subheader("Top Tags")
top_n = st.slider("Show top N tags", min_value=5, max_value=min(50, len(tag_counts)), value=min(20, len(tag_counts)))
chart_data = tag_counts.head(top_n).set_index("tag")["count"]
st.bar_chart(chart_data)

# ---------- Tag table ----------
st.subheader("All Tags")
st.dataframe(
    tag_counts,
    use_container_width=True,
    hide_index=True,
    column_config={
        "tag":     st.column_config.TextColumn("Tag"),
        "count":   st.column_config.NumberColumn("Sessions", format="%d"),
        "sources": st.column_config.TextColumn("Sources"),
    },
)

st.divider()

# ---------- Sessions by tag ----------
st.subheader("Sessions for a Tag")
selected_tag = st.selectbox("Select tag", options=tag_counts["tag"].tolist())

if selected_tag:
    session_ids = tags_df[tags_df["tag"] == selected_tag]["session_id"].tolist()
    matched = sessions_df[sessions_df["id"].isin(session_ids)].copy()

    st.caption(f"**{len(matched)}** session(s) tagged with `{selected_tag}`")

    display_cols = ["id", "name", "session_type", "working_dir", "created_at", "message_count", "accumulated_total_tokens"]
    display_cols = [c for c in display_cols if c in matched.columns]

    event = st.dataframe(
        matched[display_cols].rename(columns={"accumulated_total_tokens": "tokens", "message_count": "msgs"}),
        use_container_width=True,
        hide_index=True,
        on_select="rerun",
        selection_mode="single-row",
        column_config={
            "created_at": st.column_config.DatetimeColumn("created", format="YYYY-MM-DD HH:mm"),
            "tokens": st.column_config.NumberColumn("tokens", format="%d"),
        },
    )

    selected_rows = event.selection.rows if event.selection else []
    if selected_rows:
        chosen_id = matched.iloc[selected_rows[0]]["id"]
        st.session_state["selected_session_id"] = chosen_id
        st.success(f"Selected session `{chosen_id}`")
        if st.button("Open Session Detail →", type="primary"):
            st.switch_page("pages/2_Session_Detail.py")
