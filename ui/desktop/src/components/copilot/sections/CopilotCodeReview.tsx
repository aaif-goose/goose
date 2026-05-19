import { useMemo, useState } from 'react';
import {
  AlertCircle,
  ChevronDown,
  ExternalLink,
  Lightbulb,
  Loader2,
  Lock,
  RefreshCw,
  Search,
} from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { Switch } from '../../ui/switch';
import { Input } from '../../ui/input';
import { Button } from '../../ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';
import type {
  CopilotPrefs,
  CopilotRepo,
  ReviewModelChoice,
  ReviewOutputStyle,
  ReviewSeverity,
  TriggerPermission,
  TriggerPreference,
} from '../../../api/types.gen';
import { defineMessages, useIntl } from '../../../i18n';
import { useCopilotRepos } from '../useCopilotRepos';
import { RecipeModelSelector } from '../../recipes/shared/RecipeModelSelector';

const i18n = defineMessages({
  setupBlurb: {
    id: 'copilotCodeReview.setupBlurb',
    defaultMessage:
      'When code review is on, Goose Copilot automatically reviews pull requests and leaves inline suggestions. Mention @goose-copilot in any PR comment to ask a question, request a manual review, or apply fixes.',
  },
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
    defaultMessage: 'Personal auto review preferences',
  },
  autoReviewHelper: {
    id: 'copilotCodeReview.autoReviewHelper',
    defaultMessage:
      'All your pull requests in a Goose Copilot-enabled repository will be automatically reviewed.',
  },
  triggerLabel: {
    id: 'copilotCodeReview.triggerLabel',
    defaultMessage: 'Personal review trigger preference',
  },
  triggerHelper: {
    id: 'copilotCodeReview.triggerHelper',
    defaultMessage: 'Choose when Goose Copilot should automatically review your pull requests.',
  },
  outputStyleLabel: {
    id: 'copilotCodeReview.outputStyleLabel',
    defaultMessage: 'Review output style',
  },
  outputStyleHelper: {
    id: 'copilotCodeReview.outputStyleHelper',
    defaultMessage: 'How findings are posted back to the pull request.',
  },
  severityLabel: {
    id: 'copilotCodeReview.severityLabel',
    defaultMessage: 'Minimum severity to surface',
  },
  severityHelper: {
    id: 'copilotCodeReview.severityHelper',
    defaultMessage:
      'Findings below this rank are dropped from the review. Higher = quieter.',
  },
  reviewModelLabel: {
    id: 'copilotCodeReview.reviewModelLabel',
    defaultMessage: 'Review model',
  },
  reviewModelHelper: {
    id: 'copilotCodeReview.reviewModelHelper',
    defaultMessage:
      'Which model the review orchestrator uses. Defaults to the model goose is configured with.',
  },
  permissionsTitle: {
    id: 'copilotCodeReview.permissionsTitle',
    defaultMessage: 'Permissions',
  },
  permissionsDescription: {
    id: 'copilotCodeReview.permissionsDescription',
    defaultMessage: 'Control who can interact with @goose-copilot and what it is allowed to do.',
  },
  triggerPermissionLabel: {
    id: 'copilotCodeReview.triggerPermissionLabel',
    defaultMessage: 'Who can trigger Goose Copilot',
  },
  triggerPermissionHelper: {
    id: 'copilotCodeReview.triggerPermissionHelper',
    defaultMessage:
      'Restrict who can @mention the bot to request reviews, fixes, or replies.',
  },
  allowCommitLabel: {
    id: 'copilotCodeReview.allowCommitLabel',
    defaultMessage: 'Allow pushing commits to PR branches',
  },
  allowCommitHelper: {
    id: 'copilotCodeReview.allowCommitHelper',
    defaultMessage:
      'When someone asks @goose-copilot to fix issues, push the resulting changes to the PR branch as goose-copilot[bot].',
  },
  allowIssuesLabel: {
    id: 'copilotCodeReview.allowIssuesLabel',
    defaultMessage: 'Respond to mentions on issues',
  },
  allowIssuesHelper: {
    id: 'copilotCodeReview.allowIssuesHelper',
    defaultMessage:
      'Let @goose-copilot answer questions and draft PRs from issue comments, not just from inside pull requests.',
  },
  allowNewPrsLabel: {
    id: 'copilotCodeReview.allowNewPrsLabel',
    defaultMessage: 'Open new pull requests for issue fixes',
  },
  allowNewPrsHelper: {
    id: 'copilotCodeReview.allowNewPrsHelper',
    defaultMessage:
      'When someone @-mentions the bot on a plain issue and asks for a fix, push the changes on a fresh branch and open a PR linked back to the issue.',
  },
  allowlistLabel: {
    id: 'copilotCodeReview.allowlistLabel',
    defaultMessage: 'Allowlist',
  },
  allowlistHelper: {
    id: 'copilotCodeReview.allowlistHelper',
    defaultMessage:
      'One GitHub username per line. Only listed users can mention the bot.',
  },
  allowlistPlaceholder: {
    id: 'copilotCodeReview.allowlistPlaceholder',
    defaultMessage: 'octocat\nabhi-jay',
  },
  repoPrefsTitle: {
    id: 'copilotCodeReview.repoPrefsTitle',
    defaultMessage: 'Repository preferences',
  },
  repoPrefsDescription: {
    id: 'copilotCodeReview.repoPrefsDescription',
    defaultMessage: 'Override the personal defaults above on a per-repo basis.',
  },
  repoSearchPlaceholder: {
    id: 'copilotCodeReview.repoSearchPlaceholder',
    defaultMessage: 'Search repos or enter a GitHub org/repo',
  },
  hideArchived: {
    id: 'copilotCodeReview.hideArchived',
    defaultMessage: 'Hide archived repositories',
  },
  repoEmptyConnected: {
    id: 'copilotCodeReview.repoEmptyConnected',
    defaultMessage:
      'Per-repo overrides are coming soon — the bot uses your personal defaults for every connected repo for now.',
  },
  repoLoading: {
    id: 'copilotCodeReview.repoLoading',
    defaultMessage: 'Loading repositories…',
  },
  repoError: {
    id: 'copilotCodeReview.repoError',
    defaultMessage: 'Could not load repositories',
  },
  repoRetry: {
    id: 'copilotCodeReview.repoRetry',
    defaultMessage: 'Retry',
  },
  repoCount: {
    id: 'copilotCodeReview.repoCount',
    defaultMessage: '{shown, plural, one {# repository} other {# repositories}} accessible',
  },
  repoTruncated: {
    id: 'copilotCodeReview.repoTruncated',
    defaultMessage: 'Showing first {shown} of {total} — others omitted.',
  },
  repoNoMatch: {
    id: 'copilotCodeReview.repoNoMatch',
    defaultMessage: 'No repositories match your search.',
  },
  repoColRepo: {
    id: 'copilotCodeReview.repoColRepo',
    defaultMessage: 'Repository',
  },
  repoColVisibility: {
    id: 'copilotCodeReview.repoColVisibility',
    defaultMessage: 'Visibility',
  },
  repoColBranch: {
    id: 'copilotCodeReview.repoColBranch',
    defaultMessage: 'Default branch',
  },
});

interface RichOption<T extends string> {
  value: T;
  label: string;
  description: string;
}

interface Props {
  prefs: CopilotPrefs | null;
  onUpdate: (patch: Partial<CopilotPrefs>) => void;
}

export default function CopilotCodeReview({ prefs, onUpdate }: Props) {
  const intl = useIntl();
  const [repoQuery, setRepoQuery] = useState('');
  // Local-only UI pref; not part of the synced backend state.
  const [hideArchivedRepos, setHideArchivedRepos] = useState(true);
  // Only fetch repos once a successful install is in place (prefs is loaded).
  const { state: reposState, refresh: refreshRepos } = useCopilotRepos(prefs !== null);

  const triggerOptions = useMemo<RichOption<TriggerPreference>[]>(
    () => [
      {
        value: 'pr-open',
        label: 'On PR open',
        description: 'Review when a pull request is opened.',
      },
      {
        value: 'on-every-push',
        label: 'On every push',
        description: 'Review again whenever new commits are pushed to the pull request.',
      },
      {
        value: 'manual-only',
        label: 'Manual only',
        description: 'Only review when someone mentions @goose-copilot review.',
      },
    ],
    []
  );

  const severityOptions = useMemo<RichOption<ReviewSeverity>[]>(
    () => [
      {
        value: 'low',
        label: 'Low — surface everything',
        description: 'Every finding the model produces. Highest signal *and* noise.',
      },
      {
        value: 'medium',
        label: 'Medium (default)',
        description: 'Drop low-severity nits; show medium and above.',
      },
      {
        value: 'high',
        label: 'High — only important issues',
        description: 'Only high- and critical-severity findings.',
      },
      {
        value: 'critical',
        label: 'Critical — blocking issues only',
        description: 'Only the most severe issues that should block the PR.',
      },
    ],
    []
  );

  const outputOptions = useMemo<RichOption<ReviewOutputStyle>[]>(
    () => [
      {
        value: 'inline',
        label: 'Inline suggestions',
        description:
          'Post findings as inline review comments with GitHub suggestion blocks where possible.',
      },
      {
        value: 'summary',
        label: 'Summary comment only',
        description:
          'Post a single review comment listing all findings — no inline annotations.',
      },
      {
        value: 'both',
        label: 'Inline + summary',
        description:
          'Inline annotations on each finding, plus a one-comment summary at the top of the review.',
      },
    ],
    []
  );

  const modelOptions = useMemo<RichOption<ReviewModelChoice>[]>(
    () => [
      {
        value: 'default',
        label: "Use Goose's default model",
        description: 'Whatever model goose is configured with in Settings → Models.',
      },
      {
        value: 'custom',
        label: 'Use a different model for reviews',
        description: 'Pick a separate provider + model for code review.',
      },
    ],
    []
  );

  const triggerPermissionOptions = useMemo<RichOption<TriggerPermission>[]>(
    () => [
      {
        value: 'anyone',
        label: 'Anyone',
        description:
          'Any GitHub user who can see the PR can mention @goose-copilot. Best for open-source.',
      },
      {
        value: 'write-access',
        label: 'Collaborators with write access',
        description:
          'Only repo collaborators with write permission or higher can trigger the bot.',
      },
      {
        value: 'specific-users',
        label: 'Specific users',
        description: 'Allowlist a set of GitHub usernames (coming soon).',
      },
    ],
    []
  );

  const disabled = prefs === null;

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.reviewTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.reviewDescription)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-5">
          <div className="flex items-start gap-3 rounded-md bg-background-secondary p-3">
            <Lightbulb className="h-4 w-4 mt-0.5 shrink-0 text-text-secondary" />
            <p className="text-xs text-text-secondary">
              {intl.formatMessage(i18n.setupBlurb)}
            </p>
          </div>

          <PreferenceRow
            label={intl.formatMessage(i18n.autoReviewLabel)}
            helper={intl.formatMessage(i18n.autoReviewHelper)}
            control={
              <Switch
                disabled={disabled}
                checked={prefs?.auto_review_on_pr_open ?? true}
                onCheckedChange={(checked) => onUpdate({ auto_review_on_pr_open: checked })}
                variant="mono"
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.triggerLabel)}
            helper={intl.formatMessage(i18n.triggerHelper)}
            disabled={!prefs?.auto_review_on_pr_open}
            control={
              <RichSelect
                disabled={disabled || !prefs?.auto_review_on_pr_open}
                options={triggerOptions}
                value={prefs?.trigger_preference ?? 'pr-open'}
                onChange={(v) => onUpdate({ trigger_preference: v })}
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.outputStyleLabel)}
            helper={intl.formatMessage(i18n.outputStyleHelper)}
            control={
              <RichSelect
                disabled={disabled}
                options={outputOptions}
                value={prefs?.review_output_style ?? 'both'}
                onChange={(v) => onUpdate({ review_output_style: v })}
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.severityLabel)}
            helper={intl.formatMessage(i18n.severityHelper)}
            control={
              <RichSelect
                disabled={disabled}
                options={severityOptions}
                value={prefs?.review_severity ?? 'medium'}
                onChange={(v) => onUpdate({ review_severity: v })}
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.reviewModelLabel)}
            helper={intl.formatMessage(i18n.reviewModelHelper)}
            control={
              <RichSelect
                disabled={disabled}
                options={modelOptions}
                value={prefs?.review_model_choice ?? 'default'}
                onChange={(v) => onUpdate({ review_model_choice: v })}
              />
            }
          />
          {prefs?.review_model_choice === 'custom' && (
            <div className="ml-0 pl-4 border-l-2 border-border space-y-2">
              <RecipeModelSelector
                selectedProvider={prefs.review_provider ?? undefined}
                selectedModel={prefs.review_model ?? undefined}
                onProviderChange={(provider) =>
                  onUpdate({ review_provider: provider ?? undefined })
                }
                onModelChange={(model) => onUpdate({ review_model: model ?? undefined })}
              />
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.permissionsTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.permissionsDescription)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-5">
          <PreferenceRow
            label={intl.formatMessage(i18n.triggerPermissionLabel)}
            helper={intl.formatMessage(i18n.triggerPermissionHelper)}
            control={
              <RichSelect
                disabled={disabled}
                options={triggerPermissionOptions}
                value={prefs?.trigger_permission ?? 'anyone'}
                onChange={(v) => onUpdate({ trigger_permission: v })}
              />
            }
          />

          {prefs?.trigger_permission === 'specific-users' && (
            <div className="pl-4 border-l-2 border-border space-y-2">
              <h3 className="text-text-primary text-xs">
                {intl.formatMessage(i18n.allowlistLabel)}
              </h3>
              <p className="text-xs text-text-secondary max-w-md">
                {intl.formatMessage(i18n.allowlistHelper)}
              </p>
              <textarea
                value={(prefs?.specific_users_allowlist ?? []).join('\n')}
                onChange={(e) =>
                  onUpdate({
                    specific_users_allowlist: e.target.value
                      .split('\n')
                      .map((s) => s.trim())
                      .filter((s) => s.length > 0),
                  })
                }
                placeholder={intl.formatMessage(i18n.allowlistPlaceholder)}
                rows={4}
                className="flex w-full rounded-md border focus:border-border-secondary hover:border-border-secondary bg-background-primary px-3 py-2 text-xs font-mono transition-colors placeholder:text-text-secondary focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 resize-y min-h-[80px]"
              />
            </div>
          )}

          <PreferenceRow
            label={intl.formatMessage(i18n.allowCommitLabel)}
            helper={intl.formatMessage(i18n.allowCommitHelper)}
            control={
              <Switch
                disabled={disabled}
                checked={prefs?.allow_commit_on_fix ?? false}
                onCheckedChange={(checked) => onUpdate({ allow_commit_on_fix: checked })}
                variant="mono"
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.allowIssuesLabel)}
            helper={intl.formatMessage(i18n.allowIssuesHelper)}
            control={
              <Switch
                disabled={disabled}
                checked={prefs?.allow_act_on_issues ?? false}
                onCheckedChange={(checked) => onUpdate({ allow_act_on_issues: checked })}
                variant="mono"
              />
            }
          />

          <PreferenceRow
            label={intl.formatMessage(i18n.allowNewPrsLabel)}
            helper={intl.formatMessage(i18n.allowNewPrsHelper)}
            disabled={!prefs?.allow_act_on_issues}
            control={
              <Switch
                disabled={disabled || !prefs?.allow_act_on_issues}
                checked={prefs?.allow_open_new_prs ?? false}
                onCheckedChange={(checked) => onUpdate({ allow_open_new_prs: checked })}
                variant="mono"
              />
            }
          />
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.repoPrefsTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.repoPrefsDescription)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-4">
          <div className="flex items-center gap-3">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-text-secondary" />
              <Input
                value={repoQuery}
                onChange={(e) => setRepoQuery(e.target.value)}
                placeholder={intl.formatMessage(i18n.repoSearchPlaceholder)}
                className="pl-8 h-8 text-xs"
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-text-secondary">
                {intl.formatMessage(i18n.hideArchived)}
              </span>
              <Switch
                checked={hideArchivedRepos}
                onCheckedChange={setHideArchivedRepos}
                variant="mono"
              />
            </div>
          </div>
          <RepoTable
            state={reposState}
            query={repoQuery}
            hideArchived={hideArchivedRepos}
            onRetry={refreshRepos}
          />
        </CardContent>
      </Card>
    </div>
  );
}

function RepoTable({
  state,
  query,
  hideArchived,
  onRetry,
}: {
  state: ReturnType<typeof useCopilotRepos>['state'];
  query: string;
  hideArchived: boolean;
  onRetry: () => void;
}) {
  const intl = useIntl();

  if (state.kind === 'idle' || state.kind === 'loading') {
    return (
      <div className="h-32 rounded-md border border-dashed border-border flex items-center justify-center text-xs text-text-secondary gap-2">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        {intl.formatMessage(i18n.repoLoading)}
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <div className="h-32 rounded-md border border-dashed border-border flex flex-col items-center justify-center text-xs text-text-secondary gap-2 px-6 text-center">
        <span className="flex items-center gap-2">
          <AlertCircle className="h-3.5 w-3.5 text-amber-600" />
          {intl.formatMessage(i18n.repoError)}: {state.error}
        </span>
        <Button size="sm" variant="outline" onClick={onRetry} className="h-7 px-3 text-xs">
          <RefreshCw className="h-3 w-3 mr-1.5" />
          {intl.formatMessage(i18n.repoRetry)}
        </Button>
      </div>
    );
  }

  const lowerQuery = query.trim().toLowerCase();
  const filtered = state.data.repos.filter((r) => {
    if (hideArchived && r.archived) return false;
    if (!lowerQuery) return true;
    return r.full_name.toLowerCase().includes(lowerQuery);
  });

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-xs text-text-secondary">
        <span>{intl.formatMessage(i18n.repoCount, { shown: filtered.length })}</span>
        {state.data.truncated && (
          <span className="text-amber-700 dark:text-amber-400">
            {intl.formatMessage(i18n.repoTruncated, {
              shown: state.data.repos.length,
              total: state.data.total_count,
            })}
          </span>
        )}
      </div>
      {filtered.length === 0 ? (
        <div className="h-24 rounded-md border border-dashed border-border flex items-center justify-center text-xs text-text-secondary">
          {intl.formatMessage(i18n.repoNoMatch)}
        </div>
      ) : (
        <div className="rounded-md border border-border overflow-hidden">
          <div className="grid grid-cols-[1fr_auto_auto] gap-3 px-3 py-2 text-xs font-medium text-text-secondary bg-background-secondary border-b border-border">
            <span>{intl.formatMessage(i18n.repoColRepo)}</span>
            <span>{intl.formatMessage(i18n.repoColVisibility)}</span>
            <span>{intl.formatMessage(i18n.repoColBranch)}</span>
          </div>
          <div className="max-h-72 overflow-y-auto">
            {filtered.map((r) => (
              <RepoRow key={r.id} repo={r} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function RepoRow({ repo }: { repo: CopilotRepo }) {
  return (
    <div className="grid grid-cols-[1fr_auto_auto] gap-3 px-3 py-2 text-xs items-center border-b border-border last:border-b-0 hover:bg-background-secondary">
      <span className="flex items-center gap-2 min-w-0">
        {repo.visibility === 'private' && (
          <Lock className="h-3 w-3 shrink-0 text-text-secondary" />
        )}
        <a
          href={repo.html_url ?? '#'}
          target="_blank"
          rel="noreferrer"
          onClick={(e) => {
            e.preventDefault();
            if (repo.html_url) window.electron.openExternal(repo.html_url);
          }}
          className="truncate hover:underline"
        >
          {repo.full_name}
        </a>
        {repo.archived && (
          <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-background-tertiary text-text-secondary shrink-0">
            archived
          </span>
        )}
        <ExternalLink className="h-3 w-3 shrink-0 text-text-secondary opacity-60" />
      </span>
      <span className="text-text-secondary capitalize">{repo.visibility}</span>
      <span className="text-text-secondary font-mono text-[11px]">{repo.default_branch || '—'}</span>
    </div>
  );
}

function PreferenceRow({
  label,
  helper,
  control,
  disabled,
}: {
  label: string;
  helper: string;
  control: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <div
      className={`flex items-start justify-between gap-6 ${disabled ? 'opacity-50' : ''}`.trim()}
    >
      <div className="flex-1">
        <h3 className="text-text-primary text-xs">{label}</h3>
        <p className="text-xs text-text-secondary max-w-md mt-[2px]">{helper}</p>
      </div>
      <div className="flex items-center shrink-0">{control}</div>
    </div>
  );
}

function RichSelect<T extends string>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: RichOption<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  const current = options.find((o) => o.value === value) ?? options[0];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled}
        className="flex items-center gap-2 px-3 py-1.5 text-sm border border-border-primary rounded-md hover:border-border-primary transition-colors text-text-primary bg-background-primary disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {current.label}
        <ChevronDown className="w-4 h-4" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-max min-w-[280px] max-w-[360px]">
        <DropdownMenuRadioGroup value={value} onValueChange={(v) => onChange(v as T)}>
          {options.map((o) => (
            <DropdownMenuRadioItem key={o.value} value={o.value} className="items-start">
              <div className="flex flex-col">
                <span className="text-sm text-text-primary">{o.label}</span>
                <span className="text-xs text-text-secondary mt-0.5 whitespace-normal">
                  {o.description}
                </span>
              </div>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
