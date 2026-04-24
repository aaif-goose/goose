#!/usr/bin/env python3
"""
Simple ACP client to test the goose ACP agent.
Connects to goose acp running on stdio.

Tests:
1. Initialize - Establish connection and verify capabilities
2. session/new - Create a new session
3. session/prompt - Send a prompt to the session
4. session/load - Load an existing session (new feature)

Usage:
    # Uses `cargo run -p goose-cli -- acp` by default.
    python3 test_acp_client.py

    # Faster: point at a pre-built binary via env var.
    GOOSE_BIN=target/debug/goose.exe python3 test_acp_client.py
"""

import json
import os
import shutil
import subprocess
import sys
import time

# Windows consoles default to cp1252, which can't encode the unicode glyphs
# used in test progress output (✓, ✗, 📝). Force UTF-8 so the test runs
# uniformly on Windows, Linux, and macOS.
if sys.stdout.encoding and sys.stdout.encoding.lower() != "utf-8":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass


def _resolve_goose_cmd():
    """Return the argv to launch the ACP server.

    Preference order:
      1. $GOOSE_BIN (explicit override — fastest path, no cargo startup).
      2. target/debug/goose.exe or target/debug/goose if already built.
      3. Fall back to `cargo run -p goose-cli -- acp` (recompiles on demand).
    """
    bin_env = os.environ.get("GOOSE_BIN")
    if bin_env and os.path.isfile(bin_env):
        return [bin_env, "acp"]

    is_windows = sys.platform.startswith("win")
    exe = "goose.exe" if is_windows else "goose"
    prebuilt = os.path.join("target", "debug", exe)
    if os.path.isfile(prebuilt):
        return [prebuilt, "acp"]

    if shutil.which("cargo"):
        return ["cargo", "run", "-p", "goose-cli", "--", "acp"]

    raise RuntimeError(
        "Could not find the goose binary. Build it with "
        "`cargo build -p goose-cli` or set GOOSE_BIN=<path-to-goose>"
    )


class AcpClient:
    def __init__(self):
        # On Windows, if stderr is PIPE and nobody reads it, `cargo run`'s
        # compile chatter fills the ~4 KB pipe buffer, blocks the child's
        # stderr write, and deadlocks the stdio loop (classic subprocess
        # pitfall). Route stderr to DEVNULL — this test cares only about
        # the JSON-RPC framing on stdout.
        #
        # `bufsize=1` (line-buffered) matches text-mode semantics on both
        # Windows and POSIX; the prior `bufsize=0` is only meaningful for
        # binary streams and made stdout flushes unreliable on Windows.
        self.process = subprocess.Popen(
            _resolve_goose_cmd(),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self.request_id = 0

    def send_request(self, method, params=None, collect_notifications=False):
        """Send a request and wait for the response.

        Args:
            method: The JSON-RPC method name
            params: Optional parameters for the request
            collect_notifications: If True, collect notifications until response arrives

        Returns:
            Tuple of (response, notifications) if collect_notifications is True,
            otherwise just the response.
        """
        self.request_id += 1
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "id": self.request_id,
        }
        if params:
            request["params"] = params

        request_str = json.dumps(request)
        print(f">>> Sending: {request_str}")
        self.process.stdin.write(request_str + '\n')
        self.process.stdin.flush()

        notifications = []

        # Read responses until we get one with our request ID
        while True:
            response_line = self.process.stdout.readline()
            if not response_line:
                if collect_notifications:
                    return None, notifications
                return None

            response = json.loads(response_line)

            # Check if this is a notification (has 'method' but no 'id')
            if 'method' in response and 'id' not in response:
                print(f"<<< Notification: {response['method']}: {response.get('params', {}).get('update', {}).get('sessionUpdate', 'unknown')}")
                if collect_notifications:
                    notifications.append(response)
                continue

            if response.get('id') == self.request_id:
                print(f"<<< Response: {response_line.strip()}")
                if collect_notifications:
                    return response, notifications
                return response
            else:
                # Response for a different request ID, skip
                print(f"<<< Unexpected response ID: {response}")

    def initialize(self):
        """Initialize the ACP connection and verify capabilities."""
        return self.send_request("initialize", {
            "protocolVersion": "v1",
            "clientCapabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        })

    def new_session(self, cwd=None):
        """Create a new session (session/new)."""
        params = {
            "mcpServers": [],
            "cwd": cwd or os.getcwd()
        }
        return self.send_request("session/new", params)

    def load_session(self, session_id, cwd=None):
        """Load an existing session (session/load).

        Returns: (response, notifications) tuple with session history notifications.
        """
        params = {
            "sessionId": session_id,
            "mcpServers": [],
            "cwd": cwd or os.getcwd()
        }
        return self.send_request("session/load", params, collect_notifications=True)

    def prompt(self, session_id, text):
        """Send a prompt to the session (session/prompt).

        Returns: (response, notifications) tuple with streaming notifications.
        """
        return self.send_request("session/prompt", {
            "sessionId": session_id,
            "prompt": [
                {
                    "type": "text",
                    "text": text
                }
            ]
        }, collect_notifications=True)

    def close(self):
        if self.process:
            self.process.terminate()
            self.process.wait()


def test_new_session(client):
    """Test creating a new session and sending a prompt."""
    print("\n" + "="*60)
    print("TEST: New Session Flow")
    print("="*60)

    print("\n2. Creating new session (session/new)...")
    session_response = client.new_session()
    if session_response and 'result' in session_response:
        session_id = session_response['result']['sessionId']
        print(f"   ✓ Created session: {session_id}")
        return session_id
    else:
        print(f"   ✗ Failed to create session: {session_response}")
        return None


def test_load_session(client, session_id):
    """Test loading an existing session."""
    print("\n" + "="*60)
    print("TEST: Load Session Flow")
    print("="*60)

    print(f"\n4. Loading existing session (session/load) with ID: {session_id}")
    load_response, notifications = client.load_session(session_id)

    # Show notifications received (these are the session history)
    if notifications:
        print(f"   📝 Received {len(notifications)} notification(s) (session history replay):")
        for n in notifications:
            update = n.get('params', {}).get('update', {})
            update_type = update.get('sessionUpdate', 'unknown')
            content = update.get('content', {})
            if isinstance(content, dict):
                text = content.get('text', '')[:50]
            else:
                text = str(content)[:50]
            print(f"      - {update_type}: {text}...")

    if load_response and 'result' in load_response:
        print(f"   ✓ Session loaded successfully")
        print(f"   Response: {load_response['result']}")
        return True
    else:
        print(f"   ✗ Failed to load session: {load_response}")
        return False


def main():
    print("="*60)
    print("ACP Client Test Suite")
    print("="*60)
    print("\nStarting ACP client test...")

    client = AcpClient()

    try:
        print("\n1. Initializing agent...")
        init_response = client.initialize()
        if init_response and 'result' in init_response:
            capabilities = init_response['result'].get('agentCapabilities', {})
            print(f"   ✓ Initialized successfully")
            print(f"   - loadSession capability: {capabilities.get('loadSession', False)}")
            print(f"   - promptCapabilities: {capabilities.get('promptCapabilities', {})}")

            if not capabilities.get('loadSession'):
                print("   ⚠ Warning: loadSession capability is not advertised")
        else:
            print(f"   ✗ Failed to initialize: {init_response}")
            return 1

        session_id = test_new_session(client)
        if not session_id:
            return 1

        print("\n3. Sending prompt (session/prompt)...")
        prompt_response, notifications = client.prompt(session_id, "Hello! Say 'test successful' if you can hear me.")
        if notifications:
            print(f"   📝 Received {len(notifications)} streaming notification(s)")
        if prompt_response and 'result' in prompt_response:
            print(f"   ✓ Got response: {prompt_response['result']}")
        elif prompt_response and 'error' in prompt_response:
            print(f"   ✗ Error: {prompt_response['error']}")
        else:
            print(f"   ✗ Failed to get prompt response: {prompt_response}")

        # Close the client and start a new one to simulate reconnection
        print("\n--- Simulating client restart ---")
        client.close()
        time.sleep(1)

        client = AcpClient()

        print("\n5. Re-initializing after restart...")
        init_response = client.initialize()
        if init_response and 'result' in init_response:
            print(f"   ✓ Re-initialized successfully")
        else:
            print(f"   ✗ Failed to re-initialize: {init_response}")
            return 1

        if not test_load_session(client, session_id):
            return 1

        print("\n6. Sending prompt to loaded session...")
        prompt_response, notifications = client.prompt(session_id, "What was my previous message?")
        if notifications:
            print(f"   📝 Received {len(notifications)} streaming notification(s)")
        if prompt_response and 'result' in prompt_response:
            print(f"   ✓ Got response: {prompt_response['result']}")
        elif prompt_response and 'error' in prompt_response:
            print(f"   ✗ Error: {prompt_response['error']}")
        else:
            print(f"   ✗ Failed to get prompt response: {prompt_response}")

        print("\n" + "="*60)
        print("All tests completed!")
        print("="*60)
        return 0

    finally:
        client.close()
        print("\nTest complete.")


if __name__ == "__main__":
    sys.exit(main())
