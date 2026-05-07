# Grader runners

Small, single-purpose executables that recipes invoke as L1 graders. Reusable across recipes.

## Contract

Each runner is a Python script (`python eval-bench/grader_runners/<name>.py`) that:

- Reads a JSON object from **stdin** (or from a path passed with `--input PATH`).
  ```json
  {"output": "<recipe output text>", "task": { ...task fields from tasks.jsonl... }}
  ```
- Writes a one-line JSON result to **stdout**:
  ```json
  {"passed": true, "score": 1.0, "details": "all required sections present"}
  ```
- Exits **0** on pass, **1** on fail, **2** on input/programming error.

Runner-specific options are passed as `argparse` flags after the script path. The recipe's `graders.yaml` declares the full invocation in the `runner` field.

```yaml
graders:
  - id: g-output-shape
    level: L1
    type: code
    weight: 1.0
    runner: "python eval-bench/grader_runners/output_shape.py"
  - id: g-charter-sections
    level: L1
    type: code
    weight: 1.0
    runner: "python eval-bench/grader_runners/markdown_sections.py --required Structure,Function,Data,Interfaces,Platform,Operations,Time"
```

## Available runners

| Runner | Purpose |
|---|---|
| `output_shape.py` | Generic floor: output is a non-empty string under a configurable max length. |
| `markdown_sections.py` | Output is markdown with all required H2 (`## Section`) headers present. Used by recipes whose contract is "produce a structured markdown document with these sections." |

## Adding a new runner

1. Pick a single, sharply-scoped invariant. Don't bundle multiple checks into one runner — one dimension per grader (Anthropic discipline).
2. Subclass nothing; reuse `_common.py` for stdin parsing and result emission.
3. Ship tests in `eval-bench/tests/grader_runners/`. Cover happy path, the most likely failure paths, and the error path (malformed input on stdin should exit 2, not 1).
