import { useState, useEffect } from 'react';
import { X, FolderOpen } from 'lucide-react';
import { Button } from '../../ui/button';
import { SubRecipeFormData } from './recipeFormSchema';
import { useEscapeKey } from '../../../hooks/useEscapeKey';
import KeyValueEditor from './KeyValueEditor';
import { toastError } from '../../../toasts';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  configureTitle: {
    id: 'subRecipeModal.configureTitle',
    defaultMessage: 'Configure Subrecipe',
  },
  addTitle: {
    id: 'subRecipeModal.addTitle',
    defaultMessage: 'Add Subrecipe',
  },
  subtitle: {
    id: 'subRecipeModal.subtitle',
    defaultMessage: 'Configure a subrecipe that can be called as a tool during recipe execution',
  },
  closeModal: {
    id: 'subRecipeModal.closeModal',
    defaultMessage: 'Close subrecipe modal',
  },
  nameLabel: {
    id: 'subRecipeModal.nameLabel',
    defaultMessage: 'Name',
  },
  namePlaceholder: {
    id: 'subRecipeModal.namePlaceholder',
    defaultMessage: 'e.g., security_scan',
  },
  nameHint: {
    id: 'subRecipeModal.nameHint',
    defaultMessage: 'Unique identifier used to generate the tool name',
  },
  pathLabel: {
    id: 'subRecipeModal.pathLabel',
    defaultMessage: 'Path',
  },
  pathPlaceholder: {
    id: 'subRecipeModal.pathPlaceholder',
    defaultMessage: 'e.g., ./subrecipes/security-analysis.yaml',
  },
  browse: {
    id: 'subRecipeModal.browse',
    defaultMessage: 'Browse',
  },
  pathHint: {
    id: 'subRecipeModal.pathHint',
    defaultMessage: 'Browse for an existing recipe file or enter a path manually',
  },
  descriptionLabel: {
    id: 'subRecipeModal.descriptionLabel',
    defaultMessage: 'Description',
  },
  descriptionPlaceholder: {
    id: 'subRecipeModal.descriptionPlaceholder',
    defaultMessage: 'Optional description of what this subrecipe does...',
  },
  sequentialLabel: {
    id: 'subRecipeModal.sequentialLabel',
    defaultMessage: 'Sequential when repeated',
  },
  sequentialHint: {
    id: 'subRecipeModal.sequentialHint',
    defaultMessage: '(Forces sequential execution of multiple subrecipe instances)',
  },
  preconfiguredValues: {
    id: 'subRecipeModal.preconfiguredValues',
    defaultMessage: 'Pre-configured Values',
  },
  preconfiguredValuesHint: {
    id: 'subRecipeModal.preconfiguredValuesHint',
    defaultMessage: 'Optional parameter values that are always passed to the subrecipe',
  },
  cancel: {
    id: 'subRecipeModal.cancel',
    defaultMessage: 'Cancel',
  },
  apply: {
    id: 'subRecipeModal.apply',
    defaultMessage: 'Apply',
  },
  invalidFile: {
    id: 'subRecipeModal.invalidFile',
    defaultMessage: 'Invalid File',
  },
  invalidFileMsg: {
    id: 'subRecipeModal.invalidFileMsg',
    defaultMessage: 'Please select a YAML file (.yaml or .yml).',
  },
});

interface SubRecipeModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (subRecipe: SubRecipeFormData) => boolean;
  subRecipe?: SubRecipeFormData | null;
}

export default function SubRecipeModal({
  isOpen,
  onClose,
  onSave,
  subRecipe,
}: SubRecipeModalProps) {
  const intl = useIntl();
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [description, setDescription] = useState('');
  const [sequentialWhenRepeated, setSequentialWhenRepeated] = useState(false);
  const [values, setValues] = useState<Record<string, string>>({});

  useEscapeKey(isOpen, onClose);

  useEffect(() => {
    if (isOpen) {
      if (subRecipe) {
        setName(subRecipe.name);
        setPath(subRecipe.path);
        setDescription(subRecipe.description || '');
        setSequentialWhenRepeated(subRecipe.sequential_when_repeated ?? false);
        setValues(subRecipe.values || {});
      } else {
        setName('');
        setPath('');
        setDescription('');
        setSequentialWhenRepeated(false);
        setValues({});
      }
    }
  }, [isOpen, subRecipe]);

  const handleSave = () => {
    if (!name.trim() || !path.trim()) {
      return;
    }

    const subRecipeData: SubRecipeFormData = {
      name: name.trim(),
      path: path.trim(),
      description: description.trim() || undefined,
      sequential_when_repeated: sequentialWhenRepeated,
      values: Object.keys(values).length > 0 ? values : undefined,
    };

    if (onSave(subRecipeData)) {
      onClose();
    }
  };

  const handleBrowseFile = async () => {
    try {
      const selectedPath = await window.electron.selectFileOrDirectory();
      if (selectedPath) {
        if (!selectedPath.endsWith('.yaml') && !selectedPath.endsWith('.yml')) {
          toastError({
            title: intl.formatMessage(i18n.invalidFile),
            msg: intl.formatMessage(i18n.invalidFileMsg),
          });
          return;
        }
        setPath(selectedPath);
      }
    } catch (error) {
      console.error('Failed to browse for file:', error);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[500] flex items-center justify-center bg-black/50">
      <div className="bg-background-primary border border-borderSubtle rounded-lg w-[90vw] max-w-2xl max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-borderSubtle">
          <div>
            <h2 className="text-xl font-medium text-textProminent">
              {subRecipe ? intl.formatMessage(i18n.configureTitle) : intl.formatMessage(i18n.addTitle)}
            </h2>
            <p className="text-textSubtle text-sm">
              {intl.formatMessage(i18n.subtitle)}
            </p>
          </div>
          <Button
            onClick={onClose}
            variant="ghost"
            size="sm"
            className="p-2 hover:bg-bgSubtle rounded-lg transition-colors"
            aria-label={intl.formatMessage(i18n.closeModal)}
          >
            <X className="w-5 h-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
          {/* Name Field */}
          <div>
            <label
              htmlFor="subrecipe-name"
              className="block text-sm font-medium text-text-standard mb-2"
            >
              {intl.formatMessage(i18n.nameLabel)} <span className="text-text-danger">*</span>
            </label>
            <input
              id="subrecipe-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={intl.formatMessage(i18n.namePlaceholder)}
            />
            <p className="text-xs text-text-muted mt-1">
              {intl.formatMessage(i18n.nameHint)}
            </p>
          </div>

          {/* Path Field */}
          <div>
            <label
              htmlFor="subrecipe-path"
              className="block text-sm font-medium text-text-standard mb-2"
            >
              {intl.formatMessage(i18n.pathLabel)} <span className="text-text-danger">*</span>
            </label>
            <div className="flex gap-2">
              <input
                id="subrecipe-path"
                type="text"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                className="flex-1 p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder={intl.formatMessage(i18n.pathPlaceholder)}
              />
              <Button
                type="button"
                onClick={handleBrowseFile}
                variant="outline"
                className="px-4 py-2 flex items-center gap-2"
              >
                <FolderOpen className="w-4 h-4" />
                {intl.formatMessage(i18n.browse)}
              </Button>
            </div>
            <p className="text-xs text-text-muted mt-1">
              {intl.formatMessage(i18n.pathHint)}
            </p>
          </div>

          {/* Description Field */}
          <div>
            <label
              htmlFor="subrecipe-description"
              className="block text-sm font-medium text-text-standard mb-2"
            >
              {intl.formatMessage(i18n.descriptionLabel)}
            </label>
            <textarea
              id="subrecipe-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring resize-none"
              placeholder={intl.formatMessage(i18n.descriptionPlaceholder)}
              rows={3}
            />
          </div>

          {/* Sequential When Repeated */}
          <div className="flex items-center gap-2">
            <input
              id="subrecipe-sequential"
              type="checkbox"
              checked={sequentialWhenRepeated}
              onChange={(e) => setSequentialWhenRepeated(e.target.checked)}
              className="w-4 h-4 border-border-subtle rounded focus:ring-2 focus:ring-ring"
            />
            <label htmlFor="subrecipe-sequential" className="text-sm text-text-standard">
              {intl.formatMessage(i18n.sequentialLabel)}
            </label>
            <span className="text-xs text-text-muted">
              {intl.formatMessage(i18n.sequentialHint)}
            </span>
          </div>

          {/* Values Section */}
          <div>
            <label className="block text-sm font-medium text-text-standard mb-2">
              {intl.formatMessage(i18n.preconfiguredValues)}
            </label>
            <p className="text-xs text-text-muted mb-3">
              {intl.formatMessage(i18n.preconfiguredValuesHint)}
            </p>
            <KeyValueEditor values={values} onChange={setValues} />
          </div>
        </div>

        {/* Footer */}
        <div className="flex gap-2 p-6 border-t border-borderSubtle">
          <Button onClick={onClose} variant="outline" className="flex-1">
            {intl.formatMessage(i18n.cancel)}
          </Button>
          <Button onClick={handleSave} disabled={!name.trim() || !path.trim()} className="flex-1">
            {subRecipe ? intl.formatMessage(i18n.apply) : intl.formatMessage(i18n.addTitle)}
          </Button>
        </div>
      </div>
    </div>
  );
}
