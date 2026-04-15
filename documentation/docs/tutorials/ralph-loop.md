---
title: Ralph Loop
description: Run goose in a loop with fresh context per iteration and cross-model review
---

Ralph Loop, based on [Geoffrey Huntley's Ralph Wiggum technique](https://ghuntley.com/ralph/), is an iterative development pattern that keeps goose working on a task until it's genuinely complete.

Standard agent loops suffer from context accumulation. Every failed attempt stays in the conversation history, which means that after a few iterations, the model must process a long history of noise before it can focus on the task. Ralph Loop solves this by starting each iteration with fresh context, the core insight behind Geoffrey's approach. This implementation extends the technique with cross-model review: one model does the work, a different model reviews it, and the loop continues until the task is ready to ship.

After each iteration, the worker and reviewer models store a summary and feedback in files. These files persist between iterations but the conversation history does not. This allows a new session to start where the next model reads the files to pick up exactly where the last iteration left off.

In this tutorial, we'll use Ralph Loop to build a simple Electron-based browser and see how the iteration cycle catches missing features before shipping.

### Prerequisites

- [Install the goose CLI](/docs/getting-started/installation) because the Ralph Loop runs via the terminal.
- [Configure two models](/docs/getting-started/providers) to serve as the worker and reviewer. Using different models is recommended for higher quality reviews, though the loop still works if you use the same model for both roles.

<details>
<summary>Download the Ralph Loop Recipes</summary>

Copy and paste this in your terminal to download the Ralph Loop recipes:

```bash
mkdir -p ~/.config/goose/recipes

curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/ralph-loop.sh -o ~/.config/goose/recipes/ralph-loop.sh
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/ralph-work.yaml -o ~/.config/goose/recipes/ralph-work.yaml
curl -sL https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/src/pages/recipes/data/recipes/ralph-review.yaml -o ~/.config/goose/recipes/ralph-review.yaml

chmod +x ~/.config/goose/recipes/ralph-loop.sh
```

</details>

:::warning Cost Warning
Ralph Loop runs your agent multiple times in a loop (up to 10 iterations by default). Monitor your usage and adjust `RALPH_MAX_ITERATIONS` if needed.
:::

### Step 1: Start the Loop

To start the process, run the script from your terminal and provide your prompt in quotes. This command triggers the first iteration of the worker and reviewer cycle:

```bash
~/.config/goose/recipes/ralph-loop.sh "Create a simple browser using Electron and React"
```

:::tip For Complex Tasks
You can pass a file path instead of a string. This works well for PRDs, detailed specs, or any multi-step task that benefits from iterative development:

```bash
~/.config/goose/recipes/ralph-loop.sh ./prd.md
```
:::

### Step 2: Configure Models

The script will ask you to set your environment variables for the session:

```
Worker model [gpt-4o]: 
Worker provider [openai]: 
Reviewer model (should be different from worker): claude-sonnet-4-20250514
Reviewer provider: anthropic
Max iterations [10]: 

⚠️  Cost Warning: This will run up to 10 iterations, each using both models.
    Estimated token usage could be significant depending on your task.

Continue? [y/N]: y
```

| Variable | Description |
|--------|-------------|
| Worker model | The model that does the actual coding work. Defaults to `GOOSE_MODEL` if set. |
| Worker provider | The provider for the worker model (e.g., `openai`, `anthropic`). Defaults to `GOOSE_PROVIDER` if set. |
| Reviewer model | The model that reviews the work. Should be different from the worker for best results. |
| Reviewer provider | The provider for the reviewer model. |
| Max iterations | How many work/review cycles before giving up. Defaults to 10. |

:::tip Directly Set Environment Variables
You can skip the interactive prompts by setting environment variables directly

```bash
RALPH_WORKER_MODEL="gpt-4o" \
RALPH_WORKER_PROVIDER="openai" \
RALPH_REVIEWER_MODEL="claude-sonnet-4-20250514" \
RALPH_REVIEWER_PROVIDER="anthropic" \
~/.config/goose/recipes/ralph-loop.sh "Create a simple browser using Electron and React"
```
:::

### Step 3: Watch It Run

The terminal will show goose moving through the worker and reviewer phases. Each iteration starts with a fresh session to keep the context clean. Here's what a successful run looks like:

```
═══════════════════════════════════════════════════════════════
  Ralph Loop - Multi-Model Edition
═══════════════════════════════════════════════════════════════

  Task: Create a simple browser using Electron and React
  Worker: gpt-4o (openai)
  Reviewer: claude-sonnet-4-20250514 (anthropic)
  Max Iterations: 10

───────────────────────────────────────────────────────────────
  Iteration 1 / 10
───────────────────────────────────────────────────────────────

▶ WORK PHASE
... (goose creates initial implementation) ...

▶ REVIEW PHASE
... (goose reviews the work) ...

↻ REVISE - Feedback for next iteration:
Missing error handling for invalid URLs. Also needs back/forward navigation buttons.

───────────────────────────────────────────────────────────────
  Iteration 2 / 10
───────────────────────────────────────────────────────────────

▶ WORK PHASE
... (goose addresses feedback) ...

▶ REVIEW PHASE
... (goose reviews again) ...

═══════════════════════════════════════════════════════════════
  ✓ SHIPPED after 2 iteration(s)
═══════════════════════════════════════════════════════════════
```

## How It Works

```
Iteration 1:
  WORK PHASE  → Model A does work, writes to files
  REVIEW PHASE → Model B reviews the work
    → SHIP? Exit successfully ✓
    → REVISE? Write feedback, continue to iteration 2

Iteration 2:
  WORK PHASE  → Model A reads feedback, fixes things (fresh context!)
  REVIEW PHASE → Model B reviews again
    → SHIP? Exit successfully ✓
    → REVISE? Continue...

... repeats until SHIP or max iterations
```

### State Files

Ralph Loop uses files in `.goose/ralph/` to persist state between iterations. This is how the worker knows what to do and the reviewer knows what was done, even though each iteration starts with fresh context.

| File | Purpose |
|------|---------|
| `task.md` | The task description |
| `iteration.txt` | Current iteration number |
| `work-summary.txt` | What the worker did this iteration |
| `work-complete.txt` | Exists when worker claims done |
| `review-result.txt` | `SHIP` or `REVISE` |
| `review-feedback.txt` | Feedback for next iteration |
| `.ralph-complete` | Created on successful completion |
| `RALPH-BLOCKED.md` | Created if worker is stuck |

### Recipe Files

The Ralph Loop uses three files: a bash script that orchestrates the work/review cycle, a work recipe that tells the worker model how to make progress, and a review recipe that tells the reviewer model how to evaluate the work. Below are the contents of each file. You can [download](#prerequisites) them or copy directly from here.

<details>
<summary>The Bash Wrapper (`ralph-loop.sh`)</summary>

```bash
#!/bin/bash
#
# Ralph Loop - Multi-Model Edition
#
# Fresh context per iteration + cross-model review
# Based on Geoffrey Huntley's technique
#
# Usage: ./ralph-loop.sh "your task description here"
#    or: ./ralph-loop.sh /path/to/task.md
#
# Environment variables:
#   RALPH_WORKER_MODEL    - Model for work phase (prompts if not set)
#   RALPH_WORKER_PROVIDER - Provider for work phase (prompts if not set)
#   RALPH_REVIEWER_MODEL  - Model for review phase (prompts if not set)
#   RALPH_REVIEWER_PROVIDER - Provider for review phase (prompts if not set)
#   RALPH_MAX_ITERATIONS  - Max iterations (default: 10)
#   RALPH_RECIPE_DIR      - Recipe directory (default: ~/.config/goose/recipes)
#

set -e

INPUT="$1"
RECIPE_DIR="${RALPH_RECIPE_DIR:-$HOME/.config/goose/recipes}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

if [ -z "$INPUT" ]; then
    echo -e "${RED}Error: No task provided${NC}"
    echo "Usage: $0 \"your task description\""
    echo "   or: $0 /path/to/task.md"
    exit 1
fi

# Function to prompt for settings
prompt_for_settings() {
    local default_model="${GOOSE_MODEL:-}"
    local default_provider="${GOOSE_PROVIDER:-}"
    
    # Worker model
    if [ -n "$default_model" ]; then
        echo -ne "${BLUE}Worker model${NC} [${default_model}]: "
        read -r user_input
        WORKER_MODEL="${user_input:-$default_model}"
    else
        echo -ne "${BLUE}Worker model${NC}: "
        read -r WORKER_MODEL
        if [ -z "$WORKER_MODEL" ]; then
            echo -e "${RED}Error: Worker model is required${NC}"
            exit 1
        fi
    fi
    
    # Worker provider
    if [ -n "$default_provider" ]; then
        echo -ne "${BLUE}Worker provider${NC} [${default_provider}]: "
        read -r user_input
        WORKER_PROVIDER="${user_input:-$default_provider}"
    else
        echo -ne "${BLUE}Worker provider${NC}: "
        read -r WORKER_PROVIDER
        if [ -z "$WORKER_PROVIDER" ]; then
            echo -e "${RED}Error: Worker provider is required${NC}"
            exit 1
        fi
    fi
    
    # Reviewer model
    echo -ne "${BLUE}Reviewer model${NC} (should be different from worker): "
    read -r REVIEWER_MODEL
    if [ -z "$REVIEWER_MODEL" ]; then
        echo -e "${RED}Error: Reviewer model is required${NC}"
        echo "The reviewer should be a different model to provide fresh perspective."
        exit 1
    fi
    
    # Reviewer provider
    echo -ne "${BLUE}Reviewer provider${NC}: "
    read -r REVIEWER_PROVIDER
    if [ -z "$REVIEWER_PROVIDER" ]; then
        echo -e "${RED}Error: Reviewer provider is required${NC}"
        exit 1
    fi
    
    # Same model warning
    if [ "$WORKER_MODEL" = "$REVIEWER_MODEL" ] && [ "$WORKER_PROVIDER" = "$REVIEWER_PROVIDER" ]; then
        echo -e "${YELLOW}Warning: Worker and reviewer are the same model.${NC}"
        echo "For best results, use different models for cross-model review."
        echo -ne "Continue anyway? [y/N]: "
        read -r confirm
        if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
            exit 1
        fi
    fi
    
    # Max iterations
    echo -ne "${BLUE}Max iterations${NC} [10]: "
    read -r user_input
    MAX_ITERATIONS="${user_input:-10}"
}

# Initialize from environment variables
WORKER_MODEL="${RALPH_WORKER_MODEL:-}"
WORKER_PROVIDER="${RALPH_WORKER_PROVIDER:-}"
REVIEWER_MODEL="${RALPH_REVIEWER_MODEL:-}"
REVIEWER_PROVIDER="${RALPH_REVIEWER_PROVIDER:-}"
MAX_ITERATIONS="${RALPH_MAX_ITERATIONS:-10}"

# If any required setting is missing, prompt for all settings
if [ -z "$WORKER_MODEL" ] || [ -z "$WORKER_PROVIDER" ] || [ -z "$REVIEWER_MODEL" ] || [ -z "$REVIEWER_PROVIDER" ]; then
    prompt_for_settings
fi

# Cost warning and confirmation loop
while true; do
    echo ""
    echo -e "${YELLOW}⚠️  Cost Warning:${NC} This will run up to ${MAX_ITERATIONS} iterations, each using both models."
    echo "    Estimated token usage could be significant depending on your task."
    echo ""
    echo -ne "Continue? [y/N]: "
    read -r confirm
    
    if [ "$confirm" = "y" ] || [ "$confirm" = "Y" ]; then
        break
    else
        echo ""
        prompt_for_settings
    fi
done

STATE_DIR=".goose/ralph"
mkdir -p "$STATE_DIR"

if [ -f "$INPUT" ]; then
    cp "$INPUT" "$STATE_DIR/task.md"
    echo -e "${BLUE}Reading task from file: $INPUT${NC}"
else
    echo "$INPUT" > "$STATE_DIR/task.md"
fi

TASK=$(cat "$STATE_DIR/task.md")

rm -f "$STATE_DIR/review-result.txt"
rm -f "$STATE_DIR/review-feedback.txt"
rm -f "$STATE_DIR/work-complete.txt"
rm -f "$STATE_DIR/work-summary.txt"

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Ralph Loop - Multi-Model Edition${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  Task: ${YELLOW}$TASK${NC}"
echo -e "  Worker: ${WORKER_MODEL} (${WORKER_PROVIDER})"
echo -e "  Reviewer: ${REVIEWER_MODEL} (${REVIEWER_PROVIDER})"
echo -e "  Max Iterations: $MAX_ITERATIONS"
echo ""

for i in $(seq 1 "$MAX_ITERATIONS"); do
    echo -e "${BLUE}───────────────────────────────────────────────────────────────${NC}"
    echo -e "${BLUE}  Iteration $i / $MAX_ITERATIONS${NC}"
    echo -e "${BLUE}───────────────────────────────────────────────────────────────${NC}"
    
    echo "$i" > "$STATE_DIR/iteration.txt"
    
    echo ""
    echo -e "${YELLOW}▶ WORK PHASE${NC}"
    
    GOOSE_PROVIDER="$WORKER_PROVIDER" GOOSE_MODEL="$WORKER_MODEL" goose run --recipe "$RECIPE_DIR/ralph-work.yaml" || {
        echo -e "${RED}✗ WORK PHASE FAILED${NC}"
        exit 1
    }
    
    if [ -f "$STATE_DIR/RALPH-BLOCKED.md" ]; then
        echo ""
        echo -e "${RED}✗ BLOCKED${NC}"
        cat "$STATE_DIR/RALPH-BLOCKED.md"
        exit 1
    fi
    
    echo ""
    echo -e "${YELLOW}▶ REVIEW PHASE${NC}"
    
    GOOSE_PROVIDER="$REVIEWER_PROVIDER" GOOSE_MODEL="$REVIEWER_MODEL" goose run --recipe "$RECIPE_DIR/ralph-review.yaml" || {
        echo -e "${RED}✗ REVIEW PHASE FAILED${NC}"
        exit 1
    }
    
    if [ -f "$STATE_DIR/review-result.txt" ]; then
        RESULT=$(cat "$STATE_DIR/review-result.txt" | tr -d '[:space:]')
        
        if [ "$RESULT" = "SHIP" ]; then
            echo ""
            echo -e "${GREEN}═══════════════════════════════════════════════════════════════${NC}"
            echo -e "${GREEN}  ✓ SHIPPED after $i iteration(s)${NC}"
            echo -e "${GREEN}═══════════════════════════════════════════════════════════════${NC}"
            echo "COMPLETE: $(date)" > "$STATE_DIR/.ralph-complete"
            exit 0
        else
            echo ""
            echo -e "${YELLOW}↻ REVISE - Feedback for next iteration:${NC}"
            if [ -f "$STATE_DIR/review-feedback.txt" ]; then
                cat "$STATE_DIR/review-feedback.txt"
            fi
        fi
    else
        echo -e "${RED}✗ No review result found${NC}"
        exit 1
    fi
    
    rm -f "$STATE_DIR/work-complete.txt"
    rm -f "$STATE_DIR/review-result.txt"
    echo ""
done

echo -e "${RED}✗ Max iterations ($MAX_ITERATIONS) reached${NC}"
exit 1
```

</details>

<details>
<summary>Work Phase Recipe (`ralph-work.yaml`)</summary>

```yaml
version: 1.0.0
title: Ralph Work Phase
description: Single iteration of work - fresh context each time

instructions: |
  You are in a RALPH LOOP - one iteration of work.
  
  Your work persists through FILES ONLY. You will NOT remember previous iterations.
  
  STATE FILES (in .goose/ralph/):
  - task.md = The task you need to accomplish (READ THIS FIRST)
  - iteration.txt = Current iteration number
  - review-feedback.txt = Feedback from last review (if any)
  - work-complete.txt = Create when task is DONE (reviewer will verify)
  
  FIRST: Check your state
  1. cat .goose/ralph/task.md (YOUR TASK)
  2. cat .goose/ralph/iteration.txt 2>/dev/null || echo "1"
  3. cat .goose/ralph/review-feedback.txt 2>/dev/null
  4. ls -la to see existing work
  
  THEN: Make progress
  - If review-feedback.txt exists, ADDRESS THAT FEEDBACK FIRST
  - Read existing code/files before modifying
  - Make meaningful incremental progress
  - Run tests/verification if applicable
  
  FINALLY: Signal status
  - If task is complete: echo "done" > .goose/ralph/work-complete.txt
  - Always write a summary: echo "what I did" > .goose/ralph/work-summary.txt

prompt: |
  ## Ralph Work Phase
  
  Read your task from: .goose/ralph/task.md
  
  1. Read the task: `cat .goose/ralph/task.md`
  2. Check iteration: `cat .goose/ralph/iteration.txt 2>/dev/null || echo "1"`
  3. Check for review feedback: `cat .goose/ralph/review-feedback.txt 2>/dev/null`
  4. List existing files: `ls -la`
  5. Do the work (address feedback if any, otherwise make progress)
  6. Write summary: `echo "summary" > .goose/ralph/work-summary.txt`
  7. If complete: `echo "done" > .goose/ralph/work-complete.txt`

extensions:
  - type: builtin
    name: developer
    timeout: 600
```

</details>

<details>
<summary>Review Phase Recipe (`ralph-review.yaml`)</summary>

```yaml
version: 1.0.0
title: Ralph Review Phase
description: Cross-model review of work - returns SHIP or REVISE

instructions: |
  You are a CODE REVIEWER in a Ralph Loop.
  
  Your job: Review the work done and decide SHIP or REVISE.
  
  You are a DIFFERENT MODEL than the worker. Your fresh perspective catches mistakes.
  
  STATE FILES (in .goose/ralph/):
  - task.md = The original task (READ THIS FIRST)
  - work-summary.txt = What the worker claims to have done
  - work-complete.txt = Exists if worker claims task is complete
  
  REVIEW CRITERIA:
  1. Does the code/work actually accomplish the task?
  2. Does it run without errors?
  3. Is it reasonably complete, not half-done?
  4. Are there obvious bugs or issues?
  
  BE STRICT but FAIR:
  - Don't nitpick style if functionality is correct
  - DO reject incomplete work
  - DO reject code that doesn't run
  - DO reject if tests fail
  
  OUTPUT:
  If approved: echo "SHIP" > .goose/ralph/review-result.txt
  If needs work: 
    echo "REVISE" > .goose/ralph/review-result.txt
    echo "specific feedback" > .goose/ralph/review-feedback.txt

prompt: |
  ## Ralph Review Phase
  
  1. Read the task: `cat .goose/ralph/task.md`
  2. Read work summary: `cat .goose/ralph/work-summary.txt`
  3. Check if complete: `cat .goose/ralph/work-complete.txt 2>/dev/null`
  4. Examine the actual files created/modified
  5. Run verification (tests, build, etc.)
  6. Decide: SHIP or REVISE
  
  If SHIP: `echo "SHIP" > .goose/ralph/review-result.txt`
  If REVISE: 
    `echo "REVISE" > .goose/ralph/review-result.txt`
    `echo "specific feedback" > .goose/ralph/review-feedback.txt`

extensions:
  - type: builtin
    name: developer
    timeout: 300
```

</details>

## Usage Tips

### When to Use Ralph Loop

Ralph Loop works best for:

- **Complex, multi-step tasks** that benefit from iteration
- **Tasks with clear completion criteria** (tests pass, builds succeed)
- **Situations where you want quality gates** before shipping

It's overkill for:

- Simple one-shot tasks
- Interactive/exploratory work
- Tasks without verifiable completion criteria

### Resetting

If you want to start a completely new task, or if a previous run got stuck and you want to start over, you can clear the state directory:

```bash
rm -rf .goose/ralph
```

