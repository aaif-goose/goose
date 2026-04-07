import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../../ui/dialog';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { QRCodeSVG } from 'qrcode.react';
import {
  Loader2,
  Copy,
  Check,
  ChevronDown,
  ChevronUp,
  Info,
  ExternalLink,
  QrCode,
} from 'lucide-react';
import { errorMessage } from '../../../utils/conversionUtils';
import { startTunnel, stopTunnel, getTunnelStatus } from '../../../api/sdk.gen';
import type { TunnelInfo } from '../../../api/types.gen';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  statusIdle: {
    id: 'tunnelSection.statusIdle',
    defaultMessage: 'Tunnel is not running',
  },
  statusStarting: {
    id: 'tunnelSection.statusStarting',
    defaultMessage: 'Starting tunnel...',
  },
  statusRunning: {
    id: 'tunnelSection.statusRunning',
    defaultMessage: 'Tunnel is active',
  },
  statusError: {
    id: 'tunnelSection.statusError',
    defaultMessage: 'Tunnel encountered an error',
  },
  statusDisabled: {
    id: 'tunnelSection.statusDisabled',
    defaultMessage: 'Tunnel is disabled',
  },
  mobileApp: {
    id: 'tunnelSection.mobileApp',
    defaultMessage: 'Mobile App',
  },
  previewFeature: {
    id: 'tunnelSection.previewFeature',
    defaultMessage: 'Preview feature:',
  },
  previewDescription: {
    id: 'tunnelSection.previewDescription',
    defaultMessage: 'Enable remote access to goose from mobile devices using secure tunneling.',
  },
  getIosApp: {
    id: 'tunnelSection.getIosApp',
    defaultMessage: 'Get the iOS app',
  },
  or: {
    id: 'tunnelSection.or',
    defaultMessage: 'or',
  },
  scanQrCode: {
    id: 'tunnelSection.scanQrCode',
    defaultMessage: 'scan QR code',
  },
  tunnelStatus: {
    id: 'tunnelSection.tunnelStatus',
    defaultMessage: 'Tunnel Status',
  },
  starting: {
    id: 'tunnelSection.starting',
    defaultMessage: 'Starting...',
  },
  showQrCode: {
    id: 'tunnelSection.showQrCode',
    defaultMessage: 'Show QR Code',
  },
  stopTunnel: {
    id: 'tunnelSection.stopTunnel',
    defaultMessage: 'Stop Tunnel',
  },
  retry: {
    id: 'tunnelSection.retry',
    defaultMessage: 'Retry',
  },
  startTunnel: {
    id: 'tunnelSection.startTunnel',
    defaultMessage: 'Start Tunnel',
  },
  url: {
    id: 'tunnelSection.url',
    defaultMessage: 'URL:',
  },
  mobileAppConnection: {
    id: 'tunnelSection.mobileAppConnection',
    defaultMessage: 'Mobile App Connection',
  },
  qrCodeInstructions: {
    id: 'tunnelSection.qrCodeInstructions',
    defaultMessage: 'Scan this QR code with the goose mobile app. Do not share this code with anyone else as it is for your personal access.',
  },
  connectionDetails: {
    id: 'tunnelSection.connectionDetails',
    defaultMessage: 'Connection Details',
  },
  tunnelUrl: {
    id: 'tunnelSection.tunnelUrl',
    defaultMessage: 'Tunnel URL',
  },
  secretKey: {
    id: 'tunnelSection.secretKey',
    defaultMessage: 'Secret Key',
  },
  close: {
    id: 'tunnelSection.close',
    defaultMessage: 'Close',
  },
  downloadIosApp: {
    id: 'tunnelSection.downloadIosApp',
    defaultMessage: 'Download goose iOS App',
  },
  appStoreQrInstructions: {
    id: 'tunnelSection.appStoreQrInstructions',
    defaultMessage: 'Scan this QR code with your iPhone camera to install the goose mobile app from the App Store',
  },
  openInAppStore: {
    id: 'tunnelSection.openInAppStore',
    defaultMessage: 'Open in App Store',
  },
  failedToLoadStatus: {
    id: 'tunnelSection.failedToLoadStatus',
    defaultMessage: 'Failed to load tunnel status',
  },
  failedToStopTunnel: {
    id: 'tunnelSection.failedToStopTunnel',
    defaultMessage: 'Failed to stop tunnel',
  },
  failedToStartTunnel: {
    id: 'tunnelSection.failedToStartTunnel',
    defaultMessage: 'Failed to start tunnel',
  },
});

const IOS_APP_STORE_URL = 'https://apps.apple.com/us/app/goose-ai/id6752889295';

const STATUS_MESSAGE_KEYS = {
  idle: 'statusIdle',
  starting: 'statusStarting',
  running: 'statusRunning',
  error: 'statusError',
  disabled: 'statusDisabled',
} as const;

export default function TunnelSection() {
  const intl = useIntl();
  const [tunnelInfo, setTunnelInfo] = useState<TunnelInfo>({
    state: 'idle',
    url: '',
    hostname: '',
    secret: '',
  });
  const [showQRModal, setShowQRModal] = useState(false);
  const [showAppStoreQRModal, setShowAppStoreQRModal] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedUrl, setCopiedUrl] = useState(false);
  const [copiedSecret, setCopiedSecret] = useState(false);
  const [showDetails, setShowDetails] = useState(false);

  useEffect(() => {
    const loadTunnelInfo = async () => {
      try {
        const { data } = await getTunnelStatus();
        if (data) {
          setTunnelInfo(data);
        }
      } catch (err) {
        const errorMsg = errorMessage(err, intl.formatMessage(i18n.failedToLoadStatus));
        setError(errorMsg);
        setTunnelInfo({ state: 'error', url: '', hostname: '', secret: '' });
      }
    };

    loadTunnelInfo();
  }, [intl]);

  const handleToggleTunnel = async () => {
    if (tunnelInfo.state === 'running') {
      try {
        await stopTunnel();
        setTunnelInfo({ state: 'idle', url: '', hostname: '', secret: '' });
        setShowQRModal(false);
      } catch (err) {
        setError(errorMessage(err, intl.formatMessage(i18n.failedToStopTunnel)));
        try {
          const { data } = await getTunnelStatus();
          if (data) {
            setTunnelInfo(data);
          }
        } catch (statusErr) {
          console.error('Failed to fetch tunnel status after stop error:', statusErr);
        }
      }
    } else {
      setError(null);
      setTunnelInfo({ state: 'starting', url: '', hostname: '', secret: '' });

      try {
        const { data } = await startTunnel();
        if (data) {
          setTunnelInfo(data);
          setShowQRModal(true);
        }
      } catch (err) {
        const errorMsg = errorMessage(err, intl.formatMessage(i18n.failedToStartTunnel));
        setError(errorMsg);
        setTunnelInfo({ state: 'error', url: '', hostname: '', secret: '' });
      }
    }
  };

  const copyToClipboard = async (text: string, type: 'url' | 'secret') => {
    try {
      await navigator.clipboard.writeText(text);
      if (type === 'url') {
        setCopiedUrl(true);
        setTimeout(() => setCopiedUrl(false), 2000);
      } else {
        setCopiedSecret(true);
        setTimeout(() => setCopiedSecret(false), 2000);
      }
    } catch (err) {
      console.error('Failed to copy to clipboard:', err);
    }
  };

  const getQRCodeData = () => {
    if (tunnelInfo.state !== 'running') return '';

    const configJson = JSON.stringify({
      url: tunnelInfo.url,
      secret: tunnelInfo.secret,
    });
    const urlEncodedConfig = encodeURIComponent(configJson);
    return `goosechat://configure?data=${urlEncodedConfig}`;
  };

  if (tunnelInfo.state === 'disabled') {
    return null;
  }

  return (
    <>
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.mobileApp)}</CardTitle>
          <CardDescription className="flex flex-col gap-2">
            <div className="flex items-start gap-2 p-2 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded">
              <Info className="h-4 w-4 text-blue-600 dark:text-blue-400 flex-shrink-0 mt-0.5" />
              <div className="text-xs text-blue-800 dark:text-blue-200">
                <strong>{intl.formatMessage(i18n.previewFeature)}</strong> {intl.formatMessage(i18n.previewDescription)}{' '}
                <a
                  href={IOS_APP_STORE_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 underline hover:no-underline"
                >
                  {intl.formatMessage(i18n.getIosApp)}
                  <ExternalLink className="h-3 w-3" />
                </a>
                {' '}{intl.formatMessage(i18n.or)}{' '}
                <button
                  onClick={() => setShowAppStoreQRModal(true)}
                  className="inline-flex items-center gap-1 underline hover:no-underline"
                >
                  {intl.formatMessage(i18n.scanQrCode)}
                  <QrCode className="h-3 w-3" />
                </button>
              </div>
            </div>
          </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-4">
          {error && (
            <div className="p-3 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded text-sm text-red-800 dark:text-red-200">
              {error}
            </div>
          )}

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.tunnelStatus)}</h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n[STATUS_MESSAGE_KEYS[tunnelInfo.state]])}
              </p>
            </div>
            <div className="flex items-center gap-2">
              {tunnelInfo.state === 'starting' ? (
                <Button disabled variant="secondary" size="sm">
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {intl.formatMessage(i18n.starting)}
                </Button>
              ) : tunnelInfo.state === 'running' ? (
                <>
                  <Button onClick={() => setShowQRModal(true)} variant="default" size="sm">
                    {intl.formatMessage(i18n.showQrCode)}
                  </Button>
                  <Button onClick={handleToggleTunnel} variant="destructive" size="sm">
                    {intl.formatMessage(i18n.stopTunnel)}
                  </Button>
                </>
              ) : (
                <Button onClick={handleToggleTunnel} variant="default" size="sm">
                  {tunnelInfo.state === 'error' ? intl.formatMessage(i18n.retry) : intl.formatMessage(i18n.startTunnel)}
                </Button>
              )}
            </div>
          </div>

          {tunnelInfo.state === 'running' && (
            <div className="p-3 bg-green-100 dark:bg-green-900/20 border border-green-300 dark:border-green-800 rounded">
              <p className="text-xs text-green-800 dark:text-green-200">
                <strong>{intl.formatMessage(i18n.url)}</strong> {tunnelInfo.url}
              </p>
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={showQRModal} onOpenChange={setShowQRModal}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>{intl.formatMessage(i18n.mobileAppConnection)}</DialogTitle>
          </DialogHeader>

          {tunnelInfo.state === 'running' && (
            <div className="py-4 space-y-4">
              <div className="flex justify-center">
                <div className="p-4 bg-white rounded-lg">
                  <QRCodeSVG value={getQRCodeData()} size={200} />
                </div>
              </div>

              <div className="text-center text-sm text-text-secondary">
                {intl.formatMessage(i18n.qrCodeInstructions)}
              </div>

              <div className="border-t pt-4">
                <button
                  onClick={() => setShowDetails(!showDetails)}
                  className="flex items-center justify-between w-full text-sm font-medium hover:opacity-70 transition-opacity"
                >
                  <span>{intl.formatMessage(i18n.connectionDetails)}</span>
                  {showDetails ? (
                    <ChevronUp className="h-4 w-4" />
                  ) : (
                    <ChevronDown className="h-4 w-4" />
                  )}
                </button>

                {showDetails && (
                  <div className="mt-3 space-y-3">
                    <div>
                      <h3 className="text-xs font-medium mb-1 text-text-secondary">{intl.formatMessage(i18n.tunnelUrl)}</h3>
                      <div className="flex items-center gap-2">
                        <code className="flex-1 p-2 bg-gray-100 dark:bg-gray-800 rounded text-xs break-all overflow-hidden">
                          {tunnelInfo.url}
                        </code>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="flex-shrink-0"
                          onClick={() => tunnelInfo.url && copyToClipboard(tunnelInfo.url, 'url')}
                        >
                          {copiedUrl ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                        </Button>
                      </div>
                    </div>

                    <div>
                      <h3 className="text-xs font-medium mb-1 text-text-secondary">{intl.formatMessage(i18n.secretKey)}</h3>
                      <div className="flex items-center gap-2">
                        <code className="flex-1 p-2 bg-gray-100 dark:bg-gray-800 rounded text-xs break-all overflow-hidden">
                          {tunnelInfo.secret}
                        </code>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="flex-shrink-0"
                          onClick={() =>
                            tunnelInfo.secret && copyToClipboard(tunnelInfo.secret, 'secret')
                          }
                        >
                          {copiedSecret ? (
                            <Check className="h-4 w-4" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowQRModal(false)}>
              {intl.formatMessage(i18n.close)}
            </Button>
            <Button variant="destructive" onClick={handleToggleTunnel}>
              {intl.formatMessage(i18n.stopTunnel)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={showAppStoreQRModal} onOpenChange={setShowAppStoreQRModal}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>{intl.formatMessage(i18n.downloadIosApp)}</DialogTitle>
          </DialogHeader>

          <div className="py-4 space-y-4">
            <div className="flex justify-center">
              <div className="p-4 bg-white rounded-lg">
                <QRCodeSVG value={IOS_APP_STORE_URL} size={200} />
              </div>
            </div>

            <div className="text-center text-sm text-text-secondary">
              {intl.formatMessage(i18n.appStoreQrInstructions)}
            </div>

            <div className="text-center">
              <a
                href={IOS_APP_STORE_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-2 text-sm text-blue-600 dark:text-blue-400 hover:underline"
              >
                <ExternalLink className="h-4 w-4" />
                {intl.formatMessage(i18n.openInAppStore)}
              </a>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowAppStoreQRModal(false)}>
              {intl.formatMessage(i18n.close)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
