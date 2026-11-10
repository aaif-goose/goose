---
sidebar_position: 6
title: Updating goose
sidebar_label: Updating goose
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { DesktopAutoUpdateSteps } from '@site/src/components/DesktopAutoUpdateSteps';
import MacDesktopInstallButtons from '@site/src/components/MacDesktopInstallButtons';
import WindowsDesktopInstallButtons from '@site/src/components/WindowsDesktopInstallButtons';
import LinuxDesktopInstallButtons from '@site/src/components/LinuxDesktopInstallButtons';

The goose CLI and desktop apps are under active and continuous development. To get the newest features and fixes, you should periodically update your goose client using the following instructions.

<Tabs>
  <TabItem value="mac" label="macOS" default>
    <Tabs groupId="interface">
      <TabItem value="ui" label="goose Desktop" default>
        Update goose to the latest stable version.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <MacDesktopInstallButtons/>
        2. Unzip the downloaded zip file
        3. Drag the extracted `Goose.app` file to the `Applications` folder to overwrite your current version
        4. Launch goose Desktop

      </TabItem>
      <TabItem value="cli" label="goose CLI">
        You can update goose by running:

        ```sh
        goose update
        ```

        Additional [options](/docs/guides/goose-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        goose update --canary

        # Update and reconfigure settings
        goose update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again:

        ```sh
        curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

        To check your current goose version, use the following command:

        ```sh
        goose --version
        ```
      </TabItem>
    </Tabs>
  </TabItem>

  <TabItem value="linux" label="Linux">
    <Tabs groupId="interface">
      <TabItem value="ui" label="goose Desktop" default>
        Update goose to the latest stable version.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <LinuxDesktopInstallButtons/>

        #### For Debian/Ubuntu-based distributions
        2. In a terminal, navigate to the downloaded DEB file
        3. Run `sudo dpkg -i (filename).deb`
        4. Launch goose from the app menu
      </TabItem>
      <TabItem value="cli" label="goose CLI">
        You can update goose by running:

        ```sh
        goose update
        ```

        Additional [options](/docs/guides/goose-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        goose update --canary

        # Update and reconfigure settings
        goose update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again:

        ```sh
        curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

        To check your current goose version, use the following command:

        ```sh
        goose --version
        ```
      </TabItem>
    </Tabs>
  </TabItem>

  <TabItem value="windows" label="Windows">
    <Tabs groupId="interface">
      <TabItem value="ui" label="goose Desktop" default>
        Update goose to the latest stable version. How an update is applied depends on how goose Desktop was installed:

        - **Installer:** goose downloads the new installer, verifies its signature and opens the setup wizard when you click `Install & Restart`. Follow the wizard to complete the update.
        - **Portable ZIP:** goose downloads the new ZIP and replaces the unzipped folder with it. If the automatic update fails, use the manual update steps below.

        Not sure which you have? The installer version is listed under `Installed apps` in Windows Settings; the portable ZIP is not.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <WindowsDesktopInstallButtons/>
        2. Quit goose Desktop
        3. Run the downloaded installer, or unzip the downloaded zip file in place of your existing folder
        4. Launch goose Desktop

        <details>
          <summary>Switching from the portable ZIP to the installer</summary>

          Your settings and sessions are stored in `%APPDATA%\Block\goose`, outside the unzipped folder, so they carry over to the installed copy.

          1. Quit goose Desktop.
          2. Download and run the installer, then launch goose Desktop from the Start Menu.
          3. Delete the old unzipped folder and any shortcuts that point to it.
        </details>
      </TabItem>
      <TabItem value="cli" label="goose CLI">
        You can update goose by running:

        ```sh
        goose update
        ```

        Additional [options](/docs/guides/goose-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        goose update --canary

        # Update and reconfigure settings
        goose update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again in **Git Bash**, **MSYS2**, or **PowerShell** to update the goose CLI natively on Windows:

        ```bash
        curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```
        
        To check your current goose version, use the following command:

        ```sh
        goose --version
        ```        

        <details>
        <summary>Update via Windows Subsystem for Linux (WSL)</summary>

        To update your WSL installation, use `goose update` or run the installation script again via WSL:

        ```sh
        curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

       </details>
      </TabItem>
    </Tabs>
  </TabItem>
</Tabs>

:::info Updating in CI/CD
If you're running goose in CI or other non-interactive environments, pin a specific version with `GOOSE_VERSION` for reproducible installs. See [CI/CD Environments](/docs/tutorials/cicd) for a complete example and usage details.
:::
