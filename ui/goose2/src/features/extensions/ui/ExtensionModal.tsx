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
import type { ExtensionConfig, ExtensionEntry } from "../types";

type ExtensionType = "stdio" | "streamable_http";

interface ExtensionModalProps {
  extension?: ExtensionEntry;
  onSubmit: (
    name: string,
    config: ExtensionConfig,
    enabled: boolean,
  ) => Promise<void>;
  onDelete?: (configKey: string) => Promise<void>;
  onClose: () => void;
}

interface EnvVar {
  id: number;
  key: string;
  value: string;
}

let nextEnvId = 0;

function parseEnvVars(envs?: Record<string, string>): EnvVar[] {
  if (!envs || Object.keys(envs).length === 0)
    return [{ id: nextEnvId++, key: "", value: "" }];
  return Object.entries(envs).map(([key, value]) => ({
    id: nextEnvId++,
    key,
    value,
  }));
}

function buildEnvVars(vars: EnvVar[]): Record<string, string> {
  const result: Record<string, string> = {};
  for (const v of vars) {
    if (v.key.trim()) {
      result[v.key.trim()] = v.value;
    }
  }
  return result;
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

  const [name, setName] = useState(extension?.name ?? "");
  const [type, setType] = useState<ExtensionType>(
    extension?.type === "streamable_http" || extension?.type === "sse"
      ? "streamable_http"
      : "stdio",
  );
  const [description, setDescription] = useState(extension?.description ?? "");
  const [cmd, setCmd] = useState(
    extension?.type === "stdio" ? extension.cmd : "",
  );
  const [args, setArgs] = useState(
    extension?.type === "stdio" ? extension.args.join("\n") : "",
  );
  const [uri, setUri] = useState(
    extension?.type === "streamable_http"
      ? extension.uri
      : extension?.type === "sse"
        ? (extension.uri ?? "")
        : "",
  );
  const [timeout, setTimeout] = useState(
    String(
      extension?.type === "stdio" || extension?.type === "streamable_http"
        ? (extension.timeout ?? 300)
        : 300,
    ),
  );
  const [envVars, setEnvVars] = useState<EnvVar[]>(() => {
    if (extension?.type === "stdio") return parseEnvVars(extension.envs);
    if (extension?.type === "streamable_http")
      return parseEnvVars(extension.envs);
    return [{ id: nextEnvId++, key: "", value: "" }];
  });

  const canSubmit =
    name.trim().length > 0 &&
    (type === "stdio" ? cmd.trim().length > 0 : uri.trim().length > 0);

  const handleSubmit = async () => {
    if (!canSubmit || isSaving) return;
    setIsSaving(true);

    try {
      const trimmedName = name.trim();
      const envs = buildEnvVars(envVars);
      const timeoutNum = Number.parseInt(timeout, 10) || 300;

      let config: ExtensionConfig;

      if (type === "stdio") {
        config = {
          ...(extension?.type === "stdio" ? extension : {}),
          type: "stdio",
          name: trimmedName,
          description,
          cmd: cmd.trim(),
          args: args
            .split("\n")
            .map((a) => a.trim())
            .filter(Boolean),
          envs,
          timeout: timeoutNum,
        };
      } else {
        if (!uri.trim()) return;
        config = {
          ...(extension?.type === "streamable_http" ? extension : {}),
          type: "streamable_http",
          name: trimmedName,
          description,
          uri: uri.trim(),
          envs,
          timeout: timeoutNum,
        };
      }

      await onSubmit(trimmedName, config, extension?.enabled ?? true);
    } finally {
      setIsSaving(false);
    }
  };

  const updateEnvVar = (index: number, field: "key" | "value", val: string) => {
    setEnvVars((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], [field]: val };
      return next;
    });
  };

  const addEnvVar = () => {
    setEnvVars((prev) => [...prev, { id: nextEnvId++, key: "", value: "" }]);
  };

  const removeEnvVar = (id: number) => {
    setEnvVars((prev) => {
      if (prev.length <= 1) return [{ id: nextEnvId++, key: "", value: "" }];
      return prev.filter((v) => v.id !== id);
    });
  };

  return (
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
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("extensions.fields.namePlaceholder")}
            />
          </div>

          <div className="space-y-1.5">
            <Label htmlFor="ext-type">{t("extensions.fields.type")}</Label>
            <Select
              value={type}
              onValueChange={(v) => setType(v as ExtensionType)}
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
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={t("extensions.fields.descriptionPlaceholder")}
            />
          </div>

          {type === "stdio" && (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="ext-cmd">
                  {t("extensions.fields.command")}
                </Label>
                <Input
                  id="ext-cmd"
                  value={cmd}
                  onChange={(e) => setCmd(e.target.value)}
                  placeholder={t("extensions.fields.commandPlaceholder")}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="ext-args">
                  {t("extensions.fields.arguments")}
                </Label>
                <Textarea
                  id="ext-args"
                  value={args}
                  onChange={(e) => setArgs(e.target.value)}
                  placeholder={t("extensions.fields.argumentsPlaceholder")}
                  rows={3}
                />
              </div>
            </>
          )}

          {type === "streamable_http" && (
            <div className="space-y-1.5">
              <Label htmlFor="ext-uri">{t("extensions.fields.url")}</Label>
              <Input
                id="ext-uri"
                value={uri}
                onChange={(e) => setUri(e.target.value)}
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
              value={timeout}
              onChange={(e) => setTimeout(e.target.value)}
              min={1}
            />
          </div>

          <div className="space-y-1.5">
            <Label>{t("extensions.fields.envVars")}</Label>
            <div className="space-y-2">
              {envVars.map((env, i) => (
                <div key={env.id} className="flex items-center gap-2">
                  <Input
                    value={env.key}
                    onChange={(e) => updateEnvVar(i, "key", e.target.value)}
                    placeholder={t("extensions.fields.envKeyPlaceholder")}
                    className="flex-1"
                  />
                  <Input
                    value={env.value}
                    onChange={(e) => updateEnvVar(i, "value", e.target.value)}
                    placeholder={t("extensions.fields.envValuePlaceholder")}
                    className="flex-1"
                  />
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => removeEnvVar(env.id)}
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
                onClick={addEnvVar}
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
              onClick={async () => {
                setIsSaving(true);
                try {
                  await onDelete(extension.config_key);
                } finally {
                  setIsSaving(false);
                }
              }}
              disabled={isSaving}
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
            disabled={!canSubmit || isSaving}
          >
            {t("extensions.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
