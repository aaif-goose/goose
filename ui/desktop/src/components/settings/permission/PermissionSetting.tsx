import React, { useCallback, useEffect, useState } from 'react';
import { ScrollArea } from '../../ui/scroll-area';
import BackButton from '../../ui/BackButton';
import { FixedExtensionEntry, useConfig } from '../../ConfigContext';
import { ChevronRight } from 'lucide-react';
import PermissionModal from './PermissionModal';
import { Button } from '../../ui/button';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  permissionRules: {
    id: 'permissionSetting.permissionRules',
    defaultMessage: 'Permission Rules',
  },
  permissionRulesDescription: {
    id: 'permissionSetting.permissionRulesDescription',
    defaultMessage:
      'Hidden instructions that will be passed to the provider to help direct and add context to your responses.',
  },
  extensionRules: {
    id: 'permissionSetting.extensionRules',
    defaultMessage: 'Extension rules',
  },
});

function RuleItem({ title, description }: { title: string; description: string }) {
  const [isModalOpen, setIsModalOpen] = useState(false);

  const handleModalClose = () => {
    setIsModalOpen(false);
  };

  return (
    <>
      <Button
        className="flex items-center gap-2 w-full justify-between"
        onClick={() => setIsModalOpen(true)}
        variant="secondary"
        size="lg"
      >
        <div>
          <h3 className="font-semibold text-text-primary">{title}</h3>
          <p className="text-xs text-text-secondary mt-1">{description}</p>
        </div>
        <ChevronRight className="w-4 h-4 text-iconStandard" />
        {/* Modal for updating tool permission */}
      </Button>
      {isModalOpen && <PermissionModal onClose={handleModalClose} extensionName={title} />}
    </>
  );
}

function RulesSection({ title, rules }: { title: string; rules: React.ReactNode }) {
  return (
    <div className="space-y-4">
      <h2 className="text-xl font-medium text-text-primary">{title}</h2>
      {rules}
    </div>
  );
}

export default function PermissionSettingsView({ onClose }: { onClose: () => void }) {
  const intl = useIntl();
  const { getExtensions } = useConfig();
  const [extensions, setExtensions] = useState<FixedExtensionEntry[]>([]);

  const fetchExtensions = useCallback(async () => {
    const extensionsList = await getExtensions(true); // Force refresh
    // Filter out disabled extensions
    const enabledExtensions = extensionsList.filter((extension) => extension.enabled);
    // TODO(Douwe): this should really be a real extension:
    enabledExtensions.push({
      name: 'platform',
      type: 'builtin',
      description: 'platform',
      enabled: true,
    });
    // Sort extensions by name to maintain consistent order
    const sortedExtensions = [...enabledExtensions].sort((a, b) => {
      // First sort by builtin
      if (a.type === 'builtin' && b.type !== 'builtin') return -1;
      if (a.type !== 'builtin' && b.type === 'builtin') return 1;

      // Then sort by bundled (handle null/undefined cases)
      const aBundled = 'bundled' in a && a.bundled === true;
      const bBundled = 'bundled' in b && b.bundled === true;
      if (aBundled && !bBundled) return -1;
      if (!aBundled && bBundled) return 1;

      // Finally sort alphabetically within each group
      return a.name.localeCompare(b.name);
    });
    setExtensions(sortedExtensions);
  }, [getExtensions]);

  useEffect(() => {
    fetchExtensions();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="bg-background-primary h-screen w-full animate-[fadein_200ms_ease-in_forwards]">
      <ScrollArea className="h-full w-full">
        <div className="flex flex-col pb-24">
          <div className="px-8 pt-6 pb-4">
            <BackButton onClick={() => onClose()} className="mb-4" />
            <div className="rounded-full bg-background-inverse w-16 h-16 flex items-center justify-center mb-4">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="24"
                height="24"
                viewBox="0 0 24 24"
                className="stroke-text-inverse fill-background-inverse"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4" />
                <path d="m21 2-9.6 9.6" />
                <circle cx="7.5" cy="15.5" r="5.5" />
              </svg>
            </div>
            <h1 className="text-3xl font-medium text-text-primary mt-4">{intl.formatMessage(i18n.permissionRules)}</h1>
            <p className="text-text-secondary">
              {intl.formatMessage(i18n.permissionRulesDescription)}
            </p>
          </div>

          {/* Content Area */}
          <div className="flex-1 pt-[20px]">
            <div className="space-y-8 px-8">
              {/* Extension Rules Section */}
              <RulesSection
                title={intl.formatMessage(i18n.extensionRules)}
                rules={
                  <>
                    {extensions.map((extension) => (
                      <RuleItem
                        key={extension.name}
                        title={extension.name}
                        description={'description' in extension ? extension.description || '' : ''}
                      />
                    ))}
                  </>
                }
              />
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}
