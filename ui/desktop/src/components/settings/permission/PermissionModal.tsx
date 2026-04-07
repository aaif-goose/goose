import { useEffect, useMemo, useState } from 'react';
import { Button } from '../../ui/button';
import { ChevronDownIcon, SlidersHorizontal, AlertCircle } from 'lucide-react';
import { getTools, PermissionLevel, ToolInfo, upsertPermissions } from '../../../api';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '../../ui/dialog';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '../../ui/dropdown-menu';
import { useChatContext } from '../../../contexts/ChatContext';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  alwaysAllow: {
    id: 'permissionModal.alwaysAllow',
    defaultMessage: 'Always allow',
  },
  askBefore: {
    id: 'permissionModal.askBefore',
    defaultMessage: 'Ask before',
  },
  neverAllow: {
    id: 'permissionModal.neverAllow',
    defaultMessage: 'Never allow',
  },
  noActiveSession: {
    id: 'permissionModal.noActiveSession',
    defaultMessage: 'No active session',
  },
  noActiveSessionDescription: {
    id: 'permissionModal.noActiveSessionDescription',
    defaultMessage:
      'Start a chat session first to configure tool permissions for this extension. Tool permissions are loaded from the active session\'s extensions.',
  },
  failedToLoadTools: {
    id: 'permissionModal.failedToLoadTools',
    defaultMessage: 'Failed to load tools',
  },
  failedToLoadToolsDescription: {
    id: 'permissionModal.failedToLoadToolsDescription',
    defaultMessage:
      'Could not load tools for this extension. The extension may not be loaded in the current session.',
  },
  noToolsAvailable: {
    id: 'permissionModal.noToolsAvailable',
    defaultMessage: 'No tools available for this extension.',
  },
  close: {
    id: 'permissionModal.close',
    defaultMessage: 'Close',
  },
  cancel: {
    id: 'permissionModal.cancel',
    defaultMessage: 'Cancel',
  },
  saveChanges: {
    id: 'permissionModal.saveChanges',
    defaultMessage: 'Save Changes',
  },
});

function getFirstSentence(text: string): string {
  const match = text.match(/^([^.?!]+[.?!])/);
  return match ? match[0] : '';
}

interface PermissionModalProps {
  extensionName: string;
  onClose: () => void;
}

export default function PermissionModal({ extensionName, onClose }: PermissionModalProps) {
  const intl = useIntl();

  const permissionOptions = [
    { value: 'always_allow', label: intl.formatMessage(i18n.alwaysAllow) },
    { value: 'ask_before', label: intl.formatMessage(i18n.askBefore) },
    { value: 'never_allow', label: intl.formatMessage(i18n.neverAllow) },
  ] as { value: PermissionLevel; label: string }[];

  const chatContext = useChatContext();
  const sessionId = chatContext?.chat.sessionId || '';

  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [updatedPermissions, setUpdatedPermissions] = useState<Record<string, string>>({});
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const hasChanges = useMemo(() => {
    return Object.keys(updatedPermissions).some(
      (toolName) =>
        updatedPermissions[toolName] !== tools.find((tool) => tool.name === toolName)?.permission
    );
  }, [updatedPermissions, tools]);

  useEffect(() => {
    const fetchTools = async () => {
      if (!sessionId) {
        setIsLoading(false);
        setLoadError('no_session');
        return;
      }

      setIsLoading(true);
      setLoadError(null);

      try {
        const response = await getTools({
          query: { extension_name: extensionName, session_id: sessionId },
        });
        if (response.error) {
          console.error('Failed to get tools:', response.error);
          setLoadError('fetch_failed');
        } else {
          const filteredTools = (response.data || []).filter(
            (tool: ToolInfo) =>
              tool.name !== 'platform__read_resource' && tool.name !== 'platform__list_resources'
          );
          setTools(filteredTools);
        }
      } catch (err) {
        console.error('Error fetching tools:', err);
        setLoadError('fetch_failed');
      } finally {
        setIsLoading(false);
      }
    };

    fetchTools();
  }, [extensionName, sessionId]);

  const handleSettingChange = (toolName: string, newPermission: PermissionLevel) => {
    setUpdatedPermissions((prev) => ({
      ...prev,
      [toolName]: newPermission,
    }));
  };

  const handleClose = () => {
    onClose();
  };

  const handleSave = async () => {
    try {
      const payload = {
        tool_permissions: Object.entries(updatedPermissions).map(([toolName, permission]) => ({
          tool_name: toolName,
          permission: permission as PermissionLevel,
        })),
      };

      if (payload.tool_permissions.length === 0) {
        onClose();
        return;
      }

      const response = await upsertPermissions({
        body: payload,
      });
      if (response.error) {
        console.error('Failed to save permissions:', response.error);
      } else {
        onClose();
      }
    } catch (err) {
      console.error('Error saving permissions:', err);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          handleClose();
        }
      }}
    >
      <DialogContent className="sm:max-w-[500px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <SlidersHorizontal className="text-iconStandard" size={24} />
            {extensionName}
          </DialogTitle>
        </DialogHeader>

        <div className="py-4">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <svg
                className="animate-spin h-8 w-8 text-grey-50 dark:text-white"
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                ></circle>
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"></path>
              </svg>
            </div>
          ) : loadError === 'no_session' ? (
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <AlertCircle className="h-12 w-12 text-text-secondary mb-4" />
              <p className="text-text-primary font-medium mb-2">{intl.formatMessage(i18n.noActiveSession)}</p>
              <p className="text-sm text-text-secondary max-w-sm">
                {intl.formatMessage(i18n.noActiveSessionDescription)}
              </p>
            </div>
          ) : loadError === 'fetch_failed' ? (
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <AlertCircle className="h-12 w-12 text-text-secondary mb-4" />
              <p className="text-text-primary font-medium mb-2">{intl.formatMessage(i18n.failedToLoadTools)}</p>
              <p className="text-sm text-text-secondary max-w-sm">
                {intl.formatMessage(i18n.failedToLoadToolsDescription)}
              </p>
            </div>
          ) : tools.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <p className="text-text-secondary">{intl.formatMessage(i18n.noToolsAvailable)}</p>
            </div>
          ) : (
            <div className="space-y-4">
              {tools.map((tool) => (
                <div
                  key={tool.name}
                  className="flex items-center justify-between grid grid-cols-12"
                >
                  <div className="flex flex-col col-span-8">
                    <label className="block text-sm font-medium text-text-primary">
                      {tool.name}
                    </label>
                    <p className="text-sm text-text-secondary mb-2">
                      {getFirstSentence(tool.description)}
                    </p>
                  </div>
                  <DropdownMenu>
                    <DropdownMenuTrigger className="col-span-4">
                      <Button className="w-full" variant="secondary" size="lg">
                        {permissionOptions.find(
                          (option) =>
                            option.value === (updatedPermissions[tool.name] || tool.permission)
                        )?.label || intl.formatMessage(i18n.askBefore)}
                        <ChevronDownIcon className="h-4 w-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent>
                      {permissionOptions.map((option) => (
                        <DropdownMenuItem
                          key={option.value}
                          onSelect={() =>
                            handleSettingChange(tool.name, option.value as PermissionLevel)
                          }
                        >
                          {option.label}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              ))}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            {loadError ? intl.formatMessage(i18n.close) : intl.formatMessage(i18n.cancel)}
          </Button>
          {!loadError && (
            <Button disabled={!hasChanges} onClick={handleSave}>
              {intl.formatMessage(i18n.saveChanges)}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
