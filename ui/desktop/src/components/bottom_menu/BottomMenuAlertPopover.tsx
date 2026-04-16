import { AppEvents } from '../../constants/events';
import React, { useRef, useEffect, useCallback, useState } from 'react';
import { FaCircle } from 'react-icons/fa';
import { isEqual } from 'lodash';
import { cn } from '../../utils';
import { Alert, AlertType } from '../alerts';
import { AlertBox } from '../alerts';

interface AlertPopoverProps {
  alerts: Alert[];
  children?: React.ReactNode;
}

export default function BottomMenuAlertPopover({ alerts, children }: AlertPopoverProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const [wasAutoShown, setWasAutoShown] = useState(false);
  const [popoverPosition, setPopoverPosition] = useState({ top: 0, left: 0 });
  const [shouldShowIndicator, setShouldShowIndicator] = useState(false); // Stable indicator state
  const previousAlertsRef = useRef<Alert[]>([]);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  // Calculate popover position
  const calculatePosition = useCallback(() => {
    if (!triggerRef.current || !popoverRef.current) return;

    const triggerRect = triggerRef.current.getBoundingClientRect();
    const popoverWidth = 275;

    // Get the actual rendered height of the popover
    const popoverHeight = popoverRef.current.offsetHeight || 120;
    const offset = 8; // Small gap to avoid blocking the trigger dot

    // Position above the trigger, centered horizontally
    let top = triggerRect.top - popoverHeight - offset;
    let left = triggerRect.left + triggerRect.width / 2 - popoverWidth / 2;

    // Ensure popover doesn't go off-screen
    const viewportWidth = window.innerWidth;

    // Adjust horizontal position if off-screen
    if (left < 10) {
      left = 10;
    } else if (left + popoverWidth > viewportWidth - 10) {
      left = viewportWidth - popoverWidth - 10;
    }

    // If popover would go above viewport, show it below the trigger instead
    if (top < 10) {
      top = triggerRect.bottom + offset;
    }

    setPopoverPosition({ top, left });
  }, []);

  // Update position when popover opens
  useEffect(() => {
    if (isOpen) {
      calculatePosition();
      // Recalculate on window resize
      const handleResize = () => calculatePosition();
      window.addEventListener('resize', handleResize);
      return () => window.removeEventListener('resize', handleResize);
    }
    return undefined;
  }, [isOpen, calculatePosition]);

  // Recalculate position after popover is rendered to get actual height
  useEffect(() => {
    if (isOpen && popoverRef.current) {
      // Small delay to ensure DOM is updated
      const timer = setTimeout(() => {
        calculatePosition();
      }, 10);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [isOpen, calculatePosition]);

  // Function to start the hide timer
  const startHideTimer = useCallback((duration = 3000) => {
    // Clear any existing timer
    if (hideTimerRef.current) {
      clearTimeout(hideTimerRef.current);
    }
    // Start new timer
    hideTimerRef.current = setTimeout(() => {
      setIsOpen(false);
      setWasAutoShown(false);
    }, duration);
  }, []);

  // Manage stable indicator visibility - once we have alerts, keep showing until explicitly cleared
  useEffect(() => {
    if (alerts.length > 0) {
      setShouldShowIndicator(true);
    } else {
      setShouldShowIndicator(false);
    }
  }, [alerts.length]);

  // Handle initial show and new alerts
  useEffect(() => {
    if (alerts.length === 0) {
      return;
    }

    // Find new or changed alerts using deep comparison
    const changedAlerts = alerts.filter((alert, index) => {
      const prevAlert = previousAlertsRef.current[index];
      return !prevAlert || !isEqual(prevAlert, alert);
    });

    previousAlertsRef.current = alerts;

    // Only auto-show if any of the new/changed alerts have autoShow: true
    const hasNewAutoShowAlert = changedAlerts.some((alert) => alert.autoShow === true);

    // Auto show the popover for new auto-show alerts
    if (hasNewAutoShowAlert) {
      setIsOpen(true);
      setWasAutoShown(true);
      // Start 3 second timer for auto-show
      startHideTimer(3000);
    }
  }, [alerts, startHideTimer]);

  // Handle auto-hide based on hover state changes
  useEffect(() => {
    if (!isHovered && isOpen && !wasAutoShown) {
      // Only start 1 second timer for manual interactions
      startHideTimer(1000);
    }
  }, [isHovered, isOpen, startHideTimer, wasAutoShown]);

  // Handle click outside - but not when editing threshold
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      // Check if we're clicking on an input or button inside the popover
      const target = event.target as HTMLElement;
      const isInteractiveElement =
        target.tagName === 'INPUT' ||
        target.tagName === 'BUTTON' ||
        target.closest('button') !== null ||
        target.closest('input') !== null;

      if (
        popoverRef.current &&
        !popoverRef.current.contains(event.target as Node) &&
        !isInteractiveElement
      ) {
        setIsOpen(false);
        setWasAutoShown(false);
      }
    };

    if (isOpen) {
      // Use mouseup instead of mousedown to allow button clicks to complete
      document.addEventListener('mouseup', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mouseup', handleClickOutside);
    };
  }, [isOpen]);

  // Listen for custom event to hide the popover
  useEffect(() => {
    const handleHidePopover = () => {
      if (isOpen) {
        setIsOpen(false);
        setWasAutoShown(false);
        setIsHovered(false);
        // Clear any pending hide timer
        if (hideTimerRef.current) {
          clearTimeout(hideTimerRef.current);
          hideTimerRef.current = null;
        }
      }
    };

    window.addEventListener(AppEvents.HIDE_ALERT_POPOVER, handleHidePopover);
    return () => {
      window.removeEventListener(AppEvents.HIDE_ALERT_POPOVER, handleHidePopover);
    };
  }, [isOpen]);

  // Use shouldShowIndicator instead of alerts.length for rendering decision
  if (!shouldShowIndicator && !children) {
    return null;
  }

  // Determine the icon and styling based on the alerts (use current alerts if available, or default to info)
  const hasError = alerts.some((alert) => alert.type === AlertType.Error);
  const hasInfo = alerts.some((alert) => alert.type === AlertType.Info);
  const triggerColor = hasError
    ? 'text-[#d7040e]' // Red color for error alerts
    : hasInfo || alerts.length === 0 // Default to green for context info when no alerts
      ? 'text-[#00b300]' // Green color for info alerts
      : 'text-[#cc4b03]'; // Orange color for warning alerts

  return (
    <>
      <div className="relative">
        <button
          type="button"
          ref={triggerRef}
          className="cursor-pointer flex items-center gap-1.5 min-h-5 rounded hover:bg-background-secondary px-1"
          onClick={() => {
            if (alerts.length > 0) setIsOpen(true);
          }}
          onMouseEnter={() => {
            if (alerts.length > 0) {
              setIsOpen(true);
              setIsHovered(true);
              setWasAutoShown(false);
              if (hideTimerRef.current) {
                clearTimeout(hideTimerRef.current);
              }
            }
          }}
          onMouseLeave={() => {
            // Start a short timer to allow moving to content
            hideTimerRef.current = setTimeout(() => {
              if (!isHovered) {
                setIsHovered(false);
                setIsOpen(false);
              }
            }, 100);
          }}
        >
          {shouldShowIndicator && (
            <div className={cn('relative', triggerColor)}>
              <FaCircle size={5} />
            </div>
          )}
          {children}
        </button>
      </div>

      {/* Popover rendered separately to avoid blocking clicks */}
      {isOpen && (
        <div
          ref={popoverRef}
          className="fixed w-[275px] p-0 rounded-lg overflow-hidden bg-app border z-50 shadow-lg pointer-events-auto text-left"
          style={{
            top: `${popoverPosition.top}px`,
            left: `${popoverPosition.left}px`,
            visibility: popoverPosition.top === 0 ? 'hidden' : 'visible',
          }}
          onMouseEnter={() => {
            setIsHovered(true);
            if (hideTimerRef.current) {
              clearTimeout(hideTimerRef.current);
            }
          }}
          onMouseLeave={() => {
            setIsHovered(false);
            setIsOpen(false);
          }}
        >
          <div className="flex flex-col">
            {alerts.map((alert, index) => (
              <div key={index} className={cn(index > 0 && 'border-t border-white/20')}>
                <AlertBox alert={alert} />
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
