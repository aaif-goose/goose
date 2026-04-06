import { SyntheticEvent } from 'react';
import { Button } from '../../../../ui/button';
import { Trash2, AlertTriangle } from 'lucide-react';
import { ConfigKey } from '../../../../../api';
import { defineMessages, useIntl } from '../../../../../i18n';

const i18n = defineMessages({
  cannotDeleteActive: {
    id: 'providerSetupActions.cannotDeleteActive',
    defaultMessage:
      'You cannot delete {providerName} while it\'s currently in use. Please switch to a different model before deleting this provider.',
  },
  ok: {
    id: 'providerSetupActions.ok',
    defaultMessage: 'Ok',
  },
  confirmDeleteMessage: {
    id: 'providerSetupActions.confirmDeleteMessage',
    defaultMessage:
      'Are you sure you want to delete the configuration parameters for {providerName}? This action cannot be undone.',
  },
  confirmDelete: {
    id: 'providerSetupActions.confirmDelete',
    defaultMessage: 'Confirm Delete',
  },
  cancel: {
    id: 'providerSetupActions.cancel',
    defaultMessage: 'Cancel',
  },
  deleteProvider: {
    id: 'providerSetupActions.deleteProvider',
    defaultMessage: 'Delete Provider',
  },
  submit: {
    id: 'providerSetupActions.submit',
    defaultMessage: 'Submit',
  },
  enableProvider: {
    id: 'providerSetupActions.enableProvider',
    defaultMessage: 'Enable Provider',
  },
});

interface ProviderSetupActionsProps {
  onCancel: () => void;
  onSubmit: (e: SyntheticEvent) => void;
  onDelete?: () => void;
  showDeleteConfirmation?: boolean;
  onConfirmDelete?: () => void;
  onCancelDelete?: () => void;
  canDelete?: boolean;
  providerName?: string;
  primaryParameters?: ConfigKey[];
  isActiveProvider?: boolean; // Made optional with default false
}

/**
 * Renders the action buttons at the bottom of the provider modal.
 * Includes submit, cancel, and delete functionality with confirmation.
 */
export default function ProviderSetupActions({
  onCancel,
  onSubmit,
  onDelete,
  showDeleteConfirmation,
  onConfirmDelete,
  onCancelDelete,
  canDelete,
  providerName,
  primaryParameters,
  isActiveProvider = false, // Default value provided
}: ProviderSetupActionsProps) {
  const intl = useIntl();

  // If we're showing delete confirmation, render the delete confirmation buttons
  if (showDeleteConfirmation) {
    // Check if this is the active provider
    if (isActiveProvider) {
      return (
        <div className="w-full">
          <div className="w-full px-6 py-4 bg-yellow-600/20 border-t border-yellow-500/30">
            <p className="text-yellow-500 text-sm mb-2 flex items-start">
              <AlertTriangle className="h-4 w-4 mr-2 mt-0.5 flex-shrink-0" />
              <span>
                {intl.formatMessage(i18n.cannotDeleteActive, { providerName })}
              </span>
            </p>
          </div>
          <Button
            variant="ghost"
            onClick={onCancelDelete}
            className="w-full h-[60px] rounded-none hover:bg-background-secondary text-text-secondary hover:text-text-primary text-md font-regular"
          >
            {intl.formatMessage(i18n.ok)}
          </Button>
        </div>
      );
    }

    // Normal delete confirmation
    return (
      <div className="w-full">
        <div className="w-full px-6 py-4 bg-red-900/20 border-t border-red-500/30">
          <p className="text-red-400 text-sm mb-2">
            {intl.formatMessage(i18n.confirmDeleteMessage, { providerName })}
          </p>
        </div>
        <Button
          onClick={onConfirmDelete}
          className="w-full h-[60px] rounded-none border-b border-border-primary bg-transparent hover:bg-red-900/20 text-red-500 font-medium text-md"
        >
          <Trash2 className="h-4 w-4 mr-2" /> {intl.formatMessage(i18n.confirmDelete)}
        </Button>
        <Button
          variant="ghost"
          onClick={onCancelDelete}
          className="w-full h-[60px] rounded-none hover:bg-background-secondary text-text-secondary hover:text-text-primary text-md font-regular"
        >
          {intl.formatMessage(i18n.cancel)}
        </Button>
      </div>
    );
  }

  // Regular buttons (with delete if applicable)
  return (
    <div className="w-full">
      {canDelete && onDelete && (
        <Button
          type="button"
          onClick={onDelete}
          className="w-full h-[60px] rounded-none border-t border-border-primary bg-transparent hover:bg-background-secondary text-red-500 font-medium text-md"
        >
          <Trash2 className="h-4 w-4 mr-2" /> {intl.formatMessage(i18n.deleteProvider)}
        </Button>
      )}
      {primaryParameters && primaryParameters.length > 0 ? (
        <>
          <Button
            type="submit"
            variant="ghost"
            onClick={onSubmit}
            className="w-full h-[60px] rounded-none border-t border-border-primary text-md hover:bg-background-secondary text-text-primary font-medium"
          >
            {intl.formatMessage(i18n.submit)}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={onCancel}
            className="w-full h-[60px] rounded-none border-t border-border-primary hover:text-text-primary text-text-secondary hover:bg-background-secondary text-md font-regular"
          >
            {intl.formatMessage(i18n.cancel)}
          </Button>
        </>
      ) : (
        <>
          <Button
            type="submit"
            variant="ghost"
            onClick={onSubmit}
            className="w-full h-[60px] rounded-none border-t border-border-primary text-md hover:bg-background-secondary text-text-primary font-medium"
          >
            {intl.formatMessage(i18n.enableProvider)}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={onCancel}
            className="w-full h-[60px] rounded-none border-t border-border-primary hover:text-text-primary text-text-secondary hover:bg-background-secondary text-md font-regular"
          >
            {intl.formatMessage(i18n.cancel)}
          </Button>
        </>
      )}
    </div>
  );
}
