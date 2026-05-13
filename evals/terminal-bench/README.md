# Terminal Bench

This directory contains Terminal Bench evaluation infrastructure for Goose. The
public entrypoint is `goose bench run terminal-bench`. The Python code here is
the Harbor adapter source; the Goose CLI embeds it and writes a generated
package under Goose's state directory before starting Harbor. By default,
generated Harbor configs also go under Goose state, while Harbor job outputs go
under Goose data.
Goose runs on the host and uses a host-side benchmark tool server to operate on
the Terminal Bench task container through Docker.

At a high level:

1. Harbor creates the Terminal Bench task container.
2. The `goose-external` Harbor adapter creates a host-side wrapper named
   `developer`.
3. The wrapper launches Goose's hidden benchmark tool server:

```bash
goose bench tool-server \
  --profile developer \
  --target docker \
  --container <task-container> \
  --workdir <task-container-workdir>
```

4. The adapter launches the host Goose CLI with:

```bash
goose run \
  --no-profile \
  --with-extension "<job-logs-dir>/developer" \
  ...
```

This means benchmark runs exercise the local Goose checkout without building a
Linux Goose binary for the task container or requiring Python inside the task
container.

The executable wrapper must be named `developer`; Goose derives stdio extension
tool prefixes from the command basename, so this preserves the normal
`developer__shell`, `developer__write`, `developer__edit`, and `developer__tree`
tool names. The wrapper runs on the host; the benchmark tool server forwards
tool calls into the container with Docker.

By default, the adapter reads Docker's configured working directory for the task
container and passes it to the tool server. `--workdir` exists as an explicit
override for debugging unusual task environments.

Run a task through the Goose CLI with:

```bash
goose bench run terminal-bench \
  --model databricks/<model-name> \
  --task count-dataset-tokens
```

If your network requires a Python package mirror, pass it with
`--python-index-url`. The runner exposes that URL to both task commands and
verifier commands as `PIP_INDEX_URL`, `UV_DEFAULT_INDEX`, and `UV_INDEX_URL`.
If the flag is omitted, the runner uses the first configured host value from
`UV_DEFAULT_INDEX`, `PIP_INDEX_URL`, or `UV_INDEX_URL`.

By default the runner passes the current Goose executable to Harbor, so it
exercises the same Goose binary that launched the benchmark. Use
`--goose-binary /path/to/goose` to run a different host Goose binary instead.
Use `--dry-run` to write the Harbor config without starting a benchmark job.

Run the unit tests with:

```bash
python3 -m unittest discover -s evals/terminal-bench/tests
```
