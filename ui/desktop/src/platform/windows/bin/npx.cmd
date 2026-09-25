@ECHO OFF
SETLOCAL EnableDelayedExpansion

if not defined GOOSE_NODE_DIR (
    SET "GOOSE_NODE_DIR=%LOCALAPPDATA%\Goose\node"
)
SET "NODE_VERSION=22.14.0"
SET "MIN_SYSTEM_NODE_MAJOR=22"

REM === Check for previously downloaded portable Node.js (matching version) ===
if exist "%GOOSE_NODE_DIR%\node-v%NODE_VERSION%.installed" (
    SET "PATH=%GOOSE_NODE_DIR%;!PATH!"
    "%GOOSE_NODE_DIR%\npx.cmd" %*
    exit /b !errorlevel!
)

REM === Use a system Node.js from PATH ===
REM Look for node.exe, not npx.cmd: a bare npx.cmd lookup can find this wrapper.
for /f "delims=" %%N in ('where $PATH:node.exe 2^>nul') do (
    if exist "%%~dpNnpx.cmd" (
        "%%N" -e "process.exit(parseInt(process.versions.node) >= %MIN_SYSTEM_NODE_MAJOR% ? 0 : 1)" <nul >nul 2>&1
        if "!errorlevel!"=="0" (
            SET "PATH=%%~dpN;!PATH!"
            "%%~dpNnpx.cmd" %*
            exit /b !errorlevel!
        )
    )
)

REM === Download portable Node.js ===
echo [Goose] Node.js not found. Downloading portable Node.js v%NODE_VERSION%... 1>&2

SET "NODE_ZIP=%TEMP%\goose-node-%NODE_VERSION%.zip"
SET "NODE_EXTRACT=%TEMP%\goose-node-extract"

powershell -NoProfile -Command "$ProgressPreference='SilentlyContinue'; try { [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri 'https://nodejs.org/dist/v%NODE_VERSION%/node-v%NODE_VERSION%-win-x64.zip' -OutFile '%NODE_ZIP%' -UseBasicParsing; Expand-Archive -Path '%NODE_ZIP%' -DestinationPath '%NODE_EXTRACT%' -Force; exit 0 } catch { Write-Error $_.Exception.Message; exit 1 }"
if errorlevel 1 (
    echo [Goose] ERROR: Failed to download Node.js. Please install manually from https://nodejs.org/ 1>&2
    del "%NODE_ZIP%" >nul 2>&1
    exit /b 1
)

REM Clean previous version and install to Goose directory
rmdir /s /q "%GOOSE_NODE_DIR%" >nul 2>&1
mkdir "%GOOSE_NODE_DIR%" >nul 2>&1
xcopy /s /e /q /y "%NODE_EXTRACT%\node-v%NODE_VERSION%-win-x64\*" "%GOOSE_NODE_DIR%\" >nul 2>&1

REM Clean up
del "%NODE_ZIP%" >nul 2>&1
rmdir /s /q "%NODE_EXTRACT%" >nul 2>&1

if exist "%GOOSE_NODE_DIR%\npx.cmd" (
    echo.>"%GOOSE_NODE_DIR%\node-v%NODE_VERSION%.installed"
    SET "PATH=%GOOSE_NODE_DIR%;!PATH!"
    echo [Goose] Node.js v%NODE_VERSION% ready. 1>&2
    "%GOOSE_NODE_DIR%\npx.cmd" %*
    exit /b !errorlevel!
)

echo [Goose] ERROR: Installation failed. Please install Node.js manually from https://nodejs.org/ 1>&2
exit /b 1
