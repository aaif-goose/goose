import { useState, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Camera, X } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import { savePersonaAvatar, savePersonaAvatarBytes } from "@/shared/api/agents";
import { open } from "@tauri-apps/plugin-dialog";
import type { Avatar } from "@/shared/types/agents";

const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "gif", "webp", "svg"];

interface AvatarDropZoneProps {
  personaId: string;
  avatar: Avatar | null | undefined;
  onChange: (avatar: Avatar | null) => void;
  disabled?: boolean;
}

export function AvatarDropZone({
  personaId,
  avatar,
  onChange,
  disabled = false,
}: AvatarDropZoneProps) {
  const { t } = useTranslation("agents");
  const [isDragOver, setIsDragOver] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dragDepthRef = useRef(0);

  const avatarSrc = useAvatarSrc(avatar);

  /** Save a file dropped via HTML5 drag-and-drop (File object, no path). */
  const processFile = useCallback(
    async (file: File) => {
      setError(null);

      const ext = file.name.split(".").pop()?.toLowerCase();
      if (!ext || !IMAGE_EXTENSIONS.includes(ext)) {
        setError(t("avatar.unsupportedType"));
        return;
      }

      setIsUploading(true);
      try {
        const buffer = await file.arrayBuffer();
        const bytes = Array.from(new Uint8Array(buffer));
        const filename = await savePersonaAvatarBytes(personaId, bytes, ext);

        onChange({ type: "local", value: filename });
      } catch (err) {
        console.error("Failed to save avatar:", err);
        setError(t("avatar.saveFailed"));
      } finally {
        setIsUploading(false);
      }
    },
    [personaId, onChange, t],
  );

  /** Save a file selected via the native file picker (has a path). */
  const processPath = useCallback(
    async (filePath: string) => {
      setError(null);

      const ext = filePath.split(".").pop()?.toLowerCase();
      if (!ext || !IMAGE_EXTENSIONS.includes(ext)) {
        setError(t("avatar.unsupportedType"));
        return;
      }

      setIsUploading(true);
      try {
        const filename = await savePersonaAvatar(personaId, filePath);

        onChange({ type: "local", value: filename });
      } catch (err) {
        console.error("Failed to save avatar:", err);
        setError(t("avatar.saveFailed"));
      } finally {
        setIsUploading(false);
      }
    },
    [personaId, onChange, t],
  );

  // Standard HTML5 drag-and-drop (works when dragDropEnabled is false)
  const handleDragEnter = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      if (disabled) return;

      dragDepthRef.current += 1;
      setIsDragOver(true);
    },
    [disabled],
  );

  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      if (disabled) return;

      e.dataTransfer.dropEffect = "copy";
      setIsDragOver(true);
    },
    [disabled],
  );

  const handleDragLeave = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      if (disabled) return;

      dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
      if (dragDepthRef.current === 0) {
        setIsDragOver(false);
      }
    },
    [disabled],
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      dragDepthRef.current = 0;
      setIsDragOver(false);
      if (disabled) return;

      // File drop (from OS)
      const file = e.dataTransfer.files[0];
      if (file) {
        void processFile(file);
        return;
      }

      // URL drop (from browser)
      const url =
        e.dataTransfer.getData("text/uri-list") ||
        e.dataTransfer.getData("text/plain");
      if (url && (url.startsWith("http://") || url.startsWith("https://"))) {
        setError(null);
        onChange({ type: "url", value: url });
      }
    },
    [disabled, processFile, onChange],
  );

  const handleClick = useCallback(async () => {
    if (disabled || isUploading) return;

    const selected = await open({
      title: t("avatar.chooseDialogTitle"),
      filters: [
        {
          name: t("avatar.dialogFilterName"),
          extensions: IMAGE_EXTENSIONS,
        },
      ],
      multiple: false,
    });

    if (selected) {
      processPath(selected);
    }
  }, [disabled, isUploading, processPath, t]);

  const handleClear = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      setError(null);
      onChange(null);
    },
    [onChange],
  );

  return (
    <div className="flex flex-col items-center gap-1">
      <div className="relative">
        <Button
          type="button"
          variant="ghost"
          size="icon-lg"
          tabIndex={disabled ? -1 : 0}
          aria-label={t("avatar.uploadAria")}
          onDragEnter={handleDragEnter}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
          onClick={handleClick}
          className={cn(
            "size-16 overflow-hidden border-2 bg-muted shadow-none",
            isDragOver
              ? "scale-105 border-brand bg-brand/10 shadow-none ring-4 ring-brand/20"
              : "border-border hover:border-brand/50 hover:bg-brand/10",
            disabled && "opacity-70 cursor-not-allowed",
            isUploading && "animate-pulse",
          )}
        >
          {avatarSrc ? (
            <img
              src={avatarSrc}
              alt={t("avatar.previewAlt")}
              className="h-full w-full rounded-full object-cover"
              onError={() => setError(t("avatar.loadFailed"))}
            />
          ) : (
            <div className="flex flex-col items-center justify-center text-muted-foreground">
              <Camera className="size-5" />
            </div>
          )}
        </Button>

        {/* Clear button */}
        {avatar && !disabled && (
          <Button
            type="button"
            variant="outline"
            size="icon-xs"
            aria-label={t("avatar.removeAria")}
            onClick={handleClear}
            className="absolute -top-0.5 -right-0.5 z-10 size-5 bg-background text-muted-foreground shadow-none hover:text-foreground"
          >
            <X className="size-3" />
          </Button>
        )}
      </div>

      {error && <span className="text-[10px] text-destructive">{error}</span>}
    </div>
  );
}
