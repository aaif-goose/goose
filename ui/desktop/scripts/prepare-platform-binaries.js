const fs = require('fs');
const crypto = require('crypto');
const https = require('https');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

// Paths
const srcBinDir = path.join(__dirname, '..', 'src', 'bin');
const platformWinDir = path.join(__dirname, '..', 'src', 'platform', 'windows', 'bin');
const uvVersion = '0.11.11';
const uvDownloadUrl = `https://github.com/astral-sh/uv/releases/download/${uvVersion}/uv-x86_64-pc-windows-msvc.zip`;
const uvBinaryHashes = {
    'uv.exe': 'b1645e948603c12dd741987d0c072471195e18dd299b42334477ceac694f0af8',
    'uvx.exe': '0305c488dc29c16df1483c02a902d21a6798b0744f8e9eb34271d6b3e4bf6e2a',
};

// Platform-specific file patterns
const windowsFiles = [
    '*.exe',
    '*.dll',
    '*.cmd',
    'goose-npm/**/*'
];

const macosFiles = [
    'goosed',
    'goose',
    'jbang',
    'npx',
    'uvx',
    '*.db',
    '*.log',
    '.gitkeep'
];

// Helper function to check if file matches patterns
function matchesPattern(filename, patterns) {
    return patterns.some(pattern => {
        if (pattern.includes('**')) {
            // Handle directory patterns
            const basePattern = pattern.split('/**')[0];
            return filename.startsWith(basePattern);
        } else if (pattern.includes('*')) {
            // Handle wildcard patterns - be more precise with file extensions
            if (pattern.startsWith('*.')) {
                // For file extension patterns like *.exe, *.dll
                const extension = pattern.substring(2); // Remove "*."
                return filename.endsWith('.' + extension);
            } else {
                // For other wildcard patterns
                const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
                return regex.test(filename);
            }
        } else {
            // Exact match
            return filename === pattern;
        }
    });
}

function sha256(filePath) {
    const hash = crypto.createHash('sha256');
    hash.update(fs.readFileSync(filePath));
    return hash.digest('hex');
}

function hasExpectedHash(filePath, expectedHash) {
    return fs.existsSync(filePath) && sha256(filePath) === expectedHash;
}

function downloadFile(url, destPath, redirectsRemaining = 5) {
    return new Promise((resolve, reject) => {
        https.get(url, response => {
            if (
                response.statusCode >= 300 &&
                response.statusCode < 400 &&
                response.headers.location &&
                redirectsRemaining > 0
            ) {
                response.resume();
                downloadFile(response.headers.location, destPath, redirectsRemaining - 1)
                    .then(resolve)
                    .catch(reject);
                return;
            }

            if (response.statusCode !== 200) {
                response.resume();
                reject(new Error(`Failed to download ${url}: HTTP ${response.statusCode}`));
                return;
            }

            const file = fs.createWriteStream(destPath);
            response.pipe(file);
            file.on('finish', () => file.close(resolve));
            file.on('error', reject);
        }).on('error', reject);
    });
}

function extractZip(zipPath, destDir) {
    if (process.platform === 'win32') {
        execFileSync(
            'powershell.exe',
            [
                '-NoProfile',
                '-ExecutionPolicy',
                'Bypass',
                '-Command',
                `Expand-Archive -LiteralPath '${zipPath.replace(/'/g, "''")}' -DestinationPath '${destDir.replace(/'/g, "''")}' -Force`,
            ],
            { stdio: 'inherit' }
        );
        return;
    }

    execFileSync('unzip', ['-q', zipPath, '-d', destDir], { stdio: 'inherit' });
}

async function ensureWindowsUvBinaries() {
    const allPresent = Object.entries(uvBinaryHashes).every(([name, expectedHash]) =>
        hasExpectedHash(path.join(srcBinDir, name), expectedHash)
    );

    if (allPresent) {
        console.log(`Pinned uv ${uvVersion} binaries already present`);
        return;
    }

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-uv-'));
    const zipPath = path.join(tmpDir, 'uv.zip');
    const extractDir = path.join(tmpDir, 'extract');
    fs.mkdirSync(extractDir, { recursive: true });

    try {
        console.log(`Downloading uv ${uvVersion} from ${uvDownloadUrl}`);
        await downloadFile(uvDownloadUrl, zipPath);
        extractZip(zipPath, extractDir);

        for (const [name, expectedHash] of Object.entries(uvBinaryHashes)) {
            const extractedPath = path.join(extractDir, name);
            if (!fs.existsSync(extractedPath)) {
                throw new Error(`Downloaded uv archive did not contain ${name}`);
            }

            const actualHash = sha256(extractedPath);
            if (actualHash !== expectedHash) {
                throw new Error(
                    `${name} checksum mismatch for uv ${uvVersion}: expected ${expectedHash}, got ${actualHash}`
                );
            }

            fs.copyFileSync(extractedPath, path.join(srcBinDir, name));
            console.log(`Copied pinned ${name}`);
        }
    } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
    }
}

// Helper function to clean directory of cross-platform files
function cleanBinDirectory(targetPlatform) {
    console.log(`Cleaning bin directory for ${targetPlatform} build...`);
    
    if (!fs.existsSync(srcBinDir)) {
        console.log('src/bin directory does not exist, skipping cleanup');
        return;
    }

    const files = fs.readdirSync(srcBinDir, { withFileTypes: true });
    
    files.forEach(file => {
        const filePath = path.join(srcBinDir, file.name);
        
        if (targetPlatform === 'darwin' || targetPlatform === 'linux') {
            // For macOS/Linux, remove Windows-specific files
            if (matchesPattern(file.name, windowsFiles)) {
                console.log(`Removing Windows file: ${file.name}`);
                if (file.isDirectory()) {
                    fs.rmSync(filePath, { recursive: true, force: true });
                } else {
                    fs.unlinkSync(filePath);
                }
            }
        } else if (targetPlatform === 'win32') {
            // For Windows, remove macOS-specific files (keep only Windows files and common files)
            if (!matchesPattern(file.name, windowsFiles) && !matchesPattern(file.name, ['*.db', '*.log', '.gitkeep'])) {
                // Check if it's a macOS binary (executable without extension)
                if (file.isFile() && !path.extname(file.name) && file.name !== '.gitkeep') {
                    try {
                        // Check if file is executable (likely a macOS binary)
                        const stats = fs.statSync(filePath);
                        if (stats.mode & parseInt('111', 8)) { // Check if any execute bit is set
                            console.log(`Removing macOS binary: ${file.name}`);
                            fs.unlinkSync(filePath);
                        }
                    } catch (err) {
                        console.warn(`Could not check file ${file.name}:`, err.message);
                    }
                }
            }
        }
    });
}

// Helper function to copy platform-specific files
async function copyPlatformFiles(targetPlatform) {
    if (targetPlatform === 'win32') {
        console.log('Copying Windows-specific files...');
        
        if (!fs.existsSync(platformWinDir)) {
            console.warn('Windows platform directory does not exist');
            return;
        }

        // Ensure src/bin exists
        if (!fs.existsSync(srcBinDir)) {
            fs.mkdirSync(srcBinDir, { recursive: true });
        }

        // Copy Windows-specific scripts and authored support files.
        const files = fs.readdirSync(platformWinDir, { withFileTypes: true });
        files.forEach(file => {
            if (
                file.name === 'README.md' ||
                file.name === '.gitignore' ||
                file.name.endsWith('.exe') ||
                file.name.endsWith('.dll')
            ) {
                return;
            }

            const srcPath = path.join(platformWinDir, file.name);
            const destPath = path.join(srcBinDir, file.name);
            
            if (file.isDirectory()) {
                fs.cpSync(srcPath, destPath, { recursive: true, force: true });
                console.log(`Copied directory: ${file.name}`);
            } else {
                fs.copyFileSync(srcPath, destPath);
                console.log(`Copied: ${file.name}`);
            }
        });

        await ensureWindowsUvBinaries();
    }
}

// Main function
async function preparePlatformBinaries() {
    const targetPlatform = process.env.ELECTRON_PLATFORM || process.platform;
    
    console.log(`Preparing binaries for platform: ${targetPlatform}`);
    
    // First copy platform-specific files if needed
    await copyPlatformFiles(targetPlatform);
    
    // Then clean up cross-platform files
    cleanBinDirectory(targetPlatform);
    
    console.log('Platform binary preparation complete');
}

// Run if called directly
if (require.main === module) {
    preparePlatformBinaries().catch(error => {
        console.error(error);
        process.exit(1);
    });
}

module.exports = { preparePlatformBinaries };
