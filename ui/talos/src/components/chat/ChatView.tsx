import { useEffect, useRef } from "react";
import { Icon } from "../Icon";
import { USER } from "../../data";
import type { Message as MessageT } from "../../types";
import { Composer, type ComposerProps } from "./Composer";

function Message({ msg }: { msg: MessageT }) {
  const isUser = msg.role === "user";
  return (
    <div className={"msg " + (isUser ? "user-msg" : "")}>
      <div className={"msg-avatar " + (isUser ? "user" : "assistant")}>
        {isUser ? USER.initials : <Icon name="sparkles" size={13} />}
      </div>
      <div className="msg-body">
        <div className="msg-who">
          <span className="name">{isUser ? USER.firstName : "Claude"}</span>
          {msg.model && <span>· {msg.model}</span>}
        </div>
        <div className="msg-content">
          {msg.paragraphs?.map((p, i) => (
            <p key={i} style={{ whiteSpace: "pre-wrap" }}>
              {p}
              {msg.streaming && i === (msg.paragraphs?.length ?? 1) - 1 && (
                <span className="typing-dots" style={{ marginLeft: 4 }}>●</span>
              )}
            </p>
          ))}
          {msg.bullets && (
            <ul>
              {msg.bullets.map((b, i) => <li key={i}>{b}</li>)}
            </ul>
          )}
          {msg.followup && <p>{msg.followup}</p>}
          {msg.tool && (
            <div className="tool-use">
              <div className="tool-use-header">
                <span className="status-dot" />
                <Icon name="zap" size={12} />
                <span className="tool-name">{msg.tool.name}</span>
                <span style={{ color: "var(--color-text-muted)" }}>·</span>
                <span>{msg.tool.summary}</span>
              </div>
            </div>
          )}
          {msg.tools?.map((t) => (
            <div key={t.id} className="tool-use">
              <div className="tool-use-header">
                <span className="status-dot" />
                <Icon name="zap" size={12} />
                <span className="tool-name">{t.name}</span>
                <span style={{ color: "var(--color-text-muted)" }}>·</span>
                <span>{t.status === "executing" ? "running…" : t.status === "completed" ? "done" : "failed"}</span>
              </div>
            </div>
          ))}
        </div>
        {!isUser && (
          <div className="msg-actions">
            <button className="icon-btn tight" title="Copy"><Icon name="copy" size={12} /></button>
            <button className="icon-btn tight" title="Regenerate"><Icon name="rotate" size={12} /></button>
          </div>
        )}
      </div>
    </div>
  );
}

export function ChatView({
  messages,
  thinking,
  composerProps,
}: {
  messages: MessageT[];
  thinking: boolean;
  composerProps: ComposerProps;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages.length, thinking]);
  return (
    <>
      <div className="msg-list" ref={scrollRef}>
        <div className="msg-list-inner">
          {messages.map((m, i) => <Message key={i} msg={m} />)}
          {thinking && (
            <div className="msg">
              <div className="msg-avatar assistant"><Icon name="sparkles" size={13} /></div>
              <div className="msg-body">
                <div className="msg-who">
                  <span className="name">Claude</span>
                  <span>· thinking…</span>
                </div>
                <div className="msg-content" style={{ color: "var(--color-text-muted)" }}>
                  <span className="typing-dots">●●●</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
      <div
        style={{
          padding: "8px 16px 16px",
          borderTop: "1px solid var(--color-border-subtle)",
          background: "var(--color-canvas)",
        }}
      >
        <Composer {...composerProps} />
      </div>
    </>
  );
}
