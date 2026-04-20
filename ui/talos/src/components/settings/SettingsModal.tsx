import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "../Icon";
import type { Settings } from "../../services/settings";

interface SettingsModalProps {
  open: boolean;
  initial: Settings;
  onClose: () => void;
  onSave: (next: Settings) => Promise<void>;
}

export function SettingsModal({ open: isOpen, initial, onClose, onSave }: SettingsModalProps) {
  const [memoryDir, setMemoryDir] = useState(initial.memoryDir ?? "");
  const [projectsDir, setProjectsDir] = useState(initial.projectsDir ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setMemoryDir(initial.memoryDir ?? "");
    setProjectsDir(initial.projectsDir ?? "");
    setError(null);
  }, [initial, isOpen]);

  if (!isOpen) return null;

  const pickDirectory = async (current: string): Promise<string | null> => {
    const selected = await open({ directory: true, multiple: false, defaultPath: current || undefined });
    if (typeof selected === "string") return selected;
    return null;
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSave({
        memoryDir: memoryDir.trim() || null,
        projectsDir: projectsDir.trim() || null,
      });
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="scrim" onClick={onClose}>
      <div
        className="palette"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 520, maxWidth: "90vw", padding: 0 }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "12px 16px",
            borderBottom: "1px solid var(--color-border)",
          }}
        >
          <Icon name="settings" size={14} />
          <div style={{ flex: 1, fontWeight: 600 }}>Settings</div>
          <button className="icon-btn tight" onClick={onClose}>
            <Icon name="x" size={13} />
          </button>
        </div>

        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 18 }}>
          <PathField
            label="Memory folder"
            hint="Flat folder of notes. Each file shows in the Memory section."
            value={memoryDir}
            setValue={setMemoryDir}
            onBrowse={async () => {
              const picked = await pickDirectory(memoryDir);
              if (picked) setMemoryDir(picked);
            }}
          />
          <PathField
            label="Projects folder"
            hint="Each subfolder is a project; its files show as notes under that project."
            value={projectsDir}
            setValue={setProjectsDir}
            onBrowse={async () => {
              const picked = await pickDirectory(projectsDir);
              if (picked) setProjectsDir(picked);
            }}
          />

          {error && (
            <div style={{ color: "var(--color-danger)", fontSize: "var(--text-sm)" }}>{error}</div>
          )}

          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button className="btn ghost" onClick={onClose} disabled={saving}>
              Cancel
            </button>
            <button className="btn" onClick={handleSave} disabled={saving}>
              {saving ? "Saving\u2026" : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function PathField({
  label,
  hint,
  value,
  setValue,
  onBrowse,
}: {
  label: string;
  hint: string;
  value: string;
  setValue: (v: string) => void;
  onBrowse: () => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <label
        style={{
          fontSize: "var(--text-xs)",
          fontWeight: 600,
          color: "var(--color-text-secondary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
        }}
      >
        {label}
      </label>
      <div style={{ display: "flex", gap: 6 }}>
        <input
          style={{ flex: 1 }}
          value={value}
          placeholder="~/notes or /absolute/path"
          onChange={(e) => setValue(e.target.value)}
        />
        <button className="btn ghost" onClick={onBrowse} type="button">
          <Icon name="folder" size={13} /> Browse
        </button>
      </div>
      <div style={{ fontSize: "var(--text-xs)", color: "var(--color-text-muted)" }}>{hint}</div>
    </div>
  );
}
