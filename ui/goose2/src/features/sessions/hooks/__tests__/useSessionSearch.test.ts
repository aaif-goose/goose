import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";

const mockAcpSearchSessions = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSearchSessions: (...args: unknown[]) => mockAcpSearchSessions(...args),
}));

import { useSessionSearch } from "../useSessionSearch";

function createDeferredPromise<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const sessions: ChatSession[] = [
  {
    id: "session-1",
    acpSessionId: "acp-1",
    title: "Needle notes",
    createdAt: "2026-04-10T12:00:00Z",
    updatedAt: "2026-04-10T12:00:00Z",
    messageCount: 1,
  },
];

const resolvers = {
  getPersonaName: () => undefined,
  getProjectName: () => undefined,
};

describe("useSessionSearch", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("clears the loading state when a short query skips backend search", async () => {
    const deferred =
      createDeferredPromise<
        Array<{
          sessionId: string;
          snippet: string;
          messageId: string;
          matchCount: number;
        }>
      >();
    mockAcpSearchSessions.mockReturnValueOnce(deferred.promise);

    const { result } = renderHook(() =>
      useSessionSearch({
        sessions,
        resolvers,
      }),
    );

    await act(async () => {
      result.current.setQuery("needle");
    });
    await act(async () => {
      void result.current.search();
    });

    expect(result.current.isSearching).toBe(true);

    await act(async () => {
      result.current.setQuery("n");
    });
    await act(async () => {
      await result.current.search();
    });

    expect(result.current.isSearching).toBe(false);
    expect(result.current.submittedQuery).toBe("n");

    deferred.resolve([]);
    await act(async () => {
      await deferred.promise;
    });

    expect(result.current.isSearching).toBe(false);
  });
});
