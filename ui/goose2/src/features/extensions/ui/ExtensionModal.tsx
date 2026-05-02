import { useState } from "react";
import { useTranslation } from "react-i18next";
import { IconPlus, IconTrash } from "@tabler/icons-react";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Button } from "@/shared/ui/button";
import { ConfirmDialog } from "@/shared/ui/confirm-dialog";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { Textarea } from "@/shared/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import {
  useExtensionModalForm,
  type ExtensionModalType,
} from "../hooks/useExtensionModalForm";
import type { ExtensionConfig, ExtensionEntry } from "../types";

interface ExtensionModalProps {
  extension?: ExtensionEntry;
  onSubmit: (name: string, config: ExtensionConfig) => Promise<void>;
  onDelete?: (configKey: string) => Promise<void>;
  onClose: () => void;
}

export function ExtensionModal({
  extension,
  onSubmit,
  onDelete,
  onClose,
}: ExtensionModalProps) {
  const { t } = useTranslation("settings");
  const isEdit = !!extension;
  const [isSaving, setIsSaving] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const form = useExtensionModalForm(extension);

  const handleSubmit = async () => {
    if (!form.canSubmit || isSaving) return;
    setIsSaving(true);

    try {
      const payload = form.buildSubmitPayload();
      if (!payload) return;
      await onSubmit(payload.name, payload.config);
    } finally {
      setIsSaving(false);
    }
  };

  const handleConfirmDelete = async () => {
    if (!extension || !onDelete || isDeleting) return;

    setIsDeleting(true);
    try {
      await onDelete(extension.config_key);
      setIsDeleteDialogOpen(false);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <>
      <Dialog open onOpenChange={(open) => !open && onClose()}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>
              {isEdit
                ? t("extensions.editExtension")
                : t("extensions.addExtension")}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="ext-name">{t("extensions.fields.name")}</Label>
              <Input
                id="ext-name"
                value={form.name}
                onChange={(e) => form.setName(e.target.value)}
                placeholder={t("extensions.fields.namePlaceholder")}
              />
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="ext-type">{t("extensions.fields.type")}</Label>
              <Select
                value={form.type}
                onValueChange={(value) =>
                  form.setType(value as ExtensionModalType)
                }
              >
                <SelectTrigger id="ext-type" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="stdio">
                    {t("extensions.types.stdio")}
                  </SelectItem>
                  <SelectItem value="streamable_http">
                    {t("extensions.types.streamable_http")}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="ext-desc">
                {t("extensions.fields.description")}
              </Label>
              <Input
                id="ext-desc"
                value={form.description}
                onChange={(e) => form.setDescription(e.target.value)}
                placeholder={t("extensions.fields.descriptionPlaceholder")}
              />
            </div>

            {form.type === "stdio" && (
              <>
                <div className="space-y-1.5">
                  <Label htmlFor="ext-cmd">
                    {t("extensions.fields.command")}
                  </Label>
                  <Input
                    id="ext-cmd"
                    value={form.cmd}
                    onChange={(e) => form.setCmd(e.target.value)}
                    placeholder={t("extensions.fields.commandPlaceholder")}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="ext-args">
                    {t("extensions.fields.arguments")}
                  </Label>
                  <Textarea
                    id="ext-args"
                    value={form.args}
                    onChange={(e) => form.setArgs(e.target.value)}
                    placeholder={t("extensions.fields.argumentsPlaceholder")}
                    rows={3}
                  />
                </div>
              </>
            )}

            {form.type === "streamable_http" && (
              <div className="space-y-1.5">
                <Label htmlFor="ext-uri">{t("extensions.fields.url")}</Label>
                <Input
                  id="ext-uri"
                  value={form.uri}
                  onChange={(e) => form.setUri(e.target.value)}
                  placeholder={t("extensions.fields.urlPlaceholder")}
                />
              </div>
            )}

            <div className="space-y-1.5">
              <Label htmlFor="ext-timeout">
                {t("extensions.fields.timeout")}
              </Label>
              <Input
                id="ext-timeout"
                type="number"
                value={form.timeout}
                onChange={(e) => form.setTimeout(e.target.value)}
                min={1}
              />
            </div>

            <div className="space-y-1.5">
              <Label>{t("extensions.fields.envVars")}</Label>
              <div className="space-y-2">
                {form.envVars.map((env, i) => (
                  <div key={env.id} className="flex items-center gap-2">
                    <Input
                      value={env.key}
                      onChange={(e) =>
                        form.updateEnvVar(i, "key", e.target.value)
                      }
                      placeholder={t("extensions.fields.envKeyPlaceholder")}
                      className="flex-1"
                    />
                    <Input
                      value={env.value}
                      onChange={(e) =>
                        form.updateEnvVar(i, "value", e.target.value)
                      }
                      placeholder={t("extensions.fields.envValuePlaceholder")}
                      className="flex-1"
                    />
                    <Button
                      variant="ghost"
                      size="icon-xs"
                      onClick={() => form.removeEnvVar(env.id)}
                      className="shrink-0 hover:text-destructive"
                      aria-label={t("extensions.fields.removeEnvVar")}
                    >
                      <IconTrash className="size-3.5" />
                    </Button>
                  </div>
                ))}
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={form.addEnvVar}
                >
                  <IconPlus className="size-3.5" />
                  {t("extensions.fields.addEnvVar")}
                </Button>
              </div>
            </div>
          </div>

          <DialogFooter>
            {isEdit && onDelete && (
              <Button
                type="button"
                variant="ghost"
                onClick={() => setIsDeleteDialogOpen(true)}
                disabled={isSaving || isDeleting}
                className="mr-auto text-destructive hover:text-destructive hover:bg-destructive/10"
              >
                <IconTrash className="size-4" />
                {t("extensions.deleteExtension")}
              </Button>
            )}
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={isSaving}
            >
              {t("extensions.cancel")}
            </Button>
            <Button
              type="button"
              onClick={handleSubmit}
              disabled={!form.canSubmit || isSaving}
            >
              {t("extensions.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {isEdit && onDelete && (
        <ConfirmDialog
          open={isDeleteDialogOpen}
          onOpenChange={setIsDeleteDialogOpen}
          title={t("extensions.deleteConfirmation.title", { name: form.name })}
          description={t("extensions.deleteConfirmation.description")}
          cancelLabel={t("extensions.cancel")}
          confirmLabel={t("extensions.deleteConfirmation.confirm")}
          loadingLabel={t("extensions.deleteConfirmation.deleting")}
          isLoading={isDeleting}
          overlayClassName="z-[70]"
          positionerClassName="z-[71]"
          onConfirm={handleConfirmDelete}
          onConfirmError={() => undefined}
        />
      )}
    </>
  );
}
