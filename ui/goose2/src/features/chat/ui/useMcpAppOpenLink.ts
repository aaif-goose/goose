import type { AppRendererProps } from "@mcp-ui/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useRef, useState } from "react";

type OpenLinkParams = Parameters<
  NonNullable<AppRendererProps["onOpenLink"]>
>[0];
type OpenLinkResult = Awaited<
  ReturnType<NonNullable<AppRendererProps["onOpenLink"]>>
>;

function normalizeOpenLinkUrl(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? parsed.href
      : null;
  } catch {
    return null;
  }
}

export function useMcpAppOpenLink() {
  const [pendingOpenLinkUrl, setPendingOpenLinkUrl] = useState<string | null>(
    null,
  );
  const pendingOpenLinkResolverRef = useRef<
    ((result: OpenLinkResult) => void) | null
  >(null);

  useEffect(
    () => () => {
      pendingOpenLinkResolverRef.current?.({ isError: true });
      pendingOpenLinkResolverRef.current = null;
    },
    [],
  );

  const finishOpenLinkRequest = useCallback((result: OpenLinkResult) => {
    pendingOpenLinkResolverRef.current?.(result);
    pendingOpenLinkResolverRef.current = null;
    setPendingOpenLinkUrl(null);
  }, []);

  const handleOpenLink = useCallback(async ({ url }: OpenLinkParams) => {
    const safeUrl = normalizeOpenLinkUrl(url);
    if (!safeUrl) {
      return {
        isError: true,
        message: "Only http and https links can be opened.",
      };
    }

    pendingOpenLinkResolverRef.current?.({
      isError: true,
      message: "Another open-link request replaced this request.",
    });

    return new Promise<OpenLinkResult>((resolve) => {
      pendingOpenLinkResolverRef.current = resolve;
      setPendingOpenLinkUrl(safeUrl);
    });
  }, []);

  const handleOpenLinkModalClose = useCallback(() => {
    finishOpenLinkRequest({ isError: true });
  }, [finishOpenLinkRequest]);

  const handleConfirmOpenLink = useCallback(
    async (url: string) => {
      try {
        await openUrl(url);
        finishOpenLinkRequest({});
      } catch (error) {
        finishOpenLinkRequest({ isError: true });
        throw error;
      }
    },
    [finishOpenLinkRequest],
  );

  return {
    handleConfirmOpenLink,
    handleOpenLink,
    handleOpenLinkModalClose,
    pendingOpenLinkUrl,
  };
}
