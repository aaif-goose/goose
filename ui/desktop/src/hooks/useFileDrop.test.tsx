import type React from 'react';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../toasts', () => ({
  toastError: vi.fn(),
}));

import { toastError } from '../toasts';
import { IntlTestWrapper } from '../i18n/test-utils';
import { useFileDrop } from './useFileDrop';

const setExternalBackend = (external: boolean) => {
  (window as unknown as { appConfig: unknown }).appConfig = {
    get: (key: string) => (key === 'GOOSE_EXTERNAL_BACKEND' ? external : undefined),
  };
};

const dropEvent = (files: File[]) =>
  ({
    preventDefault: vi.fn(),
    dataTransfer: { files },
  }) as unknown as React.DragEvent<HTMLDivElement>;

const pdf = new File(['%PDF'], 'report.pdf', { type: 'application/pdf' });
const png = new File(['png'], 'chart.png', { type: 'image/png' });

describe('useFileDrop', () => {
  beforeEach(() => {
    vi.mocked(toastError).mockClear();
    (window.electron as unknown as { getPathForFile: unknown }).getPathForFile = vi.fn(
      (file: File) => `/client/${file.name}`
    );
  });

  afterEach(() => {
    delete (window as unknown as { appConfig?: unknown }).appConfig;
  });

  it('keeps the local path of a dropped non-image file with a local backend', async () => {
    setExternalBackend(false);
    const { result } = renderHook(() => useFileDrop(), { wrapper: IntlTestWrapper });

    await act(() => result.current.handleDrop(dropEvent([pdf])));

    expect(result.current.droppedFiles.map((f) => f.path)).toEqual(['/client/report.pdf']);
    expect(toastError).not.toHaveBeenCalled();
  });

  it('rejects dropped non-image files with an external backend and keeps images', async () => {
    setExternalBackend(true);
    const { result } = renderHook(() => useFileDrop(), { wrapper: IntlTestWrapper });

    await act(() => result.current.handleDrop(dropEvent([pdf, png])));

    expect(result.current.droppedFiles.map((f) => f.name)).toEqual(['chart.png']);
    expect(toastError).toHaveBeenCalledTimes(1);
    expect(vi.mocked(toastError).mock.calls[0][0].msg).toContain('report.pdf');
  });

  it('filters picked files the same way', () => {
    setExternalBackend(true);
    const { result } = renderHook(() => useFileDrop(), { wrapper: IntlTestWrapper });

    const untyped = new File(['notes'], 'notes', { type: '' });

    expect(result.current.rejectUnsupportedFiles([pdf])).toEqual([]);
    expect(result.current.rejectUnsupportedFiles([untyped])).toEqual([]);
    expect(result.current.rejectUnsupportedFiles([png])).toEqual([png]);
    expect(toastError).toHaveBeenCalledTimes(2);
  });
});
