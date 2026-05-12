import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";

interface LinkSafetyModalProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenLink?: (url: string) => Promise<void>;
  url: string;
}

export function LinkSafetyModal({
  isOpen,
  onClose,
  onOpenLink,
  url,
}: LinkSafetyModalProps) {
  const { t } = useTranslation("common");
  const [isCopied, setIsCopied] = useState(false);
  const timeoutRef = useRef<number>(0);

  useEffect(() => {
    if (isOpen) setIsCopied(false);
  }, [isOpen]);

  useEffect(
    () => () => {
      window.clearTimeout(timeoutRef.current);
    },
    [],
  );

  const handleOpen = useCallback(async () => {
    try {
      await (onOpenLink ?? openUrl)(url);
    } catch (e: unknown) {
      console.error("[linkSafety] openUrl failed:", e);
    }
    onClose();
  }, [url, onClose, onOpenLink]);

  const handleCopy = useCallback(() => {
    if (isCopied) return;
    navigator.clipboard
      .writeText(url)
      .then(() => {
        setIsCopied(true);
        timeoutRef.current = window.setTimeout(() => setIsCopied(false), 2000);
      })
      .catch((e: unknown) =>
        console.error("[linkSafety] clipboard write failed:", e),
      );
  }, [url, isCopied]);

  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open) onClose();
    },
    [onClose],
  );

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("components.linkSafety.title")}</DialogTitle>
          <DialogDescription>
            {t("components.linkSafety.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="break-all rounded-md bg-muted p-3 font-mono text-sm">
          {url}
        </div>
        <DialogFooter className="flex-row">
          <Button
            className="flex-1"
            onClick={handleCopy}
            type="button"
            variant="outline"
          >
            {isCopied
              ? t("components.linkSafety.copied")
              : t("components.linkSafety.copyLink")}
          </Button>
          <Button className="flex-1" onClick={handleOpen} type="button">
            {t("components.linkSafety.openLink")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
