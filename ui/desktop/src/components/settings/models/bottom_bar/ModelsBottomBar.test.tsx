import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  act,
  fireEvent,
  render,
  type RenderOptions,
  screen,
  waitFor,
} from '@testing-library/react';
import ModelsBottomBar from './ModelsBottomBar';
import { IntlTestWrapper } from '../../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const createDropdownRef = (): React.RefObject<HTMLDivElement> =>
  ({ current: document.createElement('div') }) as React.RefObject<HTMLDivElement>;

let mockCurrentModel: string | null = 'config-model';
let mockCurrentProvider: string | null = 'config-provider';
const mockChangeModel = vi.fn();
const mockGetProviders = vi.fn();
const mockOnModelChanged = vi.fn();
const mockPreventCloseAutoFocus = vi.fn();

vi.mock('../../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    currentModel: mockCurrentModel,
    currentProvider: mockCurrentProvider,
    changeModel: mockChangeModel,
  }),
}));

vi.mock('../../../ConfigContext', () => ({
  useConfig: () => ({
    getProviders: mockGetProviders,
  }),
}));

vi.mock('../modelInterface', () => ({
  getProviderMetadata: vi.fn().mockResolvedValue({ display_name: 'Config Provider' }),
  fetchModelReasoning: vi.fn().mockResolvedValue(null),
}));

vi.mock('../../../../acp/providers', () => ({
  acpReadThinkingEffort: vi.fn().mockResolvedValue(null),
}));

vi.mock('../predefinedModelsUtils', () => ({
  getModelDisplayName: (model: string) => `Display ${model}`,
}));

vi.mock('../../../bottom_menu/BottomMenuAlertPopover', () => ({
  default: () => null,
}));

vi.mock('../../../ui/dropdown-menu', () => ({
  DropdownMenu: ({
    children,
    open,
    onOpenChange,
  }: {
    children: React.ReactNode;
    open: boolean;
    onOpenChange: (open: boolean) => void;
  }) => (
    <div data-testid="model-menu" data-open={open}>
      <button onClick={() => onOpenChange(true)}>Open model menu</button>
      {children}
    </div>
  ),
  DropdownMenuTrigger: ({
    children,
    disabled,
    title,
  }: {
    children: React.ReactNode;
    disabled?: boolean;
    title?: string;
  }) => (
    <div data-testid="model-menu-trigger" data-disabled={disabled ? 'true' : 'false'} title={title}>
      {children}
    </div>
  ),
  DropdownMenuContent: ({
    children,
    onCloseAutoFocus,
  }: {
    children: React.ReactNode;
    onCloseAutoFocus?: (event: Pick<Event, 'preventDefault'>) => void;
  }) => (
    <div>
      <button onClick={() => onCloseAutoFocus?.({ preventDefault: mockPreventCloseAutoFocus })}>
        Complete model menu close
      </button>
      {children}
    </div>
  ),
  DropdownMenuItem: ({
    children,
    onSelect,
    onClick,
    disabled,
  }: {
    children: React.ReactNode;
    onSelect?: () => void;
    onClick?: (event: unknown) => void;
    disabled?: boolean;
  }) => (
    <button
      disabled={disabled}
      onClick={(event) => {
        if (disabled) return;
        onSelect?.();
        onClick?.(event);
      }}
    >
      {children}
    </button>
  ),
  DropdownMenuSeparator: () => null,
}));

vi.mock('../subcomponents/SwitchModelModal', () => ({
  SwitchModelModal: () => <div data-testid="switch-model-modal" />,
}));

vi.mock('../../localInference/ModelSettingsPanel', () => ({
  ModelSettingsPanel: () => null,
}));

vi.mock('../../../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

describe('ModelsBottomBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCurrentModel = 'config-model';
    mockCurrentProvider = 'config-provider';
    mockGetProviders.mockResolvedValue([]);
    mockChangeModel.mockReset();
    mockChangeModel.mockResolvedValue(false);
  });

  it('shows a loading placeholder while the active session model is still loading', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={false}
        sessionLoaded={false}
      />
    );

    expect(screen.getByTestId('model-loading-state')).toHaveTextContent('Loading model...');
  });

  it('shows the active session model once the session has loaded', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="session-model"
        sessionProvider="session-provider"
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={false}
        sessionLoaded={true}
      />
    );

    expect(screen.getByText('session-model')).toBeInTheDocument();
    expect(screen.queryByTestId('model-loading-state')).not.toBeInTheDocument();
  });

  it('shows the configured model when there is no active session', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId={null}
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={false}
      />
    );

    expect(screen.getByText('config-model')).toBeInTheDocument();
    expect(screen.queryByTestId('model-loading-state')).not.toBeInTheDocument();
  });

  it('opens model overlays after the menu closes with the appropriate focus behavior', () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="local-model"
        sessionProvider="local"
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={false}
        sessionLoaded={true}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Open model menu' }));
    expect(screen.getByTestId('model-menu')).toHaveAttribute('data-open', 'true');

    fireEvent.click(screen.getByRole('button', { name: 'Local Model Settings' }));
    expect(screen.getByTestId('model-menu')).toHaveAttribute('data-open', 'false');
    expect(
      screen.queryByRole('heading', { name: 'Local Model Settings — Display local-model' })
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Complete model menu close' }));
    expect(
      screen.getByRole('heading', { name: 'Local Model Settings — Display local-model' })
    ).toBeInTheDocument();
    expect(mockPreventCloseAutoFocus).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '×' }));
    fireEvent.click(screen.getByRole('button', { name: 'Open model menu' }));
    fireEvent.click(screen.getByRole('button', { name: 'Change Model' }));
    expect(screen.getByTestId('model-menu')).toHaveAttribute('data-open', 'false');
    expect(screen.queryByTestId('switch-model-modal')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Complete model menu close' }));
    expect(screen.getByTestId('switch-model-modal')).toBeInTheDocument();
    expect(mockPreventCloseAutoFocus).toHaveBeenCalledOnce();
  });

  it('keeps the model picker closed while a turn is running', () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="session-model"
        sessionProvider="session-provider"
        onModelChanged={mockOnModelChanged}
        sessionLoaded={true}
        modelChangeLocked
      />
    );

    const trigger = screen.getByTestId('model-menu-trigger');
    expect(trigger).toHaveAttribute('data-disabled', 'true');
    expect(trigger).toHaveAttribute(
      'title',
      'Model changes apply after this turn finishes. Stop the run to switch now.'
    );

    fireEvent.click(screen.getByRole('button', { name: 'Open model menu' }));
    fireEvent.click(screen.getByRole('button', { name: 'Change Model' }));
    fireEvent.click(screen.getByRole('button', { name: 'Complete model menu close' }));
    expect(screen.queryByTestId('switch-model-modal')).not.toBeInTheDocument();
  });

  it('applies a recent-model switch when the session is idle', async () => {
    vi.mocked(window.electron.getSetting).mockResolvedValueOnce([
      { model: 'recent-model', provider: 'recent-provider' },
    ]);
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="session-model"
        sessionProvider="session-provider"
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={false}
        sessionLoaded={true}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: /recent-model/ }));
    await waitFor(() => {
      expect(mockChangeModel).toHaveBeenCalledWith(
        'session-123',
        expect.objectContaining({ name: 'recent-model', provider: 'recent-provider' })
      );
    });
  });

  it('drops a recent-model switch when a turn starts while lookups are pending', async () => {
    vi.mocked(window.electron.getSetting).mockResolvedValueOnce([
      { model: 'recent-model', provider: 'recent-provider' },
    ]);
    const renderBar = (locked: boolean) => (
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="session-model"
        sessionProvider="session-provider"
        onModelChanged={mockOnModelChanged}
        modelChangeLocked={locked}
        sessionLoaded={true}
      />
    );
    const view = renderWithIntl(renderBar(false));

    fireEvent.click(await screen.findByRole('button', { name: /recent-model/ }));
    view.rerender(renderBar(true));
    await act(async () => {});
    expect(mockChangeModel).not.toHaveBeenCalled();
  });
});
