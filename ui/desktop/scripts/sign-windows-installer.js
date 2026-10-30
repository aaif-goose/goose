// electron-builder sign hook for the NSIS uninstaller and setup executable. The uninstaller is
// generated mid-build, so the signing job cannot sign it with the workflow action that signs the
// app executables. This runs the same cmdlet with the same arguments as that action, so every
// executable the installer ships carries the same signature.
const { spawnSync } = require('node:child_process');

const settings = {
  Endpoint: 'AZURE_SIGNING_ENDPOINT',
  CodeSigningAccountName: 'AZURE_SIGNING_ACCOUNT_NAME',
  CertificateProfileName: 'AZURE_CERTIFICATE_PROFILE_NAME',
};

// The credential types the action excludes by default. The job signs in through the Azure CLI.
const excludedCredentials = [
  'WorkloadIdentity',
  'ManagedIdentity',
  'SharedTokenCache',
  'VisualStudio',
  'VisualStudioCode',
  'AzurePowerShell',
  'AzureDeveloperCli',
  'InteractiveBrowser',
];

function quote(value) {
  return `'${value.replace(/'/g, "''")}'`;
}

exports.default = async ({ path }) => {
  const missing = Object.values(settings).filter((name) => !process.env[name]);
  if (missing.length > 0) {
    throw new Error(`Cannot sign ${path}: ${missing.join(', ')} not set`);
  }

  const command = [
    'Invoke-ArtifactSigning',
    ...Object.entries(settings).map(([param, name]) => `-${param} ${quote(process.env[name])}`),
    `-Files ${quote(path)}`,
    '-FileDigest SHA256',
    '-TimestampRfc3161 http://timestamp.acs.microsoft.com',
    '-TimestampDigest SHA256',
    ...excludedCredentials.map((name) => `-Exclude${name}Credential`),
  ].join(' ');

  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', command], {
    stdio: 'inherit',
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`Invoke-ArtifactSigning exited with ${result.status} for ${path}`);
  }
};
