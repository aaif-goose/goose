/* eslint-disable @typescript-eslint/no-explicit-any */

/**
 * @vitest-environment jsdom
 */
import React from 'react';
import { screen, render, waitFor } from '@testing-library/react';
import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import { AppInner } from './App';
import { IntlTestWrapper } from './i18n/test-utils';

// Set up globals for jsdom
Object.defineProperty(window, 'location', {
  value: {
    hash: '',
    search: '',
    href: 'http://localhost:3000',
    origin: 'http://localhost:3000',
    pathname: '/',
  },
  writable: true,
});

Object.defineProperty(window, 'history', {
  value: {
    replaceState: vi.fn(),
    state: null,
  },
  writable: true,
});

vi.mock('./utils/costDatabase', () => ({
  initializeCostDatabase: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('./api', () => {
  const test_chat = {
    data: {
      session_id: 'test',
      messages: [],
      metadata: {
        description: '',
      },
    },
  };

  return {
    initConfig: vi.fn().mockResolvedValue(undefined),
    readAllConfig: vi.fn().mockResolvedValue(undefined),
    backupConfig: vi.fn().mockResolvedValue(undefined),
    recoverConfig: vi.fn().mockResolvedValue(undefined),
    validateConfig: vi.fn().mockResolvedValue(undefined),
    startAgent: vi.fn().mockResolvedValue(test_chat),
    resumeAgent: vi.fn().mockResolvedValue(test_chat),
  };
});

vi.mock('./sessions', () => ({
  fetchSessionDetails: vi
    .fn()
    .mockResolvedValue({ sessionId: 'test', messages: [], metadata: { description: '' } }),
  generateSessionId: vi.fn(),
}));

// Mock the ConfigContext module
vi.mock('./components/ConfigContext', () => ({
  useConfig: () => ({
    read: vi.fn().mockResolvedValue(null),
    update: vi.fn(),
    getExtensions: vi.fn().mockReturnValue([]),
    addExtension: vi.fn(),
    updateExtension: vi.fn(),
    createProviderDefaults: vi.fn(),
  }),
  ConfigProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

// Mock other components to simplify testing
vi.mock('./components/ErrorBoundary', () => ({
  ErrorUI: ({ error }: { error: Error }) => <div>Error: {error.message}</div>,
}));

vi.mock('./components/ModelAndProviderContext', () => ({
  ModelAndProviderProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useModelAndProvider: () => ({
    provider: null,
    model: null,
    getCurrentModelAndProvider: vi.fn(),
    setCurrentModelAndProvider: vi.fn(),
  }),
}));

vi.mock('./contexts/ChatContext', () => ({
  ChatProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useChatContext: () => ({
    chat: {
      id: 'test-id',
      name: 'Test Chat',
      messages: [],
      recipe: null,
    },
    setChat: vi.fn(),
    setPairChat: vi.fn(), // Keep this from HEAD
    resetChat: vi.fn(),
    hasActiveSession: false,
    setRecipe: vi.fn(),
    clearRecipe: vi.fn(),
    contextKey: 'hub',
  }),
  DEFAULT_CHAT_TITLE: 'New Chat', // Keep this from HEAD
}));

vi.mock('./components/ui/ConfirmationModal', () => ({
  ConfirmationModal: () => null,
}));

vi.mock('react-toastify', () => ({
  ToastContainer: () => null,
}));

vi.mock('./components/GoosehintsModal', () => ({
  GoosehintsModal: () => null,
}));

vi.mock('./components/AnnouncementModal', () => ({
  default: () => null,
}));

// Create mocks that we can track and configure per test
const mockNavigate = vi.fn();
const mockSearchParams = new URLSearchParams();
const mockSetSearchParams = vi.fn();

// Mock react-router-dom to avoid HashRouter issues in tests
vi.mock('react-router-dom', () => ({
  HashRouter: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  Routes: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  Route: ({ element }: { element: React.ReactNode }) => element,
  useNavigate: () => mockNavigate,
  useLocation: () => ({ state: null, pathname: '/' }),
  useSearchParams: () => [mockSearchParams, mockSetSearchParams],
  Outlet: () => null,
}));

// Mock electron API
const mockElectron = {
  getConfig: vi.fn().mockReturnValue({
    GOOSE_ALLOWLIST_WARNING: false,
    GOOSE_WORKING_DIR: '/test/dir',
  }),
  logInfo: vi.fn(),
  on: vi.fn(),
  off: vi.fn(),
  reactReady: vi.fn(),
  getAllowedExtensions: vi.fn().mockResolvedValue([]),
  platform: 'darwin',
  createChatWindow: vi.fn(),
  getSetting: vi.fn().mockResolvedValue(null),
  setSetting: vi.fn().mockResolvedValue(undefined),
};

// Mock appConfig
const mockAppConfig = {
  get: vi.fn((key: string) => {
    if (key === 'GOOSE_WORKING_DIR') return '/test/dir';
    return null;
  }),
};

// Attach mocks to window
(window as any).electron = mockElectron;
(window as any).appConfig = mockAppConfig;

// Mock matchMedia
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation((query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(), // deprecated
    removeListener: vi.fn(), // deprecated
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

describe('App Component - Brand New State', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockNavigate.mockClear();
    mockSetSearchParams.mockClear();

    // Reset search params
    mockSearchParams.forEach((_, key) => {
      mockSearchParams.delete(key);
    });

    window.location.hash = '';
    window.location.search = '';
    window.location.pathname = '/';
    window.sessionStorage?.clear?.();
    window.localStorage?.clear?.();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('should redirect to "/" when app is brand new (no provider configured)', async () => {
    // Mock no provider configured
    mockElectron.getConfig.mockReturnValue({
      GOOSE_DEFAULT_PROVIDER: null,
      GOOSE_DEFAULT_MODEL: null,
      GOOSE_ALLOWLIST_WARNING: false,
    });

    render(<AppInner />, { wrapper: IntlTestWrapper });

    // Wait for initialization
    await waitFor(() => {
      expect(mockElectron.reactReady).toHaveBeenCalled();
    });

    // The app should initialize without any navigation calls since we're already at "/"
    // No navigate calls should be made when no provider is configured
    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it('should handle deep links correctly when app is brand new', async () => {
    // Mock no provider configured
    mockElectron.getConfig.mockReturnValue({
      GOOSE_DEFAULT_PROVIDER: null,
      GOOSE_DEFAULT_MODEL: null,
      GOOSE_ALLOWLIST_WARNING: false,
    });

    // Set up search params to simulate view=settings deep link
    mockSearchParams.set('view', 'settings');

    render(<AppInner />, { wrapper: IntlTestWrapper });

    // Wait for initialization
    await waitFor(() => {
      expect(mockElectron.reactReady).toHaveBeenCalled();
    });

    expect(screen.getByText(/^Welcome to goose/)).toBeInTheDocument();
  });

  it('should not redirect when provider is configured', async () => {
    // Mock provider configured
    mockElectron.getConfig.mockReturnValue({
      GOOSE_DEFAULT_PROVIDER: 'openai',
      GOOSE_DEFAULT_MODEL: 'gpt-4',
      GOOSE_ALLOWLIST_WARNING: false,
    });

    render(<AppInner />, { wrapper: IntlTestWrapper });

    // Wait for initialization
    await waitFor(() => {
      expect(mockElectron.reactReady).toHaveBeenCalled();
    });

    // Should not navigate anywhere since provider is configured and we're already at "/"
    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it('should handle config recovery gracefully', async () => {
    // Mock config error that triggers recovery
    const { readAllConfig, recoverConfig } = await import('./api');
    console.log(recoverConfig);
    vi.mocked(readAllConfig).mockRejectedValueOnce(new Error('Config read error'));

    mockElectron.getConfig.mockReturnValue({
      GOOSE_DEFAULT_PROVIDER: null,
      GOOSE_DEFAULT_MODEL: null,
      GOOSE_ALLOWLIST_WARNING: false,
    });

    render(<AppInner />, { wrapper: IntlTestWrapper });

    // Wait for initialization and recovery
    await waitFor(() => {
      expect(mockElectron.reactReady).toHaveBeenCalled();
    });

    // App should still initialize without any navigation calls
    expect(mockNavigate).not.toHaveBeenCalled();
  });
});
