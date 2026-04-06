import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { Check } from '../../icons';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { errorMessage } from '../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  dialogTitle: {
    id: 'goosehintsModal.dialogTitle',
    defaultMessage: 'Configure Project Hints (.goosehints)',
  },
  dialogDescription: {
    id: 'goosehintsModal.dialogDescription',
    defaultMessage:
      'Provide additional context about your project to improve communication with Goose',
  },
  helpText1: {
    id: 'goosehintsModal.helpText1',
    defaultMessage:
      '.goosehints is a text file used to provide additional context about your project and improve the communication with Goose.',
  },
  helpText2: {
    id: 'goosehintsModal.helpText2',
    defaultMessage:
      "Please make sure {bold} extension is enabled in the extensions page. This extension is required to use .goosehints. You'll need to restart your session for .goosehints updates to take effect.",
  },
  helpText3: {
    id: 'goosehintsModal.helpText3',
    defaultMessage: 'See {link} for more information.',
  },
  helpTextLink: {
    id: 'goosehintsModal.helpTextLink',
    defaultMessage: 'using .goosehints',
  },
  errorReading: {
    id: 'goosehintsModal.errorReading',
    defaultMessage: 'Error reading .goosehints file: {error}',
  },
  fileFound: {
    id: 'goosehintsModal.fileFound',
    defaultMessage: '.goosehints file found at: {filePath}',
  },
  fileCreating: {
    id: 'goosehintsModal.fileCreating',
    defaultMessage: 'Creating new .goosehints file at: {filePath}',
  },
  placeholder: {
    id: 'goosehintsModal.placeholder',
    defaultMessage: 'Enter project hints here...',
  },
  savedSuccessfully: {
    id: 'goosehintsModal.savedSuccessfully',
    defaultMessage: 'Saved successfully',
  },
  close: {
    id: 'goosehintsModal.close',
    defaultMessage: 'Close',
  },
  saving: {
    id: 'goosehintsModal.saving',
    defaultMessage: 'Saving...',
  },
  save: {
    id: 'goosehintsModal.save',
    defaultMessage: 'Save',
  },
  failedToAccess: {
    id: 'goosehintsModal.failedToAccess',
    defaultMessage: 'Failed to access .goosehints file',
  },
  failedToSave: {
    id: 'goosehintsModal.failedToSave',
    defaultMessage: 'Failed to save .goosehints file',
  },
  developer: {
    id: 'goosehintsModal.developer',
    defaultMessage: 'Developer',
  },
});

const HelpText = () => {
  const intl = useIntl();

  return (
    <div className="text-sm flex-col space-y-4 text-text-secondary">
      <p>{intl.formatMessage(i18n.helpText1)}</p>
      <p>
        {intl.formatMessage(i18n.helpText2, {
          bold: <span className="font-bold">{intl.formatMessage(i18n.developer)}</span>,
        })}
      </p>
      <p>
        {intl.formatMessage(i18n.helpText3, {
          link: (
            <Button
              variant="link"
              className="text-blue-500 hover:text-blue-600 p-0 h-auto"
              onClick={() =>
                window.open(
                  'https://block.github.io/goose/docs/guides/using-goosehints/',
                  '_blank'
                )
              }
            >
              {intl.formatMessage(i18n.helpTextLink)}
            </Button>
          ),
        })}
      </p>
    </div>
  );
};

const ErrorDisplay = ({ error }: { error: Error }) => {
  const intl = useIntl();

  return (
    <div className="text-sm text-text-secondary">
      <div className="text-red-600">
        {intl.formatMessage(i18n.errorReading, { error: errorMessage(error) })}
      </div>
    </div>
  );
};

const FileInfo = ({ filePath, found }: { filePath: string; found: boolean }) => {
  const intl = useIntl();

  return (
    <div className="text-sm font-medium mb-2">
      {found ? (
        <div className="text-green-600">
          <Check className="w-4 h-4 inline-block" />{' '}
          {intl.formatMessage(i18n.fileFound, { filePath })}
        </div>
      ) : (
        <div>{intl.formatMessage(i18n.fileCreating, { filePath })}</div>
      )}
    </div>
  );
};

const getGoosehintsFile = async (filePath: string) => await window.electron.readFile(filePath);

interface GoosehintsModalProps {
  directory: string;
  setIsGoosehintsModalOpen: (isOpen: boolean) => void;
}

export const GoosehintsModal = ({ directory, setIsGoosehintsModalOpen }: GoosehintsModalProps) => {
  const intl = useIntl();
  const goosehintsFilePath = `${directory}/.goosehints`;
  const [goosehintsFile, setGoosehintsFile] = useState<string>('');
  const [goosehintsFileFound, setGoosehintsFileFound] = useState<boolean>(false);
  const [goosehintsFileReadError, setGoosehintsFileReadError] = useState<string>('');
  const [isSaving, setIsSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  useEffect(() => {
    const fetchGoosehintsFile = async () => {
      try {
        const { file, error, found } = await getGoosehintsFile(goosehintsFilePath);
        setGoosehintsFile(file);
        setGoosehintsFileFound(found);
        setGoosehintsFileReadError(found && error ? error : '');
      } catch (error) {
        console.error('Error fetching .goosehints file:', error);
        setGoosehintsFileReadError(intl.formatMessage(i18n.failedToAccess));
      }
    };
    if (directory) fetchGoosehintsFile();
  }, [directory, goosehintsFilePath, intl]);

  const writeFile = async () => {
    setIsSaving(true);
    setSaveSuccess(false);
    try {
      await window.electron.writeFile(goosehintsFilePath, goosehintsFile);
      setSaveSuccess(true);
      setGoosehintsFileFound(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (error) {
      console.error('Error writing .goosehints file:', error);
      setGoosehintsFileReadError(intl.formatMessage(i18n.failedToSave));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={true} onOpenChange={(open) => setIsGoosehintsModalOpen(open)}>
      <DialogContent className="w-[80vw] max-w-[80vw] sm:max-w-[80vw] max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{intl.formatMessage(i18n.dialogTitle)}</DialogTitle>
          <DialogDescription>{intl.formatMessage(i18n.dialogDescription)}</DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 pt-2 pb-4">
          <HelpText />

          <div>
            {goosehintsFileReadError ? (
              <ErrorDisplay error={new Error(goosehintsFileReadError)} />
            ) : (
              <div className="space-y-2">
                <FileInfo filePath={goosehintsFilePath} found={goosehintsFileFound} />
                <textarea
                  value={goosehintsFile}
                  className="w-full h-80 border rounded-md p-2 text-sm resize-none bg-background-primary text-text-primary border-border-primary focus:outline-none focus:ring-2 focus:ring-blue-500"
                  onChange={(event) => setGoosehintsFile(event.target.value)}
                  placeholder={intl.formatMessage(i18n.placeholder)}
                />
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          {saveSuccess && (
            <span className="text-green-600 text-sm flex items-center gap-1 mr-auto">
              <Check className="w-4 h-4" />
              {intl.formatMessage(i18n.savedSuccessfully)}
            </span>
          )}
          <Button variant="outline" onClick={() => setIsGoosehintsModalOpen(false)}>
            {intl.formatMessage(i18n.close)}
          </Button>
          <Button onClick={writeFile} disabled={isSaving}>
            {isSaving ? intl.formatMessage(i18n.saving) : intl.formatMessage(i18n.save)}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
