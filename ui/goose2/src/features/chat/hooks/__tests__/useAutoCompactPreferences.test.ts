import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  DEFAULT_AUTO_COMPACT_THRESHOLD,
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

  afterEach(() => {
    vi.useRealTimers();
  });

  it("hydrates from the stored threshold value", async () => {
    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: vi.fn().mockResolvedValue({
          values: [{ key: "autoCompactThreshold", value: 0.65 }],
        }),
        GoosePreferencesSave: vi.fn().mockResolvedValue({}),
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
      .mockResolvedValueOnce({
        values: [{ key: "autoCompactThreshold", value: null }],
      })
      .mockResolvedValue({
        values: [{ key: "autoCompactThreshold", value: 0.9 }],
      });

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: read,
        GoosePreferencesSave: upsert,
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
      values: [{ key: "autoCompactThreshold", value: 0.9 }],
    });
    expect(eventListener).toHaveBeenCalledTimes(1);
    expect(result.current.autoCompactThreshold).toBe(0.9);

    window.removeEventListener(AUTO_COMPACT_PREFERENCES_EVENT, eventListener);
  });

  it("does not mark preferences hydrated when the initial read fails", async () => {
    mockGetClient.mockRejectedValue(new Error("ACP not ready"));

    const { result } = renderHook(() => useAutoCompactPreferences());

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(result.current.isHydrated).toBe(false);
    expect(result.current.autoCompactThreshold).toBe(
      DEFAULT_AUTO_COMPACT_THRESHOLD,
    );
  });

  it("retries hydration after a transient read failure", async () => {
    vi.useFakeTimers();
    const read = vi
      .fn()
      .mockRejectedValueOnce(new Error("ACP not ready"))
      .mockResolvedValueOnce({
        values: [{ key: "autoCompactThreshold", value: 0.65 }],
      });

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: read,
        GoosePreferencesSave: vi.fn().mockResolvedValue({}),
      },
    });

    const { result } = renderHook(() => useAutoCompactPreferences());

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(result.current.isHydrated).toBe(false);
    expect(result.current.autoCompactThreshold).toBe(
      DEFAULT_AUTO_COMPACT_THRESHOLD,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });

    expect(result.current.autoCompactThreshold).toBe(0.65);
    expect(result.current.isHydrated).toBe(true);
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("backs off repeated hydration retries", async () => {
    vi.useFakeTimers();
    const read = vi.fn().mockRejectedValue(new Error("ACP not ready"));

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: read,
        GoosePreferencesSave: vi.fn().mockResolvedValue({}),
      },
    });

    renderHook(() => useAutoCompactPreferences());

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(read).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(999);
    });
    expect(read).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(read).toHaveBeenCalledTimes(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1999);
    });
    expect(read).toHaveBeenCalledTimes(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(read).toHaveBeenCalledTimes(3);
  });
});
