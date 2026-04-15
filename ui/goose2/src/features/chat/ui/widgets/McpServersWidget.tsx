import { useTranslation } from "react-i18next";
import { IconServer } from "@tabler/icons-react";
import { Widget } from "./Widget";

export function McpServersWidget() {
  const { t } = useTranslation("chat");

  return (
    <Widget
      title={t("contextPanel.widgets.mcpServers")}
      icon={<IconServer className="size-3.5" />}
    >
      <p className="text-foreground-subtle">
        {t("contextPanel.empty.noServersConfigured")}
      </p>
    </Widget>
  );
}
