export const PLUGIN_FRAME_SANDBOX = 'allow-scripts';

const PLUGIN_FRAME_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  'img-src data: blob:',
  'font-src data:',
  'media-src data: blob:',
  "connect-src 'none'",
  "frame-src 'none'",
  "object-src 'none'",
  "form-action 'none'",
  "base-uri 'none'",
].join('; ');

const RELAY_SCRIPT =
  "(function(){var frame=document.getElementById('plugin');" +
  "window.addEventListener('message',function(event){" +
  "if(event.source===window.parent){frame.contentWindow.postMessage(event.data,'*');}" +
  "else if(event.source===frame.contentWindow){window.parent.postMessage(event.data,'*');}" +
  '});})();';

function escapeAttribute(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

export function buildSandboxedDocument(pluginHtml: string): string {
  return (
    '<!doctype html><html><head><meta charset="utf-8">' +
    `<meta http-equiv="Content-Security-Policy" content="${PLUGIN_FRAME_CSP}">` +
    '<meta name="referrer" content="no-referrer">' +
    '<style>html,body{margin:0;height:100%}iframe{display:block;border:0;width:100%;height:100%}</style>' +
    '</head><body>' +
    `<iframe id="plugin" sandbox="${PLUGIN_FRAME_SANDBOX}" srcdoc="${escapeAttribute(pluginHtml)}"></iframe>` +
    `<script>${RELAY_SCRIPT}</script>` +
    '</body></html>'
  );
}
