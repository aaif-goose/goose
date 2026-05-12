import { describe, expect, it } from "vitest";
import {
  DEFAULT_CHAT_TITLE,
  getDisplaySessionTitle,
  getEditableSessionTitle,
  getSessionTitleFromDraft,
  isDefaultChatTitle,
  isSessionTitleUnchanged,
} from "./sessionTitle";

describe("sessionTitle", () => {
  it("maps the internal default title to the localized display title", () => {
    expect(getDisplaySessionTitle(DEFAULT_CHAT_TITLE, "Nuevo chat")).toBe(
      "Nuevo chat",
    );
    expect(getEditableSessionTitle(DEFAULT_CHAT_TITLE, "Nuevo chat")).toBe(
      "Nuevo chat",
    );
  });

  it("treats the ACP title-case default title as the default title", () => {
    expect(isDefaultChatTitle("New Chat")).toBe(true);
    expect(getDisplaySessionTitle("New Chat", "Nuevo chat")).toBe("Nuevo chat");
  });

  it("treats the localized default title as unchanged while the sentinel is still internal", () => {
    expect(
      isSessionTitleUnchanged("Nuevo chat", DEFAULT_CHAT_TITLE, "Nuevo chat"),
    ).toBe(true);
    expect(
      isSessionTitleUnchanged("Renamed chat", DEFAULT_CHAT_TITLE, "Nuevo chat"),
    ).toBe(false);
  });

  it("falls back to attachment-based titles for attachment-only sends", () => {
    expect(
      getSessionTitleFromDraft("", [
        {
          id: "file-1",
          kind: "file",
          name: "report.pdf",
          path: "/tmp/report.pdf",
        },
      ]),
    ).toBe("Attached file");

    expect(
      getSessionTitleFromDraft("   ", [
        {
          id: "dir-1",
          kind: "directory",
          name: "screenshots",
          path: "/tmp/screenshots",
        },
        {
          id: "dir-2",
          kind: "directory",
          name: "receipts",
          path: "/tmp/receipts",
        },
      ]),
    ).toBe("Attached folders");
  });
});
