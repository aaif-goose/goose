import { useEffect, useState, forwardRef } from 'react';
import { Gear } from '../../icons';
import { ConfigureApproveMode } from './ConfigureApproveMode';
import PermissionRulesModal from '../permission/PermissionRulesModal';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  autonomousLabel: {
    id: 'modeSelectionItem.autonomousLabel',
    defaultMessage: 'Autonomous',
  },
  autonomousDescription: {
    id: 'modeSelectionItem.autonomousDescription',
    defaultMessage: 'Full file modification capabilities, edit, create, and delete files freely.',
  },
  manualLabel: {
    id: 'modeSelectionItem.manualLabel',
    defaultMessage: 'Manual',
  },
  manualDescription: {
    id: 'modeSelectionItem.manualDescription',
    defaultMessage: 'All tools, extensions and file modifications will require human approval',
  },
  smartLabel: {
    id: 'modeSelectionItem.smartLabel',
    defaultMessage: 'Smart',
  },
  smartDescription: {
    id: 'modeSelectionItem.smartDescription',
    defaultMessage: 'Intelligently determine which actions need approval based on risk level',
  },
  chatOnlyLabel: {
    id: 'modeSelectionItem.chatOnlyLabel',
    defaultMessage: 'Chat only',
  },
  chatOnlyDescription: {
    id: 'modeSelectionItem.chatOnlyDescription',
    defaultMessage: 'Engage with the selected provider without using tools or extensions.',
  },
});

export interface GooseMode {
  key: string;
  labelDescriptor: { id: string; defaultMessage: string };
  descriptionDescriptor: { id: string; defaultMessage: string };
}

export const all_goose_modes: GooseMode[] = [
  {
    key: 'auto',
    labelDescriptor: i18n.autonomousLabel,
    descriptionDescriptor: i18n.autonomousDescription,
  },
  {
    key: 'approve',
    labelDescriptor: i18n.manualLabel,
    descriptionDescriptor: i18n.manualDescription,
  },
  {
    key: 'smart_approve',
    labelDescriptor: i18n.smartLabel,
    descriptionDescriptor: i18n.smartDescription,
  },
  {
    key: 'chat',
    labelDescriptor: i18n.chatOnlyLabel,
    descriptionDescriptor: i18n.chatOnlyDescription,
  },
];

interface ModeSelectionItemProps {
  currentMode: string;
  mode: GooseMode;
  showDescription: boolean;
  isApproveModeConfigure: boolean;
  handleModeChange: (newMode: string) => void;
}

export const ModeSelectionItem = forwardRef<HTMLDivElement, ModeSelectionItemProps>(
  ({ currentMode, mode, showDescription, isApproveModeConfigure, handleModeChange }, ref) => {
    const intl = useIntl();
    const [checked, setChecked] = useState(currentMode == mode.key);
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [isPermissionModalOpen, setIsPermissionModalOpen] = useState(false);

    useEffect(() => {
      setChecked(currentMode === mode.key);
    }, [currentMode, mode.key]);

    return (
      <div ref={ref} className="group hover:cursor-pointer text-sm">
        <div
          className={`flex items-center justify-between text-text-primary py-2 px-2 ${checked ? 'bg-background-secondary' : 'bg-background-primary hover:bg-background-secondary'} rounded-lg transition-all`}
          onClick={() => handleModeChange(mode.key)}
        >
          <div className="flex">
            <div>
              <h3 className="text-text-primary">{intl.formatMessage(mode.labelDescriptor)}</h3>
              {showDescription && (
                <p className="text-text-secondary mt-[2px]">{intl.formatMessage(mode.descriptionDescriptor)}</p>
              )}
            </div>
          </div>

          <div className="relative flex items-center gap-2">
            {!isApproveModeConfigure && (mode.key == 'approve' || mode.key == 'smart_approve') && (
              <button
                onClick={(e) => {
                  e.stopPropagation(); // Prevent triggering the mode change
                  setIsPermissionModalOpen(true);
                }}
              >
                <Gear className="w-4 h-4 text-text-secondary hover:text-text-primary" />
              </button>
            )}
            <input
              type="radio"
              name="modes"
              value={mode.key}
              checked={checked}
              onChange={() => handleModeChange(mode.key)}
              className="peer sr-only"
            />
            <div
              className="h-4 w-4 rounded-full border border-border-primary 
                    peer-checked:border-[6px] peer-checked:border-black dark:peer-checked:border-white
                    peer-checked:bg-white dark:peer-checked:bg-black
                    transition-all duration-200 ease-in-out group-hover:border-border-primary"
            ></div>
          </div>
        </div>
        <div>
          <div>
            {isDialogOpen ? (
              <ConfigureApproveMode
                onClose={() => {
                  setIsDialogOpen(false);
                }}
                handleModeChange={handleModeChange}
                currentMode={currentMode}
              />
            ) : null}
          </div>
        </div>

        <PermissionRulesModal
          isOpen={isPermissionModalOpen}
          onClose={() => setIsPermissionModalOpen(false)}
        />
      </div>
    );
  }
);

ModeSelectionItem.displayName = 'ModeSelectionItem';
