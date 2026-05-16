import { useEffect, useState } from 'react';
import { Info } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { Switch } from '../../ui/switch';
import { defineMessages, useIntl } from '../../../i18n';

const PREFS_STORAGE_KEY = 'goose-copilot:preferences';

interface CopilotPrefs {
  autoReviewOnPrOpen: boolean;
  allowCommitOnFix: boolean;
  exhaustiveReview: boolean;
}

const DEFAULT_PREFS: CopilotPrefs = {
  autoReviewOnPrOpen: true,
  allowCommitOnFix: false,
  exhaustiveReview: false,
};

function loadPrefs(): CopilotPrefs {
  const raw = localStorage.getItem(PREFS_STORAGE_KEY);
  if (!raw) return DEFAULT_PREFS;
  try {
    return { ...DEFAULT_PREFS, ...(JSON.parse(raw) as Partial<CopilotPrefs>) };
  } catch {
    return DEFAULT_PREFS;
  }
}

function savePrefs(prefs: CopilotPrefs) {
  localStorage.setItem(PREFS_STORAGE_KEY, JSON.stringify(prefs));
}

const i18n = defineMessages({
  reviewTitle: {
    id: 'copilotCodeReview.reviewTitle',
    defaultMessage: 'Code review',
  },
  reviewDescription: {
    id: 'copilotCodeReview.reviewDescription',
    defaultMessage: 'Configure when and how Goose Copilot reviews pull requests.',
  },
  autoReviewLabel: {
    id: 'copilotCodeReview.autoReviewLabel',
    defaultMessage: 'Auto-review on PR open',
  },
  autoReviewHelper: {
    id: 'copilotCodeReview.autoReviewHelper',
    defaultMessage:
      'Run a review automatically when a pull request is opened on a connected repo.',
  },
  exhaustiveLabel: {
    id: 'copilotCodeReview.exhaustiveLabel',
    defaultMessage: 'Exhaustive review',
  },
  exhaustiveHelper: {
    id: 'copilotCodeReview.exhaustiveHelper',
    defaultMessage:
      'Keep looking for additional findings until the model stops finding new issues. Uses more tokens.',
  },
  allowCommitLabel: {
    id: 'copilotCodeReview.allowCommitLabel',
    defaultMessage: 'Let @goose-copilot push commits',
  },
  allowCommitHelper: {
    id: 'copilotCodeReview.allowCommitHelper',
    defaultMessage:
      'When someone asks @goose-copilot to fix issues, push the resulting changes to the PR branch as goose-copilot[bot].',
  },
  notWiredYet: {
    id: 'copilotCodeReview.notWiredYet',
    defaultMessage:
      'These preferences are stored locally — the bot still runs with permissive defaults. Backend wiring is in progress.',
  },
});

export default function CopilotCodeReview() {
  const intl = useIntl();
  const [prefs, setPrefs] = useState<CopilotPrefs>(loadPrefs);

  useEffect(() => {
    savePrefs(prefs);
  }, [prefs]);

  const togglePref = (key: keyof CopilotPrefs) => (checked: boolean) =>
    setPrefs((p) => ({ ...p, [key]: checked }));

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.reviewTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.reviewDescription)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-4">
          <PreferenceRow
            label={intl.formatMessage(i18n.autoReviewLabel)}
            helper={intl.formatMessage(i18n.autoReviewHelper)}
            checked={prefs.autoReviewOnPrOpen}
            onCheckedChange={togglePref('autoReviewOnPrOpen')}
          />
          <PreferenceRow
            label={intl.formatMessage(i18n.exhaustiveLabel)}
            helper={intl.formatMessage(i18n.exhaustiveHelper)}
            checked={prefs.exhaustiveReview}
            onCheckedChange={togglePref('exhaustiveReview')}
          />
          <PreferenceRow
            label={intl.formatMessage(i18n.allowCommitLabel)}
            helper={intl.formatMessage(i18n.allowCommitHelper)}
            checked={prefs.allowCommitOnFix}
            onCheckedChange={togglePref('allowCommitOnFix')}
          />

          <div className="flex gap-2 items-start text-xs text-text-secondary border-t border-border pt-4">
            <Info className="h-3.5 w-3.5 mt-0.5 shrink-0" />
            <span>{intl.formatMessage(i18n.notWiredYet)}</span>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function PreferenceRow({
  label,
  helper,
  checked,
  onCheckedChange,
}: {
  label: string;
  helper: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div>
        <h3 className="text-text-primary text-xs">{label}</h3>
        <p className="text-xs text-text-secondary max-w-md mt-[2px]">{helper}</p>
      </div>
      <div className="flex items-center">
        <Switch checked={checked} onCheckedChange={onCheckedChange} variant="mono" />
      </div>
    </div>
  );
}
