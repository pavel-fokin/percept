#!/usr/bin/env python3
"""Capture a coding agent's events through percept's public CLI.

The first argument names the client; every event is recorded under it
as its source. The hook input is the shape Claude Code and Codex share.
"""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def text_field(data, name):
    value = data[name]
    if not isinstance(value, str):
        raise ValueError(f"{name} must be a string")
    return value


def checkout_root(cwd):
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], cwd=cwd,
        capture_output=True, text=True, timeout=5,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.strip())
    return Path(result.stdout.strip()).resolve()


def publish(binary, root, client, actor, kind, payload, cause=None):
    command = [
        str(binary), "events", "publish", "--source", client,
        "--actor", actor, "--type", kind, "--payload", json.dumps(payload),
    ]
    if cause:
        command.extend(["--causation", cause])
    result = subprocess.run(
        command, cwd=root, capture_output=True, text=True, timeout=5,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or f"percept exited {result.returncode}")
    return result.stdout.strip()


def claude_reply(path):
    reply = []
    with Path(path).open() as transcript:
        for line in transcript:
            entry = json.loads(line)
            content = entry.get("message", {}).get("content", [])
            if entry.get("type") == "user":
                if isinstance(content, str) or not any(
                    block.get("type") == "tool_result" for block in content
                ):
                    reply = []
            elif entry.get("type") == "assistant":
                if isinstance(content, str):
                    reply.append(content)
                else:
                    reply.extend(
                        text_field(block, "text") for block in content
                        if block.get("type") == "text"
                    )
    return "\n".join(reply)


def capture(client, data):
    event = text_field(data, "hook_event_name")
    if event not in ("UserPromptSubmit", "PostToolUse", "Stop"):
        raise ValueError(f"unsupported hook event {event!r}")
    root = checkout_root(text_field(data, "cwd"))
    session = text_field(data, "session_id")
    if not session:
        raise ValueError("session_id must not be empty")
    turn = data.get("turn_id", "")
    if not isinstance(turn, str):
        raise ValueError("turn_id must be a string")
    binary = Path(os.environ.get("PERCEPT_BIN") or Path.home() / ".percept/bin/percept")
    binary = binary.expanduser().resolve()
    if not binary.is_file():
        return {}
    # The binary keeps its own default for the log, so a worktree build
    # reads the same log from a hook and from the shell. Only the turn
    # state lives here.
    home = Path(os.environ.get("PERCEPT_HOME") or Path.home() / ".percept").resolve()
    sessions = home / "hook-sessions"
    sessions.mkdir(parents=True, exist_ok=True)
    key = hashlib.sha256(json.dumps([client, str(root), session, turn]).encode()).hexdigest()
    # One file per turn is both the lock and the state: the prompt's event
    # id, which the turn's later events cite as their cause.
    with (sessions / key).open("a+") as state:
        fcntl.flock(state, fcntl.LOCK_EX)
        state.seek(0)
        cause = state.read().strip() or None
        if event == "UserPromptSubmit":
            # Invalidate before publishing: a failed prompt must not inherit the previous cause.
            state.seek(0)
            state.truncate()
            state.flush()
            prompt = text_field(data, "prompt")
            event_id = publish(binary, root, client, "user", "message.received", {"content": prompt})
            state.write(event_id)
            return {"hookSpecificOutput": {
                "hookEventName": event,
                "additionalContext": f"percept event {event_id}",
            }}
        if event == "PostToolUse":
            tool = text_field(data, "tool_name")
            arguments = data["tool_input"]
            response = data["tool_response"]
            if not isinstance(response, str):
                response = json.dumps(response)
            call_id = publish(binary, root, client, "model", "tool.called", {
                "tool": tool, "arguments": arguments,
            }, cause)
            publish(binary, root, client, "system", "tool.resulted", {"content": response}, call_id)
        else:
            reply = data.get("last_assistant_message")
            if reply is None and data.get("transcript_path"):
                reply = claude_reply(text_field(data, "transcript_path"))
            if reply is not None and not isinstance(reply, str):
                raise ValueError("last_assistant_message must be a string")
            if reply and reply.strip():
                publish(binary, root, client, "model", "message.received", {"content": reply}, cause)
            (sessions / key).unlink()
    return {}


def main():
    output = {}
    failed = False
    try:
        client = sys.argv[1]
        if not client:
            raise ValueError("client name must not be empty")
        data = json.load(sys.stdin)
        if not isinstance(data, dict):
            raise ValueError("hook input must be a JSON object")
        output = capture(client, data)
    except Exception as error:  # the hook never fails the agent's turn
        print(f"percept hook: {error}", file=sys.stderr)
        failed = True
    print(json.dumps(output))
    # Non-zero without blocking: the client shows stderr only then.
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
