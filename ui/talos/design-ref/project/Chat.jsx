// Chat.jsx — center panel: tabs, empty state, message list, composer, footer ribbon.

const { useState: useStC, useRef: useRefC, useEffect: useEffectC } = React;

function TabBar({ tabs, activeTab, onActivate, onClose, onNew }) {
  return (
    <div className="tab-bar">
      <div className="tab-strip">
        {tabs.map(t => (
          <div
            key={t.id}
            className={'tab ' + (activeTab === t.id ? 'active' : '')}
            onClick={() => onActivate(t.id)}
            onAuxClick={e => { if (e.button === 1) { e.preventDefault(); onClose(t.id); } }}
          >
            <Icon name="message-square" size={12} className="tab-icon"/>
            <span className="tab-title">{t.title}</span>
            <button className="tab-close" onClick={e => { e.stopPropagation(); onClose(t.id); }}>
              <Icon name="x" size={11}/>
            </button>
          </div>
        ))}
        <button className="icon-btn" onClick={onNew} title="New tab (⌘N)" style={{marginLeft: 4, alignSelf: 'center'}}>
          <Icon name="plus" size={14}/>
        </button>
      </div>
      <div className="tab-actions">
        <button className="icon-btn" title="Tab overflow"><Icon name="chevron-down" size={14}/></button>
      </div>
    </div>
  );
}

// ---- Token popover ----
function TokenPopover({ used, max, autoCompactPct, onCompact, onChangeThreshold, variant }) {
  const pct = Math.round((used / max) * 100);
  const totalDots = 60;
  const filledCount = Math.max(1, Math.round((pct / 100) * totalDots));
  const thresholdDot = Math.round((autoCompactPct / 100) * totalDots);
  const dots = Array.from({length: totalDots}, (_, i) => {
    let cls = 'dot';
    if (i === thresholdDot) cls = 'dot threshold';
    else if (i < filledCount) cls = 'dot filled';
    return <div key={i} className={cls}/>;
  });
  const fmt = n => n >= 1000 ? (n / 1000).toFixed(n >= 10000 ? 0 : 1).replace(/\.0$/, '') + 'k' : n;
  const style = variant === 'floating'
    ? { bottom: '100%', right: 0, marginBottom: 8 }
    : { bottom: 'calc(100% + 6px)', left: '50%', transform: 'translateX(-50%)' };
  return (
    <div className="popover token-popover" style={style} onClick={e => e.stopPropagation()}>
      <div className="title">Context window</div>
      <div className="sub">
        Auto compact at {autoCompactPct}%
        <button title="Edit threshold"><Icon name="pencil" size={11}/></button>
      </div>
      <div className="dot-track">{dots}</div>
      <div className="legend">
        <span className="used">{fmt(used)} tokens · {pct}%</span>
        <span>{fmt(max)}</span>
      </div>
      <div className="actions">
        <button className="btn" onClick={onCompact}>
          <Icon name="scroll" size={13}/> Compact now
        </button>
        <button className="btn ghost" onClick={() => onChangeThreshold(autoCompactPct === 80 ? 70 : 80)}>
          <Icon name="sliders" size={13}/> Adjust threshold
        </button>
      </div>
    </div>
  );
}

// ---- Model dropdown ----
function ModelMenu({ model, onPick, onClose }) {
  return (
    <div className="popover" style={{ bottom: '100%', left: 0, marginBottom: 8, minWidth: 320 }} onClick={e => e.stopPropagation()}>
      <div className="popover-header">Model</div>
      {MODELS.map(m => (
        <button key={m.id} className="menu-item" onClick={() => { onPick(m.id); onClose(); }}>
          <div style={{flex: 1}}>
            <div style={{display: 'flex', alignItems: 'center', gap: 6}}>
              <span>{m.name}</span>
              {m.badge && <span style={{fontSize: 10, color: 'var(--color-text-muted)', fontFamily: 'var(--font-mono)'}}>{m.badge}</span>}
            </div>
            <div className="desc">{m.desc} · {(m.max/1000).toFixed(0)}k ctx</div>
          </div>
          {model === m.id && <Icon name="check" size={14} className="check"/>}
        </button>
      ))}
    </div>
  );
}

// ---- MCP dropdown ----
function MCPMenu({ servers, setServers, onClose }) {
  const toggle = id => setServers(servers.map(s => s.id === id ? {...s, on: !s.on} : s));
  return (
    <div className="popover" style={{ bottom: '100%', left: 0, marginBottom: 8, minWidth: 280 }} onClick={e => e.stopPropagation()}>
      <div className="popover-header">MCP servers</div>
      {servers.map(s => (
        <div key={s.id} className="menu-item" style={{cursor: 'default'}}>
          <div style={{flex: 1}}>
            <div>{s.name}</div>
            <div className="desc">{s.desc}</div>
          </div>
          <button className={'toggle ' + (s.on ? 'on' : '')} onClick={() => toggle(s.id)}/>
        </div>
      ))}
      <div className="popover-sep"/>
      <button className="menu-item"><Icon name="plus" size={13}/> Add server…</button>
    </div>
  );
}

// ---- Context folder dropdown ----
function ContextMenu({ current, onPick, onClose }) {
  return (
    <div className="popover" style={{ bottom: '100%', left: 0, marginBottom: 8, minWidth: 260 }} onClick={e => e.stopPropagation()}>
      <div className="popover-header">Context folder</div>
      {CONTEXT_FOLDERS.map(f => (
        <button key={f.id} className="menu-item" onClick={() => { onPick(f.id); onClose(); }}>
          <Icon name={f.kind === 'default' ? 'brain' : f.kind === 'picker' ? 'folder' : 'folder-kanban'} size={14}/>
          <span style={{flex: 1}}>{f.name}</span>
          {current === f.id && <Icon name="check" size={13} className="check"/>}
        </button>
      ))}
    </div>
  );
}

// ---- Composer ----
function Composer({
  value, setValue, onSend, attachments, setAttachments,
  model, setModel, mcpServers, setMcpServers, contextFolder, setContextFolder,
  tokenPopoverVariant,
}) {
  const [open, setOpen] = useStC(null); // 'model' | 'mcp' | 'context' | 'token'
  const [autoCompactPct, setAutoCompactPct] = useStC(80);
  const ref = useRefC(null);
  const activeModel = MODELS.find(m => m.id === model) || MODELS[0];
  const ctx = CONTEXT_FOLDERS.find(f => f.id === contextFolder) || CONTEXT_FOLDERS[0];
  const activeMcpCount = mcpServers.filter(s => s.on).length;
  const usedTokens = 14820;
  const pct = Math.round((usedTokens / activeModel.max) * 100);
  const fillClass = pct > 80 ? 'danger' : pct > 60 ? 'warn' : '';

  useEffectC(() => {
    const close = () => setOpen(null);
    if (open) {
      document.addEventListener('click', close);
      return () => document.removeEventListener('click', close);
    }
  }, [open]);

  const handleKey = e => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (value.trim()) onSend();
    }
  };

  return (
    <div className="composer-wrap">
      <div className="composer">
        {attachments.length > 0 && (
          <div className="attachments">
            {attachments.map((a, i) => (
              <div key={i} className="attachment-chip">
                <Icon name="file-text" size={11}/>
                <span>{a}</span>
                <button className="remove" onClick={() => setAttachments(attachments.filter((_, j) => j !== i))}>
                  <Icon name="x" size={10}/>
                </button>
              </div>
            ))}
          </div>
        )}
        <textarea
          ref={ref}
          className="composer-input"
          placeholder="Ask anything, or paste a file…"
          value={value}
          onChange={e => setValue(e.target.value)}
          onKeyDown={handleKey}
          rows={2}
        />
        <div className="composer-footer">
          {/* Context folder */}
          <div style={{position: 'relative'}}>
            <button className={'footer-btn ' + (open === 'context' ? 'active' : '')}
                    onClick={e => { e.stopPropagation(); setOpen(open === 'context' ? null : 'context'); }}>
              <Icon name={ctx.kind === 'default' ? 'brain' : ctx.kind === 'picker' ? 'folder' : 'folder-kanban'} size={13}/>
              <span>{ctx.name}</span>
              <Icon name="chevron-down" size={11} className="chev"/>
            </button>
            {open === 'context' && <ContextMenu current={contextFolder} onPick={setContextFolder} onClose={() => setOpen(null)}/>}
          </div>
          {/* Attach */}
          <button className="footer-btn" title="Attach file"
                  onClick={() => setAttachments([...attachments, `file-${attachments.length + 1}.md`])}>
            <Icon name="paperclip" size={13}/>
          </button>
          {/* Slash command hint */}
          <button className="footer-btn" title="Slash commands (/)">
            <Icon name="slash-command" size={13}/>
          </button>

          <div className="footer-spacer"/>

          {/* Token counter */}
          <div style={{position: 'relative'}}>
            <button className={'footer-btn ' + (open === 'token' ? 'active' : '')}
                    onMouseEnter={() => setOpen('token')}
                    onClick={e => { e.stopPropagation(); setOpen(open === 'token' ? null : 'token'); }}>
              <div className="token-indicator">
                <div className="token-bar">
                  <div className={'token-bar-fill ' + fillClass} style={{width: pct + '%'}}/>
                </div>
                <span className="kbd-hint">{(usedTokens/1000).toFixed(1)}k / {(activeModel.max/1000).toFixed(0)}k</span>
              </div>
            </button>
            {open === 'token' && (
              <div onMouseLeave={() => setOpen(null)}>
                <TokenPopover used={usedTokens} max={activeModel.max}
                              autoCompactPct={autoCompactPct}
                              onChangeThreshold={setAutoCompactPct}
                              onCompact={() => setOpen(null)}
                              variant={tokenPopoverVariant}/>
              </div>
            )}
          </div>

          <div className="footer-divider"/>

          {/* Model selector */}
          <div style={{position: 'relative'}}>
            <button className={'footer-btn ' + (open === 'model' ? 'active' : '')}
                    onClick={e => { e.stopPropagation(); setOpen(open === 'model' ? null : 'model'); }}>
              <Icon name="model" size={13}/>
              <span>{activeModel.name.replace('Claude ', '')}</span>
              <Icon name="chevron-down" size={11} className="chev"/>
            </button>
            {open === 'model' && <ModelMenu model={model} onPick={setModel} onClose={() => setOpen(null)}/>}
          </div>

          {/* MCP */}
          <div style={{position: 'relative'}}>
            <button className={'footer-btn ' + (open === 'mcp' ? 'active' : '')}
                    onClick={e => { e.stopPropagation(); setOpen(open === 'mcp' ? null : 'mcp'); }}>
              <Icon name="plug" size={13}/>
              <span>MCP</span>
              <span className="mcp-badge">{activeMcpCount}</span>
              <Icon name="chevron-down" size={11} className="chev"/>
            </button>
            {open === 'mcp' && <MCPMenu servers={mcpServers} setServers={setMcpServers} onClose={() => setOpen(null)}/>}
          </div>

          {/* Bug reporter */}
          <button className="footer-btn" title="Report a bug">
            <Icon name="bug" size={13}/>
          </button>
        </div>
      </div>
    </div>
  );
}

// ---- Empty state ----
function EmptyState({ greetingLayout, composerProps, name, onPromptPick }) {
  const hour = new Date().getHours();
  const tod = hour < 5 ? 'evening' : hour < 12 ? 'morning' : hour < 18 ? 'afternoon' : 'evening';
  const chips = [
    { ico: 'message-square', label: 'Summarize the PRD draft' },
    { ico: 'file-text', label: 'Review my ACP notes' },
    { ico: 'workflow', label: 'Run weekly status digest' },
    { ico: 'pencil', label: 'Draft a Slack update' },
  ];
  const isTop = greetingLayout === 'top';
  return (
    <div className="empty-state" style={isTop ? {justifyContent: 'flex-start', paddingTop: 120} : {}}>
      <div className="greeting">
        Good {tod}, <span className="accent">{name}</span>.<br/>
        <em>What are we working on?</em>
      </div>
      <Composer {...composerProps}/>
      {greetingLayout === 'chips' && (
        <div className="prompt-chips">
          {chips.map((c, i) => (
            <button key={i} className="prompt-chip" onClick={() => onPromptPick(c.label)}>
              <Icon name={c.ico} size={12}/> {c.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ---- Messages ----
function Message({ msg, bubble }) {
  const isUser = msg.role === 'user';
  return (
    <div className={'msg ' + (bubble ? 'bubble ' : '') + (isUser ? 'user-msg' : '')}>
      {!bubble && (
        <div className={'msg-avatar ' + (isUser ? 'user' : 'assistant')}>
          {isUser ? USER.initials : <Icon name="sparkles" size={13}/>}
        </div>
      )}
      {bubble && isUser && (
        <div className={'msg-avatar user'}>{USER.initials}</div>
      )}
      {bubble && !isUser && (
        <div className={'msg-avatar assistant'}><Icon name="sparkles" size={13}/></div>
      )}
      <div className="msg-body">
        {!bubble && (
          <div className="msg-who">
            <span className="name">{isUser ? USER.firstName : 'Claude'}</span>
            {msg.model && <span>· {msg.model}</span>}
          </div>
        )}
        <div className="msg-content">
          {msg.paragraphs?.map((p, i) => <p key={i}>{p}</p>)}
          {msg.bullets && (
            <ul>
              {msg.bullets.map((b, i) => <li key={i}>{b}</li>)}
            </ul>
          )}
          {msg.followup && <p>{msg.followup}</p>}
          {msg.tool && (
            <div className="tool-use">
              <div className="tool-use-header">
                <span className="status-dot"/>
                <Icon name="zap" size={12}/>
                <span className="tool-name">{msg.tool.name}</span>
                <span style={{color: 'var(--color-text-muted)'}}>·</span>
                <span>{msg.tool.summary}</span>
              </div>
            </div>
          )}
        </div>
        {!isUser && (
          <div className="msg-actions">
            <button className="icon-btn tight" title="Copy"><Icon name="copy" size={12}/></button>
            <button className="icon-btn tight" title="Regenerate"><Icon name="rotate" size={12}/></button>
          </div>
        )}
      </div>
    </div>
  );
}

function ChatView({ messages, thinking, composerProps, bubble }) {
  const scrollRef = useRefC(null);
  useEffectC(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages.length, thinking]);
  return (
    <>
      <div className="msg-list" ref={scrollRef}>
        <div className="msg-list-inner">
          {messages.map((m, i) => <Message key={i} msg={m} bubble={bubble}/>)}
          {thinking && (
            <div className={'msg ' + (bubble ? 'bubble' : '')}>
              <div className="msg-avatar assistant"><Icon name="sparkles" size={13}/></div>
              <div className="msg-body">
                <div className="msg-who"><span className="name">Claude</span><span>· thinking…</span></div>
                <div className="msg-content" style={{color: 'var(--color-text-muted)'}}>
                  <span className="typing-dots">●●●</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
      <div style={{padding: '8px 16px 16px', borderTop: '1px solid var(--color-border-subtle)', background: 'var(--color-canvas)'}}>
        <Composer {...composerProps}/>
      </div>
    </>
  );
}

Object.assign(window, { TabBar, Composer, EmptyState, ChatView, TokenPopover });
