---
title: VMware AIops Extension
description: Add vmware-aiops MCP Server as a goose Extension for natural language VMware vCenter/ESXi operations
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

This tutorial covers how to add [vmware-aiops](https://github.com/zw008/VMware-AIops) as a goose extension, enabling natural language management of VMware vCenter and ESXi infrastructure. With this extension, goose can list VMs, check health, power on/off machines, deploy from templates, run commands inside VMs, and orchestrate multi-step operations with automatic rollback.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    Install vmware-aiops first:
    ```sh
    uv tool install vmware-aiops
    vmware-aiops mcp-config install --agent goose
    ```
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    ```sh
    uv tool install vmware-aiops
    vmware-aiops mcp-config install --agent goose
    ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info Prerequisites
You need [uv](https://docs.astral.sh/uv/) and a running VMware vCenter Server or ESXi host. VMware Tools must be running inside guest VMs for Guest Operations features.
:::

**Step 1: Install and configure credentials**

```sh
uv tool install vmware-aiops

mkdir -p ~/.vmware-aiops
cat > ~/.vmware-aiops/config.yaml << 'EOF'
targets:
  my-vcenter:
    host: vcenter.example.com
    username: administrator@vsphere.local
    password_env: VMWARE_PASSWORD
    verify_ssl: false
EOF

echo "VMWARE_PASSWORD=your_password" > ~/.vmware-aiops/.env
chmod 600 ~/.vmware-aiops/.env
```

**Step 2: Verify connectivity**

```sh
vmware-aiops doctor
```

**Step 3: Add to goose**

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="vmware-aiops"
      extensionName="VMware AIops"
      description="Natural language VMware vCenter/ESXi monitoring and operations"
      type="stdio"
      command="uvx"
      args={["--from", "vmware-aiops", "vmware-aiops-mcp"]}
      timeout={300}
      envVars={[
        { name: "VMWARE_AIOPS_CONFIG", label: "Path to config.yaml (e.g. /Users/you/.vmware-aiops/config.yaml)" }
      ]}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="vmware-aiops"
      description="Natural language VMware vCenter/ESXi monitoring and operations"
      type="stdio"
      command="uvx --from vmware-aiops vmware-aiops-mcp"
      timeout={300}
      envVars={[
        { key: "VMWARE_AIOPS_CONFIG", value: "/Users/you/.vmware-aiops/config.yaml" }
      ]}
      infoNote={
        <>
          Set <code>VMWARE_AIOPS_CONFIG</code> to the absolute path of your config.yaml. See the <a href="https://github.com/zw008/VMware-AIops" target="_blank" rel="noopener noreferrer">vmware-aiops repository</a> for full setup instructions.
        </>
      }
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, I'll use goose to perform a daily health check across my vCenter environment and remediate a disk space issue on a VM — all via natural language.

### goose Prompt

```
Check the health of my vCenter environment. Show any active alarms and
powered-off VMs that haven't been used recently. If datastore usage is
above 85%, tell me which VMs are using the most space.
```

### goose Output

```
I'll start with a full health check of your vCenter environment.

--- get_alarms ---

Found 2 active alarms:
- esxi-03: CPU ready > 5% (Warning) — sustained for 2 hours
- datastore02: 87% capacity used (Critical) ← needs attention

--- list_virtual_machines (power_state=poweredOff) ---

4 powered-off VMs found:
- vm-test-old (last powered on: 21 days ago)
- vm-dev-unused (last powered on: 14 days ago)
- vm-staging-v1 (last powered on: 8 days ago)
- vm-backup-runner (last powered on: 3 days ago)

--- browse_datastore (datastore02) ---

Largest consumers on datastore02:
1. vm-db-prod: 320 GB (snapshots not cleaned up)
2. vm-web-cluster: 180 GB
3. vm-test-old: 95 GB (powered off)

Recommendation: Delete vm-test-old (powered off 21 days, 95 GB) to free
space immediately. Also, vm-db-prod has stale snapshots — consolidating
them would recover ~120 GB.

Shall I create a plan to remove vm-test-old and consolidate vm-db-prod snapshots?
```

```
Yes, go ahead.
```

```
--- vm_create_plan ---

Plan created:
Step 1: Power off check for vm-test-old (already off ✓)
Step 2: Delete vm-test-old
Step 3: Consolidate snapshots on vm-db-prod

⚠️  Step 2 is irreversible. Step 3 is safe (read-only snapshot merge).

--- vm_apply_plan ---

Step 1/3: Verified vm-test-old is powered off ✓
Step 2/3: Deleted vm-test-old — freed 95 GB ✓
Step 3/3: Consolidated vm-db-prod snapshots — freed 118 GB ✓

datastore02 is now at 64% capacity. All alarms should clear within 5 minutes.
```
