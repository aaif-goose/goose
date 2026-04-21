import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  AUTO_COMPACT_THRESHOLD_CONFIG_KEY,
} from "../../lib/autoCompact";

const mockGetClient = vi.fn();

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mockGetClient(),
}));

import { useAutoCompactPreferences } from "../useAutoCompactPreferences";

describe("useAutoCompactPreferences", () => {
  beforeEach(() => {
    mockGetClient.mockReset();
  });

  it("hydrates from the stored threshold value", async () => {
    mockGetClient.mockResolvedValue({
      goose: {
        GooseConfigRead: vi.fn().mockResolvedValue({ value: 0.65 }),
        GooseConfigUpsert: vi.fn().mockResolvedValue({}),
      },
    });

    const { result } = renderHook(() => useAutoCompactPreferences());

    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    expect(result.current.autoCompactThreshold).toBe(0.65);
  });

  it("persists threshold updates and broadcasts them", async () => {
    const upsert = vi.fn().mockResolvedValue({});
    const read = vi
      .fn()
      .mockResolvedValueOnce({ value: null })
      .mockResolvedValue({ value: 0.9 });

    mockGetClient.mockResolvedValue({
      goose: {
        GooseConfigRead: read,
        GooseConfigUpsert: upsert,
      },
    });

    const eventListener = vi.fn();
    window.addEventListener(AUTO_COMPACT_PREFERENCES_EVENT, eventListener);

    const { result } = renderHook(() => useAutoCompactPreferences());

    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    await act(async () => {
      await result.current.setAutoCompactThreshold(0.9);
    });

    expect(upsert).toHaveBeenCalledWith({
      key: AUTO_COMPACT_THRESHOLD_CONFIG_KEY,
      value: 0.9,
    });
    expect(eventListener).toHaveBeenCalledTimes(1);
    expect(result.current.autoCompactThreshold).toBe(0.9);

    window.removeEventListener(AUTO_COMPACT_PREFERENCES_EVENT, eventListener);
  });
});
