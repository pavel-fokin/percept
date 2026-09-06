import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("agent-hook.py").resolve()
ROOT = SCRIPT.parent.parent


class AgentHookTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="percept hooks ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.repo = self.root / "checkout with spaces"
        self.repo.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        self.home = self.root / "storage"
        self.home.mkdir()
        self.binary = self.root / "bin with spaces" / "percept"
        self.binary.parent.mkdir()
        self.binary.write_text(
            f"#!{sys.executable}\n"
            "import json, os, pathlib, sys, uuid\n"
            "args = dict(zip(sys.argv[3::2], sys.argv[4::2]))\n"
            "if args.get('--payload') == '-':\n"
            "    args['--payload'] = sys.stdin.read()\n"
            "if os.environ.get('FAIL_CAPTURE'):\n"
            "    print('disk is full', file=sys.stderr)\n"
            "    sys.exit(1)\n"
            "event_id = str(uuid.uuid4())\n"
            "entry = {'id': event_id, 'cwd': os.getcwd(), 'args': args}\n"
            "path = pathlib.Path(os.environ['PERCEPT_HOME']) / (event_id + '.event')\n"
            "path.write_text(json.dumps(entry))\n"
            "print(event_id)\n"
        )
        self.binary.chmod(0o755)
        self.env = dict(os.environ, PERCEPT_HOME=str(self.home), PERCEPT_BIN=str(self.binary))

    def run_hook(self, event, client="codex", cwd=None, env=None, raw=None, ok=True, **fields):
        """Runs the hook. A failure exits 1 with a `percept hook:` line on
        stderr and still prints a JSON object, so the client shows the
        error without blocking the turn."""
        data = dict(hook_event_name=event, session_id="session", cwd=str(cwd or self.repo))
        data.update(fields)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), client],
            input=json.dumps(data) if raw is None else raw,
            text=True, capture_output=True, env=env or self.env, cwd=self.root,
        )
        self.assertEqual(result.returncode, 0 if ok else 1, result.stderr)
        if not ok:
            self.assertIn("percept hook:", result.stderr)
        json.loads(result.stdout)
        return result

    def state_files(self):
        return list((self.home / "hook-sessions").glob("*"))

    def events(self):
        return [json.loads(path.read_text()) for path in self.home.glob("*.event")]

    def prompt(self, **fields):
        result = self.run_hook("UserPromptSubmit", prompt="hello", **fields)
        self.assertEqual(result.stderr, "")
        context = json.loads(result.stdout)["hookSpecificOutput"]["additionalContext"]
        return context.removeprefix("percept event ")

    def test_prompt_context_names_committed_event(self):
        event_id = self.prompt()
        event = self.events()[0]
        self.assertEqual(event_id, event["id"])
        self.assertEqual(event["args"]["--actor"], "user")
        self.assertEqual(json.loads(event["args"]["--payload"]), {"content": "hello"})

    def test_any_client_name_becomes_the_events_source(self):
        self.prompt(client="opencode")
        self.assertEqual(self.events()[0]["args"]["--source"], "opencode")

    def test_an_empty_client_name_is_refused_without_blocking(self):
        result = self.run_hook("UserPromptSubmit", client="", prompt="hello", ok=False)
        self.assertIn("client name must not be empty", result.stderr)
        self.assertEqual(self.events(), [])

    def test_tool_result_cites_call_and_call_cites_prompt(self):
        prompt = self.prompt()
        self.run_hook("PostToolUse", tool_name="Bash", tool_input={"command": "ls"},
                      tool_response={"stdout": "files"})
        events = {event["args"]["--type"]: event for event in self.events()}
        call = events["tool.called"]
        result = events["tool.resulted"]
        self.assertEqual(call["args"]["--causation"], prompt)
        self.assertEqual(result["args"]["--causation"], call["id"])
        self.assertEqual(json.loads(result["args"]["--payload"])["content"], '{"stdout": "files"}')

    def test_stop_uses_last_assistant_message_and_prompt_cause(self):
        prompt = self.prompt()
        result = self.run_hook("Stop", last_assistant_message="reply")
        self.assertEqual(json.loads(result.stdout), {})
        reply = next(event for event in self.events() if event["args"]["--actor"] == "model")
        self.assertEqual(reply["args"]["--causation"], prompt)
        self.assertEqual(json.loads(reply["args"]["--payload"]), {"content": "reply"})

    def test_stop_clears_the_turns_state(self):
        self.prompt()
        self.assertEqual(len(self.state_files()), 1)
        self.run_hook("Stop", last_assistant_message="reply")
        self.assertEqual(self.state_files(), [])

    def test_the_binary_keeps_its_own_default_log(self):
        env = dict(self.env)
        del env["PERCEPT_HOME"]
        probe = self.root / "probe"
        probe.write_text(
            f"#!{sys.executable}\n"
            "import os, sys, uuid\n"
            "open(sys.argv[0] + '.seen', 'w').write(os.environ.get('PERCEPT_HOME', 'unset'))\n"
            "print(uuid.uuid4())\n"
        )
        probe.chmod(0o755)
        self.run_hook("UserPromptSubmit", prompt="hello", env=dict(env, PERCEPT_BIN=str(probe)))
        self.assertEqual((self.root / "probe.seen").read_text(), "unset")

    def test_concurrent_clients_checkouts_sessions_and_turns_keep_separate_causes(self):
        second = self.root / "second checkout"
        subprocess.run([
            "git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
            "commit", "-q", "--allow-empty", "-m", "test fixture",
        ], cwd=self.repo, check=True)
        subprocess.run(["git", "worktree", "add", "-q", "--detach", str(second)],
                       cwd=self.repo, check=True)
        cases = [
            dict(client="codex", cwd=self.repo, session_id="one", turn_id="a"),
            dict(client="claude-code", cwd=self.repo, session_id="one", turn_id="a"),
            dict(client="codex", cwd=second, session_id="one", turn_id="a"),
            dict(client="codex", cwd=self.repo, session_id="two", turn_id="a"),
            dict(client="codex", cwd=self.repo, session_id="one", turn_id="b"),
        ]
        with concurrent.futures.ThreadPoolExecutor() as pool:
            prompts = list(pool.map(lambda case: self.prompt(**case), cases))
            list(pool.map(lambda item: self.run_hook("Stop", last_assistant_message=str(item[0]),
                                                    **item[1]), enumerate(cases)))
        replies = [event for event in self.events() if event["args"]["--actor"] == "model"]
        self.assertEqual(len(replies), len(cases))
        for reply in replies:
            index = int(json.loads(reply["args"]["--payload"])["content"])
            self.assertEqual(reply["args"]["--causation"], prompts[index])
            self.assertEqual(reply["args"]["--source"], cases[index]["client"])
            self.assertEqual(reply["cwd"], str(cases[index]["cwd"]))

    def test_subdirectory_uses_checkout_root_and_existing_prompt(self):
        prompt = self.prompt()
        nested = self.repo / "nested"
        nested.mkdir()
        self.run_hook("Stop", cwd=nested, last_assistant_message="reply")
        reply = next(event for event in self.events() if event["args"]["--actor"] == "model")
        self.assertEqual(reply["cwd"], str(self.repo))
        self.assertEqual(reply["args"]["--causation"], prompt)

    def test_failed_prompt_removes_previous_cause(self):
        self.prompt()
        result = self.run_hook("UserPromptSubmit", prompt="next",
                               env=dict(self.env, FAIL_CAPTURE="1"), ok=False)
        self.assertIn("disk is full", result.stderr)
        self.run_hook("Stop", last_assistant_message="reply")
        reply = next(event for event in self.events() if event["args"]["--actor"] == "model")
        self.assertNotIn("--causation", reply["args"])

    def test_invalid_prompt_removes_previous_cause(self):
        self.prompt()
        result = self.run_hook("UserPromptSubmit", prompt={}, ok=False)
        self.assertIn("prompt must be a string", result.stderr)
        self.run_hook("Stop", last_assistant_message="reply")
        reply = next(event for event in self.events() if event["args"]["--actor"] == "model")
        self.assertNotIn("--causation", reply["args"])

    def test_missing_binary_leaves_coding_available_and_writes_nothing(self):
        result = self.run_hook("Stop", last_assistant_message="reply",
                               env=dict(self.env, PERCEPT_BIN=str(self.root / "missing")))
        self.assertEqual(result.stderr, "")
        self.assertEqual(json.loads(result.stdout), {})
        self.assertEqual(self.events(), [])
        self.assertFalse((self.home / "hook-sessions").exists())

    def test_storage_override_does_not_change_default_binary_location(self):
        user_home = self.root / "user home"
        installed = user_home / ".percept/bin/percept"
        installed.parent.mkdir(parents=True)
        shutil.copyfile(self.binary, installed)
        installed.chmod(0o755)
        env = dict(self.env, HOME=str(user_home))
        del env["PERCEPT_BIN"]
        result = self.run_hook("UserPromptSubmit", prompt="hello", env=env)
        self.assertEqual(result.stderr, "")
        self.assertEqual(len(self.events()), 1)

    def test_unknown_turn_does_not_inherit_another_turns_prompt(self):
        self.prompt(turn_id="earlier")
        self.run_hook("Stop", turn_id="later", last_assistant_message="reply")
        reply = next(event for event in self.events() if event["args"]["--actor"] == "model")
        self.assertNotIn("--causation", reply["args"])

    def test_empty_reply_is_not_an_event(self):
        self.run_hook("Stop", last_assistant_message=" \n")
        self.assertEqual(self.events(), [])

    def test_null_reply_is_not_an_error_or_event(self):
        result = self.run_hook("Stop", last_assistant_message=None)
        self.assertEqual(result.stderr, "")
        self.assertEqual(self.events(), [])

    def test_an_unreadable_transcript_is_an_error_not_an_event(self):
        self.run_hook("Stop", transcript_path=str(self.root / "absent"), ok=False)
        self.assertEqual(self.events(), [])

    def test_claude_fallback_reads_only_current_turn_and_preserves_tool_results(self):
        transcript = self.root / "transcript.jsonl"
        entries = [
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "old"}]}},
            {"type": "user", "message": {"content": "current prompt"}},
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "start"}]}},
            {"type": "user", "message": {"content": [{"type": "tool_result", "content": "ok"}]}},
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "end"}]}},
        ]
        transcript.write_text("\n".join(map(json.dumps, entries)))
        self.run_hook("Stop", client="claude-code", transcript_path=str(transcript),
                      last_assistant_message=None)
        self.assertEqual(json.loads(self.events()[0]["args"]["--payload"]), {"content": "start\nend"})

    def test_malformed_input_reports_error_without_blocking(self):
        result = self.run_hook("Stop", raw="{broken", ok=False)
        self.assertEqual(json.loads(result.stdout), {})

    def test_a_payload_longer_than_an_argument_is_published_whole(self):
        self.run_hook("UserPromptSubmit", prompt="x" * 3_000_000)
        payload = json.loads(self.events()[0]["args"]["--payload"])
        self.assertEqual(len(payload["content"]), 3_000_000)

    def test_configured_commands_resolve_script_from_nested_checkout_with_spaces(self):
        scripts = self.repo / "scripts"
        scripts.mkdir()
        shutil.copyfile(SCRIPT, scripts / SCRIPT.name)
        nested = self.repo / "nested"
        nested.mkdir()
        for config in [".claude/settings.json", ".codex/hooks.json"]:
            with self.subTest(config=config):
                hooks = json.loads((ROOT / config).read_text())["hooks"]
                for event in ["UserPromptSubmit", "PostToolUse", "Stop"]:
                    command = hooks[event][0]["hooks"][0]["command"]
                    data = dict(hook_event_name=event, session_id="configured", cwd=str(nested),
                                prompt="hello", tool_name="test", tool_input={}, tool_response="ok",
                                last_assistant_message="reply")
                    result = subprocess.run(command, shell=True, cwd=nested, env=self.env,
                                            input=json.dumps(data), text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stderr, "")
                    json.loads(result.stdout)


if __name__ == "__main__":
    unittest.main()
