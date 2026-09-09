import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import { ModelSettingsPanel } from './ModelSettingsPanel';
import type { ModelSettings } from '../../../acp/local-inference';

const mock = vi.hoisted(() => ({ read: vi.fn(), save: vi.fn() }));
vi.mock('../../../acp/local-inference', () => ({
  getModelSettingsInfo: mock.read,
  updateModelSettings: mock.save,
  listBuiltinChatTemplates: async () => ['chatml'],
}));

let saved: ModelSettings;
let format: string;
beforeEach(() => {
  vi.clearAllMocks();
  saved = {
    sampling: { type: 'Inherit' },
    useMlock: false,
    visionCapable: false,
    imageTokenEstimate: 256,
    mmprojSizeBytes: 0,
  };
  format = 'safetensors';
  mock.read.mockImplementation(async () => ({
    settings: JSON.parse(JSON.stringify(saved)),
    format,
    backendId: saved.backendId ?? (format === 'gguf' ? 'llamacpp' : 'eredu'),
    defaultBackendId: format === 'gguf' ? 'llamacpp' : 'eredu',
    availableBackends: format === 'gguf' ? ['llamacpp', 'eredu'] : ['eredu'],
    effectiveGeneration: { temperature: 0.6, top_k: 17, top_p: 0.83, min_p: 0.03, do_sample: true },
  }));
  mock.save.mockImplementation(async (_model: string, settings: ModelSettings) => {
    saved = JSON.parse(JSON.stringify(settings));
    return saved;
  });
});

async function open() {
  render(<ModelSettingsPanel modelId="fixture" />, { wrapper: IntlTestWrapper });
  await screen.findByLabelText('Inference backend');
}

describe('inherited local generation settings', () => {
  it('uses format defaults without persisting a backend override', async () => {
    await open();
    expect(screen.getByLabelText('Inference backend')).toHaveValue('');
    expect(screen.getByText('Default (Eredu)')).toBeInTheDocument();
    expect(screen.getByText('Model format: SafeTensors')).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: 'llama.cpp' })).not.toBeInTheDocument();
    expect(mock.save).not.toHaveBeenCalled();
  });

  it('switches a GGUF model to Eredu and restores the format default', async () => {
    format = 'gguf';
    await open();
    expect(screen.getByText('Model format: GGUF')).toBeInTheDocument();
    expect(screen.getByLabelText('GPU layers')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Inference backend'), { target: { value: 'eredu' } });
    await waitFor(() => expect(saved.backendId).toBe('eredu'));
    await waitFor(() => expect(screen.queryByLabelText('GPU layers')).not.toBeInTheDocument());
    fireEvent.click(screen.getByTitle('Reset to defaults'));
    await waitFor(() => expect(mock.save).toHaveBeenCalledTimes(2));
    expect(saved.backendId).toBe('eredu');
    fireEvent.change(screen.getByLabelText('Inference backend'), { target: { value: '' } });
    await waitFor(() => expect(saved.backendId).toBeNull());
    await screen.findByLabelText('GPU layers');
    expect(screen.getByText('Default (llama.cpp)')).toBeInTheDocument();
  });

  it('opening and saving an unrelated field never persists displayed effective values', async () => {
    await open();
    expect(mock.save).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText('Context size'), { target: { value: '4096' } });
    await waitFor(() => expect(saved.contextSize).toBe(4096));
    expect(saved.sampling).toEqual({ type: 'Inherit' });
    expect(saved.repeatPenalty).toBeUndefined();
    expect(saved.enableThinking).toBeUndefined();
    expect(screen.queryByLabelText('GPU layers')).not.toBeInTheDocument();
  });

  it('overrides one sampling field and clears it back to inheritance', async () => {
    await open();
    fireEvent.change(screen.getByLabelText('Sampling Strategy'), {
      target: { value: 'Temperature' },
    });
    await screen.findByLabelText('Temperature');
    fireEvent.change(screen.getByLabelText('Temperature'), { target: { value: '0.9' } });
    await waitFor(() => expect(saved.sampling).toEqual({ type: 'Temperature', temperature: 0.9 }));
    fireEvent.change(screen.getByLabelText('Temperature'), { target: { value: '' } });
    await waitFor(() => expect(saved.sampling).toEqual({ type: 'Temperature', temperature: null }));
    expect(screen.getByLabelText('Top K')).toHaveValue(null);
  });

  it('keeps legacy explicit values on read and resets them only on user action', async () => {
    saved.sampling = { type: 'Temperature', temperature: 0.8, topK: 40, topP: 0.95, minP: 0.05 };
    saved.repeatPenalty = 1;
    await open();
    expect(screen.getByLabelText('Temperature')).toHaveValue(0.8);
    expect(mock.save).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTitle('Reset to defaults'));
    await waitFor(() => expect(saved.sampling).toEqual({ type: 'Inherit' }));
    expect(saved.repeatPenalty).toBeNull();
    expect(saved.enableThinking).toBeNull();
  });
});
