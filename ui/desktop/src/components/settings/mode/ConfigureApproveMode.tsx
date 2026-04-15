import React, { useEffect, useState } from 'react';
import { Card } from '../../ui/card';
import { Button } from '../../ui/button';
import { GooseMode, ModeSelectionItem } from './ModeSelectionItem';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'configureApproveMode.title',
    defaultMessage: 'Configure approve mode',
  },
  description: {
    id: 'configureApproveMode.description',
    defaultMessage: 'Approve requests can either be given to all tool requests or determine which actions may need integration',
  },
  manualApproval: {
    id: 'configureApproveMode.manualApproval',
    defaultMessage: 'Manual approval',
  },
  manualApprovalDescription: {
    id: 'configureApproveMode.manualApprovalDescription',
    defaultMessage: 'All tools, extensions and file modifications will require human approval',
  },
  smartApproval: {
    id: 'configureApproveMode.smartApproval',
    defaultMessage: 'Smart approval',
  },
  smartApprovalDescription: {
    id: 'configureApproveMode.smartApprovalDescription',
    defaultMessage: 'Intelligently determine which actions need approval based on risk level',
  },
  saving: {
    id: 'configureApproveMode.saving',
    defaultMessage: 'Saving...',
  },
  save: {
    id: 'configureApproveMode.save',
    defaultMessage: 'Save',
  },
  cancel: {
    id: 'configureApproveMode.cancel',
    defaultMessage: 'Cancel',
  },
});

interface ConfigureApproveModeProps {
  onClose: () => void;
  handleModeChange: (newMode: string) => void;
  currentMode: string | null;
}

export function ConfigureApproveMode({
  onClose,
  handleModeChange,
  currentMode,
}: ConfigureApproveModeProps) {
  const intl = useIntl();
  const approveModes: GooseMode[] = [
    {
      key: 'approve',
      labelDescriptor: i18n.manualApproval,
      descriptionDescriptor: i18n.manualApprovalDescription,
    },
    {
      key: 'smart_approve',
      labelDescriptor: i18n.smartApproval,
      descriptionDescriptor: i18n.smartApprovalDescription,
    },
  ];

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [approveMode, setApproveMode] = useState(currentMode);

  useEffect(() => {
    setApproveMode(currentMode);
  }, [currentMode]);

  const handleModeSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    setIsSubmitting(true);
    try {
      handleModeChange(approveMode || '');
      onClose();
    } catch (error) {
      console.error('Error configuring goose mode:', error);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/30">
      <Card className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[440px] bg-background-primary rounded-xl overflow-hidden p-[16px] pt-[24px] pb-0">
        <div className="px-4 pb-0 space-y-6">
          {/* Header */}
          <div className="flex">
            <h2 className="text-2xl font-regular text-text-primary">{intl.formatMessage(i18n.title)}</h2>
          </div>

          <div className="mt-[24px]">
            <p className="text-sm text-text-secondary mb-6">
              {intl.formatMessage(i18n.description)}
            </p>
            <div className="space-y-4">
              {approveModes.map((mode) => (
                <ModeSelectionItem
                  key={mode.key}
                  mode={mode}
                  showDescription={true}
                  currentMode={approveMode || ''}
                  isApproveModeConfigure={true}
                  handleModeChange={(newMode) => {
                    setApproveMode(newMode);
                  }}
                />
              ))}
            </div>
          </div>

          {/* Actions */}
          <div className="mt-[8px] ml-[-32px] mr-[-32px] pt-[16px]">
            <Button
              type="submit"
              variant="ghost"
              disabled={isSubmitting}
              onClick={handleModeSubmit}
              className="w-full h-[60px] rounded-none border-t border-border-primary hover:bg-background-secondary text-text-primary dark:border-gray-600 text-base font-regular"
            >
              {isSubmitting ? intl.formatMessage(i18n.saving) : intl.formatMessage(i18n.save)}
            </Button>
            <Button
              type="button"
              variant="ghost"
              disabled={isSubmitting}
              onClick={onClose}
              className="w-full h-[60px] rounded-none border-t border-border-primary text-text-secondary hover:bg-background-secondary dark:border-gray-600 text-base font-regular"
            >
              {intl.formatMessage(i18n.cancel)}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
