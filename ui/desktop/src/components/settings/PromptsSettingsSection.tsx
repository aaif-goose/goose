import { useState, useEffect, useCallback } from 'react';
import {
  getPrompt,
  getPrompts,
  PromptContentResponse,
  Template,
  resetPrompt,
  savePrompt,
} from '../../api';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { AlertTriangle, RotateCcw, ArrowLeft } from 'lucide-react';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  failedToLoadPrompts: {
    id: 'promptsSettings.failedToLoadPrompts',
    defaultMessage: 'Failed to load prompts',
  },
  failedToLoadPrompt: {
    id: 'promptsSettings.failedToLoadPrompt',
    defaultMessage: 'Failed to load prompt',
  },
  confirmResetAll: {
    id: 'promptsSettings.confirmResetAll',
    defaultMessage: 'Are you sure you want to reset all prompts to their defaults? This cannot be undone.',
  },
  allPromptsReset: {
    id: 'promptsSettings.allPromptsReset',
    defaultMessage: 'All prompts reset to defaults',
  },
  failedToResetPrompts: {
    id: 'promptsSettings.failedToResetPrompts',
    defaultMessage: 'Failed to reset prompts',
  },
  promptSaved: {
    id: 'promptsSettings.promptSaved',
    defaultMessage: 'Prompt saved',
  },
  failedToSavePrompt: {
    id: 'promptsSettings.failedToSavePrompt',
    defaultMessage: 'Failed to save prompt',
  },
  confirmResetOne: {
    id: 'promptsSettings.confirmResetOne',
    defaultMessage: 'Are you sure you want to reset this prompt to its default? This cannot be undone.',
  },
  promptResetToDefault: {
    id: 'promptsSettings.promptResetToDefault',
    defaultMessage: 'Prompt reset to default',
  },
  failedToResetPrompt: {
    id: 'promptsSettings.failedToResetPrompt',
    defaultMessage: 'Failed to reset prompt',
  },
  confirmReplaceWithDefault: {
    id: 'promptsSettings.confirmReplaceWithDefault',
    defaultMessage: 'Replace current content with default? Your changes will be lost.',
  },
  confirmUnsavedBack: {
    id: 'promptsSettings.confirmUnsavedBack',
    defaultMessage: 'You have unsaved changes. Are you sure you want to go back?',
  },
  backToList: {
    id: 'promptsSettings.backToList',
    defaultMessage: 'Back to List',
  },
  resetToDefault: {
    id: 'promptsSettings.resetToDefault',
    defaultMessage: 'Reset to Default',
  },
  save: {
    id: 'promptsSettings.save',
    defaultMessage: 'Save',
  },
  editPromptTitle: {
    id: 'promptsSettings.editPromptTitle',
    defaultMessage: 'Edit: {name}',
  },
  customized: {
    id: 'promptsSettings.customized',
    defaultMessage: 'Customized',
  },
  templateTip: {
    id: 'promptsSettings.templateTip',
    defaultMessage: 'Template variables like {extensionsExample} or {forExample} are replaced with actual values at runtime. Be careful not to remove required variables.',
  },
  editingLabel: {
    id: 'promptsSettings.editingLabel',
    defaultMessage: 'Editing: {name}',
  },
  restoreDefault: {
    id: 'promptsSettings.restoreDefault',
    defaultMessage: 'Restore Default',
  },
  enterPromptContent: {
    id: 'promptsSettings.enterPromptContent',
    defaultMessage: 'Enter prompt content...',
  },
  unsavedChanges: {
    id: 'promptsSettings.unsavedChanges',
    defaultMessage: 'You have unsaved changes',
  },
  promptEditingTitle: {
    id: 'promptsSettings.promptEditingTitle',
    defaultMessage: 'Prompt Editing',
  },
  promptEditingDescription: {
    id: 'promptsSettings.promptEditingDescription',
    defaultMessage: "Customize the prompts that define goose's behavior in different contexts. These prompts use Jinja2 templating syntax. Be careful when modifying template variables, as incorrect changes can break functionality. Please share any improvements with the community.",
  },
  resetAll: {
    id: 'promptsSettings.resetAll',
    defaultMessage: 'Reset All',
  },
  edit: {
    id: 'promptsSettings.edit',
    defaultMessage: 'Edit',
  },
});

export default function PromptsSettingsSection() {
  const intl = useIntl();
  const [prompts, setPrompts] = useState<Template[]>([]);
  const [selectedPrompt, setSelectedPrompt] = useState<string | null>(null);
  const [promptData, setPromptData] = useState<PromptContentResponse | null>(null);
  const [content, setContent] = useState('');
  const [hasChanges, setHasChanges] = useState(false);

  const fetchPrompts = useCallback(async () => {
    try {
      const response = await getPrompts();
      if (response.data) {
        setPrompts(response.data.prompts);
      }
    } catch (error) {
      console.error('Failed to fetch prompts:', error);
      toast.error(intl.formatMessage(i18n.failedToLoadPrompts));
    }
  }, [intl]);

  useEffect(() => {
    fetchPrompts();
  }, [fetchPrompts]);

  useEffect(() => {
    if (selectedPrompt) {
      const fetchPrompt = async () => {
        try {
          const response = await getPrompt({ path: { name: selectedPrompt } });
          if (response.data) {
            setPromptData(response.data);
            setContent(response.data.content);
          }
        } catch (error) {
          console.error('Failed to fetch prompt:', error);
          toast.error(intl.formatMessage(i18n.failedToLoadPrompt));
        }
      };
      fetchPrompt();
    }
  }, [selectedPrompt, intl]);

  useEffect(() => {
    if (promptData) {
      setHasChanges(content !== promptData.content);
    }
  }, [content, promptData]);

  const handleResetAll = async () => {
    if (
      !window.confirm(
        intl.formatMessage(i18n.confirmResetAll)
      )
    ) {
      return;
    }

    try {
      const customizedPrompts = prompts.filter((p) => p.is_customized);
      for (const prompt of customizedPrompts) {
        await resetPrompt({ path: { name: prompt.name } });
      }
      toast.success(intl.formatMessage(i18n.allPromptsReset));
      fetchPrompts();
    } catch (error) {
      console.error('Failed to reset all prompts:', error);
      toast.error(intl.formatMessage(i18n.failedToResetPrompts));
    }
  };

  const handleSave = async () => {
    if (!selectedPrompt) return;
    try {
      await savePrompt({
        path: { name: selectedPrompt },
        body: { content },
      });
      toast.success(intl.formatMessage(i18n.promptSaved));
      setPromptData((prev) => (prev ? { ...prev, content, is_customized: true } : null));
      fetchPrompts();
    } catch (error) {
      console.error('Failed to save prompt:', error);
      toast.error(intl.formatMessage(i18n.failedToSavePrompt));
    }
  };

  const handleReset = async () => {
    if (!selectedPrompt) return;
    if (
      !window.confirm(
        intl.formatMessage(i18n.confirmResetOne)
      )
    ) {
      return;
    }

    try {
      await resetPrompt({ path: { name: selectedPrompt } });
      if (promptData) {
        setContent(promptData.default_content);
        setPromptData({ ...promptData, content: promptData.default_content, is_customized: false });
      }
      fetchPrompts();
      toast.success(intl.formatMessage(i18n.promptResetToDefault));
    } catch (error) {
      console.error('Failed to reset prompt:', error);
      toast.error(intl.formatMessage(i18n.failedToResetPrompt));
    }
  };

  const handleRestoreDefault = () => {
    if (promptData) {
      if (hasChanges) {
        if (!window.confirm(intl.formatMessage(i18n.confirmReplaceWithDefault))) {
          return;
        }
      }
      setContent(promptData.default_content);
    }
  };

  const handleBack = () => {
    if (hasChanges) {
      if (!window.confirm(intl.formatMessage(i18n.confirmUnsavedBack))) {
        return;
      }
    }
    setSelectedPrompt(null);
    setPromptData(null);
    setContent('');
  };

  const hasCustomizedPrompts = prompts.some((p) => p.is_customized);

  if (selectedPrompt) {
    return (
      <div className="space-y-4 pr-4 pb-8 mt-1">
        <Card className="pb-2 rounded-lg">
          <CardHeader className="pb-4">
            <div className="flex items-center justify-between mb-4">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleBack}
                className="flex items-center gap-2"
              >
                <ArrowLeft className="h-4 w-4" />
                {intl.formatMessage(i18n.backToList)}
              </Button>
              <div className="flex items-center gap-2">
                {promptData?.is_customized && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleReset}
                    className="flex items-center gap-2"
                  >
                    <RotateCcw className="h-4 w-4" />
                    {intl.formatMessage(i18n.resetToDefault)}
                  </Button>
                )}
                <Button onClick={handleSave} disabled={!hasChanges} size="sm">
                  {intl.formatMessage(i18n.save)}
                </Button>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <CardTitle>{intl.formatMessage(i18n.editPromptTitle, { name: selectedPrompt })}</CardTitle>
              {promptData?.is_customized && (
                <span className="px-2 py-0.5 text-xs rounded-full bg-blue-500/20 text-blue-600 dark:text-blue-400">
                  {intl.formatMessage(i18n.customized)}
                </span>
              )}
            </div>
          </CardHeader>
          <CardContent className="px-4 space-y-4 flex flex-col h-full">
            <div className="text-sm text-text-secondary bg-background-secondary p-3 rounded-lg">
              <p>
                {intl.formatMessage(i18n.templateTip, {
                  extensionsExample: '{{ extensions }}',
                  forExample: '{% for item in list %}',
                })}
              </p>
            </div>

            <div className="space-y-2 flex-1 flex flex-col min-h-0">
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium">{intl.formatMessage(i18n.editingLabel, { name: selectedPrompt })}</label>
                {promptData?.is_customized && content !== promptData.default_content && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={handleRestoreDefault}
                    className="text-xs"
                  >
                    {intl.formatMessage(i18n.restoreDefault)}
                  </Button>
                )}
              </div>
              <textarea
                value={content}
                className="w-full flex-1 min-h-[500px] border rounded-md p-3 text-sm font-mono resize-y bg-background-primary text-text-primary border-border-primary focus:outline-none focus:ring-2 focus:ring-blue-500"
                onChange={(e) => setContent(e.target.value)}
                placeholder={intl.formatMessage(i18n.enterPromptContent)}
                spellCheck={false}
              />
            </div>

            {hasChanges && (
              <div className="text-sm text-yellow-600 dark:text-yellow-400">
                {intl.formatMessage(i18n.unsavedChanges)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="pb-2 rounded-lg border-yellow-500/50 bg-yellow-500/10">
        <CardHeader className="pb-2">
          <div className="flex items-start gap-3">
            <AlertTriangle className="h-5 w-5 text-yellow-500 flex-shrink-0 mt-1" />
            <div className="flex-1">
              <CardTitle className="text-yellow-600 dark:text-yellow-400">{intl.formatMessage(i18n.promptEditingTitle)}</CardTitle>
              <p className="text-sm text-text-secondary mt-2">
                {intl.formatMessage(i18n.promptEditingDescription)}
              </p>
            </div>
            {hasCustomizedPrompts && (
              <Button
                variant="outline"
                size="sm"
                onClick={handleResetAll}
                className="flex items-center gap-2 border-yellow-500/50 hover:bg-yellow-500/20"
              >
                <RotateCcw className="h-4 w-4" />
                {intl.formatMessage(i18n.resetAll)}
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent className="px-4 pt-4">
          <div className="space-y-2">
            {prompts.map((prompt) => (
              <div
                key={prompt.name}
                className="flex items-center justify-between p-3 rounded-lg border border-border-primary hover:bg-background-secondary transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <h4 className="font-medium text-text-primary truncate">{prompt.name}</h4>
                    {prompt.is_customized && (
                      <span className="px-2 py-0.5 text-xs rounded-full bg-blue-500/20 text-blue-600 dark:text-blue-400">
                        {intl.formatMessage(i18n.customized)}
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-text-secondary mt-0.5 truncate">
                    {prompt.description}
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setSelectedPrompt(prompt.name)}
                  className="ml-4"
                >
                  {intl.formatMessage(i18n.edit)}
                </Button>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
