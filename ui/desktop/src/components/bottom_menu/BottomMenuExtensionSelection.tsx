import { AppEvents } from '../../constants/events';
import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Puzzle } from 'lucide-react';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import { FixedExtensionEntry, useConfig } from '../ConfigContext';
import { toastService } from '../../toasts';
import { formatExtensionName } from '../settings/extensions/subcomponents/ExtensionList';
import { nameToKey } from '../settings/extensions/utils';
import { ExtensionConfig, getSessionExtensions } from '../../api';
import { addToAgent, removeFromAgent } from '../settings/extensions/agent-api';
import {
  setExtensionOverride,
  getExtensionOverride,
  getExtensionOverrides,
} from '../../store/extensionOverrides';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  manageExtensions: {
    id: 'bottomMenuExtensionSelection.manageExtensions',
    defaultMessage: 'manage extensions',
  },
  searchExtensions: {
    id: 'bottomMenuExtensionSelection.searchExtensions',
    defaultMessage: 'search extensions...',
  },
  extensionsForNewChats: {
    id: 'bottomMenuExtensionSelection.extensionsForNewChats',
    defaultMessage: 'Extensions for new chats',
  },
  extensionsForThisSession: {
    id: 'bottomMenuExtensionSelection.extensionsForThisSession',
    defaultMessage: 'Extensions for this chat session',
  },
  noExtensionsFound: {
    id: 'bottomMenuExtensionSelection.noExtensionsFound',
    defaultMessage: 'no extensions found',
  },
  noExtensionsAvailable: {
    id: 'bottomMenuExtensionSelection.noExtensionsAvailable',
    defaultMessage: 'no extensions available',
  },
  extensionUpdated: {
    id: 'bottomMenuExtensionSelection.extensionUpdated',
    defaultMessage: 'Extension Updated',
  },
  extensionWillBeEnabled: {
    id: 'bottomMenuExtensionSelection.extensionWillBeEnabled',
    defaultMessage: '{name} will be enabled in new chats',
  },
  extensionWillBeDisabled: {
    id: 'bottomMenuExtensionSelection.extensionWillBeDisabled',
    defaultMessage: '{name} will be disabled in new chats',
  },
  extensionToggleError: {
    id: 'bottomMenuExtensionSelection.extensionToggleError',
    defaultMessage: 'Extension Toggle Error',
  },
  noActiveSession: {
    id: 'bottomMenuExtensionSelection.noActiveSession',
    defaultMessage: 'No active session found. Please start a chat session first.',
  },
});

interface BottomMenuExtensionSelectionProps {
  sessionId: string | null;
}

export const BottomMenuExtensionSelection = ({ sessionId }: BottomMenuExtensionSelectionProps) => {
  const intl = useIntl();
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [sessionExtensions, setSessionExtensions] = useState<ExtensionConfig[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [pendingSort, setPendingSort] = useState(false);
  const [togglingExtension, setTogglingExtension] = useState<string | null>(null);
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const [isSessionExtensionsLoaded, setIsSessionExtensionsLoaded] = useState(false);
  const sortTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { extensionsList: allExtensions } = useConfig();
  const isHubView = !sessionId;

  useEffect(() => {
    setIsSessionExtensionsLoaded(false);
    setSessionExtensions([]);
  }, [sessionId]);

  useEffect(() => {
    const handleExtensionsLoaded = () => {
      setRefreshTrigger((prev) => prev + 1);
    };

    window.addEventListener(AppEvents.SESSION_EXTENSIONS_LOADED, handleExtensionsLoaded);

    return () => {
      window.removeEventListener(AppEvents.SESSION_EXTENSIONS_LOADED, handleExtensionsLoaded);
    };
  }, []);

  useEffect(() => {
    return () => {
      if (sortTimeoutRef.current) {
        clearTimeout(sortTimeoutRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (refreshTrigger === 0 && !isOpen) {
      return;
    }

    const fetchExtensions = async () => {
      if (!sessionId) {
        return;
      }

      try {
        const response = await getSessionExtensions({
          path: { session_id: sessionId },
        });

        if (response.data?.extensions) {
          setSessionExtensions(response.data.extensions);
          setIsSessionExtensionsLoaded(true);
        }
      } catch (error) {
        console.error('Failed to fetch session extensions:', error);
        setIsSessionExtensionsLoaded(true);
      }
    };

    fetchExtensions();
  }, [sessionId, isOpen, refreshTrigger]);

  const handleToggle = useCallback(
    async (extensionConfig: FixedExtensionEntry) => {
      if (togglingExtension === extensionConfig.name) {
        return;
      }

      setIsTransitioning(true);
      setTogglingExtension(extensionConfig.name);

      if (isHubView) {
        const currentState = getExtensionOverride(extensionConfig.name) ?? extensionConfig.enabled;
        setExtensionOverride(extensionConfig.name, !currentState);
        setPendingSort(true);

        if (sortTimeoutRef.current) {
          clearTimeout(sortTimeoutRef.current);
        }

        // Delay the re-sort to allow animation
        sortTimeoutRef.current = setTimeout(() => {
          setHubUpdateTrigger((prev) => prev + 1);
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingExtension(null);
        }, 800);

        toastService.success({
          title: intl.formatMessage(i18n.extensionUpdated),
          msg: intl.formatMessage(
            !currentState ? i18n.extensionWillBeEnabled : i18n.extensionWillBeDisabled,
            { name: formatExtensionName(extensionConfig.name) }
          ),
        });
        return;
      }

      if (!sessionId) {
        setIsTransitioning(false);
        setTogglingExtension(null);
        toastService.error({
          title: intl.formatMessage(i18n.extensionToggleError),
          msg: intl.formatMessage(i18n.noActiveSession),
          traceback: 'No session ID available',
        });
        return;
      }

      try {
        if (extensionConfig.enabled) {
          await removeFromAgent(extensionConfig.name, sessionId, true);
        } else {
          await addToAgent(extensionConfig, sessionId, true);
        }

        setPendingSort(true);

        if (sortTimeoutRef.current) {
          clearTimeout(sortTimeoutRef.current);
        }

        sortTimeoutRef.current = setTimeout(async () => {
          const response = await getSessionExtensions({
            path: { session_id: sessionId },
          });

          if (response.data?.extensions) {
            setSessionExtensions(response.data.extensions);
          }
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingExtension(null);
        }, 800);
      } catch {
        setIsTransitioning(false);
        setPendingSort(false);
        setTogglingExtension(null);
      }
    },
    [sessionId, isHubView, togglingExtension, intl]
  );

  // Merge all available extensions with session-specific or hub override state
  const extensionsList = useMemo(() => {
    const hubOverrides = getExtensionOverrides();

    if (isHubView) {
      return allExtensions.map(
        (ext) =>
          ({
            ...ext,
            enabled: hubOverrides.has(ext.name) ? hubOverrides.get(ext.name)! : ext.enabled,
          }) as FixedExtensionEntry
      );
    }

    const sessionExtensionKeys = new Set(sessionExtensions.map((ext) => nameToKey(ext.name)));
    const globalExtensionKeys = new Set(allExtensions.map((ext) => nameToKey(ext.name)));

    const mergedExtensions = allExtensions.map(
      (ext) =>
        ({
          ...ext,
          enabled: sessionExtensionKeys.has(nameToKey(ext.name)),
        }) as FixedExtensionEntry
    );

    for (const sessionExtension of sessionExtensions) {
      if (globalExtensionKeys.has(nameToKey(sessionExtension.name))) {
        continue;
      }

      mergedExtensions.push({
        ...sessionExtension,
        enabled: true,
      });
    }

    return mergedExtensions;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allExtensions, sessionExtensions, isHubView, hubUpdateTrigger]);

  const filteredExtensions = useMemo(() => {
    return extensionsList.filter((ext) => {
      const query = searchQuery.toLowerCase();
      return (
        ext.name.toLowerCase().includes(query) ||
        (ext.description && ext.description.toLowerCase().includes(query))
      );
    });
  }, [extensionsList, searchQuery]);

  const sortedExtensions = useMemo(() => {
    return [...filteredExtensions].sort((a, b) => {
      // Primary sort: enabled first
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;

      // Secondary sort: alphabetically by name
      return a.name.localeCompare(b.name);
    });
  }, [filteredExtensions]);

  const activeCount = useMemo(() => {
    return extensionsList.filter((ext) => ext.enabled).length;
  }, [extensionsList]);

  const shouldHideTrigger =
    extensionsList.length === 0 || (!isHubView && !isSessionExtensionsLoaded);

  return (
    <DropdownMenu
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) {
          setSearchQuery('');
          if (sortTimeoutRef.current) {
            clearTimeout(sortTimeoutRef.current);
          }
          setIsTransitioning(false);
          setPendingSort(false);
          setTogglingExtension(null);
        }
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          className={`flex items-center [&_svg]:size-4 text-text-primary/70 hover:text-text-primary hover:scale-100 hover:bg-transparent text-xs cursor-pointer ${shouldHideTrigger ? 'invisible' : ''}`}
          title={intl.formatMessage(i18n.manageExtensions)}
        >
          <Puzzle className="mr-1 h-4 w-4" />
          <span>{activeCount}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
        }}
      >
        <div className="p-2">
          <Input
            type="text"
            placeholder={intl.formatMessage(i18n.searchExtensions)}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-8 text-sm"
            autoFocus
          />
          <p className="text-xs text-text-primary/60 mt-1.5">
            {intl.formatMessage(
              isHubView ? i18n.extensionsForNewChats : i18n.extensionsForThisSession
            )}
          </p>
        </div>
        <div
          className={`max-h-[400px] overflow-y-auto transition-opacity duration-300 ${
            isTransitioning && pendingSort ? 'opacity-50' : 'opacity-100'
          }`}
        >
          {sortedExtensions.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-primary/70">
              {intl.formatMessage(
                searchQuery ? i18n.noExtensionsFound : i18n.noExtensionsAvailable
              )}
            </div>
          ) : (
            sortedExtensions.map((ext) => {
              const isToggling = togglingExtension === ext.name;
              return (
                <div
                  key={ext.name}
                  className={`flex items-center justify-between px-2 py-2 transition-all duration-300 ${
                    isToggling ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  onClick={() => !isToggling && handleToggle(ext)}
                  title={ext.description || ext.name}
                >
                  <div className="text-sm font-medium text-text-primary">
                    {formatExtensionName(ext.name)}
                  </div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Switch
                      checked={ext.enabled}
                      onCheckedChange={() => handleToggle(ext)}
                      variant="mono"
                      disabled={isToggling}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
