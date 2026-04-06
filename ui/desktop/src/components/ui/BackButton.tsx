import React, { useEffect, useCallback } from 'react';
import { ArrowLeft } from 'lucide-react';
import { Button } from './button';
import type { VariantProps } from 'class-variance-authority';
import { buttonVariants } from './button';
import { cn } from '../../utils';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  back: {
    id: 'backButton.back',
    defaultMessage: 'Back',
  },
});

interface BackButtonProps extends VariantProps<typeof buttonVariants> {
  onClick?: () => void;
  className?: string;
  showText?: boolean;
  shape?: 'pill' | 'round';
}

const BackButton: React.FC<BackButtonProps> = ({
  onClick,
  className = '',
  variant = 'secondary',
  size = 'default',
  shape = 'pill',
  showText = true,
  ...props
}) => {
  const intl = useIntl();
  const handleExit = useCallback(() => {
    if (onClick) {
      onClick(); // Custom onClick handler passed via props
    } else if (window.history.length > 1) {
      window.history.back(); // Navigate to the previous page
    } else {
      console.warn('No history to go back to');
    }
  }, [onClick]);

  // Set up mouse back button event listener.
  useEffect(() => {
    const handleMouseBack = () => {
      handleExit();
    };

    if (window.electron) {
      const mouseBackHandler = (e: MouseEvent) => {
        // MouseButton 3 or 4 is typically back button.
        if (e.button === 3 || e.button === 4) {
          handleExit();
          e.preventDefault();
        }
      };

      window.electron.on('mouse-back-button-clicked', handleMouseBack);

      // Also listen for mouseup events directly, for better OS compatibility.
      document.addEventListener('mouseup', mouseBackHandler);

      return () => {
        if (window.electron) {
          window.electron.off('mouse-back-button-clicked', handleMouseBack);
        }
        document.removeEventListener('mouseup', mouseBackHandler);
      };
    }

    return undefined;
  }, [handleExit]);

  return (
    <Button
      onClick={handleExit}
      variant={variant}
      size={size}
      shape={shape}
      className={cn(
        'rounded-full px-6 py-2 flex items-center gap-2 text-text-primary hover:cursor-pointer',
        className
      )}
      {...props}
    >
      <ArrowLeft />
      {showText && intl.formatMessage(i18n.back)}
    </Button>
  );
};

export default BackButton;
