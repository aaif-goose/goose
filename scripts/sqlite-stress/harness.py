#!/usr/bin/env python3
"""
SQLite load test harness for goose sessions (issue #12898).

Drives N concurrent ACP sessions against `goose acp`, backed by a fake
OpenAI-compatible LLM so runs are free and repeatable, and samples the
session database while the load runs.

Standard library only. See README.md in this directory for usage.
"""

import argparse
import json
import os
import platform
import queue
import random
import re
import shutil
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BUSY_RE = re.compile(r"database is locked|database table is locked|SQLITE_BUSY|\(code: 5\)", re.I)
SCENARIOS = ("writers", "readers", "external", "all")


# --------------------------------------------------------------------------
# Fake LLM
# --------------------------------------------------------------------------


def make_llm_handler(delay_s, reply_chars):
    filler = ("lorem ipsum dolor sit amet " * (reply_chars // 27 + 1))[:reply_chars]

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def _json(self, status, payload):
            body = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if self.path.rstrip("/").endswith("/models"):
                self._json(200, {"object": "list", "data": [{"id": "harness-model", "object": "model"}]})
            else:
                self._json(404, {"error": {"message": "not found"}})

        def do_POST(self):
            length = int(self.headers.get("Content-Length") or 0)
            try:
                req = json.loads(self.rfile.read(length) or b"{}")
            except ValueError:
                req = {}
            if delay_s:
                time.sleep(delay_s)
            text = f"harness reply {random.randint(0, 1 << 30)}: {filler}"
            usage = {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
            created = int(time.time())
            if not req.get("stream"):
                self._json(200, {
                    "id": "chatcmpl-harness", "object": "chat.completion", "created": created,
                    "model": "harness-model",
                    "choices": [{"index": 0, "finish_reason": "stop",
                                 "message": {"role": "assistant", "content": text}}],
                    "usage": usage,
                })
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "close")
            self.end_headers()
            base = {"id": "chatcmpl-harness", "object": "chat.completion.chunk",
                    "created": created, "model": "harness-model"}
            chunks = [
                {**base, "choices": [{"index": 0, "delta": {"role": "assistant", "content": text},
                                      "finish_reason": None}]},
                {**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
                {**base, "choices": [], "usage": usage},
            ]
            for c in chunks:
                self.wfile.write(f"data: {json.dumps(c)}\n\n".encode())
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            self.close_connection = True

    return Handler


class _LlmServer(ThreadingHTTPServer):
    # The default backlog of 5 delays bursts of concurrent sessions, which would skew turn latency.
    request_queue_size = 128
    daemon_threads = True


def start_fake_llm(delay_s, reply_chars):
    server = _LlmServer(("127.0.0.1", 0), make_llm_handler(delay_s, reply_chars))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


# --------------------------------------------------------------------------
# ACP connection (one goose acp process, many concurrent requests)
# --------------------------------------------------------------------------


class AcpConnection:
    def __init__(self, cmd, env, stderr_path):
        self.stderr_file = open(stderr_path, "w")
        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr_file,
            env=env, text=True, bufsize=1,
        )
        self.next_id = 0
        self.lock = threading.Lock()
        self.pending = {}
        self.notifications = 0
        threading.Thread(target=self._read_loop, daemon=True).start()

    def _read_loop(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if "method" in msg and "id" in msg:
                self._answer_agent_request(msg)
            elif "method" in msg:
                self.notifications += 1
            else:
                q = self.pending.pop(msg.get("id"), None)
                if q:
                    q.put(msg)
        for q in list(self.pending.values()):
            q.put({"error": {"message": "goose acp process exited"}})
        self.pending.clear()

    def _write(self, msg):
        with self.lock:
            self.proc.stdin.write(json.dumps(msg) + "\n")
            self.proc.stdin.flush()

    def _answer_agent_request(self, msg):
        # The harness runs with no extensions, so tool permission prompts should
        # not happen. Cancel anything that does so the turn can finish.
        if msg["method"] == "session/request_permission":
            self._write({"jsonrpc": "2.0", "id": msg["id"], "result": {"outcome": {"outcome": "cancelled"}}})
        else:
            self._write({"jsonrpc": "2.0", "id": msg["id"],
                         "error": {"code": -32601, "message": "not supported by harness"}})

    def request(self, method, params, timeout):
        with self.lock:
            self.next_id += 1
            rid = self.next_id
        q = queue.Queue()
        self.pending[rid] = q
        try:
            self._write({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        except (BrokenPipeError, OSError) as e:
            self.pending.pop(rid, None)
            return {"error": {"message": f"write failed: {e}"}}
        try:
            return q.get(timeout=timeout)
        except queue.Empty:
            self.pending.pop(rid, None)
            return {"error": {"message": f"timeout after {timeout}s"}}

    def close(self):
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        self.stderr_file.close()


# --------------------------------------------------------------------------
# Load generators
# --------------------------------------------------------------------------


def session_worker(conn, idx, args, cwd, stop, records):
    start = time.monotonic()
    resp = conn.request("session/new", {"cwd": cwd, "mcpServers": []}, args.turn_timeout)
    created = time.monotonic()
    if "result" not in resp:
        records.append({"worker": idx, "kind": "session_new", "ok": False,
                        "latency_s": created - start, "error": json.dumps(resp.get("error"))})
        return
    sid = resp["result"]["sessionId"]
    records.append({"worker": idx, "kind": "session_new", "ok": True, "latency_s": created - start})
    for turn in range(args.turns):
        if stop.is_set():
            break
        t0 = time.monotonic()
        resp = conn.request("session/prompt", {
            "sessionId": sid,
            "prompt": [{"type": "text", "text": f"harness worker {idx} turn {turn}"}],
        }, args.turn_timeout)
        dt = time.monotonic() - t0
        if "result" in resp:
            stop_reason = resp["result"].get("stopReason")
            records.append({"worker": idx, "kind": "turn", "turn": turn, "ok": stop_reason == "end_turn",
                            "stop_reason": stop_reason, "latency_s": dt, "t": t0})
        else:
            records.append({"worker": idx, "kind": "turn", "turn": turn, "ok": False,
                            "latency_s": dt, "t": t0, "error": json.dumps(resp.get("error"))})
        if args.think_ms:
            time.sleep(args.think_ms / 1000)


def reader_loop(db_path, hold_s, stop, stats):
    """Hold long read transactions to pin old WAL snapshots."""
    while not stop.is_set() and not db_path.exists():
        time.sleep(0.1)
    conn = sqlite3.connect(str(db_path), timeout=5, isolation_level=None)
    while not stop.is_set():
        try:
            conn.execute("BEGIN")
            conn.execute("SELECT count(*) FROM messages").fetchone()
            stats["reader_snapshots"] += 1
            stop.wait(hold_s)
            conn.execute("COMMIT")
        except sqlite3.OperationalError as e:
            stats["reader_errors"] += 1
            if BUSY_RE.search(str(e)):
                stats["reader_busy"] += 1
            try:
                conn.execute("ROLLBACK")
            except sqlite3.OperationalError:
                pass
            stop.wait(0.2)
    conn.close()


def external_loop(goose_cmd, env, interval_s, stop, stats):
    """Run other goose CLI processes against the same database, like a second client would."""
    while not stop.wait(interval_s):
        t0 = time.monotonic()
        try:
            p = subprocess.run(goose_cmd + ["session", "list", "--format", "json"], env=env,
                               capture_output=True, text=True, timeout=60)
            stats["external_runs"] += 1
            stats["external_latency_s"].append(time.monotonic() - t0)
            if p.returncode != 0:
                stats["external_failures"] += 1
                if BUSY_RE.search(p.stderr or ""):
                    stats["external_busy"] += 1
        except subprocess.TimeoutExpired:
            stats["external_failures"] += 1


# --------------------------------------------------------------------------
# Sampler
# --------------------------------------------------------------------------


def count_db_handles(pids, db_path):
    """Count open file handles on sessions.db held by the goose processes (roughly one per pooled connection)."""
    target = str(db_path.resolve())
    if platform.system() == "Linux":
        n = 0
        for pid in pids:
            try:
                for fd in os.listdir(f"/proc/{pid}/fd"):
                    try:
                        if os.readlink(f"/proc/{pid}/fd/{fd}") == target:
                            n += 1
                    except OSError:
                        pass
            except OSError:
                pass
        return n
    if not pids or not shutil.which("lsof"):
        return None
    out = subprocess.run(["lsof", "-Fn", "-p", ",".join(map(str, pids))], capture_output=True, text=True).stdout
    return sum(1 for line in out.splitlines()
               if line.startswith("n") and os.path.realpath(line[1:]) == target)


def sampler_loop(db_path, pids_fn, args, stop, samples):
    wal = Path(str(db_path) + "-wal")
    conn = None
    probe_table_ready = False
    while not stop.wait(args.sample_interval):
        if not db_path.exists():
            continue
        if conn is None:
            conn = sqlite3.connect(str(db_path), timeout=args.probe_timeout, isolation_level=None)
        s = {"t": time.monotonic(), "wal_bytes": wal.stat().st_size if wal.exists() else 0,
             "db_handles": count_db_handles(pids_fn(), db_path)}
        if args.checkpoint_probe:
            try:
                busy, log, ckpt = conn.execute("PRAGMA wal_checkpoint(PASSIVE)").fetchone()
                s.update(ckpt_busy=busy, ckpt_log=log, ckpt_done=ckpt)
            except sqlite3.OperationalError as e:
                s["ckpt_error"] = str(e)
        if args.write_probe:
            try:
                if not probe_table_ready:
                    conn.execute("CREATE TABLE IF NOT EXISTS harness_probe (id INTEGER PRIMARY KEY, t REAL)")
                    probe_table_ready = True
                t0 = time.monotonic()
                conn.execute("BEGIN IMMEDIATE")
                conn.execute("INSERT INTO harness_probe (t) VALUES (?)", (time.time(),))
                conn.execute("COMMIT")
                s["probe_commit_s"] = time.monotonic() - t0
            except sqlite3.OperationalError as e:
                s["probe_error"] = str(e)
                s["probe_busy"] = bool(BUSY_RE.search(str(e)))
                try:
                    conn.execute("ROLLBACK")
                except sqlite3.OperationalError:
                    pass
        samples.append(s)
    if conn:
        conn.close()


# --------------------------------------------------------------------------
# One run
# --------------------------------------------------------------------------


def pct(values, p):
    if not values:
        return None
    values = sorted(values)
    k = (len(values) - 1) * p / 100
    lo, hi = int(k), min(int(k) + 1, len(values) - 1)
    return values[lo] + (values[hi] - values[lo]) * (k - lo)


def scan_busy(paths):
    hits = 0
    for p in paths:
        try:
            with open(p, errors="replace") as f:
                hits += sum(1 for line in f if BUSY_RE.search(line))
        except OSError:
            pass
    return hits


def run_once(args, n_sessions, llm_url):
    root = Path(tempfile.mkdtemp(prefix=f"goose-stress-{n_sessions}-"))
    work = root / "work"
    work.mkdir()
    db_path = root / "data" / "sessions" / "sessions.db"
    env = dict(os.environ)
    env.update({
        "GOOSE_PATH_ROOT": str(root),
        "GOOSE_DISABLE_KEYRING": "1",
        "GOOSE_PROVIDER": "openai",
        "GOOSE_MODEL": "harness-model",
        "OPENAI_API_KEY": "harness-fake-key",
        "OPENAI_HOST": llm_url,
        "OPENAI_BASE_PATH": "v1/chat/completions",
        "GOOSE_TELEMETRY_ENABLED": "false",
    })
    for kv in args.env:
        k, _, v = kv.partition("=")
        env[k] = v
    goose_cmd = args.goose.split()

    n_procs = n_sessions if args.topology == "processes" else 1
    conns, stderr_paths = [], []
    for i in range(n_procs):
        sp = root / f"goose-acp-{i}.stderr"
        stderr_paths.append(sp)
        conns.append(AcpConnection(goose_cmd + ["acp"], env, sp))

    init_params = {"protocolVersion": 1, "clientCapabilities": {},
                   "clientInfo": {"name": "sqlite-stress-harness", "version": "0.1.0"}}
    for c in conns:
        r = c.request("initialize", init_params, args.turn_timeout)
        if "result" not in r:
            for c2 in conns:
                c2.close()
            raise SystemExit(f"initialize failed: {r}. See {root}")

    stop, sampler_stop = threading.Event(), threading.Event()
    records, samples = [], []
    side = {"reader_snapshots": 0, "reader_errors": 0, "reader_busy": 0, "external_runs": 0,
            "external_failures": 0, "external_busy": 0, "external_latency_s": []}
    threads = [threading.Thread(target=sampler_loop, daemon=True,
                                args=(db_path, lambda: [c.proc.pid for c in conns], args, sampler_stop, samples))]
    if args.scenario in ("readers", "all"):
        for _ in range(args.readers):
            threads.append(threading.Thread(target=reader_loop, daemon=True,
                                            args=(db_path, args.reader_hold, stop, side)))
    if args.scenario in ("external", "all"):
        threads.append(threading.Thread(target=external_loop, daemon=True,
                                        args=(goose_cmd, env, args.external_interval, stop, side)))
    for t in threads:
        t.start()

    t_start = time.monotonic()
    workers = [threading.Thread(target=session_worker,
                                args=(conns[i % n_procs], i, args, str(work), stop, records))
               for i in range(n_sessions)]
    for w in workers:
        w.start()
    deadline = t_start + args.max_duration if args.max_duration else None
    for w in workers:
        w.join(timeout=None if deadline is None else max(0, deadline - time.monotonic()))
    stop.set()
    for w in workers:
        w.join(timeout=args.turn_timeout)
    elapsed = time.monotonic() - t_start
    time.sleep(args.sample_interval)  # one last sample after load stops
    sampler_stop.set()
    for t in threads:
        t.join(timeout=30)
    for c in conns:
        c.close()

    log_files = list((root / "state").rglob("*.log")) if (root / "state").exists() else []
    turns = [r for r in records if r["kind"] == "turn"]
    ok_lat = [r["latency_s"] for r in turns if r["ok"]]
    probe = [s["probe_commit_s"] for s in samples if "probe_commit_s" in s]
    ckpt = [s for s in samples if "ckpt_busy" in s]
    handles = [s["db_handles"] for s in samples if s.get("db_handles") is not None]
    failed = [r for r in turns if not r["ok"]]
    summary = {
        "label": args.label,
        "scenario": args.scenario,
        "topology": args.topology,
        "sessions": n_sessions,
        "goose_processes": n_procs,
        "elapsed_s": round(elapsed, 2),
        "turns_total": len(turns),
        "turns_ok": len(ok_lat),
        "turns_failed": len(failed),
        "turns_failed_busy": sum(1 for r in failed if BUSY_RE.search(r.get("error") or "")),
        "session_new_failed": sum(1 for r in records if r["kind"] == "session_new" and not r["ok"]),
        "turn_p50_s": pct(ok_lat, 50),
        "turn_p95_s": pct(ok_lat, 95),
        "probe_commit_p50_s": pct(probe, 50),
        "probe_commit_p95_s": pct(probe, 95),
        "probe_busy": sum(1 for s in samples if s.get("probe_busy")),
        "probe_samples": len([s for s in samples if "probe_commit_s" in s or "probe_error" in s]),
        "busy_lines_in_goose_logs": scan_busy(stderr_paths + log_files),
        "wal_max_bytes": max((s["wal_bytes"] for s in samples), default=0),
        "wal_final_bytes": samples[-1]["wal_bytes"] if samples else 0,
        "db_handles_max": max(handles, default=None),
        "ckpt_busy_fraction": (sum(1 for s in ckpt if s["ckpt_busy"]) / len(ckpt)) if ckpt else None,
        "ckpt_log_frames_max": max((s["ckpt_log"] for s in ckpt), default=None),
        "ckpt_uncheckpointed_frames_max": max((s["ckpt_log"] - s["ckpt_done"] for s in ckpt if s["ckpt_log"] >= 0),
                                              default=None),
        **{k: v for k, v in side.items() if k != "external_latency_s"},
        "external_p95_s": pct(side["external_latency_s"], 95),
    }
    result = {"summary": summary, "config": {k: v for k, v in vars(args).items() if k != "func"},
              "records": records, "samples": samples, "goose_root": str(root),
              "failure_examples": [r.get("error") for r in failed[:5]]}
    if not args.keep:
        shutil.rmtree(root, ignore_errors=True)
        result["goose_root"] = None
    return result


# --------------------------------------------------------------------------
# Output
# --------------------------------------------------------------------------

COLUMNS = [
    ("label", "label", "{}"), ("scenario", "scenario", "{}"), ("topology", "topology", "{}"),
    ("sessions", "N", "{}"),
    ("turns_ok", "ok", "{}"), ("turns_failed", "fail", "{}"), ("turn_p50_s", "turn p50", "{:.2f}s"),
    ("turn_p95_s", "turn p95", "{:.2f}s"), ("probe_commit_p50_s", "commit p50", "{:.1f}ms"),
    ("probe_commit_p95_s", "commit p95", "{:.1f}ms"), ("probe_busy", "probe busy", "{}"),
    ("busy_lines_in_goose_logs", "busy logs", "{}"), ("wal_max_bytes", "WAL max", "{:.1f}MB"),
    ("ckpt_busy_fraction", "ckpt busy", "{:.0%}"), ("db_handles_max", "db fds", "{}"),
]


def fmt(key, spec, v):
    if v is None:
        return "-"
    if key.startswith("probe_commit"):
        v *= 1000
    if key == "wal_max_bytes":
        v /= 1e6
    return spec.format(v)


def print_table(summaries):
    rows = [[h for _, h, _ in COLUMNS]] + [[fmt(k, s_, s.get(k)) for k, _, s_ in COLUMNS] for s in summaries]
    widths = [max(len(r[i]) for r in rows) for i in range(len(COLUMNS))]
    for j, r in enumerate(rows):
        print("  ".join(c.rjust(w) for c, w in zip(r, widths)))
        if j == 0:
            print("  ".join("-" * w for w in widths))


def cmd_run(args):
    llm = start_fake_llm(args.llm_delay_ms / 1000, args.reply_chars)
    llm_url = f"http://127.0.0.1:{llm.server_address[1]}"
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    summaries = []
    for n in [int(x) for x in args.sessions.split(",")]:
        print(f"[{args.label}] scenario={args.scenario} topology={args.topology} sessions={n} ...", flush=True)
        res = run_once(args, n, llm_url)
        path = out_dir / f"{args.label}-{args.scenario}-{args.topology}-{n}.json"
        path.write_text(json.dumps(res, indent=2))
        s = res["summary"]
        summaries.append(s)
        print(f"  {s['turns_ok']}/{s['turns_total']} turns ok, turn p95 "
              f"{fmt('turn_p95_s', '{:.2f}s', s['turn_p95_s'])}, WAL max "
              f"{fmt('wal_max_bytes', '{:.1f}MB', s['wal_max_bytes'])} -> {path}", flush=True)
        for e in res["failure_examples"]:
            print(f"    failure: {e[:300]}")
        if res["goose_root"]:
            print(f"    kept goose root: {res['goose_root']}")
    llm.shutdown()
    print()
    print_table(summaries)


def cmd_summarize(args):
    summaries = []
    for p in args.files:
        summaries.append(json.loads(Path(p).read_text())["summary"])
    summaries.sort(key=lambda s: (s["scenario"], s["topology"], s["sessions"], s["label"]))
    print_table(summaries)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("run", help="run the load and write one JSON result per session count")
    r.add_argument("--goose", default="goose", help="goose command, e.g. ./target/release/goose (default: goose on PATH)")
    r.add_argument("--label", default="baseline", help="name for this build/config, used in result file names")
    r.add_argument("--scenario", choices=SCENARIOS, default="writers",
                   help="writers: sessions only; readers: + long read snapshots; "
                        "external: + `goose session list` processes; all: everything")
    r.add_argument("--topology", choices=("processes", "shared"), default="processes",
                   help="processes: one `goose acp` per session; shared: one `goose acp` hosting all sessions")
    r.add_argument("--sessions", default="1,4,8,16", help="comma separated session counts to sweep")
    r.add_argument("--turns", type=int, default=20, help="prompts per session")
    r.add_argument("--think-ms", type=int, default=0, help="pause between a session's turns")
    r.add_argument("--turn-timeout", type=float, default=120)
    r.add_argument("--max-duration", type=float, default=0, help="stop a run after this many seconds (0 = no limit)")
    r.add_argument("--llm-delay-ms", type=int, default=50, help="fake LLM latency per request")
    r.add_argument("--reply-chars", type=int, default=2000, help="size of each fake reply (drives WAL growth)")
    r.add_argument("--readers", type=int, default=2, help="long-lived reader connections (readers/all)")
    r.add_argument("--reader-hold", type=float, default=10, help="seconds each reader holds its snapshot")
    r.add_argument("--external-interval", type=float, default=1.0, help="seconds between external CLI runs")
    r.add_argument("--sample-interval", type=float, default=1.0)
    r.add_argument("--probe-timeout", type=float, default=5.0, help="busy timeout for the probe writer")
    r.add_argument("--no-checkpoint-probe", dest="checkpoint_probe", action="store_false",
                   help="don't run PRAGMA wal_checkpoint(PASSIVE) each sample (it checkpoints as a side effect)")
    r.add_argument("--no-write-probe", dest="write_probe", action="store_false",
                   help="don't time a tiny BEGIN IMMEDIATE write each sample")
    r.add_argument("--env", action="append", default=[], metavar="KEY=VALUE",
                   help="extra env for goose processes, for config variants")
    r.add_argument("--out", default="stress-results", help="directory for JSON results")
    r.add_argument("--keep", action="store_true", help="keep the temp goose root (db, logs, stderr)")
    r.set_defaults(func=cmd_run)

    s = sub.add_parser("summarize", help="print one comparison table from earlier result files")
    s.add_argument("files", nargs="+")
    s.set_defaults(func=cmd_summarize)

    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
