---
title: Beads Extension
description: Add Beads MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';

<!-- <YouTubeShortEmbed videoUrl="https://www.youtube.com/embed/VIDEO_ID" /> -->

This tutorial covers how to add the [Beads MCP Server](https://github.com/steveyegge/beads) as a goose extension. Beads is a distributed, git-backed issue tracker designed for AI agents. It enables persistent task tracking with dependency management, so multiple goose sessions can coordinate work on complex projects.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=uvx&arg=beads-mcp&id=beads&name=Beads&description=Git-backed%20issue%20tracker%20for%20AI%20agent%20task%20management)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  uvx beads-mcp
  ```
  </TabItem>
</Tabs>
:::

## Configuration

:::info
Note that you'll need [uv](https://docs.astral.sh/uv/#installation) installed on your system to run this command, as it uses `uvx`.
:::

:::note Troubleshooting
If the extension fails to start with `ModuleNotFoundError: No module named 'packaging'` (fixed in versions after 0.49.1), run:
```sh
uv tool install beads-mcp --with packaging
```
:::

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="beads"
      extensionName="Beads"
      description="Git-backed issue tracker for AI agent task management"
      type="stdio"
      command="uvx"
      args={["beads-mcp"]}
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="beads"
      description="Git-backed issue tracker for AI agent task management"
      type="stdio"
      command="uvx beads-mcp"
      timeout={300}
    />
  </TabItem>
</Tabs>

## Example Usage

In this example, we'll use Beads to coordinate building an expense tracker web app across **multiple parallel sessions**. This demonstrates how Beads enables multiple goose instances to work on the same project without conflicts.

### Overview

We'll run **4 goose sessions**:
1. **Step 1**: Create all tasks with dependencies
2. **Step 2**: Build frontend tasks
3. **Step 3**: Build backend tasks (in parallel with Step 2)
4. **Step 4**: Wait for blockers, then wire everything together

---

### Step 1: Plan the Project

Create a new directory and start a goose session.

#### goose Prompt
```
I want to build an expense tracker web app. Initialize beads and create all tasks with dependencies:

Frontend tasks:
- "HTML Structure" - create form, expense list, total display (no dependencies)
- "CSS Styling" - style the app (depends on "HTML Structure")
- "Wire Up UI" - connect buttons to functions (depends on "CSS Styling" AND all backend tasks)

Backend tasks:
- "Storage Setup" - localStorage helpers to save/load expenses (no dependencies)
- "Add Expense" - function to add expense to storage (depends on "Storage Setup")
- "Calculate Total" - function to sum all expenses (depends on "Storage Setup")
- "Delete Expense" - function to remove expense (depends on "Storage Setup")

Show me all tasks and which ones are ready to work on.
```

#### goose Output
```
I'll initialize beads and create all the tasks with their dependencies.

─── context | beads ──────────────────────────
action: init

─── create | beads ──────────────────────────
title: HTML Structure
description: Create form, expense list, and total display for the expense tracker app
labels: frontend

─── create | beads ──────────────────────────
title: Storage Setup
description: Create localStorage helpers to save/load expenses
labels: backend

─── create | beads ──────────────────────────
title: CSS Styling
deps: ultimate-beads-test-987
description: Style the expense tracker app with CSS
labels: frontend

─── create | beads ──────────────────────────
title: Add Expense
deps: ultimate-beads-test-6gs
description: Function to add expense to storage
labels: backend

─── create | beads ──────────────────────────
title: Calculate Total
deps: ultimate-beads-test-6gs
description: Function to sum all expenses
labels: backend

─── create | beads ──────────────────────────
title: Delete Expense
deps: ultimate-beads-test-6gs
description: Function to remove expense from storage
labels: backend

─── create | beads ──────────────────────────
title: Wire Up UI
deps: ultimate-beads-test-vt2, ultimate-beads-test-5x5, ultimate-beads-test-29u, ultimate-beads-test-ae4
description: Connect buttons to functions - wire up the complete UI
labels: frontend

Here's the complete overview of your expense tracker tasks:

## 📋 All Tasks

### Frontend Tasks
| ID | Task | Status | Blocked By |
|----|------|--------|------------|
| `987` | **HTML Structure** | ✅ Ready | - |
| `vt2` | CSS Styling | 🚫 Blocked | HTML Structure |
| `m7i` | Wire Up UI | 🚫 Blocked | CSS Styling, Add Expense, Calculate Total, Delete Expense |

### Backend Tasks
| ID | Task | Status | Blocked By |
|----|------|--------|------------|
| `6gs` | **Storage Setup** | ✅ Ready | - |
| `5x5` | Add Expense | 🚫 Blocked | Storage Setup |
| `29u` | Calculate Total | 🚫 Blocked | Storage Setup |
| `ae4` | Delete Expense | 🚫 Blocked | Storage Setup |

## 📊 Dependency Graph

HTML Structure (987)          Storage Setup (6gs)
       │                      /      |      \
       ▼                     ▼       ▼       ▼
  CSS Styling (vt2)    Add Expense  Calculate  Delete Expense
       │                  (5x5)    Total (29u)    (ae4)
       │                    \        |         /
       └────────────────────────────┼────────┘
                                    ▼
                            Wire Up UI (m7i)

## 🚀 Ready to Work On

Two tasks have no blockers and can be started immediately:

1. **HTML Structure** (frontend)
2. **Storage Setup** (backend)

These two foundation tasks can be worked on **in parallel** since they're independent of each other!
```

---

### Step 2: Build the Frontend

Open a **new terminal** in the same directory and start a new goose session.

#### goose Prompt
```
Check beads for the expense tracker project.

Build the "HTML Structure" task.

When done, mark it complete in beads, then build "CSS Styling" and mark it complete.
```

#### goose Output
```
─── ready | beads ──────────────────────────

📋 Ready work (2 issues with no blockers):
1. [● P2] [task] ultimate-beads-test-987: HTML Structure
2. [● P2] [task] ultimate-beads-test-6gs: Storage Setup

I'll build the HTML Structure task first.

─── text_editor | developer ──────────────────────────
path: index.html
command: write

HTML Structure is complete. Now let me mark it as complete in beads:

─── shell | developer ──────────────────────────
command: bd close ultimate-beads-test-987 -m "Created index.html with expense form, expense list, and total display"

✓ Closed ultimate-beads-test-987

Now let me build the CSS Styling task:

─── text_editor | developer ──────────────────────────
path: styles.css
command: write

─── shell | developer ──────────────────────────
command: bd close ultimate-beads-test-vt2 -m "Created styles.css with responsive design"

✓ Closed ultimate-beads-test-vt2

Let me verify the current state:

─── shell | developer ──────────────────────────
command: bd ready

📋 Ready work (2 issues with no blockers):
1. [● P2] [task] ultimate-beads-test-29u: Calculate Total
2. [● P2] [task] ultimate-beads-test-ae4: Delete Expense

## ✅ Completed Tasks

### 1. HTML Structure (`ultimate-beads-test-987`)
Created `index.html` with:
- **Expense Form** - Input fields for expense name, amount, category, and date
- **Total Display** - Section showing the total expenses amount
- **Expense List** - Table layout with headers for Name, Category, Date, Amount, and Action

### 2. CSS Styling (`ultimate-beads-test-vt2`)
Created `styles.css` with:
- **Modern design** using CSS custom properties for consistent theming
- **Responsive grid layout** for the expense list
- **Mobile-responsive design** with breakpoints

## 📋 Next Ready Tasks
The dependency chain has unblocked new work:
1. Calculate Total
2. Delete Expense
```

---

### Step 3: Build the Backend

Open a **third terminal** in the same directory and start another goose session. Run this in parallel with Step 2.

#### goose Prompt
```
Check beads for the expense tracker project.

Build the "Storage Setup" task and mark it complete.

Then build "Add Expense", "Calculate Total", and "Delete Expense", marking each complete as you go.
```

#### goose Output
```
─── ready | beads ──────────────────────────

📋 Ready work (2 issues with no blockers):
1. [● P2] [task] ultimate-beads-test-987: HTML Structure
2. [● P2] [task] ultimate-beads-test-6gs: Storage Setup

I'll start with Storage Setup.

─── update | beads ──────────────────────────
issue_id: ultimate-beads-test-6gs
status: in_progress

─── text_editor | developer ──────────────────────────
path: storage.js
command: write

─── close | beads ──────────────────────────
issue_id: ultimate-beads-test-6gs
reason: Implemented localStorage helpers: getExpenses, saveExpenses, clearExpenses, generateId

Now let me build "Add Expense":

─── text_editor | developer ──────────────────────────
path: addExpense.js
command: write

─── close | beads ──────────────────────────
issue_id: ultimate-beads-test-5x5
reason: Implemented addExpense() function

Now "Calculate Total":

─── text_editor | developer ──────────────────────────
path: calculateTotal.js
command: write

─── close | beads ──────────────────────────
issue_id: ultimate-beads-test-29u
reason: Implemented calculateTotal() and calculateTotalByCategory()

And finally "Delete Expense":

─── text_editor | developer ──────────────────────────
path: deleteExpense.js
command: write

─── close | beads ──────────────────────────
issue_id: ultimate-beads-test-ae4
reason: Implemented deleteExpense() and deleteExpensesByCategory()

## ✅ Completed Tasks

| Task | ID | File Created |
|------|-----|--------------|
| **Storage Setup** | `6gs` | `storage.js` |
| **Add Expense** | `5x5` | `addExpense.js` |
| **Calculate Total** | `29u` | `calculateTotal.js` |
| **Delete Expense** | `ae4` | `deleteExpense.js` |

All backend tasks complete! The "Wire Up UI" task should now be unblocked.
```

---

### Step 4: Wire It All Together

Open a **fourth terminal** in the same directory. This session will wait for the parallel work to complete before proceeding.

#### goose Prompt
```
Check beads for the expense tracker "Wire Up UI" task.

If blocked, tell me what it's waiting on and keep checking every minute.

If ready, implement it - connect the UI buttons to the backend functions.

When done, mark the task complete in beads.
```

#### goose Output
```
─── blocked | beads ──────────────────────────

Found it! The **"Wire Up UI"** task (`ultimate-beads-test-m7i`) is currently **blocked** by 4 dependencies:

| Blocker ID | Title | Status |
|------------|-------|--------|
| `29u` | Calculate Total | blocked |
| `5x5` | Add Expense | blocked |
| `ae4` | Delete Expense | blocked |
| `vt2` | CSS Styling | blocked |

## 🔄 Status: BLOCKED

The **"Wire Up UI"** task is waiting on:
1. **Add Expense** - Function to add expense to storage
2. **Calculate Total** - Function to sum all expenses
3. **Delete Expense** - Function to remove expense from storage
4. **CSS Styling** - Style the expense tracker app with CSS

I'll check again in 1 minute to see if these blockers have been completed...

─── shell | developer ──────────────────────────
command: sleep 60

─── show | beads ──────────────────────────
issue_id: ultimate-beads-test-m7i

🎉 **All blockers are now CLOSED!** The "Wire Up UI" task is now **READY** to work on!

| Blocker | Title | Status |
|---------|-------|--------|
| `29u` | Calculate Total | ✅ closed |
| `5x5` | Add Expense | ✅ closed |
| `ae4` | Delete Expense | ✅ closed |
| `vt2` | CSS Styling | ✅ closed |

─── update | beads ──────────────────────────
issue_id: ultimate-beads-test-m7i
status: in_progress

─── text_editor | developer ──────────────────────────
path: app.js
command: write

─── close | beads ──────────────────────────
issue_id: ultimate-beads-test-m7i
reason: Implemented app.js to wire up UI - connected form submission to addExpense, delete buttons to deleteExpense, total display to calculateTotal

## ✅ Task Complete: Wire Up UI

**Created `app.js`** that:
- Connects form submission to `addExpense()`
- Renders expenses from storage on page load
- Wires delete buttons to `deleteExpense()`
- Updates total display using `calculateTotal()`

To test it, run: open index.html

Or start a local server: python3 -m http.server 8000 and visit http://localhost:8000
```

