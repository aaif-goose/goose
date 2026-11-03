import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ModelAndProviderProvider, useModelAndProvider } from './ModelAndProviderContext';

const acpSetSessionProviderModel = vi.fn();

vi.mock('../acp/providers', () => ({
  acpReadDefaults: vi.fn(async () => ({ providerId: 'provider', modelId: 'model' })),
  acpSaveDefaults: vi.fn(async () => undefined),
  acpSetSessionProviderModel: (...args: unknown[]) => acpSetSessionProviderModel(...args),
}));

vi.mock('../acp/chatSessionStore', () => ({
  acpChatSessionStore: { getSnapshot: () => null },
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
      data-ref={isModelChangePending() ? 'yes' : 'no'}
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
    acpSetSessionProviderModel.mockReset();
  });

  it('keeps modelChangePending set until the session config RPCs settle', async () => {
    let resolveChange: (value: { providerId: string; modelId: string }) => void = () => {};
    acpSetSessionProviderModel.mockImplementation(
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
  });
});
