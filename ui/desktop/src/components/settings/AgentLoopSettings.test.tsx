import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { IntlTestWrapper } from '../../i18n/test-utils';
import AgentLoopSettings from './AgentLoopSettings';

// Two config layers as goose merges them: the user's config.yaml, which writes go to, over a
// lower layer such as /etc/goose/config.yaml.
const userConfig: Record<string, unknown> = {};
const lowerConfig: Record<string, unknown> = {};
let failReads = false;

const read = vi.fn(
  async (key: string, _isSecret: boolean, options?: { throwOnError?: boolean }) => {
    if (failReads && options?.throwOnError) throw new Error('read failed');
    return key in userConfig ? userConfig[key] : key in lowerConfig ? lowerConfig[key] : null;
  }
);
const upsert = vi.fn(async (key: string, value: unknown) => {
  userConfig[key] = value;
});
const remove = vi.fn(async (key: string) => {
  delete userConfig[key];
});

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ read, upsert, remove }),
}));

type Field = {
  label: string;
  key: string;
  userValue: number;
  userShown: number;
  lowerValue: number;
  lowerShown: number;
  defaultShown: number | null;
};

const fields: Field[] = [
  {
    label: 'Turns',
    key: 'GOOSE_MAX_TURNS',
    userValue: 50,
    userShown: 50,
    lowerValue: 200,
    lowerShown: 200,
    defaultShown: 1000,
  },
  {
    label: 'Threshold %',
    key: 'GOOSE_AUTO_COMPACT_THRESHOLD',
    userValue: 0.5,
    userShown: 50,
    lowerValue: 0.7,
    lowerShown: 70,
    defaultShown: 80,
  },
  {
    label: 'Token limit',
    key: 'GOOSE_AUTO_COMPACT_TOKEN_LIMIT',
    userValue: 500000,
    userShown: 500000,
    lowerValue: 1000000,
    lowerShown: 1000000,
    defaultShown: 225000,
  },
  {
    label: 'Cutoff',
    key: 'GOOSE_TOOL_CALL_CUTOFF',
    userValue: 20,
    userShown: 20,
    lowerValue: 30,
    lowerShown: 30,
    defaultShown: null,
  },
  {
    label: 'Retry timeout',
    key: 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS',
    userValue: 120,
    userShown: 120,
    lowerValue: 900,
    lowerShown: 900,
    defaultShown: 300,
  },
  {
    label: 'Failure timeout',
    key: 'GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS',
    userValue: 120,
    userShown: 120,
    lowerValue: 900,
    lowerShown: 900,
    defaultShown: 600,
  },
  {
    label: 'Block limit',
    key: 'GOOSE_STOP_HOOK_BLOCK_CAP',
    userValue: 3,
    userShown: 3,
    lowerValue: 12,
    lowerShown: 12,
    defaultShown: 8,
  },
];

function deferred() {
  let resolve: () => void = () => {};
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function renderLoaded() {
  render(
    <IntlTestWrapper>
      <AgentLoopSettings />
    </IntlTestWrapper>
  );
  // The load finishes in one batch, so once one setting has arrived they all have.
  await waitFor(() => expect(screen.getByLabelText('Enable Slash commands')).not.toBeChecked());
}

function input(label: string) {
  return screen.getByLabelText(label) as HTMLInputElement;
}

// Failed saves reject from the blur handler, as other failed saves on this page do.
async function ignoringUnhandledRejections(run: () => Promise<void>) {
  const ignore = () => {};
  process.on('unhandledRejection', ignore);
  try {
    await run();
  } finally {
    process.off('unhandledRejection', ignore);
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  failReads = false;
  for (const key of Object.keys(userConfig)) delete userConfig[key];
  for (const key of Object.keys(lowerConfig)) delete lowerConfig[key];
  // Loaded state differs from every default, so renderLoaded can tell when loading is done.
  userConfig.GOOSE_SLASH_COMMANDS_ENABLED = false;
  userConfig.GOOSE_TOOL_PAIR_SUMMARIZATION = true;
});

describe.each(fields)('$label', (field) => {
  it.each([
    ['the built-in default', () => {}, field.defaultShown],
    ['a lower config layer', () => (lowerConfig[field.key] = field.lowerValue), field.lowerShown],
    ['the user config', () => (userConfig[field.key] = field.userValue), field.userShown],
  ] as const)('writes nothing when left unchanged with %s', async (_source, setUp, shown) => {
    setUp();
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input(field.label)).toHaveValue(shown));

    await user.click(input(field.label));
    await user.tab();

    expect(upsert).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
  });

  it('clearing removes the user override and shows the lower layer value', async () => {
    lowerConfig[field.key] = field.lowerValue;
    userConfig[field.key] = field.userValue;
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input(field.label)).toHaveValue(field.userShown));

    await user.clear(input(field.label));
    await user.tab();

    await waitFor(() => expect(input(field.label)).toHaveValue(field.lowerShown));
    expect(remove).toHaveBeenCalledWith(field.key, false);
    expect(upsert).not.toHaveBeenCalled();
    expect(userConfig).not.toHaveProperty(field.key);

    await user.click(input(field.label));
    await user.tab();
    expect(upsert).not.toHaveBeenCalled();
    expect(remove).toHaveBeenCalledTimes(1);
  });

  it('clearing with no lower layer shows the built-in default', async () => {
    userConfig[field.key] = field.userValue;
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input(field.label)).toHaveValue(field.userShown));

    await user.clear(input(field.label));
    await user.tab();

    await waitFor(() => expect(remove).toHaveBeenCalledWith(field.key, false));
    await waitFor(() => expect(input(field.label)).toHaveValue(field.defaultShown));
    expect(upsert).not.toHaveBeenCalled();
    expect(userConfig).not.toHaveProperty(field.key);
  });

  it('saves a changed value once', async () => {
    const user = userEvent.setup();
    await renderLoaded();

    await user.clear(input(field.label));
    await user.type(input(field.label), String(field.userShown));
    await user.tab();
    await user.click(input(field.label));
    await user.tab();

    await waitFor(() => expect(userConfig[field.key]).toBe(field.userValue));
    expect(upsert).toHaveBeenCalledTimes(1);
  });
});

describe('unusual values from config', () => {
  it.each([
    [
      'a threshold of 0 from a lower layer',
      lowerConfig,
      'GOOSE_AUTO_COMPACT_THRESHOLD',
      0,
      'Threshold %',
      0,
    ],
    [
      'a threshold of 0 from the user config',
      userConfig,
      'GOOSE_AUTO_COMPACT_THRESHOLD',
      0,
      'Threshold %',
      0,
    ],
    [
      'a threshold of 1.0 from a lower layer',
      lowerConfig,
      'GOOSE_AUTO_COMPACT_THRESHOLD',
      1.0,
      'Threshold %',
      100,
    ],
    [
      'a threshold of 1.0 from the user config',
      userConfig,
      'GOOSE_AUTO_COMPACT_THRESHOLD',
      1.0,
      'Threshold %',
      100,
    ],
    ['a threshold of 0.57', userConfig, 'GOOSE_AUTO_COMPACT_THRESHOLD', 0.57, 'Threshold %', 57],
    [
      'a threshold of 0.33333',
      userConfig,
      'GOOSE_AUTO_COMPACT_THRESHOLD',
      0.33333,
      'Threshold %',
      33.33,
    ],
    ['max turns above the field maximum', userConfig, 'GOOSE_MAX_TURNS', 20000, 'Turns', 20000],
    [
      'a token limit below the field minimum',
      lowerConfig,
      'GOOSE_AUTO_COMPACT_TOKEN_LIMIT',
      500,
      'Token limit',
      500,
    ],
  ] as const)(
    'writes nothing when %s is left unchanged',
    async (_case, layer, key, value, label, shown) => {
      layer[key] = value;
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input(label)).toHaveValue(shown));

      await user.click(input(label));
      await user.tab();

      expect(upsert).not.toHaveBeenCalled();
      expect(remove).not.toHaveBeenCalled();
      expect(input(label)).toHaveValue(shown);
    }
  );
});

describe('numbers stored as strings', () => {
  it.each([
    ['Cutoff', 'GOOSE_TOOL_CALL_CUTOFF', '20', 20],
    ['Turns', 'GOOSE_MAX_TURNS', '20', 20],
    ['Threshold %', 'GOOSE_AUTO_COMPACT_THRESHOLD', '0.5', 50],
    ['Retry timeout', 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', ' 120 ', 120],
    ['Turns', 'GOOSE_MAX_TURNS', 'abc', 1000],
  ] as const)(
    '%s (%s) stored as %j shows %s and writes nothing when left unchanged',
    async (label, key, stored, shown) => {
      userConfig[key] = stored;
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input(label)).toHaveValue(shown));

      await user.click(input(label));
      await user.tab();

      expect(upsert).not.toHaveBeenCalled();
      expect(remove).not.toHaveBeenCalled();
    }
  );

  it('clearing a cutoff stored as a string removes it and shows the value in effect', async () => {
    userConfig.GOOSE_TOOL_CALL_CUTOFF = '20';
    lowerConfig.GOOSE_TOOL_CALL_CUTOFF = 30;
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Cutoff')).toHaveValue(20));

    await user.clear(input('Cutoff'));
    await user.tab();

    await waitFor(() => expect(input('Cutoff')).toHaveValue(30));
    expect(remove).toHaveBeenCalledWith('GOOSE_TOOL_CALL_CUTOFF', false);
    expect(userConfig).not.toHaveProperty('GOOSE_TOOL_CALL_CUTOFF');
    expect(upsert).not.toHaveBeenCalled();
  });

  it('clearing max turns stored as a string removes it and shows the default', async () => {
    userConfig.GOOSE_MAX_TURNS = '20';
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Turns')).toHaveValue(20));

    await user.clear(input('Turns'));
    await user.tab();

    await waitFor(() => expect(input('Turns')).toHaveValue(1000));
    expect(remove).toHaveBeenCalledWith('GOOSE_MAX_TURNS', false);
    expect(userConfig).not.toHaveProperty('GOOSE_MAX_TURNS');
    expect(upsert).not.toHaveBeenCalled();
  });
});

describe('values goose rejects for an integer field', () => {
  it.each([
    ['Retry timeout', 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', '1.5', 300],
    ['Retry timeout', 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', 1.5, 300],
    ['Turns', 'GOOSE_MAX_TURNS', '-1', 1000],
    ['Turns', 'GOOSE_MAX_TURNS', -1, 1000],
    ['Turns', 'GOOSE_MAX_TURNS', '20.0', 1000],
    ['Block limit', 'GOOSE_STOP_HOOK_BLOCK_CAP', 2 ** 32, 8],
    ['Cutoff', 'GOOSE_TOOL_CALL_CUTOFF', '-1', null],
    ['Cutoff', 'GOOSE_TOOL_CALL_CUTOFF', 1.5, null],
  ] as const)(
    '%s (%s) set to %j shows the default goose uses (%s) and writes nothing when left unchanged',
    async (label, key, stored, shown) => {
      userConfig[key] = stored;
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input(label)).toHaveValue(shown));

      await user.click(input(label));
      await user.tab();

      expect(upsert).not.toHaveBeenCalled();
      expect(remove).not.toHaveBeenCalled();
    }
  );

  it.each([
    ['Retry timeout', 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', '1.5', 900],
    ['Turns', 'GOOSE_MAX_TURNS', '-1', 200],
  ] as const)(
    'clearing %s (%s) set to %j still removes it and shows %s',
    async (label, key, stored, effective) => {
      userConfig[key] = stored;
      lowerConfig[key] = effective;
      const user = userEvent.setup();
      await renderLoaded();

      await user.clear(input(label));
      await user.tab();

      await waitFor(() => expect(input(label)).toHaveValue(effective));
      expect(remove).toHaveBeenCalledWith(key, false);
      expect(userConfig).not.toHaveProperty(key);
      expect(upsert).not.toHaveBeenCalled();
    }
  );
});

describe('a rejected cutoff shown as Auto', () => {
  const key = 'GOOSE_TOOL_CALL_CUTOFF';

  it('is removed when the user types a value, clears it and leaves the field', async () => {
    userConfig[key] = '-1';
    lowerConfig[key] = 30;
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Cutoff')).toHaveValue(null));

    await user.type(input('Cutoff'), '20');
    await user.clear(input('Cutoff'));
    await user.tab();

    await waitFor(() => expect(input('Cutoff')).toHaveValue(30));
    expect(remove).toHaveBeenCalledWith(key, false);
    expect(userConfig).not.toHaveProperty(key);
    expect(upsert).not.toHaveBeenCalled();
  });

  it('is left alone when the field is untouched', async () => {
    userConfig[key] = '-1';
    const user = userEvent.setup();
    await renderLoaded();

    await user.click(input('Cutoff'));
    await user.tab();

    expect(remove).not.toHaveBeenCalled();
    expect(upsert).not.toHaveBeenCalled();
  });
});

describe('typing before the initial read finishes', () => {
  const key = 'GOOSE_MAX_TURNS';

  const defaultRead = read.getMockImplementation();
  afterEach(() => {
    if (defaultRead) read.mockImplementation(defaultRead);
  });

  // Holds every read's result until released, so inputs can be used while the page is still loading.
  function holdReads() {
    const gate = deferred();
    read.mockImplementation(
      async (k: string, _s: boolean, options?: { throwOnError?: boolean }) => {
        // The value is read now but delivered when the gate opens, like a slow round trip.
        const value = k in userConfig ? userConfig[k] : k in lowerConfig ? lowerConfig[k] : null;
        if (!options?.throwOnError) await gate.promise;
        return value;
      }
    );
    return gate;
  }

  function renderPage() {
    render(
      <IntlTestWrapper>
        <AgentLoopSettings />
      </IntlTestWrapper>
    );
  }

  it('keeps the typed value and still saves it on blur', async () => {
    userConfig[key] = 50;
    userConfig.GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS = 900;
    const gate = holdReads();
    const user = userEvent.setup();
    renderPage();

    await user.clear(input('Turns'));
    await user.type(input('Turns'), '60');
    gate.resolve();

    await waitFor(() => expect(input('Retry timeout')).toHaveValue(900));
    expect(input('Turns')).toHaveValue(60);

    await user.tab();
    await waitFor(() => expect(userConfig[key]).toBe(60));
    expect(upsert).toHaveBeenCalledWith(key, 60, false);
  });

  it('keeps a value saved before the read finished', async () => {
    userConfig[key] = 50;
    const gate = holdReads();
    const user = userEvent.setup();
    renderPage();

    await user.clear(input('Turns'));
    await user.type(input('Turns'), '60');
    await user.tab();
    await waitFor(() => expect(userConfig[key]).toBe(60));
    gate.resolve();
    await waitFor(() => expect(input('Retry timeout')).toHaveValue(300));

    expect(input('Turns')).toHaveValue(60);
    await user.click(input('Turns'));
    await user.tab();
    expect(upsert).toHaveBeenCalledTimes(1);
  });

  it('saves a value typed and blurred before the read finished even if it equals the default', async () => {
    userConfig[key] = 50;
    const gate = holdReads();
    const user = userEvent.setup();
    renderPage();

    await user.clear(input('Turns'));
    await user.type(input('Turns'), '1000');
    await user.tab();
    gate.resolve();

    await waitFor(() => expect(userConfig[key]).toBe(1000));
    await waitFor(() => expect(input('Retry timeout')).toHaveValue(300));
    expect(input('Turns')).toHaveValue(1000);
  });

  it('keeps toggles clicked before the read finished and populates the others', async () => {
    userConfig.GOOSE_SLASH_COMMANDS_ENABLED = true;
    userConfig.GOOSE_TOOL_PAIR_SUMMARIZATION = false;
    userConfig.GOOSE_AUTO_EFFORT_ENABLED = true;
    const gate = holdReads();
    const user = userEvent.setup();
    renderPage();

    await user.click(screen.getByLabelText('Enable Slash commands'));
    await user.click(screen.getByLabelText('Enable Tool pair compaction'));
    gate.resolve();

    await waitFor(() =>
      expect(screen.getByLabelText('Enable Automatic thinking effort')).toBeChecked()
    );
    expect(screen.getByLabelText('Enable Slash commands')).not.toBeChecked();
    expect(screen.getByLabelText('Enable Tool pair compaction')).toBeChecked();
    // Cutoff is only editable while tool pair compaction is on.
    expect(input('Cutoff')).toBeEnabled();
    expect(upsert).toHaveBeenCalledTimes(2);
    expect(userConfig.GOOSE_SLASH_COMMANDS_ENABLED).toBe(false);
    expect(userConfig.GOOSE_TOOL_PAIR_SUMMARIZATION).toBe(true);
  });

  it('keeps the API key state saved before the read finished', async () => {
    userConfig.GOOSE_AUTO_EFFORT_ENABLED = false;
    const gate = holdReads();
    const user = userEvent.setup();
    renderPage();

    await user.click(screen.getByLabelText('Enable Automatic thinking effort'));
    await user.type(document.querySelector('input[type="password"]') as HTMLInputElement, 'secret');
    await user.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() => expect(upsert).toHaveBeenCalledWith('TYPESAFE_API_KEY', 'secret', true));
    gate.resolve();
    await waitFor(() => expect(input('Retry timeout')).toHaveValue(300));

    expect(screen.getByLabelText('Enable Automatic thinking effort')).toBeChecked();
    expect(screen.getByRole('button', { name: /remove/i })).toBeInTheDocument();
  });

  it('lets the read set a toggle again when its save failed', async () => {
    userConfig.GOOSE_SLASH_COMMANDS_ENABLED = true;
    const gate = holdReads();
    upsert.mockRejectedValueOnce(new Error('offline'));
    await ignoringUnhandledRejections(async () => {
      const user = userEvent.setup();
      renderPage();

      await user.click(screen.getByLabelText('Enable Slash commands'));
      await waitFor(() => expect(upsert).toHaveBeenCalledTimes(1));
      gate.resolve();

      await waitFor(() => expect(screen.getByLabelText('Enable Slash commands')).toBeChecked());
    });
  });

  it('populates fields the user did not touch', async () => {
    userConfig[key] = 50;
    const gate = holdReads();
    renderPage();
    gate.resolve();

    await waitFor(() => expect(input('Turns')).toHaveValue(50));
  });
});

describe('rapid edits', () => {
  const key = 'GOOSE_AUTO_COMPACT_TOKEN_LIMIT';

  beforeEach(() => {
    lowerConfig[key] = 1000000;
  });

  it('keeps the latest value when it returns to the original while a save is pending', async () => {
    const firstSave = deferred();
    upsert.mockImplementationOnce(async (k: string, value: unknown) => {
      await firstSave.promise;
      userConfig[k] = value;
    });
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Token limit')).toHaveValue(1000000));

    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '500000');
    await user.tab();
    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '1000000');
    await user.tab();
    firstSave.resolve();

    await waitFor(() => expect(upsert).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(userConfig[key]).toBe(1000000));
    expect(upsert).toHaveBeenLastCalledWith(key, 1000000, false);
  });

  it('keeps the latest value of each field when pending saves interleave across fields', async () => {
    const tokenSave = deferred();
    const turnsSave = deferred();
    upsert
      .mockImplementationOnce(async (k: string, value: unknown) => {
        await tokenSave.promise;
        userConfig[k] = value;
      })
      .mockImplementationOnce(async (k: string, value: unknown) => {
        await turnsSave.promise;
        userConfig[k] = value;
      });
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Token limit')).toHaveValue(1000000));

    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '500000');
    await user.tab();
    await user.clear(input('Turns'));
    await user.type(input('Turns'), '50');
    await user.tab();
    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '700000');
    await user.tab();
    await user.clear(input('Turns'));
    await user.type(input('Turns'), '60');
    await user.tab();
    turnsSave.resolve();
    tokenSave.resolve();

    await waitFor(() => expect(upsert).toHaveBeenCalledTimes(4));
    await waitFor(() => {
      expect(userConfig[key]).toBe(700000);
      expect(userConfig.GOOSE_MAX_TURNS).toBe(60);
    });
  });

  it('keeps input typed while a reset is pending and saves it on blur', async () => {
    userConfig[key] = 300000;
    const pendingRemove = deferred();
    remove.mockImplementationOnce(async (k: string) => {
      await pendingRemove.promise;
      delete userConfig[k];
    });
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Token limit')).toHaveValue(300000));

    await user.clear(input('Token limit'));
    await user.tab();
    await user.type(input('Token limit'), '500000');
    pendingRemove.resolve();

    await waitFor(() => expect(read).toHaveBeenLastCalledWith(key, false, { throwOnError: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(input('Token limit')).toHaveValue(500000);

    await user.tab();
    await waitFor(() => expect(userConfig[key]).toBe(500000));
  });

  it('does not let a pending reset overwrite a value saved after it', async () => {
    userConfig[key] = 300000;
    const pendingRemove = deferred();
    remove.mockImplementationOnce(async (k: string) => {
      await pendingRemove.promise;
      delete userConfig[k];
    });
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Token limit')).toHaveValue(300000));

    await user.clear(input('Token limit'));
    await user.tab();
    await user.type(input('Token limit'), '500000');
    await user.tab();
    pendingRemove.resolve();

    await waitFor(() => expect(userConfig[key]).toBe(500000));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(input('Token limit')).toHaveValue(500000);

    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '1000000');
    await user.tab();
    await waitFor(() => expect(userConfig[key]).toBe(1000000));
  });

  it('retries a failed reset after an earlier reset finishes behind a newer one', async () => {
    userConfig.GOOSE_MAX_TURNS = 50;
    const firstRemove = deferred();
    remove
      .mockImplementationOnce(async (k: string) => {
        await firstRemove.promise;
        delete userConfig[k];
      })
      .mockRejectedValueOnce(new Error('offline'));
    await ignoringUnhandledRejections(async () => {
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input('Turns')).toHaveValue(50));

      await user.clear(input('Turns'));
      await user.tab();
      await user.type(input('Turns'), '60');
      await user.tab();
      await user.clear(input('Turns'));
      await user.tab();
      firstRemove.resolve();

      await waitFor(() => expect(remove).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(userConfig.GOOSE_MAX_TURNS).toBe(60));
      await new Promise((resolve) => setTimeout(resolve, 0));

      // The failed reset left 60 in config, so blurring the blank field must try again.
      await user.click(input('Turns'));
      await user.tab();
      await waitFor(() => expect(remove).toHaveBeenCalledTimes(3));
      await waitFor(() => expect(userConfig).not.toHaveProperty('GOOSE_MAX_TURNS'));
    });
  });

  it('saves the previous value again after clearing it', async () => {
    userConfig[key] = 500000;
    const user = userEvent.setup();
    await renderLoaded();
    await waitFor(() => expect(input('Token limit')).toHaveValue(500000));

    await user.clear(input('Token limit'));
    await user.tab();
    await waitFor(() => expect(input('Token limit')).toHaveValue(1000000));
    await user.clear(input('Token limit'));
    await user.type(input('Token limit'), '500000');
    await user.tab();

    await waitFor(() => expect(userConfig[key]).toBe(500000));
  });

  it('retries a failed save on the next blur', async () => {
    upsert.mockRejectedValueOnce(new Error('offline'));
    await ignoringUnhandledRejections(async () => {
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input('Token limit')).toHaveValue(1000000));

      await user.clear(input('Token limit'));
      await user.type(input('Token limit'), '500000');
      await user.tab();
      await waitFor(() => expect(upsert).toHaveBeenCalledTimes(1));
      await user.click(input('Token limit'));
      await user.tab();

      await waitFor(() => expect(userConfig[key]).toBe(500000));
    });
  });

  it('leaves the field blank when the effective value cannot be read after clearing', async () => {
    userConfig[key] = 500000;
    await ignoringUnhandledRejections(async () => {
      const user = userEvent.setup();
      await renderLoaded();
      await waitFor(() => expect(input('Token limit')).toHaveValue(500000));

      failReads = true;
      await user.clear(input('Token limit'));
      await user.tab();
      await waitFor(() =>
        expect(read).toHaveBeenLastCalledWith(key, false, { throwOnError: true })
      );
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(input('Token limit')).toHaveValue(null);
      expect(userConfig).not.toHaveProperty(key);

      // No fallback was adopted as the saved value, so typing the default still saves it.
      await user.type(input('Token limit'), '225000');
      await user.tab();
      await waitFor(() => expect(userConfig[key]).toBe(225000));
    });
  });
});
