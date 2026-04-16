import type { OpenDialogOptions, OpenDialogReturnValue } from 'electron';
import {
  app,
  App,
  BrowserWindow,
  dialog,
  globalShortcut,
  ipcMain,
  Menu,
  MenuItem,
  net,
  Notification,
  powerSaveBlocker,
  screen,
  session,
  shell,
  Tray,
} from 'electron';
import { pathToFileURL, format as formatUrl, URLSearchParams } from 'node:url';
import { Buffer } from 'node:buffer';
import fs from 'node:fs/promises';
import fsSync from 'node:fs';
import started from 'electron-squirrel-startup';
import path from 'node:path';
import os from 'node:os';
import { execFileSync, spawn } from 'child_process';
import 'dotenv/config';
import { checkServerStatus } from './goosed';
import { startGoosed } from './goosed';
import { createClient, createConfig } from './api/client';
import { expandTilde } from './utils/pathUtils';
import log from './utils/logger';
import { ensureWinShims } from './utils/winShims';
import { addRecentDir, loadRecentDirs } from './utils/recentDirs';
import { formatAppName, errorMessage, formatErrorForLogging } from './utils/conversionUtils';
import type { Settings, SettingKey } from './utils/settings';
import { defaultSettings, getKeyboardShortcuts } from './utils/settings';
import * as crypto from 'crypto';
import * as yaml from 'yaml';
import windowStateKeeper from 'electron-window-state';
import {
  getUpdateAvailable,
  registerUpdateIpcHandlers,
  setTrayRef,
  setupAutoUpdater,
  updateTrayMenu,
} from './utils/autoUpdater';
import { UPDATES_ENABLED } from './updates';
import './utils/recipeHash';
import { Client } from './api/client';
import { GooseApp } from './api';
import * as mesh from './mesh';
import installExtension, { REACT_DEVELOPER_TOOLS } from 'electron-devtools-installer';
import { BLOCKED_PROTOCOLS, WEB_PROTOCOLS } from './utils/urlSecurity';
import { buildCSP } from './utils/csp';

function shouldSetupUpdater(): boolean {
  // Setup updater if either the flag is enabled OR dev updates are enabled
  return UPDATES_ENABLED || process.env.ENABLE_DEV_UPDATES === 'true';
}

// Settings management
const SETTINGS_FILE = path.join(app.getPath('userData'), 'settings.json');

function getSettings(): Settings {
  if (fsSync.existsSync(SETTINGS_FILE)) {
    let stored: Partial<Settings>;
    try {
      const data = fsSync.readFileSync(SETTINGS_FILE, 'utf8');
      stored = JSON.parse(data) as Partial<Settings>;
    } catch (err) {
      console.error('Failed to read settings.json, using defaults:', err);
      return defaultSettings;
    }
    return {
      ...defaultSettings,
      ...stored,
      externalGoosed: {
        ...defaultSettings.externalGoosed,
        ...(stored.externalGoosed ?? {}),
      },
      keyboardShortcuts: {
        ...defaultSettings.keyboardShortcuts,
        ...(stored.keyboardShortcuts ?? {}),
      },
      sessionSharing: {
        ...defaultSettings.sessionSharing,
        ...(stored.sessionSharing ?? {}),
      },
    };
  }
  return defaultSettings;
}

function updateSettings(modifier: (settings: Settings) => void): void {
  const settings = getSettings();
  modifier(settings);
  fsSync.writeFileSync(SETTINGS_FILE, JSON.stringify(settings, null, 2));
}

async function configureProxy() {
  const httpsProxy = process.env.HTTPS_PROXY || process.env.https_proxy;
  const httpProxy = process.env.HTTP_PROXY || process.env.http_proxy;
  const noProxy = process.env.NO_PROXY || process.env.no_proxy || '';

  const proxyUrl = httpsProxy || httpProxy;

  if (proxyUrl) {
    console.log('[Main] Configuring proxy');
    await session.defaultSession.setProxy({
      proxyRules: proxyUrl,
      proxyBypassRules: noProxy,
    });
    console.log('[Main] Proxy configured successfully');
  }
}

if (started) app.quit();

// Certificate trust for goosed servers (local and external).
// Both certificate-error (renderer) and setCertificateVerifyProc (main-process
// net.fetch) pin to the exact cert fingerprint. For locally-spawned goosed the
// fingerprint comes from its stdout; for external backends we use Trust-On-First-Use
// (TOFU) — the first TLS handshake pins the cert for the lifetime of the process.
let pinnedCertFingerprint: string | null = null;

// Cached hostname of the configured external goosed server, updated when a
// chat is created so we don't hit the filesystem on every TLS handshake.
let trustedExternalHostname: string | null = null;

function isLocalhost(hostname: string): boolean {
  return hostname === '127.0.0.1' || hostname === 'localhost';
}

function isTrustedHost(hostname: string): boolean {
  if (isLocalhost(hostname)) return true;
  return trustedExternalHostname !== null && hostname === trustedExternalHostname;
}

function normalizeFingerprint(fp: string): string {
  if (fp.startsWith('sha256/')) {
    const b64 = fp.slice('sha256/'.length);
    const buf = Buffer.from(b64, 'base64');
    return Array.from(buf)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join(':')
      .toUpperCase();
  }
  return fp.toUpperCase();
}

// Renderer requests: pin to the exact cert goosed generated once known.
// Before the fingerprint is available (during the health-check bootstrap
// window) any localhost cert is accepted so the server can come up.
app.on('certificate-error', (event, _webContents, url, _error, certificate, callback) => {
  const parsed = new URL(url);
  if (!isTrustedHost(parsed.hostname)) {
    callback(false);
    return;
  }
  if (pinnedCertFingerprint) {
    const match =
      normalizeFingerprint(certificate.fingerprint) === pinnedCertFingerprint.toUpperCase();
    event.preventDefault();
    callback(match);
  } else {
    // TOFU: pin the certificate from the first successful handshake.
    pinnedCertFingerprint = normalizeFingerprint(certificate.fingerprint);
    event.preventDefault();
    callback(true);
  }
});

// Main-process net.fetch: pin to the exact cert goosed generated.
app.whenReady().then(() => {
  session.defaultSession.setCertificateVerifyProc((request, callback) => {
    if (!isTrustedHost(request.hostname)) {
      callback(-3);
      return;
    }
    if (!pinnedCertFingerprint) {
      // TOFU: pin the certificate from the first successful handshake.
      pinnedCertFingerprint = normalizeFingerprint(request.certificate.fingerprint);
      callback(0);
      return;
    }
    const match =
      normalizeFingerprint(request.certificate.fingerprint) === pinnedCertFingerprint.toUpperCase();
    callback(match ? 0 : -2);
  });
});

if (process.env.ENABLE_PLAYWRIGHT) {
  const debugPort = process.env.PLAYWRIGHT_DEBUG_PORT || '9222';
  console.log(`[Main] Enabling Playwright remote debugging on port ${debugPort}`);
  app.commandLine.appendSwitch('remote-debugging-port', debugPort);
}

// In development mode, force registration as the default protocol client
// In production, register normally
if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
  // Development mode - force registration
  console.log('[Main] Development mode: Forcing protocol registration for goose://');
  app.setAsDefaultProtocolClient('goose');

  if (process.platform === 'darwin') {
    try {
      // Reset the default handler to ensure dev version takes precedence
      spawn('open', ['-a', process.execPath, '--args', '--reset-protocol-handler', 'goose'], {
        detached: true,
        stdio: 'ignore',
      });
    } catch {
      console.warn('[Main] Could not reset protocol handler');
    }
  }
} else {
  // Production mode - normal registration
  app.setAsDefaultProtocolClient('goose');
}

// Apply single instance lock on Windows and Linux where it's needed for deep links
// macOS uses the 'open-url' event instead
let gotTheLock = true;
if (process.platform !== 'darwin') {
  gotTheLock = app.requestSingleInstanceLock();

  if (!gotTheLock) {
    app.quit();
  } else {
    app.on('second-instance', (_event, commandLine) => {
      const protocolUrl = commandLine.find((arg) => arg.startsWith('goose://'));
      if (protocolUrl) {
        const parsedUrl = new URL(protocolUrl);
        // If it's a bot/recipe URL, handle it directly by creating a new window
        if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'recipe') {
          app.whenReady().then(async () => {
            const recentDirs = loadRecentDirs();
            const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

            const deeplinkData = parseRecipeDeeplink(protocolUrl);
            const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

            await createChat(app, {
              dir: openDir || undefined,
              recipeDeeplink: deeplinkData?.config,
              scheduledJobId: scheduledJobId || undefined,
              recipeParameters: deeplinkData?.parameters,
            });
          });
          return; // Skip the rest of the handler
        }

        // For non-bot URLs, continue with normal handling
        handleProtocolUrl(protocolUrl);
      }

      // Only focus existing windows for non-bot/recipe URLs
      const existingWindows = BrowserWindow.getAllWindows();
      if (existingWindows.length > 0) {
        const mainWindow = existingWindows[0];
        if (mainWindow.isMinimized()) {
          mainWindow.restore();
        }
        mainWindow.focus();
      }
    });
  }

  // Handle protocol URLs on Windows and Linux startup
  const protocolUrl = process.argv.find((arg) => arg.startsWith('goose://'));
  if (protocolUrl) {
    app.whenReady().then(() => {
      handleProtocolUrl(protocolUrl);
    });
  }
}

let firstOpenWindow: BrowserWindow;
let pendingDeepLink: string | null = null;
let openUrlHandledLaunch = false;

async function handleProtocolUrl(url: string) {
  if (!url) return;

  pendingDeepLink = url;

  const parsedUrl = new URL(url);
  const recentDirs = loadRecentDirs();
  const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

  if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'recipe') {
    // For bot/recipe URLs, get existing window or create new one
    const existingWindows = BrowserWindow.getAllWindows();
    const targetWindow =
      existingWindows.length > 0
        ? existingWindows[0]
        : await createChat(app, { dir: openDir || undefined });
    await processProtocolUrl(parsedUrl, targetWindow);
  } else {
    // For other URL types, reuse existing window if available
    const existingWindows = BrowserWindow.getAllWindows();
    if (existingWindows.length > 0) {
      firstOpenWindow = existingWindows[0];
      if (firstOpenWindow.isMinimized()) {
        firstOpenWindow.restore();
      }
      firstOpenWindow.focus();
    } else {
      firstOpenWindow = await createChat(app, { dir: openDir || undefined });
    }

    if (firstOpenWindow) {
      const webContents = firstOpenWindow.webContents;
      if (webContents.isLoadingMainFrame()) {
        webContents.once('did-finish-load', async () => {
          await processProtocolUrl(parsedUrl, firstOpenWindow);
        });
      } else {
        await processProtocolUrl(parsedUrl, firstOpenWindow);
      }
    }
  }
}

async function processProtocolUrl(parsedUrl: URL, window: BrowserWindow) {
  const recentDirs = loadRecentDirs();
  const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

  if (parsedUrl.hostname === 'extension') {
    window.webContents.send('add-extension', pendingDeepLink);
  } else if (parsedUrl.hostname === 'sessions') {
    window.webContents.send('open-shared-session', pendingDeepLink);
  } else if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'recipe') {
    const deeplinkData = parseRecipeDeeplink(pendingDeepLink ?? parsedUrl.toString());
    const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

    // Create a new window and ignore the passed-in window
    await createChat(app, {
      dir: openDir || undefined,
      recipeDeeplink: deeplinkData?.config,
      scheduledJobId: scheduledJobId || undefined,
      recipeParameters: deeplinkData?.parameters,
    });
    pendingDeepLink = null;
  }
}

let windowDeeplinkURL: string | null = null;

app.on('open-url', async (_event, url) => {
  if (process.platform !== 'win32') {
    const parsedUrl = new URL(url);

    log.info('[Main] Received open-url event:', url);

    await app.whenReady();

    const recentDirs = loadRecentDirs();
    const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

    // Handle bot/recipe URLs by directly creating a new window
    if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'recipe') {
      log.info('[Main] Detected bot/recipe URL, creating new chat window');
      openUrlHandledLaunch = true;
      const deeplinkData = parseRecipeDeeplink(url);
      if (deeplinkData) {
        windowDeeplinkURL = url;
      }
      const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

      await createChat(app, {
        dir: openDir || undefined,
        recipeDeeplink: deeplinkData?.config,
        scheduledJobId: scheduledJobId || undefined,
        recipeParameters: deeplinkData?.parameters,
      });
      windowDeeplinkURL = null;
      return;
    }

    // For extension/session URLs, store the deep link for processing after React is ready
    pendingDeepLink = url;
    log.info('[Main] Stored pending deep link for processing after React ready:', url);

    const existingWindows = BrowserWindow.getAllWindows();
    if (existingWindows.length > 0) {
      firstOpenWindow = existingWindows[0];
      if (firstOpenWindow.isMinimized()) firstOpenWindow.restore();
      firstOpenWindow.focus();
      if (parsedUrl.hostname === 'extension') {
        firstOpenWindow.webContents.send('add-extension', pendingDeepLink);
        pendingDeepLink = null;
      } else if (parsedUrl.hostname === 'sessions') {
        firstOpenWindow.webContents.send('open-shared-session', pendingDeepLink);
        pendingDeepLink = null;
      }
    } else {
      openUrlHandledLaunch = true;
      firstOpenWindow = await createChat(app, { dir: openDir || undefined });
    }
  }
});

// Handle macOS drag-and-drop onto dock icon
app.on('will-finish-launching', () => {
  if (process.platform === 'darwin') {
    app.setAboutPanelOptions({
      applicationName: 'Goose',
      applicationVersion: app.getVersion(),
    });
  }
});

// Handle drag-and-drop onto dock icon
app.on('open-file', async (event, filePath) => {
  event.preventDefault();
  await handleFileOpen(filePath);
});

// Handle multiple files/folders (macOS only)
if (process.platform === 'darwin') {
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  app.on('open-files' as any, async (event: any, filePaths: string[]) => {
    event.preventDefault();
    for (const filePath of filePaths) {
      await handleFileOpen(filePath);
    }
  });
}

async function handleFileOpen(filePath: string) {
  try {
    if (!filePath || typeof filePath !== 'string') {
      return;
    }

    const stats = fsSync.lstatSync(filePath);
    let targetDir = filePath;

    // If it's a file, use its parent directory
    if (stats.isFile()) {
      targetDir = path.dirname(filePath);
    }

    // Add to recent directories
    addRecentDir(targetDir);

    // Create new window for the directory
    const newWindow = await createChat(app, { dir: targetDir });

    // Focus the new window
    if (newWindow) {
      newWindow.show();
      newWindow.focus();
      newWindow.moveTop();
    }
  } catch (error) {
    console.error('Failed to handle file open:', error);

    // Show user-friendly error notification
    new Notification({
      title: 'Goose',
      body: `Could not open directory: ${path.basename(filePath)}`,
    }).show();
  }
}

declare var MAIN_WINDOW_VITE_DEV_SERVER_URL: string;
declare var MAIN_WINDOW_VITE_NAME: string;

function getAppUrl(): URL {
  return MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL)
    : pathToFileURL(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));
}

// Parse command line arguments
const parseArgs = () => {
  let dirPath = null;

  // Remove first two elements in dev mode (electron and script path)
  const args = !dirPath && app.isPackaged ? process.argv : process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--dir' && i + 1 < args.length) {
      dirPath = args[i + 1];
      break;
    }
  }

  return { dirPath };
};

interface BundledConfig {
  defaultProvider?: string;
  defaultModel?: string;
  predefinedModels?: string;
  baseUrlShare?: string;
  version?: string;
}

const getBundledConfig = (): BundledConfig => {
  //{env-macro-start}//
  //needed when goose is bundled for a specific provider
  //{env-macro-end}//
  return {
    defaultProvider: process.env.GOOSE_DEFAULT_PROVIDER,
    defaultModel: process.env.GOOSE_DEFAULT_MODEL,
    predefinedModels: process.env.GOOSE_PREDEFINED_MODELS,
    baseUrlShare: process.env.GOOSE_BASE_URL_SHARE,
    version: process.env.GOOSE_VERSION,
  };
};

const { defaultProvider, defaultModel, predefinedModels, baseUrlShare, version } =
  getBundledConfig();

const resolveGoosePathRoot = (): string | undefined => {
  const pathRoot = process.env.GOOSE_PATH_ROOT?.trim();
  if (pathRoot) {
    return expandTilde(pathRoot);
  }
  return undefined;
};

const GENERATED_SECRET = crypto.randomBytes(32).toString('hex');

const getServerSecret = (settings: Settings): string => {
  if (settings.externalGoosed?.enabled && settings.externalGoosed.secret) {
    return settings.externalGoosed.secret;
  }
  if (process.env.GOOSE_EXTERNAL_BACKEND) {
    if (!process.env.GOOSE_SERVER__SECRET_KEY) {
      throw new Error(
        'GOOSE_SERVER__SECRET_KEY must be set when using GOOSE_EXTERNAL_BACKEND. ' +
          'Set it to the same value on both the server and the desktop client.'
      );
    }
    return process.env.GOOSE_SERVER__SECRET_KEY;
  }
  return GENERATED_SECRET;
};

let appConfig = {
  GOOSE_DEFAULT_PROVIDER: defaultProvider,
  GOOSE_DEFAULT_MODEL: defaultModel,
  GOOSE_PREDEFINED_MODELS: predefinedModels,
  GOOSE_API_HOST: 'https://localhost',
  GOOSE_PATH_ROOT: resolveGoosePathRoot(),
  GOOSE_WORKING_DIR: '',
  GOOSE_LOCALE: process.env.GOOSE_LOCALE || undefined,
  // If GOOSE_ALLOWLIST_WARNING env var is not set, defaults to false (strict blocking mode)
  GOOSE_ALLOWLIST_WARNING: process.env.GOOSE_ALLOWLIST_WARNING === 'true',
};

const windowMap = new Map<number, BrowserWindow>();
const goosedClients = new Map<number, Client>();
const appWindows = new Map<string, BrowserWindow>();

const windowPowerSaveBlockers = new Map<number, number>(); // windowId -> blockerId
// Track pending initial messages per window
const pendingInitialMessages = new Map<number, string>(); // windowId -> initialMessage

interface CreateChatOptions {
  initialMessage?: string;
  dir?: string;
  resumeSessionId?: string;
  viewType?: string;
  recipeDeeplink?: string;
  recipeId?: string;
  scheduledJobId?: string;
  recipeParameters?: Record<string, string>;
}

const createChat = async (app: App, options: CreateChatOptions = {}) => {
  const {
    initialMessage,
    dir,
    resumeSessionId,
    viewType,
    recipeDeeplink,
    recipeId,
    scheduledJobId,
    recipeParameters,
  } = options;
  const settings = getSettings();
  const serverSecret = getServerSecret(settings);

  // Update the cached trusted-external-hostname so the TLS handlers allow
  // connections to the configured remote backend.
  if (settings.externalGoosed?.enabled && settings.externalGoosed.url) {
    try {
      trustedExternalHostname = new URL(settings.externalGoosed.url).hostname;
    } catch {
      trustedExternalHostname = null;
    }
  } else {
    trustedExternalHostname = null;
  }

  // If the user provided a cert fingerprint for the external backend, pin it
  // directly (skips TOFU). Otherwise reset so the first handshake pins via TOFU.
  if (settings.externalGoosed?.enabled && settings.externalGoosed.certFingerprint) {
    pinnedCertFingerprint = normalizeFingerprint(settings.externalGoosed.certFingerprint);
  } else {
    pinnedCertFingerprint = null;
  }

  const goosedResult = await startGoosed({
    serverSecret,
    dir: dir || os.homedir(),
    env: {
      GOOSE_PATH_ROOT: appConfig.GOOSE_PATH_ROOT as string | undefined,
    },
    externalGoosed: settings.externalGoosed,
    isPackaged: app.isPackaged,
    resourcesPath: app.isPackaged ? process.resourcesPath : undefined,
    logger: log,
  });

  // For locally-spawned goosed, pin using the fingerprint from stdout.
  // For external backends the TOFU path in the cert handlers will pin
  // the fingerprint on the first successful TLS handshake.
  if (goosedResult.certFingerprint) {
    pinnedCertFingerprint = goosedResult.certFingerprint;
  }

  app.on('will-quit', async () => {
    log.info('App quitting, terminating goosed server');
    await goosedResult.cleanup();
  });

  const {
    baseUrl,
    workingDir,
    process: goosedProcess,
    errorLog,
    stopErrorLogCollection,
  } = goosedResult;

  const mainWindowState = windowStateKeeper({
    defaultWidth: 940,
    defaultHeight: 800,
  });

  const mainWindow = new BrowserWindow({
    titleBarStyle: process.platform === 'darwin' ? 'hidden' : 'default',
    trafficLightPosition: process.platform === 'darwin' ? { x: 20, y: 16 } : undefined,
    vibrancy: process.platform === 'darwin' ? 'window' : undefined,
    frame: process.platform !== 'darwin',
    x: mainWindowState.x,
    y: mainWindowState.y,
    width: mainWindowState.width,
    height: mainWindowState.height,
    minWidth: 450,
    resizable: true,
    useContentSize: true,
    icon: path.join(__dirname, '../images/icon.icns'),
    webPreferences: {
      spellcheck: settings.spellcheckEnabled ?? true,
      preload: path.join(__dirname, 'preload.js'),
      webSecurity: true,
      nodeIntegration: false,
      contextIsolation: true,
      additionalArguments: [
        JSON.stringify({
          ...appConfig,
          GOOSE_API_HOST: baseUrl,
          GOOSE_WORKING_DIR: workingDir,
          REQUEST_DIR: dir,
          GOOSE_BASE_URL_SHARE: baseUrlShare,
          GOOSE_VERSION: version,
          recipeDeeplink: recipeDeeplink,
          recipeId: recipeId,
          recipeParameters: recipeParameters,
          scheduledJobId: scheduledJobId,
          SECURITY_ML_MODEL_MAPPING: process.env.SECURITY_ML_MODEL_MAPPING,
        }),
      ],
      partition: 'persist:goose',
    },
  });

  if (!app.isPackaged) {
    installExtension(REACT_DEVELOPER_TOOLS, {
      loadExtensionOptions: { allowFileAccess: true },
      session: mainWindow.webContents.session,
    })
      .then(() => log.info('added react dev tools'))
      .catch((err) => log.info('failed to install react dev tools:', err));
  }

  // Re-create the client with Electron's net.fetch so requests to the local
  // self-signed HTTPS server go through the session's certificate handling.
  const goosedClient = createClient(
    createConfig({
      baseUrl,
      fetch: net.fetch as unknown as typeof globalThis.fetch,
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': serverSecret,
      },
    })
  );
  goosedClients.set(mainWindow.id, goosedClient);

  const serverReady = await checkServerStatus(goosedClient, errorLog);
  if (!serverReady) {
    const isUsingExternalBackend = settings.externalGoosed?.enabled;

    if (isUsingExternalBackend) {
      const response = dialog.showMessageBoxSync({
        type: 'error',
        title: 'External Backend Unreachable',
        message: `Could not connect to external backend at ${settings.externalGoosed?.url}`,
        detail: 'The external goosed server may not be running.',
        buttons: ['Disable External Backend & Retry', 'Quit'],
        defaultId: 0,
        cancelId: 1,
      });

      if (response === 0) {
        updateSettings((s) => {
          if (s.externalGoosed) {
            s.externalGoosed.enabled = false;
          }
        });
        mainWindow.destroy();
        return createChat(app, { initialMessage, dir });
      }
    } else {
      dialog.showMessageBoxSync({
        type: 'error',
        title: 'Goose Failed to Start',
        message: 'The backend server failed to start.',
        detail: errorLog.join('\n'),
        buttons: ['OK'],
      });
    }
    app.quit();
  }

  // errorLog is only needed during startup to detect fatal errors.
  // Stop collecting stderr to avoid unbounded memory growth over long sessions.
  stopErrorLogCollection();
  errorLog.length = 0;

  // Nudge the user if mesh is their provider but isn't running.
  // Delay to let the renderer mount before sending the IPC event.
  setTimeout(() => {
    mesh.checkProviderRunning(goosedClient).then((ok) => {
      if (!ok && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('mesh-not-running');
      }
    }).catch(() => {});
  }, 5000);

  // Let windowStateKeeper manage the window
  mainWindowState.manage(mainWindow);

  mainWindow.webContents.session.setSpellCheckerLanguages(['en-US', 'en-GB']);
  mainWindow.webContents.on('context-menu', (_event, params) => {
    const menu = new Menu();
    const hasSpellingSuggestions = params.dictionarySuggestions.length > 0 || params.misspelledWord;

    if (hasSpellingSuggestions) {
      for (const suggestion of params.dictionarySuggestions) {
        menu.append(
          new MenuItem({
            label: suggestion,
            click: () => mainWindow.webContents.replaceMisspelling(suggestion),
          })
        );
      }

      if (params.misspelledWord) {
        menu.append(
          new MenuItem({
            label: 'Add to dictionary',
            click: () =>
              mainWindow.webContents.session.addWordToSpellCheckerDictionary(params.misspelledWord),
          })
        );
      }

      if (params.selectionText) {
        menu.append(new MenuItem({ type: 'separator' }));
      }
    }
    if (params.selectionText) {
      menu.append(
        new MenuItem({
          label: 'Cut',
          accelerator: 'CmdOrCtrl+X',
          role: 'cut',
        })
      );
      menu.append(
        new MenuItem({
          label: 'Copy',
          accelerator: 'CmdOrCtrl+C',
          role: 'copy',
        })
      );
    }

    // Only show paste in editable fields (text inputs)
    if (params.isEditable) {
      menu.append(
        new MenuItem({
          label: 'Paste',
          accelerator: 'CmdOrCtrl+V',
          role: 'paste',
        })
      );
    }

    if (menu.items.length > 0) {
      menu.popup();
    }
  });

  // Handle new window creation for links (fallback for any links not handled by onClick)
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    try {
      const protocol = new URL(url).protocol;
      if (BLOCKED_PROTOCOLS.includes(protocol)) {
        return { action: 'deny' };
      }
    } catch {
      return { action: 'deny' };
    }

    shell.openExternal(url);
    return { action: 'deny' };
  });

  // Handle new-window events (alternative approach for external links)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('new-window' as any, function (event: any, url: string) {
    event.preventDefault();
    try {
      const protocol = new URL(url).protocol;
      if (BLOCKED_PROTOCOLS.includes(protocol)) {
        return;
      }
    } catch {
      return;
    }
    shell.openExternal(url);
  });

  const windowId = mainWindow.id;
  const url = getAppUrl();

  let appPath = '/';
  const routeMap: Record<string, string> = {
    chat: '/',
    pair: '/pair',
    settings: '/settings',
    sessions: '/sessions',
    schedules: '/schedules',
    recipes: '/recipes',
    skills: '/skills',
    permission: '/permission',
    ConfigureProviders: '/configure-providers',
    sharedSession: '/shared-session',
  };

  if (viewType) {
    appPath = routeMap[viewType] || '/';
  }
  if (
    appPath === '/' &&
    (recipeDeeplink !== undefined || recipeId !== undefined || initialMessage)
  ) {
    appPath = '/pair';
  }

  let searchParams = new URLSearchParams();
  if (resumeSessionId) {
    searchParams.set('resumeSessionId', resumeSessionId);
    if (appPath === '/') {
      appPath = '/pair';
    }
  }

  // Goose's react app uses HashRouter, so the path + search params follow a #/
  url.hash = `${appPath}?${searchParams.toString()}`;
  let formattedUrl = formatUrl(url);
  log.info('Opening URL: ', formattedUrl);
  mainWindow.loadURL(formattedUrl);

  // If we have an initial message, store it to send after React is ready
  if (initialMessage) {
    pendingInitialMessages.set(mainWindow.id, initialMessage);
  }

  // Set up local keyboard shortcuts that only work when the window is focused
  mainWindow.webContents.on('before-input-event', (event, input) => {
    if (input.key === 'r' && input.meta) {
      mainWindow.reload();
      event.preventDefault();
    }

    if (input.key === 'i' && input.alt && input.meta) {
      mainWindow.webContents.openDevTools();
      event.preventDefault();
    }
  });

  mainWindow.on('app-command', (e, cmd) => {
    if (cmd === 'browser-backward') {
      mainWindow.webContents.send('mouse-back-button-clicked');
      e.preventDefault();
    }
  });

  // Handle mouse back button (button 3)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('mouse-up' as any, function (_event: any, mouseButton: number) {
    // MouseButton 3 is the back button.
    if (mouseButton === 3) {
      mainWindow.webContents.send('mouse-back-button-clicked');
    }
  });

  windowMap.set(windowId, mainWindow);

  // Handle window closure
  mainWindow.on('closed', () => {
    windowMap.delete(windowId);

    // Clean up pending initial message
    pendingInitialMessages.delete(windowId);

    if (windowPowerSaveBlockers.has(windowId)) {
      const blockerId = windowPowerSaveBlockers.get(windowId)!;
      try {
        powerSaveBlocker.stop(blockerId);
        console.log(
          `[Main] Stopped power save blocker ${blockerId} for closing window ${windowId}`
        );
      } catch (error) {
        console.error(
          `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
          error
        );
      }
      windowPowerSaveBlockers.delete(windowId);
    }

    if (goosedProcess && typeof goosedProcess === 'object' && 'kill' in goosedProcess) {
      goosedProcess.kill();
    }
  });
  return mainWindow;
};

let activeLauncherWindow: BrowserWindow | null = null;

const createLauncher = () => {
  if (activeLauncherWindow && !activeLauncherWindow.isDestroyed()) {
    activeLauncherWindow.focus();
    return activeLauncherWindow;
  }

  const launcherWindow = new BrowserWindow({
    width: 600,
    height: 80,
    frame: false,
    transparent: process.platform === 'darwin',
    backgroundColor: process.platform === 'darwin' ? '#00000000' : '#ffffff',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      additionalArguments: [JSON.stringify(appConfig)],
      partition: 'persist:goose',
    },
    skipTaskbar: true,
    alwaysOnTop: true,
    resizable: false,
    movable: true,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    hasShadow: true,
    vibrancy: process.platform === 'darwin' ? 'window' : undefined,
  });

  // Center on screen
  const primaryDisplay = screen.getPrimaryDisplay();
  const { width, height } = primaryDisplay.workAreaSize;
  const windowBounds = launcherWindow.getBounds();

  launcherWindow.setPosition(
    Math.round(width / 2 - windowBounds.width / 2),
    Math.round(height / 3 - windowBounds.height / 2)
  );

  // Load launcher window content
  const url = getAppUrl();

  url.hash = '/launcher';
  launcherWindow.loadURL(formatUrl(url));
  activeLauncherWindow = launcherWindow;

  launcherWindow.on('closed', () => {
    activeLauncherWindow = null;
  });

  // Destroy window when it loses focus
  launcherWindow.on('blur', () => {
    launcherWindow.destroy();
  });

  // Also destroy on escape key
  launcherWindow.webContents.on('before-input-event', (event, input) => {
    if (input.key === 'Escape') {
      launcherWindow.destroy();
      event.preventDefault();
    }
  });

  return launcherWindow;
};

// Track tray instance
let tray: Tray | null = null;

const destroyTray = () => {
  if (tray) {
    tray.destroy();
    tray = null;
  }
};

const disableTray = () => {
  updateSettings((s) => {
    s.showMenuBarIcon = false;
  });
};

const createTray = () => {
  destroyTray();

  const possiblePaths = [
    path.join(process.resourcesPath, 'images', 'iconTemplate.png'),
    path.join(process.cwd(), 'src', 'images', 'iconTemplate.png'),
    path.join(__dirname, '..', 'images', 'iconTemplate.png'),
    path.join(__dirname, 'images', 'iconTemplate.png'),
    path.join(process.cwd(), 'images', 'iconTemplate.png'),
  ];

  const iconPath = possiblePaths.find((p) => fsSync.existsSync(p));

  if (!iconPath) {
    console.warn('[Main] Tray icon not found. App will continue without system tray.');
    disableTray();
    return;
  }

  try {
    tray = new Tray(iconPath);
    setTrayRef(tray);
    updateTrayMenu(getUpdateAvailable());

    if (process.platform === 'win32') {
      tray.on('click', showWindow);
    }
  } catch (error) {
    console.error('[Main] Tray creation failed. App will continue without system tray.', error);
    disableTray();
    tray = null;
  }
};

const showWindow = async () => {
  const windows = BrowserWindow.getAllWindows();

  if (windows.length === 0) {
    log.info('No windows are open, creating a new one...');
    const recentDirs = loadRecentDirs();
    const openDir = recentDirs.length > 0 ? recentDirs[0] : null;
    await createChat(app, { dir: openDir || undefined });
    return;
  }

  const initialOffsetX = 30;
  const initialOffsetY = 30;

  // Iterate over all windows
  windows.forEach((win, index) => {
    const currentBounds = win.getBounds();
    const newX = currentBounds.x + initialOffsetX * index;
    const newY = currentBounds.y + initialOffsetY * index;

    win.setBounds({
      x: newX,
      y: newY,
      width: currentBounds.width,
      height: currentBounds.height,
    });

    if (!win.isVisible()) {
      win.show();
    }

    win.focus();
  });
};

const buildRecentFilesMenu = () => {
  const recentDirs = loadRecentDirs();
  return recentDirs.map((dir) => ({
    label: dir,
    click: async () => {
      await createChat(app, { dir });
    },
  }));
};

const openDirectoryDialog = async (): Promise<OpenDialogReturnValue> => {
  // Get the current working directory from the focused window
  let defaultPath: string | undefined;
  const currentWindow = BrowserWindow.getFocusedWindow();

  if (currentWindow) {
    try {
      const currentWorkingDir = await currentWindow.webContents.executeJavaScript(
        `window.appConfig ? window.appConfig.get('GOOSE_WORKING_DIR') : null`
      );

      if (currentWorkingDir && typeof currentWorkingDir === 'string') {
        // Verify the directory exists before using it as default
        try {
          const stats = fsSync.lstatSync(currentWorkingDir);
          if (stats.isDirectory()) {
            defaultPath = currentWorkingDir;
          }
        } catch (error) {
          if (error && typeof error === 'object' && 'code' in error) {
            const fsError = error as { code?: string; message?: string };
            if (
              fsError.code === 'ENOENT' ||
              fsError.code === 'EACCES' ||
              fsError.code === 'EPERM'
            ) {
              console.warn(
                `Current working directory not accessible (${fsError.code}): ${currentWorkingDir}, falling back to home directory`
              );
              defaultPath = os.homedir();
            } else {
              console.warn(
                `Unexpected filesystem error (${fsError.code}) for directory ${currentWorkingDir}:`,
                fsError.message
              );
              defaultPath = os.homedir();
            }
          } else {
            console.warn(`Unexpected error checking directory ${currentWorkingDir}:`, error);
            defaultPath = os.homedir();
          }
        }
      }
    } catch (error) {
      console.warn('Failed to get current working directory from window:', error);
    }
  }

  if (!defaultPath) {
    defaultPath = os.homedir();
  }

  const result = (await dialog.showOpenDialog({
    properties: ['openFile', 'openDirectory', 'createDirectory'],
    defaultPath: defaultPath,
  })) as unknown as OpenDialogReturnValue;

  if (!result.canceled && result.filePaths.length > 0) {
    const selectedPath = result.filePaths[0];

    // If a file was selected, use its parent directory
    let dirToAdd = selectedPath;
    try {
      const stats = fsSync.lstatSync(selectedPath);

      // Reject symlinks for security
      if (stats.isSymbolicLink()) {
        console.warn(`Selected path is a symlink, using parent directory for security`);
        dirToAdd = path.dirname(selectedPath);
      } else if (stats.isFile()) {
        dirToAdd = path.dirname(selectedPath);
      }
    } catch {
      console.warn(`Could not stat selected path, using parent directory`);
      dirToAdd = path.dirname(selectedPath); // Fallback to parent directory
    }

    addRecentDir(dirToAdd);

    let deeplinkData: RecipeDeeplinkData | undefined = undefined;
    if (windowDeeplinkURL) {
      deeplinkData = parseRecipeDeeplink(windowDeeplinkURL);
    }
    await createChat(app, {
      dir: dirToAdd,
      recipeDeeplink: deeplinkData?.config,
      recipeParameters: deeplinkData?.parameters,
    });
  }
  return result;
};

interface RecipeDeeplinkData {
  config: string;
  parameters?: Record<string, string>;
}

function parseRecipeDeeplink(url: string): RecipeDeeplinkData | undefined {
  const parsedUrl = new URL(url);
  let recipeDeeplink = parsedUrl.searchParams.get('config');
  if (recipeDeeplink && !url.includes(recipeDeeplink)) {
    // URLSearchParams decodes + as space, which can break encoded configs
    // Parse raw query to preserve "+" characters in values like config
    const search = parsedUrl.search || '';
    const configMatch = search.match(/(?:[?&])config=([^&]*)/);
    let recipeDeeplinkTmp = configMatch ? configMatch[1] : null;
    if (recipeDeeplinkTmp) {
      try {
        recipeDeeplink = decodeURIComponent(recipeDeeplinkTmp);
      } catch (error) {
        console.error('[Main] parseRecipeDeeplink - Failed to decode:', errorMessage(error));
        return undefined;
      }
    }
  }
  if (!recipeDeeplink) {
    return undefined;
  }

  // Extract all query parameters except 'config' and 'scheduledJob' as recipe parameters
  // Use raw query string parsing to preserve '+' characters (consistent with config handling)
  const parameters: Record<string, string> = {};
  const search = parsedUrl.search || '';
  const paramMatches = search.matchAll(/[?&]([^=&]+)=([^&]*)/g);

  for (const match of paramMatches) {
    const key = match[1];
    const rawValue = match[2];

    if (key !== 'config' && key !== 'scheduledJob') {
      try {
        parameters[key] = decodeURIComponent(rawValue);
      } catch {
        // If decoding fails, use raw value
        parameters[key] = rawValue;
      }
    }
  }

  return {
    config: recipeDeeplink,
    parameters: Object.keys(parameters).length > 0 ? parameters : undefined,
  };
}

// Global error handler
const handleFatalError = (error: Error) => {
  const windows = BrowserWindow.getAllWindows();
  windows.forEach((win) => {
    win.webContents.send('fatal-error', error.message || 'An unexpected error occurred');
  });
};

process.on('uncaughtException', (error) => {
  console.error('Uncaught Exception:', formatErrorForLogging(error));
  handleFatalError(error);
});

process.on('unhandledRejection', (error) => {
  console.error('Unhandled Rejection:', formatErrorForLogging(error));
  handleFatalError(error instanceof Error ? error : new Error(String(error)));
});

ipcMain.on('react-ready', (event) => {
  log.info('React ready event received');

  // Get the window that sent the react-ready event
  const window = BrowserWindow.fromWebContents(event.sender);
  const windowId = window?.id;

  // Send any pending initial message for this window
  if (windowId && pendingInitialMessages.has(windowId)) {
    const initialMessage = pendingInitialMessages.get(windowId)!;
    log.info('Sending pending initial message to window:', initialMessage);
    window.webContents.send('set-initial-message', initialMessage);
    pendingInitialMessages.delete(windowId);
  }

  if (pendingDeepLink && window) {
    log.info('Processing pending deep link:', pendingDeepLink);
    try {
      const parsedUrl = new URL(pendingDeepLink);
      if (parsedUrl.hostname === 'extension') {
        log.info('Sending add-extension IPC to ready window');
        window.webContents.send('add-extension', pendingDeepLink);
      } else if (parsedUrl.hostname === 'sessions') {
        log.info('Sending open-shared-session IPC to ready window');
        window.webContents.send('open-shared-session', pendingDeepLink);
      }
      pendingDeepLink = null;
    } catch (error) {
      log.error('Error processing pending deep link:', error);
      pendingDeepLink = null;
    }
  } else {
    log.info('No pending deep link to process');
  }

  log.info('React ready - window is prepared for deep links');
});

ipcMain.handle('open-external', async (_event, url: string) => {
  const parsedUrl = new URL(url);

  if (BLOCKED_PROTOCOLS.includes(parsedUrl.protocol)) {
    console.warn(`[Main] Blocked dangerous protocol: ${parsedUrl.protocol}`);
    return;
  }

  await shell.openExternal(url);
});

ipcMain.handle('directory-chooser', async () => {
  return dialog.showOpenDialog({
    properties: ['openDirectory', 'createDirectory'],
    defaultPath: os.homedir(),
  });
});

ipcMain.handle('add-recent-dir', (_event, dir: string) => {
  if (dir) {
    addRecentDir(dir);
  }
});

ipcMain.handle('get-setting', (_event, key: SettingKey) => {
  const settings = getSettings();
  return settings[key];
});

// Valid setting keys for runtime validation
const validSettingKeys: Set<string> = new Set([
  'showMenuBarIcon',
  'showDockIcon',
  'enableWakelock',
  'spellcheckEnabled',
  'externalGoosed',
  'globalShortcut',
  'keyboardShortcuts',
  'theme',
  'useSystemTheme',
  'responseStyle',
  'showPricing',
  'sessionSharing',
  'seenAnnouncementIds',
  'navExpandedWidth',
]);

ipcMain.handle('set-setting', (_event, key: SettingKey, value: unknown) => {
  // Validate key at runtime to prevent prototype pollution
  if (!validSettingKeys.has(key)) {
    console.error(`Invalid setting key rejected: ${key}`);
    return;
  }

  const settings = getSettings();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (settings as any)[key] = value;
  fsSync.writeFileSync(SETTINGS_FILE, JSON.stringify(settings, null, 2));

  // Re-register shortcuts if keyboard shortcuts changed
  if (key === 'keyboardShortcuts') {
    registerGlobalShortcuts();
  }
});

ipcMain.handle('get-secret-key', () => {
  const settings = getSettings();
  return getServerSecret(settings);
});

ipcMain.handle('get-goosed-host-port', async (event) => {
  const windowId = BrowserWindow.fromWebContents(event.sender)?.id;
  if (!windowId) {
    return null;
  }
  const client = goosedClients.get(windowId);
  if (!client) {
    return null;
  }
  return client.getConfig().baseUrl || null;
});

// Handle menu bar icon visibility
ipcMain.handle('set-menu-bar-icon', async (_event, show: boolean) => {
  updateSettings((s) => {
    s.showMenuBarIcon = show;
  });

  if (show) {
    createTray();
  } else {
    destroyTray();
  }
  return true;
});

ipcMain.handle('get-menu-bar-icon-state', () => {
  try {
    const settings = getSettings();
    return settings.showMenuBarIcon ?? true;
  } catch (error) {
    console.error('Error getting menu bar icon state:', error);
    return true;
  }
});

// Handle dock icon visibility (macOS only)
ipcMain.handle('set-dock-icon', async (_event, show: boolean) => {
  if (process.platform !== 'darwin') return false;

  const settings = getSettings();
  updateSettings((s) => {
    s.showDockIcon = show;
  });

  if (show) {
    app.dock?.show();
  } else {
    // Only hide the dock if we have a menu bar icon to maintain accessibility
    if (settings.showMenuBarIcon) {
      app.dock?.hide();
      setTimeout(() => {
        focusWindow();
      }, 50);
    }
  }
  return true;
});

ipcMain.handle('get-dock-icon-state', () => {
  try {
    if (process.platform !== 'darwin') return true;
    const settings = getSettings();
    return settings.showDockIcon ?? true;
  } catch (error) {
    console.error('Error getting dock icon state:', error);
    return true;
  }
});

// Handle opening system notifications preferences
ipcMain.handle('open-notifications-settings', async () => {
  try {
    if (process.platform === 'darwin') {
      spawn('open', ['x-apple.systempreferences:com.apple.preference.notifications']);
      return true;
    } else if (process.platform === 'win32') {
      // Windows: Open notification settings in Settings app
      spawn('ms-settings:notifications', { shell: true });
      return true;
    } else if (process.platform === 'linux') {
      // Linux: Try different desktop environments
      function canSpawn(cmd: string): boolean {
        try {
          execFileSync('which', [cmd], { stdio: 'ignore' });
          return true;
        } catch {
          return false;
        }
      }

      // GNOME
      if (canSpawn('gnome-control-center')) {
        spawn('gnome-control-center', ['notifications']);
        return true;
      }

      // KDE Plasma
      if (canSpawn('systemsettings5')) {
        spawn('systemsettings5', ['kcm_notifications']);
        return true;
      }

      // XFCE
      if (canSpawn('xfce4-settings-manager')) {
        spawn('xfce4-settings-manager', ['--socket-id=notifications']);
        return true;
      }

      console.warn('Could not find a suitable settings application for Linux');
      return false;
    } else {
      console.warn(
        `Opening notification settings is not supported on platform: ${process.platform}`
      );
      return false;
    }
  } catch (error) {
    console.error('Error opening notification settings:', error);
    return false;
  }
});

// Handle wakelock setting
ipcMain.handle('set-wakelock', async (_event, enable: boolean) => {
  updateSettings((s) => {
    s.enableWakelock = enable;
  });

  // Stop all existing power save blockers when disabling the setting
  if (!enable) {
    for (const [windowId, blockerId] of windowPowerSaveBlockers.entries()) {
      try {
        powerSaveBlocker.stop(blockerId);
        console.log(
          `[Main] Stopped power save blocker ${blockerId} for window ${windowId} due to wakelock setting disabled`
        );
      } catch (error) {
        console.error(
          `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
          error
        );
      }
    }
    windowPowerSaveBlockers.clear();
  }

  return true;
});

ipcMain.handle('get-wakelock-state', () => {
  try {
    const settings = getSettings();
    return settings.enableWakelock ?? false;
  } catch (error) {
    console.error('Error getting wakelock state:', error);
    return false;
  }
});

ipcMain.handle('set-spellcheck', async (_event, enable: boolean) => {
  updateSettings((s) => {
    s.spellcheckEnabled = enable;
  });
  return true;
});

ipcMain.handle('get-spellcheck-state', () => {
  try {
    const settings = getSettings();
    return settings.spellcheckEnabled ?? true;
  } catch (error) {
    console.error('Error getting spellcheck state:', error);
    return true;
  }
});

// Add file/directory selection handler
ipcMain.handle('select-file-or-directory', async (_event, defaultPath?: string) => {
  const dialogOptions: OpenDialogOptions = {
    properties: process.platform === 'darwin' ? ['openFile', 'openDirectory'] : ['openFile'],
  };

  // Set default path if provided
  if (defaultPath) {
    // Expand tilde to home directory
    const expandedPath = expandTilde(defaultPath);

    // Check if the path exists
    try {
      const stats = await fs.stat(expandedPath);
      if (stats.isDirectory()) {
        dialogOptions.defaultPath = expandedPath;
      } else {
        dialogOptions.defaultPath = path.dirname(expandedPath);
      }
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
    } catch (error) {
      // If path doesn't exist, fall back to home directory and log error
      console.error(`Default path does not exist: ${expandedPath}, falling back to home directory`);
      dialogOptions.defaultPath = os.homedir();
    }
  }

  const result = (await dialog.showOpenDialog(dialogOptions)) as unknown as OpenDialogReturnValue;

  if (!result.canceled && result.filePaths.length > 0) {
    return result.filePaths[0];
  }
  return null;
});

// ── Mesh-LLM lifecycle (see mesh.ts) ────────────────────────────────

ipcMain.handle('check-mesh', () => mesh.check());
ipcMain.handle('start-mesh', (_event, args: string[]) => mesh.start(args));
ipcMain.handle('stop-mesh', () => mesh.stop());

ipcMain.handle('check-ollama', async () => {
  try {
    return new Promise((resolve) => {
      // Run `ps` and filter for "ollama"
      const ps = spawn('ps', ['aux']);
      const grep = spawn('grep', ['-iw', '[o]llama']);

      let output = '';
      let errorOutput = '';

      // Pipe ps output to grep
      ps.stdout.pipe(grep.stdin);

      grep.stdout.on('data', (data) => {
        output += data.toString();
      });

      grep.stderr.on('data', (data) => {
        errorOutput += data.toString();
      });

      grep.on('close', (code) => {
        if (code !== null && code !== 0 && code !== 1) {
          // grep returns 1 when no matches found
          console.error('Error executing grep command:', errorOutput);
          return resolve(false);
        }

        const trimmedOutput = output.trim();

        const isRunning = trimmedOutput.length > 0;
        resolve(isRunning);
      });

      ps.on('error', (error) => {
        console.error('Error executing ps command:', error);
        resolve(false);
      });

      grep.on('error', (error) => {
        console.error('Error executing grep command:', error);
        resolve(false);
      });

      // Close ps stdin when done
      ps.stdout.on('end', () => {
        grep.stdin.end();
      });
    });
  } catch (err) {
    console.error('Error checking for Ollama:', err);
    return false;
  }
});

ipcMain.handle('read-file', async (_event, filePath) => {
  try {
    const expandedPath = expandTilde(filePath);
    if (process.platform === 'win32') {
      const buffer = await fs.readFile(expandedPath);
      return { file: buffer.toString('utf8'), filePath: expandedPath, error: null, found: true };
    }
    // Non-Windows: keep previous behavior via cat for parity
    return await new Promise((resolve) => {
      const cat = spawn('cat', [expandedPath]);
      let output = '';
      let errorOutput = '';

      cat.stdout.on('data', (data) => {
        output += data.toString();
      });

      cat.stderr.on('data', (data) => {
        errorOutput += data.toString();
      });

      cat.on('close', (code) => {
        if (code !== 0) {
          resolve({ file: '', filePath: expandedPath, error: errorOutput || null, found: false });
          return;
        }
        resolve({ file: output, filePath: expandedPath, error: null, found: true });
      });

      cat.on('error', (error) => {
        console.error('Error reading file:', error);
        resolve({ file: '', filePath: expandedPath, error, found: false });
      });
    });
  } catch (error) {
    console.error('Error reading file:', error);
    return { file: '', filePath: expandTilde(filePath), error, found: false };
  }
});

ipcMain.handle('write-file', async (_event, filePath, content) => {
  try {
    // Expand tilde to home directory
    const expandedPath = expandTilde(filePath);
    await fs.writeFile(expandedPath, content, { encoding: 'utf8' });
    return true;
  } catch (error) {
    console.error('Error writing to file:', error);
    return false;
  }
});

// Enhanced file operations
ipcMain.handle('ensure-directory', async (_event, dirPath) => {
  try {
    // Expand tilde to home directory
    const expandedPath = expandTilde(dirPath);

    await fs.mkdir(expandedPath, { recursive: true });
    return true;
  } catch (error) {
    console.error('Error creating directory:', error);
    return false;
  }
});

ipcMain.handle('list-files', async (_event, dirPath, extension) => {
  try {
    // Expand tilde to home directory
    const expandedPath = expandTilde(dirPath);

    const files = await fs.readdir(expandedPath);
    if (extension) {
      return files.filter((file) => file.endsWith(extension));
    }
    return files;
  } catch (error) {
    console.error('Error listing files:', error);
    return [];
  }
});

ipcMain.handle('show-message-box', async (_event, options) => {
  return dialog.showMessageBox(options);
});

ipcMain.handle('show-save-dialog', async (_event, options) => {
  return dialog.showSaveDialog(options);
});

ipcMain.handle('get-allowed-extensions', async () => {
  return await getAllowList();
});

const createNewWindow = async (app: App, dir?: string | null) => {
  const recentDirs = loadRecentDirs();
  const openDir = dir || (recentDirs.length > 0 ? recentDirs[0] : undefined);
  return await createChat(app, { dir: openDir });
};

const focusWindow = () => {
  const windows = BrowserWindow.getAllWindows();
  if (windows.length > 0) {
    windows.forEach((win) => {
      win.show();
    });
    windows[windows.length - 1].webContents.send('focus-input');
  } else {
    createNewWindow(app);
  }
};

const registerGlobalShortcuts = () => {
  globalShortcut.unregisterAll();

  const settings = getSettings();
  const shortcuts = getKeyboardShortcuts(settings);

  if (shortcuts.focusWindow) {
    try {
      globalShortcut.register(shortcuts.focusWindow, () => {
        focusWindow();
      });
    } catch (e) {
      console.error('Error registering focus window hotkey:', e);
    }
  }

  if (shortcuts.quickLauncher) {
    try {
      globalShortcut.register(shortcuts.quickLauncher, () => {
        createLauncher();
      });
    } catch (e) {
      console.error('Error registering launcher hotkey:', e);
    }
  }
};

async function appMain() {
  await configureProxy();

  // Ensure Windows shims are available before any MCP processes are spawned
  await ensureWinShims();

  registerUpdateIpcHandlers();

  // Handle microphone permission requests
  session.defaultSession.setPermissionRequestHandler((_webContents, permission, callback) => {
    console.log('Permission requested:', permission);
    // Allow microphone and media access
    if (permission === 'media') {
      callback(true);
    } else {
      // Default behavior for other permissions
      callback(true);
    }
  });

  // Add CSP headers to all sessions — recomputed on every response so that
  // changes to externalGoosed settings take effect without restarting the app.
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    const currentSettings = getSettings();
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': buildCSP(currentSettings.externalGoosed),
      },
    });
  });

  // Migrate old settings format if needed (one-time migration)
  const settings = getSettings();
  if (!settings.keyboardShortcuts && settings.globalShortcut !== undefined) {
    updateSettings((s) => {
      s.keyboardShortcuts = getKeyboardShortcuts(s);
      delete s.globalShortcut;
    });
  }

  // Register global shortcuts based on settings
  registerGlobalShortcuts();

  session.defaultSession.webRequest.onBeforeSendHeaders((details, callback) => {
    details.requestHeaders['Origin'] = 'http://localhost:5173';
    callback({ cancel: false, requestHeaders: details.requestHeaders });
  });

  if (settings.showMenuBarIcon) {
    createTray();
  }

  if (process.platform === 'darwin' && !settings.showDockIcon && settings.showMenuBarIcon) {
    app.dock?.hide();
  }

  const { dirPath } = parseArgs();

  if (!openUrlHandledLaunch) {
    await createNewWindow(app, dirPath);
  } else {
    log.info('[Main] Skipping window creation in appMain - open-url already handled launch');
  }

  // Setup auto-updater AFTER window is created and displayed (with delay to avoid blocking)
  setTimeout(() => {
    if (shouldSetupUpdater()) {
      log.info('Setting up auto-updater after window creation...');
      try {
        setupAutoUpdater();
      } catch (error) {
        log.error('Error setting up auto-updater:', error);
      }
    }
  }, 2000);

  if (process.platform === 'darwin') {
    const dockMenu = Menu.buildFromTemplate([
      {
        label: 'New Window',
        click: () => {
          createNewWindow(app);
        },
      },
    ]);
    app.dock?.setMenu(dockMenu);
  }

  const menu = Menu.getApplicationMenu();

  const shortcuts = getKeyboardShortcuts(settings);

  const appMenu = menu?.items.find((item) => item.label === 'Goose');
  if (appMenu?.submenu) {
    appMenu.submenu.insert(1, new MenuItem({ type: 'separator' }));
    if (shortcuts.settings) {
      appMenu.submenu.insert(
        1,
        new MenuItem({
          label: 'Settings',
          accelerator: shortcuts.settings,
          click() {
            const focusedWindow = BrowserWindow.getFocusedWindow();
            if (focusedWindow) focusedWindow.webContents.send('set-view', 'settings');
          },
        })
      );
    }
    appMenu.submenu.insert(1, new MenuItem({ type: 'separator' }));
  }

  const editMenu = menu?.items.find((item) => item.label === 'Edit');
  if (editMenu?.submenu) {
    const selectAllIndex = editMenu.submenu.items.findIndex((item) => item.label === 'Select All');

    const findSubmenu = Menu.buildFromTemplate([
      {
        label: 'Find…',
        accelerator: shortcuts.find || undefined,
        click() {
          const focusedWindow = BrowserWindow.getFocusedWindow();
          if (focusedWindow) focusedWindow.webContents.send('find-command');
        },
      },
      {
        label: 'Find Next',
        accelerator: shortcuts.findNext || undefined,
        click() {
          const focusedWindow = BrowserWindow.getFocusedWindow();
          if (focusedWindow) focusedWindow.webContents.send('find-next');
        },
      },
      {
        label: 'Find Previous',
        accelerator: shortcuts.findPrevious || undefined,
        click() {
          const focusedWindow = BrowserWindow.getFocusedWindow();
          if (focusedWindow) focusedWindow.webContents.send('find-previous');
        },
      },
      {
        label: 'Use Selection for Find',
        accelerator: process.platform === 'darwin' ? 'Command+E' : undefined,
        click() {
          const focusedWindow = BrowserWindow.getFocusedWindow();
          if (focusedWindow) focusedWindow.webContents.send('use-selection-find');
        },
        visible: process.platform === 'darwin', // Only show on Mac
      },
    ]);

    editMenu.submenu.insert(
      selectAllIndex + 1,
      new MenuItem({
        label: 'Find',
        submenu: findSubmenu,
      })
    );
  }

  const fileMenu = menu?.items.find((item) => item.label === 'File');

  if (fileMenu?.submenu) {
    // Use a counter to track the actual insertion index
    let menuIndex = 0;

    if (shortcuts.newChat) {
      fileMenu.submenu.insert(
        menuIndex++,
        new MenuItem({
          label: 'New Chat',
          accelerator: shortcuts.newChat,
          click() {
            const focusedWindow = BrowserWindow.getFocusedWindow();
            if (focusedWindow) focusedWindow.webContents.send('new-chat');
          },
        })
      );
    }

    if (shortcuts.newChatWindow) {
      fileMenu.submenu.insert(
        menuIndex++,
        new MenuItem({
          label: 'New Chat Window',
          accelerator: shortcuts.newChatWindow,
          click() {
            ipcMain.emit('create-chat-window');
          },
        })
      );
    }

    if (shortcuts.openDirectory) {
      fileMenu.submenu.insert(
        menuIndex++,
        new MenuItem({
          label: 'Open Directory...',
          accelerator: shortcuts.openDirectory,
          click: () => openDirectoryDialog(),
        })
      );
    }

    const recentFilesSubmenu = buildRecentFilesMenu();
    if (recentFilesSubmenu.length > 0) {
      fileMenu.submenu.insert(
        menuIndex++,
        new MenuItem({
          label: 'Recent Directories',
          submenu: recentFilesSubmenu,
        })
      );
    }

    fileMenu.submenu.insert(menuIndex++, new MenuItem({ type: 'separator' }));

    if (shortcuts.focusWindow) {
      fileMenu.submenu.append(
        new MenuItem({
          label: 'Focus Goose Window',
          accelerator: shortcuts.focusWindow,
          click() {
            focusWindow();
          },
        })
      );
    }

    if (shortcuts.quickLauncher) {
      fileMenu.submenu.append(
        new MenuItem({
          label: 'Quick Launcher',
          accelerator: shortcuts.quickLauncher,
          click() {
            createLauncher();
          },
        })
      );
    }
  }

  if (menu) {
    let windowMenu = menu.items.find((item) => item.label === 'Window');

    if (!windowMenu) {
      windowMenu = new MenuItem({
        label: 'Window',
        submenu: Menu.buildFromTemplate([]),
      });

      const helpMenuIndex = menu.items.findIndex((item) => item.label === 'Help');
      if (helpMenuIndex >= 0) {
        menu.items.splice(helpMenuIndex, 0, windowMenu);
      } else {
        menu.items.push(windowMenu);
      }
    }

    if (windowMenu.submenu) {
      if (shortcuts.alwaysOnTop) {
        windowMenu.submenu.append(
          new MenuItem({
            label: 'Always on Top',
            type: 'checkbox',
            accelerator: shortcuts.alwaysOnTop,
            click(menuItem) {
              const focusedWindow = BrowserWindow.getFocusedWindow();
              if (focusedWindow) {
                const isAlwaysOnTop = menuItem.checked;

                if (process.platform === 'darwin') {
                  focusedWindow.setAlwaysOnTop(isAlwaysOnTop, 'floating');
                } else {
                  focusedWindow.setAlwaysOnTop(isAlwaysOnTop);
                }

                console.log(
                  `[Main] Set always-on-top to ${isAlwaysOnTop} for window ${focusedWindow.id}`
                );
              }
            },
          })
        );
      }
    }

    const viewMenu = menu.items.find((item) => item.label === 'View');
    if (viewMenu?.submenu && shortcuts.toggleNavigation) {
      viewMenu.submenu.append(new MenuItem({ type: 'separator' }));
      viewMenu.submenu.append(
        new MenuItem({
          label: 'Toggle Navigation',
          accelerator: shortcuts.toggleNavigation,
          click() {
            const focusedWindow = BrowserWindow.getFocusedWindow();
            if (focusedWindow) {
              focusedWindow.webContents.send('toggle-navigation');
            }
          },
        })
      );
    }
  }

  // on macOS, the topbar is hidden
  if (menu && process.platform !== 'darwin') {
    let helpMenu = menu.items.find((item) => item.label === 'Help');

    // If Help menu doesn't exist, create it and add it to the menu
    if (!helpMenu) {
      helpMenu = new MenuItem({
        label: 'Help',
        submenu: Menu.buildFromTemplate([]), // Start with an empty submenu
      });
      // Find a reasonable place to insert the Help menu, usually near the end
      const insertIndex = menu.items.length > 0 ? menu.items.length - 1 : 0;
      menu.items.splice(insertIndex, 0, helpMenu);
    }

    // Ensure the Help menu has a submenu before appending
    if (helpMenu.submenu) {
      // Add a separator before the About item if the submenu is not empty
      if (helpMenu.submenu.items.length > 0) {
        helpMenu.submenu.append(new MenuItem({ type: 'separator' }));
      }

      // Create the About Goose menu item with a submenu
      const aboutGooseMenuItem = new MenuItem({
        label: 'About Goose',
        submenu: Menu.buildFromTemplate([]), // Start with an empty submenu for About
      });

      // Add the Version menu item (display only) to the About Goose submenu
      if (aboutGooseMenuItem.submenu) {
        aboutGooseMenuItem.submenu.append(
          new MenuItem({
            label: `Version ${version || app.getVersion()}`,
            enabled: false,
          })
        );
      }

      helpMenu.submenu.append(aboutGooseMenuItem);
    }
  }

  if (menu) {
    Menu.setApplicationMenu(menu);
  }

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createNewWindow(app);
    }
  });

  ipcMain.on('create-chat-window', (event, options = {}) => {
    const { query, dir, resumeSessionId, viewType, recipeId } = options;

    let resolvedDir = dir;
    if (!resolvedDir?.trim()) {
      const recentDirs = loadRecentDirs();
      resolvedDir = recentDirs.length > 0 ? recentDirs[0] : undefined;
    }

    const isFromLauncher = query && !resumeSessionId && !viewType && !recipeId;

    if (isFromLauncher) {
      const senderWindow = BrowserWindow.fromWebContents(event.sender);
      const launcherWindowId = senderWindow?.id;
      const allWindows = BrowserWindow.getAllWindows();

      const existingWindows = allWindows.filter(
        (win) => !win.isDestroyed() && win.id !== launcherWindowId
      );

      if (existingWindows.length > 0) {
        const targetWindow = existingWindows[0];
        targetWindow.show();
        targetWindow.focus();
        targetWindow.webContents.send('set-initial-message', query);
        return;
      }
    }

    createChat(app, {
      initialMessage: query,
      dir: resolvedDir,
      resumeSessionId,
      viewType,
      recipeId,
    });
  });

  ipcMain.on('close-window', (event) => {
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window && !window.isDestroyed()) {
      window.close();
    }
  });

  ipcMain.on('notify', (event, data) => {
    try {
      // Validate notification data
      if (!data || typeof data !== 'object') {
        console.error('Invalid notification data');
        return;
      }

      // Validate title and body
      if (typeof data.title !== 'string' || typeof data.body !== 'string') {
        console.error('Invalid notification title or body');
        return;
      }

      // Limit the length of title and body
      const MAX_LENGTH = 1000;
      if (data.title.length > MAX_LENGTH || data.body.length > MAX_LENGTH) {
        console.error('Notification title or body too long');
        return;
      }

      // Remove any HTML tags for security
      const sanitizeText = (text: string) => text.replace(/<[^>]*>/g, '');

      const notification = new Notification({
        title: sanitizeText(data.title),
        body: sanitizeText(data.body),
      });

      // Add click handler to focus the window
      notification.on('click', () => {
        const window = BrowserWindow.fromWebContents(event.sender);
        if (window) {
          if (window.isMinimized()) {
            window.restore();
          }
          window.show();
          window.focus();
        }
      });

      notification.show();
    } catch (error) {
      console.error('Error showing notification:', error);
    }
  });

  ipcMain.on('logInfo', (_event, info) => {
    try {
      // Validate log info
      if (info === undefined || info === null) {
        console.error('Invalid log info: undefined or null');
        return;
      }

      // Convert to string if not already
      const logMessage = String(info);

      // Limit log message length
      const MAX_LENGTH = 10000; // 10KB limit
      if (logMessage.length > MAX_LENGTH) {
        console.error('Log message too long');
        return;
      }

      // Log the sanitized message
      log.info('from renderer:', logMessage);
    } catch (error) {
      console.error('Error logging info:', error);
    }
  });

  ipcMain.on('broadcast-theme-change', (event, themeData) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender);
    const allWindows = BrowserWindow.getAllWindows();

    allWindows.forEach((window) => {
      if (window.id !== senderWindow?.id) {
        window.webContents.send('theme-changed', themeData);
      }
    });
  });

  ipcMain.on('reload-app', (event) => {
    // Get the window that sent the event
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window) {
      window.reload();
    }
  });

  // Handle metadata fetching from main process
  ipcMain.handle('fetch-metadata', async (_event, url) => {
    try {
      // Validate URL
      const parsedUrl = new URL(url);

      // Only allow http and https protocols for fetching web content
      if (!WEB_PROTOCOLS.includes(parsedUrl.protocol)) {
        throw new Error('Invalid URL protocol. Only HTTP and HTTPS are allowed.');
      }

      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (compatible; Goose/1.0)',
        },
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      // Set a reasonable size limit (e.g., 10MB)
      const MAX_SIZE = 10 * 1024 * 1024; // 10MB
      const contentLength = parseInt(response.headers.get('content-length') || '0');
      if (contentLength > MAX_SIZE) {
        throw new Error('Response too large');
      }

      const text = await response.text();
      if (text.length > MAX_SIZE) {
        throw new Error('Response too large');
      }

      return text;
    } catch (error) {
      console.error('Error fetching metadata:', error);
      throw error;
    }
  });

  ipcMain.on('open-in-chrome', (_event, url) => {
    try {
      // Validate URL
      const parsedUrl = new URL(url);

      // Only allow http and https protocols for browser URLs
      if (!WEB_PROTOCOLS.includes(parsedUrl.protocol)) {
        console.error('Invalid URL protocol. Only HTTP and HTTPS are allowed.');
        return;
      }

      // On macOS, use the 'open' command with Chrome
      if (process.platform === 'darwin') {
        spawn('open', ['-a', 'Google Chrome', url]);
      } else if (process.platform === 'win32') {
        // On Windows, start is built-in command of cmd.exe
        spawn('cmd.exe', ['/c', 'start', '', 'chrome', url]);
      } else {
        // On Linux, use xdg-open with chrome
        spawn('xdg-open', [url]);
      }
    } catch (error) {
      console.error('Error opening URL in browser:', error);
    }
  });

  // Handle app restart
  ipcMain.on('restart-app', () => {
    app.relaunch();
    app.exit(0);
  });

  // Handler for getting app version
  ipcMain.on('get-app-version', (event) => {
    event.returnValue = app.getVersion();
  });

  ipcMain.handle('open-directory-in-explorer', async (_event, path: string) => {
    try {
      return !!(await shell.openPath(path));
    } catch (error) {
      console.error('Error opening directory in explorer:', error);
      return false;
    }
  });

  ipcMain.handle('launch-app', async (event, gooseApp: GooseApp) => {
    try {
      const launchingWindow = BrowserWindow.fromWebContents(event.sender);
      if (!launchingWindow) {
        throw new Error('Could not find launching window');
      }

      const launchingWindowId = launchingWindow.id;
      const launchingClient = goosedClients.get(launchingWindowId);
      if (!launchingClient) {
        throw new Error('No client found for launching window');
      }

      const appWindow = new BrowserWindow({
        title: formatAppName(gooseApp.name),
        width: gooseApp.width ?? 800,
        height: gooseApp.height ?? 600,
        resizable: gooseApp.resizable ?? true,
        useContentSize: true,
        webPreferences: {
          preload: path.join(__dirname, 'preload.js'),
          nodeIntegration: false,
          contextIsolation: true,
          webSecurity: true,
          partition: 'persist:goose',
        },
      });

      goosedClients.set(appWindow.id, launchingClient);
      appWindows.set(gooseApp.name, appWindow);

      appWindow.on('close', () => {
        goosedClients.delete(appWindow.id);
        appWindows.delete(gooseApp.name);
      });

      const workingDir = app.getPath('home');
      const extensionName = gooseApp.mcpServers?.[0] ?? '';

      const url = getAppUrl();

      const searchParams = new URLSearchParams();
      searchParams.set('resourceUri', gooseApp.uri);
      searchParams.set('extensionName', extensionName);
      searchParams.set('appName', gooseApp.name);
      searchParams.set('workingDir', workingDir);

      url.hash = `/standalone-app?${searchParams.toString()}`;
      await appWindow.loadURL(formatUrl(url));
      appWindow.show();
    } catch (error) {
      console.error('Failed to launch app:', error);
      throw error;
    }
  });

  ipcMain.handle('refresh-app', async (_event, gooseApp: GooseApp) => {
    try {
      const appWindow = appWindows.get(gooseApp.name);
      if (!appWindow || appWindow.isDestroyed()) {
        console.log(`App window for '${gooseApp.name}' not found or destroyed, skipping refresh`);
        return;
      }

      // Bring to front first
      if (appWindow.isMinimized()) {
        appWindow.restore();
      }
      appWindow.show();
      appWindow.focus();

      // Then reload
      await appWindow.webContents.reload();
    } catch (error) {
      console.error('Failed to refresh app:', error);
      throw error;
    }
  });

  ipcMain.handle('close-app', async (_event, appName: string) => {
    try {
      const appWindow = appWindows.get(appName);
      if (!appWindow || appWindow.isDestroyed()) {
        console.log(`App window for '${appName}' not found or destroyed, skipping close`);
        return;
      }

      appWindow.close();
    } catch (error) {
      console.error('Failed to close app:', error);
      throw error;
    }
  });
}

app.whenReady().then(async () => {
  try {
    await appMain();
  } catch (error) {
    dialog.showErrorBox('Goose Error', `Failed to create main window: ${error}`);
    app.quit();
  }
});

async function getAllowList(): Promise<string[]> {
  if (!process.env.GOOSE_ALLOWLIST) {
    return [];
  }

  const response = await fetch(process.env.GOOSE_ALLOWLIST);

  if (!response.ok) {
    throw new Error(
      `Failed to fetch allowed extensions: ${response.status} ${response.statusText}`
    );
  }

  // Parse the YAML content
  const yamlContent = await response.text();
  const parsedYaml = yaml.parse(yamlContent);

  // Extract the commands from the extensions array
  if (parsedYaml && parsedYaml.extensions && Array.isArray(parsedYaml.extensions)) {
    const commands = parsedYaml.extensions.map(
      (ext: { id: string; command: string }) => ext.command
    );
    console.log(`Fetched ${commands.length} allowed extension commands`);
    return commands;
  } else {
    console.error('Invalid YAML structure:', parsedYaml);
    return [];
  }
}

app.on('will-quit', async () => {
  // Stop the mesh child process if we spawned one.
  mesh.cleanup();

  for (const [windowId, blockerId] of windowPowerSaveBlockers.entries()) {
    try {
      powerSaveBlocker.stop(blockerId);
      console.log(
        `[Main] Stopped power save blocker ${blockerId} for window ${windowId} during app quit`
      );
    } catch (error) {
      console.error(
        `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
        error
      );
    }
  }
  windowPowerSaveBlockers.clear();

  globalShortcut.unregisterAll();
});

app.on('window-all-closed', () => {
  // Only quit if we're not on macOS or don't have a tray icon
  if (process.platform !== 'darwin' || !tray) {
    app.quit();
  }
});
