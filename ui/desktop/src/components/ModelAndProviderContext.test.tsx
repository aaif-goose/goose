import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import {
  isModelChangePendingNow,
  isModelChangeSubmitHeld,
  ModelAndProviderProvider,
  resetModelChangeTrackingForTests,
  useModelAndProvider,
} from './ModelAndProviderContext';

const acpMocks = vi.hoisted(() => ({
  acpSetSessionProviderModel: vi.fn(),
  acpSaveDefaults: vi.fn(async () => undefined),
  snapshot: null as {
    session?: {
      provider_name?: string | null;
      model_config?: { model_name: string; toolshim: boolean };
    };
  } | null,
}));

vi.mock('../acp/providers', () => ({
  acpReadDefaults: vi.fn(async () => ({ providerId: 'provider', modelId: 'model' })),
  acpSaveDefaults: () => acpMocks.acpSaveDefaults(),
  acpSetSessionProviderModel: (
    sessionId: string,
    providerId: string,
    modelId?: string | null,
    thinkingEffort?: string | null
  ) => acpMocks.acpSetSessionProviderModel(sessionId, providerId, modelId, thinkingEffort),
}));

vi.mock('../acp/chatSessionStore', () => ({
  acpChatSessionStore: { getSnapshot: () => acpMocks.snapshot },
  acpChatSessionActions: { setSessionMetadata: vi.fn() },
}));

vi.mock('../toasts', () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('./settings/models/modelInterface', () => ({
  getProviderMetadata: vi.fn(),
}));

vi.mock('./settings/models/predefinedModelsUtils', () => ({
  getModelDisplayName: (model: string) => model,
  getProviderDisplayName: () => null,
}));

function Probe() {
  const { modelChangePending, changeModel, isModelChangePending } = useModelAndProvider();
  return (
    <button
      type="button"
      data-pending={modelChangePending ? 'yes' : 'no'}
      data-ref={isModelChangePending('session-1') ? 'yes' : 'no'}
      onClick={() => {
        void changeModel('session-1', { name: 'next', provider: 'other' });
      }}
    >
      switch
    </button>
  );
}

describe('ModelAndProviderProvider changeModel', () => {
  beforeEach(() => {
    resetModelChangeTrackingForTests();
    acpMocks.snapshot = null;
    acpMocks.acpSetSessionProviderModel.mockReset();
    acpMocks.acpSaveDefaults.mockClear();
  });

  it('keeps modelChangePending set until the session config RPCs settle', async () => {
    let resolveChange: (value: { providerId: string; modelId: string }) => void = () => {};
    acpMocks.acpSetSessionProviderModel.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveChange = resolve;
        })
    );

    render(
      <IntlTestWrapper>
        <ModelAndProviderProvider>
          <Probe />
        </ModelAndProviderProvider>
      </IntlTestWrapper>
    );

    const button = await screen.findByRole('button', { name: 'switch' });
    expect(button).toHaveAttribute('data-pending', 'no');

    fireEvent.click(button);
    expect(button).toHaveAttribute('data-ref', 'yes');
    expect(await screen.findByRole('button', { name: 'switch' })).toHaveAttribute(
      'data-pending',
      'yes'
    );

    await act(async () => {
      resolveChange({ providerId: 'other', modelId: 'next' });
    });

    expect(await screen.findByRole('button', { name: 'switch' })).toHaveAttribute(
      'data-pending',
      'no'
    );
    expect(screen.getByRole('button', { name: 'switch' })).toHaveAttribute('data-ref', 'no');
    expect(isModelChangePendingNow('session-2')).toBe(false);
    expect(isModelChangePendingNow(null)).toBe(false);
  });

  it('does not mark another session pending while this session switches', async () => {
    let resolveChange: (value: { providerId: string; modelId: string }) => void = () => {};
    acpMocks.acpSetSessionProviderModel.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveChange = resolve;
        })
    );

    render(
      <IntlTestWrapper>
        <ModelAndProviderProvider>
          <Probe />
        </ModelAndProviderProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('button', { name: 'switch' }));
    expect(isModelChangePendingNow('session-1')).toBe(true);
    expect(isModelChangePendingNow('session-2')).toBe(false);
    expect(isModelChangePendingNow(null)).toBe(false);

    await act(async () => {
      resolveChange({ providerId: 'other', modelId: 'next' });
    });
  });

  it('keeps a default change off the session pending map', async () => {
    let resolveSave: (value: undefined) => void = () => {};
    acpMocks.acpSaveDefaults.mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          resolveSave = resolve;
        })
    );

    function DefaultProbe() {
      const { changeModel } = useModelAndProvider();
      return (
        <button
          type="button"
          onClick={() => {
            void changeModel(null, { name: 'next', provider: 'other' });
          }}
        >
          defaults
        </button>
      );
    }

    render(
      <IntlTestWrapper>
        <ModelAndProviderProvider>
          <DefaultProbe />
        </ModelAndProviderProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('button', { name: 'defaults' }));
    expect(isModelChangePendingNow(null)).toBe(true);
    expect(isModelChangePendingNow('session-1')).toBe(false);
    expect(acpMocks.acpSetSessionProviderModel).not.toHaveBeenCalled();

    await act(async () => {
      resolveSave(undefined);
    });
    expect(isModelChangePendingNow(null)).toBe(false);
  });

  it('releases submits after a failed switch rolls back', async () => {
    acpMocks.snapshot = {
      session: {
        provider_name: 'provider',
        model_config: { model_name: 'model', toolshim: false },
      },
    };
    acpMocks.acpSetSessionProviderModel
      .mockRejectedValueOnce(new Error('invalid model'))
      .mockResolvedValueOnce({ providerId: 'provider', modelId: 'model' });

    render(
      <IntlTestWrapper>
        <ModelAndProviderProvider>
          <Probe />
        </ModelAndProviderProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('button', { name: 'switch' }));
    await act(async () => {
      await Promise.resolve();
    });

    expect(acpMocks.acpSetSessionProviderModel).toHaveBeenCalledTimes(2);
    expect(isModelChangePendingNow('session-1')).toBe(false);
    expect(isModelChangeSubmitHeld('session-1')).toBe(false);
  });

  it('holds submits when a failed switch cannot roll back', async () => {
    acpMocks.snapshot = {
      session: {
        provider_name: 'provider',
        model_config: { model_name: 'model', toolshim: false },
      },
    };
    acpMocks.acpSetSessionProviderModel.mockRejectedValue(new Error('provider down'));

    render(
      <IntlTestWrapper>
        <ModelAndProviderProvider>
          <Probe />
        </ModelAndProviderProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('button', { name: 'switch' }));
    await act(async () => {
      await Promise.resolve();
    });

    expect(isModelChangePendingNow('session-1')).toBe(false);
    expect(isModelChangeSubmitHeld('session-1')).toBe(true);
    expect(isModelChangeSubmitHeld('session-2')).toBe(false);
  });
});
