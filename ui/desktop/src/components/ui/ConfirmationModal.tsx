import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './dialog';
import { Button } from './button';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  processing: {
    id: 'confirmationModal.processing',
    defaultMessage: 'Processing...',
  },
  defaultConfirm: {
    id: 'confirmationModal.defaultConfirm',
    defaultMessage: 'Yes',
  },
  defaultCancel: {
    id: 'confirmationModal.defaultCancel',
    defaultMessage: 'No',
  },
});

export function ConfirmationModal({
  isOpen,
  title,
  message,
  detail,
  onConfirm,
  onCancel,
  confirmLabel,
  cancelLabel,
  isSubmitting = false,
  confirmVariant = 'default',
}: {
  isOpen: boolean;
  title: string;
  message: string;
  detail?: React.ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
  confirmLabel?: string;
  cancelLabel?: string;
  isSubmitting?: boolean; // To handle debounce state
  confirmVariant?: 'default' | 'destructive' | 'outline' | 'secondary' | 'ghost' | 'link';
}) {
  const intl = useIntl();

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="sm:max-w-[425px] max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{message}</DialogDescription>
        </DialogHeader>

        {detail && (
          <div className="overflow-y-auto min-h-0 text-sm text-text-muted break-all">{detail}</div>
        )}

        <DialogFooter className="pt-2 shrink-0">
          <Button
            variant="outline"
            onClick={onCancel}
            disabled={isSubmitting}
            className="focus-visible:ring-2 focus-visible:ring-background-accent focus-visible:ring-offset-2 focus-visible:ring-offset-background-default"
          >
            {cancelLabel || intl.formatMessage(i18n.defaultCancel)}
          </Button>
          <Button
            variant={confirmVariant}
            onClick={onConfirm}
            disabled={isSubmitting}
            className="focus-visible:ring-2 focus-visible:ring-background-accent focus-visible:ring-offset-2 focus-visible:ring-offset-background-default"
          >
            {isSubmitting ? intl.formatMessage(i18n.processing) : (confirmLabel || intl.formatMessage(i18n.defaultConfirm))}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
