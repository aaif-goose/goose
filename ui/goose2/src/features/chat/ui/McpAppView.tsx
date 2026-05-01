import { CodeBlock } from "@/shared/ui/ai-elements/code-block";
import { useTranslation } from "react-i18next";
import type { McpAppPayload } from "@/shared/types/messages";

interface McpAppViewProps {
  payload: McpAppPayload;
}

export function McpAppView({ payload }: McpAppViewProps) {
  const { t } = useTranslation("chat");

  // Currently we just render the MCP App payload as JSON.
  // Up next, we'll replace this with actual HTML rendering and host bridging.
  return (
    <div className="my-3" data-testid="mcp-app-view">
      <div className="mb-2 text-muted-foreground text-xs uppercase tracking-wide">
        {t("message.mcpAppUnderConstruction")}
      </div>
      <CodeBlock code={JSON.stringify(payload, null, 2)} language="json" />
    </div>
  );
}
