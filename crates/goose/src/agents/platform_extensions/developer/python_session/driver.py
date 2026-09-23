"""goose Python Session driver.

Stdlib-only persistent Python session. Speaks JSON-lines with the goose
process: requests on stdin, responses on the duplicated real stdout.
File descriptor 1 is re-pointed at stderr after startup so stray writes
from user code (e.g. an uncaptured subprocess) cannot corrupt the protocol.
"""

import ast
import contextlib
import importlib
import io
import json
import os
import pickle
import re
import reprlib
import signal
import subprocess
import sys
import threading
import time
import traceback

MAX_CHARS = max(1024, int(os.environ.get("GOOSE_PYTHON_SESSION_MAX_OUTPUT_CHARS", "16384")))
MAX_IMAGES = max(1, int(os.environ.get("GOOSE_PYTHON_SESSION_MAX_IMAGES", "8")))
SH_CAPTURE_CAP = 64 * 1024 * 1024
SH_DRAIN_GRACE_SECS = 0.5
NS_MAX_ENTRIES = 50
NS_MAX_CHARS = 1200
STATE_PATH = os.environ.get("GOOSE_PYTHON_SESSION_STATE_PATH", "")
STATE_VALUE_CAP = 8 * 1024 * 1024
STATE_TOTAL_CAP = 64 * 1024 * 1024
_DRIVER_FILE = globals().get("__file__", "<python-session-driver>")
# Lone surrogates (e.g. from bytes decoded with surrogateescape) cannot be
# serialized to valid JSON; the host's parser rejects them.
_SURROGATE_RE = re.compile("[\ud800-\udfff]")
# Process-group ids of shell commands currently running in their own session,
# so a SIGTERM teardown can reap them instead of orphaning them.
_child_sessions = set()


def _truncate(text, limit=MAX_CHARS):
    if len(text) <= limit:
        return text, False
    marker = (
        "\n[... output truncated at {} chars; the full value is still in your "
        "variable if you assigned it - slice it instead of re-printing ...]"
    ).format(limit)
    return text[:limit] + marker, True


class _BudgetedRepr(reprlib.Repr):
    """repr() of a huge trailing expression materializes all of it before the
    output cap applies. reprlib bounds each container by element count and each
    string by length, but not the total, so a running character budget also
    stops the traversal once the result could no longer fit in a response."""

    def __init__(self, budget):
        super().__init__()
        self._budget = budget
        self._remaining = budget
        for attr in (
            "maxlist",
            "maxtuple",
            "maxdict",
            "maxset",
            "maxfrozenset",
            "maxdeque",
            "maxarray",
            "maxlong",
        ):
            setattr(self, attr, budget)
        self.maxstring = self.maxother = budget
        self.maxlevel = 32

    def repr(self, x):
        self._remaining = self._budget
        return super().repr(x)

    def repr1(self, x, level):
        if self._remaining <= 0:
            return "..."
        text = super().repr1(x, level)
        self._remaining -= len(text)
        return text


_ECHO_REPR = _BudgetedRepr(MAX_CHARS + 16)


def _tail(text, limit):
    text = text.rstrip()
    if len(text) <= limit:
        return text
    return "[... {} chars omitted ...]\n".format(len(text) - limit) + text[-limit:]


class _BoundedWriter(io.StringIO):
    """A stdout/stderr sink that stops growing once a cell floods it, so one
    runaway print cannot exhaust the kernel's memory before output is capped."""

    _HARD_CAP = MAX_CHARS * 4

    def __init__(self):
        super().__init__()
        self._size = 0

    def write(self, text):
        remaining = self._HARD_CAP - self._size
        if remaining > 0:
            chunk = text[:remaining]
            self._size += len(chunk)
            super().write(chunk)
        return len(text)


class ShellResult:
    """Result of sh(); full stdout/stderr stay on the object as .out/.err."""

    def __init__(self, command, code, out, err, timed_out=False):
        self.command = command
        self.code = code
        self.out = out
        self.err = err
        self.timed_out = timed_out

    @property
    def ok(self):
        return self.code == 0

    def __repr__(self):
        status = "exit={}".format(self.code)
        if self.timed_out:
            status += " (timed out; process group killed)"
        parts = ["sh {}: {}".format(json.dumps(self.command), status)]
        if self.out.strip():
            parts.append(_tail(self.out, 4000))
        if self.err.strip():
            parts.append("stderr:\n" + _tail(self.err, 2000))
        return "\n".join(parts)


class _PipeDrain(threading.Thread):
    """Reads one child pipe to EOF, keeping at most SH_CAPTURE_CAP bytes, so a
    command flooding its output cannot grow the kernel until the cell times out."""

    def __init__(self, pipe):
        super().__init__(daemon=True)
        self._pipe = pipe
        self.kept = bytearray()
        self.dropped = 0
        self.start()

    def run(self):
        while True:
            try:
                chunk = os.read(self._pipe.fileno(), 1 << 16)
            except OSError:
                return
            if not chunk:
                return
            room = SH_CAPTURE_CAP - len(self.kept)
            if room > 0:
                self.kept += chunk[:room]
            self.dropped += max(0, len(chunk) - room)

    def text(self, label):
        text = bytes(self.kept).decode("utf-8", errors="replace")
        if self.dropped:
            text += "\n[... {} more bytes of {} dropped; sh() keeps at most {} MiB per stream ...]".format(
                self.dropped, label, SH_CAPTURE_CAP >> 20
            )
        if self.is_alive():
            text += "\n[... {} is still open (a backgrounded process holds it); later output was not captured ...]".format(
                label
            )
        return text


def sh(command, timeout=None, cwd=None, env=None):
    """Run a shell command; returns ShellResult(code, out, err)."""
    posix = os.name == "posix"
    executable = "/bin/bash" if posix and os.path.exists("/bin/bash") else None
    # Running a model-authored command line is this helper's purpose, the same
    # as the Developer shell tool; goose's tool permission mode gates the cell.
    proc = subprocess.Popen(  # nosemgrep: Intersect.semgrep.custom_ruleset.rules.subprocess-shell-true
        command,
        shell=True,
        executable=executable,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=cwd,
        env=env,
        start_new_session=posix,
    )
    if posix:
        _child_sessions.add(proc.pid)
    out = _PipeDrain(proc.stdout)
    err = _PipeDrain(proc.stderr)

    def _kill_group():
        try:
            if posix:
                os.killpg(proc.pid, signal.SIGKILL)
            else:
                # proc.kill() would stop only the shell, orphaning the command it
                # launched (which may still hold the pipes open); taskkill /T ends
                # the whole tree.
                subprocess.run(
                    ["taskkill", "/F", "/T", "/PID", str(proc.pid)],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
        except (ProcessLookupError, PermissionError, OSError):
            pass

    timed_out = False
    try:
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            _kill_group()
            proc.wait()
        except KeyboardInterrupt:
            _kill_group()
            raise
    finally:
        if posix:
            _child_sessions.discard(proc.pid)
    for drain in (out, err):
        drain.join(SH_DRAIN_GRACE_SECS)
    return ShellResult(
        command,
        None if timed_out else proc.returncode,
        out.text("stdout"),
        err.text("stderr"),
        timed_out=timed_out,
    )


def edit(path, old, new):
    """Replace exactly one occurrence of `old` in the file at `path`.

    With old="" and a nonexistent path, creates the file with `new`.
    """
    if old == "" and not os.path.exists(path):
        os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(new)
        return "Created {} ({} chars)".format(path, len(new))
    with open(path, encoding="utf-8", newline="") as f:
        text = f.read()
    count = text.count(old)
    if count == 0:
        raise ValueError("edit: old text not found in {}".format(path))
    if count > 1:
        raise ValueError(
            "edit: old text matches {} times in {}; add surrounding context to "
            "make it unique".format(count, path)
        )
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text.replace(old, new, 1))
    return "Edited {}".format(path)


_pending_images = []
_images_dropped = 0


def view_image(path, crop=None):
    """Show an image (screenshot, diagram, photo) to yourself in this cell's result.

    `path` is a local file or http(s) URL; the host loads the pixels and returns
    them to you, so read a code screenshot or a diagram by VIEWING it, not by OCR.
    `crop=(x, y, width, height)` zooms into a pixel rectangle. Returns None.
    """
    global _images_dropped
    source = str(path)
    if "://" not in source:
        source = os.path.abspath(source)
    req = {"source": source}
    if crop is not None:
        x, y, width, height = (int(v) for v in crop)
        # The host reads these as unsigned 32-bit ints; reject out-of-range values
        # here so a bad crop is a normal cell error, not an unparseable response
        # that hangs the cell until timeout.
        if any(not (0 <= v <= 0xFFFFFFFF) for v in (x, y, width, height)):
            raise ValueError("view_image crop values must be between 0 and 4294967295")
        req["crop"] = {"x": x, "y": y, "width": width, "height": height}
    # The host shows at most MAX_IMAGES per cell; queuing every request from a
    # loop over a large directory would only bloat the response.
    if len(_pending_images) < MAX_IMAGES:
        _pending_images.append(req)
    else:
        _images_dropped += 1


NS = {
    "__name__": "__main__",
    "__builtins__": __builtins__,
    "sh": sh,
    "edit": edit,
    "view_image": view_image,
}
_HELPERS = frozenset(("sh", "edit", "view_image"))
# Names the session owns; everything else in NS, including `_scratch`, belongs
# to the model and is listed and persisted.
_INTERNAL_NAMES = frozenset(("__name__", "__builtins__", "_")) | _HELPERS
_cell_count = 0


def _run_cell(code):
    global _cell_count, _images_dropped
    _cell_count += 1
    del _pending_images[:]
    _images_dropped = 0
    filename = "<cell {}>".format(_cell_count)
    stdout_buf = _BoundedWriter()
    stderr_buf = _BoundedWriter()
    value_repr = None
    error = None
    started = time.monotonic()

    try:
        tree = ast.parse(code, filename=filename, mode="exec")
    except SyntaxError:
        return {
            "ok": False,
            "stdout": "",
            "stderr": "",
            "value": None,
            "error": "".join(traceback.format_exception_only(*sys.exc_info()[:2])).rstrip(),
            "duration_ms": 0,
        }

    trailing_expr = None
    if tree.body and isinstance(tree.body[-1], ast.Expr):
        trailing_expr = ast.Expression(tree.body.pop(-1).value)

    try:
        with contextlib.redirect_stdout(stdout_buf), contextlib.redirect_stderr(stderr_buf):
            if tree.body:
                # The session exists to run the model's code; see the sh() note.
                exec(compile(tree, filename, "exec"), NS)  # nosemgrep: Intersect.semgrep.custom_ruleset.rules.exec-detected
            if trailing_expr is not None:
                value = eval(compile(trailing_expr, filename, "eval"), NS)
                if value is not None:
                    NS["_"] = value
                    value_repr = _ECHO_REPR.repr(value)
    except KeyboardInterrupt:
        error = (
            "KeyboardInterrupt: cell interrupted (timeout or cancellation). "
            "The session and its variables are preserved."
        )
    except SystemExit as exc:
        error = "SystemExit: {!r} (the session keeps running; exit is disabled)".format(
            exc.code
        )
    except BaseException:
        error = _format_traceback()

    duration_ms = int((time.monotonic() - started) * 1000)
    return {
        "ok": error is None,
        "stdout": stdout_buf.getvalue(),
        "stderr": stderr_buf.getvalue(),
        "value": value_repr,
        "error": error,
        "duration_ms": duration_ms,
        "images": list(_pending_images),
        "images_dropped": _images_dropped,
    }


def _format_traceback():
    etype, value, tb = sys.exc_info()
    frames = traceback.extract_tb(tb)
    kept = [f for f in frames if f.filename != _DRIVER_FILE]
    lines = ["Traceback (most recent call last):"]
    lines += traceback.format_list(kept or frames)
    lines += traceback.format_exception_only(etype, value)
    return "".join(
        line if line.endswith("\n") else line + "\n" for line in lines
    ).rstrip()


def _size_hint(value):
    # Exact built-in types only: len()/repr() on subclasses would run user code
    # during the post-cell namespace probe, which could block or kill the kernel
    # outside a cell.
    kind = type(value)
    if kind is bool:
        return repr(value)
    if kind in (int, float):
        try:
            return repr(value)[:40]
        except ValueError:
            # Beyond sys.get_int_max_str_digits().
            return kind.__name__
    if kind in (str, bytes, bytearray, list, tuple, set, frozenset, dict):
        return "{} len={}".format(kind.__name__, len(value))
    return kind.__name__


def _namespace_listing():
    entries = []
    for name, value in NS.items():
        if name in _INTERNAL_NAMES:
            continue
        if isinstance(value, type(sys)):
            continue
        entries.append("{}: {}".format(name, _size_hint(value)))
    omitted = max(0, len(entries) - NS_MAX_ENTRIES)
    listing = "; ".join(entries[:NS_MAX_ENTRIES])
    if len(listing) > NS_MAX_CHARS:
        listing = listing[:NS_MAX_CHARS] + " ..."
    if omitted:
        listing += " (+{} more)".format(omitted)
    return listing


class _CapExceeded(Exception):
    pass


class _CappedSink:
    """A pickle target that aborts once output passes the cap, so an oversized
    variable is skipped without ever materializing its full pickle in memory."""

    def __init__(self, cap):
        self._cap = cap
        self._size = 0
        self._chunks = []

    def write(self, data):
        self._size += len(data)
        if self._size > self._cap:
            raise _CapExceeded()
        self._chunks.append(data)
        return len(data)

    def value(self):
        return b"".join(self._chunks)


def _dump_capped(value, cap):
    sink = _CappedSink(cap)
    try:
        pickle.dump(value, sink, protocol=pickle.HIGHEST_PROTOCOL)
    except Exception:
        return None
    return sink.value()


def _save_state():
    """Best-effort per-variable pickle so the namespace survives process restarts.

    Each value and the snapshot as a whole are capped, so the save that follows
    every cell stays bounded however large the namespace grows.
    """
    if not STATE_PATH:
        return
    names = []
    keep = {}
    sizes = {}
    modules = {}
    for name, value in NS.items():
        if name in _INTERNAL_NAMES:
            continue
        names.append(name)
        if isinstance(value, type(sys)):
            modules[name] = value.__name__
            continue
        blob = _dump_capped(value, STATE_VALUE_CAP)
        if blob is not None:
            keep[name] = value
            sizes[name] = len(blob)
    # Serialize the survivors as one object graph so shared references
    # (e.g. `b = a`) are still shared after a restore. Drop the largest
    # variables until the combined snapshot fits the total cap.
    graph = _dump_capped(keep, STATE_TOTAL_CAP)
    while graph is None and keep:
        del keep[max(keep, key=lambda n: sizes.get(n, 0))]
        graph = _dump_capped(keep, STATE_TOTAL_CAP)
    if graph is None:
        return
    # Two goose processes can hold the same session, so each writer stages its
    # own file before the atomic replace.
    tmp = "%s.%d.tmp" % (STATE_PATH, os.getpid())
    try:
        # Snapshots can hold credentials; create them owner-only (no effect on
        # Windows, which does not use these mode bits). O_NOFOLLOW refuses a
        # planted symlink at the temp path.
        flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_NOFOLLOW", 0)
        fd = os.open(tmp, flags, 0o600)
        with os.fdopen(fd, "wb") as f:
            pickle.dump(
                {
                    "python": sys.version_info[:2],
                    "names": names,
                    "graph": graph,
                    "modules": modules,
                },
                f,
            )
        os.replace(tmp, STATE_PATH)
    except Exception:
        with contextlib.suppress(OSError):
            os.unlink(tmp)


class _Unrestorable:
    """Stands in for a class or function the snapshot names but this process can
    no longer import, so the rest of the graph still loads."""

    def __new__(cls, *args, **kwargs):
        return object.__new__(cls)

    def __init__(self, *args, **kwargs):
        pass

    def __call__(self, *args, **kwargs):
        return _Unrestorable()

    def __setstate__(self, state):
        pass

    # Subclasses of list, dict, and set are rebuilt through these.
    def append(self, item):
        pass

    def extend(self, items):
        pass

    def add(self, item):
        pass

    def __setitem__(self, key, value):
        pass


class _TolerantUnpickler(pickle.Unpickler):
    def find_class(self, module, name):
        try:
            return super().find_class(module, name)
        except Exception:
            return _Unrestorable


class _Tainted(Exception):
    pass


class _TaintCheck(pickle.Pickler):
    def persistent_id(self, obj):
        if obj is _Unrestorable or isinstance(obj, _Unrestorable):
            raise _Tainted
        return None


class _NullSink:
    def write(self, data):
        return len(data)


def _reaches_unrestorable(value):
    try:
        _TaintCheck(_NullSink(), protocol=pickle.HIGHEST_PROTOCOL).dump(value)
    except _Tainted:
        return True
    except Exception:
        return False
    return False


def _load_graph(graph):
    """Returns (values, tolerant): strict load first; if a class or module is
    gone, load with placeholders so only the variables reaching them are lost."""
    try:
        return pickle.loads(graph), False
    except Exception:
        return _TolerantUnpickler(io.BytesIO(graph)).load(), True


def _restore_state():
    """Returns (restored_names, dropped_names) where dropped variables existed at
    save time but could not be persisted or loaded again."""
    if not STATE_PATH or not os.path.exists(STATE_PATH):
        return [], []
    try:
        with open(STATE_PATH, "rb") as f:
            state = pickle.load(f)
        names = list(state.get("names", []))
        if state.get("python") != sys.version_info[:2]:
            return [], names
        graph = state.get("graph", b"")
        values, tolerant = _load_graph(graph) if graph else ({}, False)
        modules = state.get("modules", {})
    except Exception:
        return [], []
    restored = []
    for name, value in values.items():
        if tolerant and _reaches_unrestorable(value):
            continue
        NS[name] = value
        restored.append(name)
    # Module aliases (`import pandas as pd`) are re-imported rather than pickled.
    for alias, module_name in modules.items():
        try:
            NS[alias] = importlib.import_module(module_name)
        except Exception:
            continue
        restored.append(alias)
    dropped = [name for name in names if name not in restored]
    return restored, dropped


def _respond(proto, req_id, payload):
    payload["id"] = req_id
    for key in ("stdout", "stderr", "value", "error"):
        if isinstance(payload.get(key), str):
            payload[key], _ = _truncate(payload[key])
    # Replace lone surrogates before the host parses this line, so inspecting a
    # non-UTF-8 file or filename cannot wedge the cell until its timeout.
    line = _SURROGATE_RE.sub("\ufffd", json.dumps(payload, ensure_ascii=False))
    proto.write(line + "\n")
    proto.flush()


def _terminate(_signum, _frame):
    # Reap shell commands still running in their own session before exiting, so a
    # host teardown (user stop, extension disable) does not orphan them.
    for pgid in list(_child_sessions):
        try:
            os.killpg(pgid, signal.SIGKILL)
        except OSError:
            pass
    os._exit(0)


def main():
    # Explicit UTF-8: responses may carry non-ASCII, and the inherited fd would
    # otherwise encode with the locale (crashing under e.g. LC_ALL=C).
    proto = os.fdopen(
        os.dup(1), "w", buffering=1, encoding="utf-8", errors="backslashreplace"
    )
    os.dup2(2, 1)
    if os.name == "posix":
        signal.signal(signal.SIGTERM, _terminate)

    # The driver runs as a script, so sys.path[0] is its temp directory; make it
    # the current directory instead (like a REPL) so cells can import project
    # modules, and so imports follow later os.chdir calls.
    sys.path[0] = ""

    if sys.version_info < (3, 9):
        _respond(
            proto,
            0,
            {
                "ok": False,
                "error": "Python 3.9 or newer is required, found {}".format(
                    sys.version.split()[0]
                ),
            },
        )
        return

    restored, dropped = _restore_state()
    _respond(
        proto,
        0,
        {
            "ok": True,
            "ready": True,
            "python": sys.version.split()[0],
            "restored": restored,
            "dropped": dropped,
        },
    )

    while True:
        try:
            line = sys.stdin.readline()
        except KeyboardInterrupt:
            continue
        if not line:
            return
        try:
            req = json.loads(line)
        except ValueError:
            continue
        req_id = req.get("id", -1)
        op = req.get("op")
        try:
            if op == "exec":
                result = _run_cell(req.get("code", ""))
                try:
                    _save_state()
                except BaseException:
                    pass
                _respond(proto, req_id, result)
            elif op == "ns":
                _respond(proto, req_id, {"ok": True, "ns": _namespace_listing()})
            elif op == "shutdown":
                _respond(proto, req_id, {"ok": True})
                return
            else:
                _respond(proto, req_id, {"ok": False, "error": "unknown op: {!r}".format(op)})
        except KeyboardInterrupt:
            _respond(proto, req_id, {"ok": False, "error": "KeyboardInterrupt while idle"})


if __name__ == "__main__":
    main()
