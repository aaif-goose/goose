import React, { useState } from 'react';
import { AlertTriangle, Download, Github } from 'lucide-react';
import { Button } from './button';
import { toastError } from '../../toasts';
import { diagnostics, systemInfo } from '../../api';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  reportProblem: {
    id: 'diagnosticsModal.reportProblem',
    defaultMessage: 'Report a Problem',
  },
  description: {
    id: 'diagnosticsModal.description',
    defaultMessage:
      'You can download a diagnostics zip file to share with the team, or file a bug directly on GitHub with your system details pre-filled. A diagnostics report contains the following:',
  },
  systemInfo: {
    id: 'diagnosticsModal.systemInfo',
    defaultMessage: 'Basic system info',
  },
  sessionMessages: {
    id: 'diagnosticsModal.sessionMessages',
    defaultMessage: 'Your current session messages',
  },
  logFiles: {
    id: 'diagnosticsModal.logFiles',
    defaultMessage: 'Recent log files',
  },
  configSettings: {
    id: 'diagnosticsModal.configSettings',
    defaultMessage: 'Configuration settings',
  },
  sensitiveWarning: {
    id: 'diagnosticsModal.sensitiveWarning',
    defaultMessage:
      'If your session contains sensitive information, do not share the diagnostics file publicly.',
  },
  attachHint: {
    id: 'diagnosticsModal.attachHint',
    defaultMessage: 'If you file a bug, consider attaching the diagnostics report to it.',
  },
  cancel: {
    id: 'diagnosticsModal.cancel',
    defaultMessage: 'Cancel',
  },
  downloading: {
    id: 'diagnosticsModal.downloading',
    defaultMessage: 'Downloading...',
  },
  download: {
    id: 'diagnosticsModal.download',
    defaultMessage: 'Download',
  },
  opening: {
    id: 'diagnosticsModal.opening',
    defaultMessage: 'Opening...',
  },
  fileBug: {
    id: 'diagnosticsModal.fileBug',
    defaultMessage: 'File Bug on GitHub',
  },
  diagnosticsErrorTitle: {
    id: 'diagnosticsModal.diagnosticsErrorTitle',
    defaultMessage: 'Diagnostics Error',
  },
  diagnosticsErrorMsg: {
    id: 'diagnosticsModal.diagnosticsErrorMsg',
    defaultMessage: 'Failed to download diagnostics',
  },
  systemInfoErrorTitle: {
    id: 'diagnosticsModal.systemInfoErrorTitle',
    defaultMessage: 'Error',
  },
  systemInfoErrorMsg: {
    id: 'diagnosticsModal.systemInfoErrorMsg',
    defaultMessage: 'Failed to get system information',
  },
});

interface DiagnosticsModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
}

export const DiagnosticsModal: React.FC<DiagnosticsModalProps> = ({
  isOpen,
  onClose,
  sessionId,
}) => {
  const intl = useIntl();
  const [isDownloading, setIsDownloading] = useState(false);
  const [isFilingBug, setIsFilingBug] = useState(false);

  const handleDownload = async () => {
    setIsDownloading(true);

    try {
      const response = await diagnostics({
        path: { session_id: sessionId },
        throwOnError: true,
      });

      const blob = new Blob([response.data], { type: 'application/zip' });
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `diagnostics_${sessionId}.zip`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      window.URL.revokeObjectURL(url);

      onClose();
    } catch {
      toastError({
        title: intl.formatMessage(i18n.diagnosticsErrorTitle),
        msg: intl.formatMessage(i18n.diagnosticsErrorMsg),
      });
    } finally {
      setIsDownloading(false);
    }
  };

  const handleFileGitHubIssue = async () => {
    setIsFilingBug(true);

    try {
      const response = await systemInfo({ throwOnError: true });
      const info = response.data;

      const providerModel =
        info.provider && info.model
          ? `${info.provider} – ${info.model}`
          : info.provider || info.model || '[e.g. Google – gemini-1.5-pro]';

      const extensions =
        info.enabled_extensions.length > 0
          ? info.enabled_extensions.join(', ')
          : '[e.g. Computer Controller, Figma]';

      const body = `**Describe the bug**

💡 Before filing, please check common issues:  
https://goose-docs.ai/docs/troubleshooting  

📦 To help us debug faster, attach your **diagnostics zip** if possible.  
👉 How to capture it: https://goose-docs.ai/docs/troubleshooting/diagnostics-and-reporting/

A clear and concise description of what the bug is.

---

**To Reproduce**
Steps to reproduce the behavior:
1. Go to '...'
2. Click on '....'
3. Scroll down to '....'
4. See error

---

**Expected behavior**
A clear and concise description of what you expected to happen.

---

**Screenshots**
If applicable, add screenshots to help explain your problem.

---

**Please provide the following information**
- **OS & Arch:** ${info.os} ${info.os_version} ${info.architecture}
- **Interface:** UI
- **Version:** ${info.app_version}
- **Extensions enabled:** ${extensions}
- **Provider & Model:** ${providerModel}

---

**Additional context**
Add any other context about the problem here.
`;

      const params = new URLSearchParams({
        template: 'bug_report.md',
        body: body,
        labels: 'bug',
      });

      window.open(`https://github.com/aaif-goose/goose/issues/new?${params.toString()}`, '_blank');
      onClose();
    } catch {
      toastError({
        title: intl.formatMessage(i18n.systemInfoErrorTitle),
        msg: intl.formatMessage(i18n.systemInfoErrorMsg),
      });
    } finally {
      setIsFilingBug(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-background-primary border border-border-primary rounded-lg p-6 max-w-md mx-4">
        <div className="flex items-start gap-3 mb-4">
          <AlertTriangle className="text-orange-500 flex-shrink-0 mt-1" size={20} />
          <div>
            <h3 className="text-lg font-semibold text-text-primary mb-2">{intl.formatMessage(i18n.reportProblem)}</h3>
            <p className="text-sm text-text-secondary mb-3">
              {intl.formatMessage(i18n.description)}
            </p>
            <ul className="text-sm text-text-secondary list-disc list-inside space-y-1 mb-3">
              <li>{intl.formatMessage(i18n.systemInfo)}</li>
              <li>{intl.formatMessage(i18n.sessionMessages)}</li>
              <li>{intl.formatMessage(i18n.logFiles)}</li>
              <li>{intl.formatMessage(i18n.configSettings)}</li>
            </ul>
            <p className="text-sm text-text-secondary">
              <strong>Warning:</strong> {intl.formatMessage(i18n.sensitiveWarning)}
            </p>
            <p className="text-sm text-text-secondary">
              {intl.formatMessage(i18n.attachHint)}
            </p>
          </div>
        </div>
        <div className="flex gap-2 justify-end">
          <Button
            onClick={onClose}
            variant="outline"
            size="sm"
            disabled={isDownloading || isFilingBug}
          >
            {intl.formatMessage(i18n.cancel)}
          </Button>
          <Button
            onClick={handleDownload}
            variant="outline"
            size="sm"
            disabled={isDownloading || isFilingBug}
          >
            <Download size={16} className="mr-1" />
            {isDownloading ? intl.formatMessage(i18n.downloading) : intl.formatMessage(i18n.download)}
          </Button>
          <Button
            onClick={handleFileGitHubIssue}
            variant="outline"
            size="sm"
            disabled={isDownloading || isFilingBug}
            className="bg-slate-600 text-white hover:bg-slate-700"
          >
            <Github size={16} className="mr-1" />
            {isFilingBug ? intl.formatMessage(i18n.opening) : intl.formatMessage(i18n.fileBug)}
          </Button>
        </div>
      </div>
    </div>
  );
};
