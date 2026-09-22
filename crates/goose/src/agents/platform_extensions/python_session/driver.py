"""goose Python Session driver.

Stdlib-only persistent Python session. Speaks JSON-lines with the goose
process: requests on stdin, responses on the duplicated real stdout.
File descriptor 1 is re-pointed at stderr after startup so stray writes
from user code (e.g. an uncaptured subprocess) cannot corrupt the protocol.
"""

import ast
import contextlib
import io
import json
import os
import pickle
import signal
import subprocess
import sys
import time
import traceback

MAX_CHARS = max(1024, int(os.environ.get("GOOSE_PYTHON_SESSION_MAX_OUTPUT_CHARS", "16384")))
NS_MAX_ENTRIES = 50
NS_MAX_CHARS = 1200
STATE_PATH = os.environ.get("GOOSE_PYTHON_SESSION_STATE_PATH", "")
STATE_VALUE_CAP = 8 * 1024 * 1024
_DRIVER_FILE = globals().get("__file__", "<python-session-driver>")


def _truncate(text, limit=MAX_CHARS):
    if len(text) <= limit:
        return text, False
    marker = (
        "\n[... output truncated at {} chars; the full value is still in your "
        "variable if you assigned it - slice it instead of re-printing ...]"
    ).format(limit)
    return text[:limit] + marker, True


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


def sh(command, timeout=None, cwd=None, env=None):
    """Run a shell command; returns ShellResult(code, out, err)."""
    posix = os.name == "posix"
    executable = "/bin/bash" if posix and os.path.exists("/bin/bash") else None
    proc = subprocess.Popen(
        command,
        shell=True,
        executable=executable,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        errors="replace",
        cwd=cwd,
        env=env,
        start_new_session=posix,
    )

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

    try:
        out, err = proc.communicate(timeout=timeout)
        return ShellResult(command, proc.returncode, out, err)
    except subprocess.TimeoutExpired:
        _kill_group()
        out, err = proc.communicate()
        return ShellResult(command, None, out or "", err or "", timed_out=True)
    except KeyboardInterrupt:
        _kill_group()
        raise


def edit(path, old, new):
    """Replace exactly one occurrence of `old` in the file at `path`.

    With old="" and a nonexistent path, creates the file with `new`.
    """
    if old == "" and not os.path.exists(path):
        os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
        with open(path, "w") as f:
            f.write(new)
        return "Created {} ({} chars)".format(path, len(new))
    with open(path) as f:
        text = f.read()
    count = text.count(old)
    if count == 0:
        raise ValueError("edit: old text not found in {}".format(path))
    if count > 1:
        raise ValueError(
            "edit: old text matches {} times in {}; add surrounding context to "
            "make it unique".format(count, path)
        )
    with open(path, "w") as f:
        f.write(text.replace(old, new, 1))
    return "Edited {}".format(path)


_pending_images = []


def view_image(path, crop=None):
    """Show an image (screenshot, diagram, photo) to yourself in this cell's result.

    `path` is a local file or http(s) URL; the host loads the pixels and returns
    them to you, so read a code screenshot or a diagram by VIEWING it, not by OCR.
    `crop=(x, y, width, height)` zooms into a pixel rectangle. Returns None.
    """
    source = str(path)
    if "://" not in source:
        source = os.path.abspath(source)
    req = {"source": source}
    if crop is not None:
        x, y, width, height = crop
        req["crop"] = {"x": int(x), "y": int(y), "width": int(width), "height": int(height)}
    _pending_images.append(req)


NS = {
    "__name__": "__main__",
    "__builtins__": __builtins__,
    "sh": sh,
    "edit": edit,
    "view_image": view_image,
}
_HELPERS = frozenset(("sh", "edit", "view_image"))
_cell_count = 0


def _run_cell(code):
    global _cell_count
    _cell_count += 1
    del _pending_images[:]
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
                exec(compile(tree, filename, "exec"), NS)
            if trailing_expr is not None:
                value = eval(compile(trailing_expr, filename, "eval"), NS)
                if value is not None:
                    NS["_"] = value
                    value_repr = repr(value)
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
    try:
        if isinstance(value, (int, float, bool)):
            return repr(value)[:40]
        if isinstance(value, str):
            return "str len={}".format(len(value))
        if hasattr(value, "shape"):
            return "{} shape={}".format(type(value).__name__, value.shape)
        if hasattr(value, "__len__"):
            return "{} len={}".format(type(value).__name__, len(value))
    except Exception:
        pass
    return type(value).__name__


def _namespace_listing():
    entries = []
    for name, value in NS.items():
        if name.startswith("_") or name in _HELPERS:
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
    """Best-effort per-variable pickle so the namespace survives process restarts."""
    if not STATE_PATH:
        return
    blobs = {}
    names = []
    for name, value in NS.items():
        if name.startswith("_") or name in _HELPERS or isinstance(value, type(sys)):
            continue
        names.append(name)
        blob = _dump_capped(value, STATE_VALUE_CAP)
        if blob is not None:
            blobs[name] = blob
    try:
        tmp = STATE_PATH + ".tmp"
        with open(tmp, "wb") as f:
            pickle.dump(
                {"python": sys.version_info[:2], "names": names, "blobs": blobs}, f
            )
        os.replace(tmp, STATE_PATH)
    except Exception:
        pass


def _restore_state():
    """Returns (restored_names, dropped_names) where dropped variables existed at
    save time but could not be persisted (unpicklable or over the size cap)."""
    if not STATE_PATH or not os.path.exists(STATE_PATH):
        return [], []
    try:
        with open(STATE_PATH, "rb") as f:
            state = pickle.load(f)
        if state.get("python") != sys.version_info[:2]:
            return [], []
        blobs = state.get("blobs", {})
        names = state.get("names", list(blobs.keys()))
    except Exception:
        return [], []
    restored = []
    for name, blob in blobs.items():
        try:
            NS[name] = pickle.loads(blob)
            restored.append(name)
        except Exception:
            pass
    dropped = [name for name in names if name not in NS]
    return restored, dropped


def _respond(proto, req_id, payload):
    payload["id"] = req_id
    for key in ("stdout", "stderr", "value", "error"):
        if isinstance(payload.get(key), str):
            payload[key], _ = _truncate(payload[key])
    proto.write(json.dumps(payload) + "\n")
    proto.flush()


def main():
    proto = os.fdopen(os.dup(1), "w", buffering=1)
    os.dup2(2, 1)

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
