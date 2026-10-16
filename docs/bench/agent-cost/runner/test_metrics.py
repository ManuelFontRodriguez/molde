#!/usr/bin/env python3
"""Unit tests for metrics.py on synthetic transcripts (review 2 B3, I1, I14). The event
shapes follow the one real transcript seen so far (LOG.md, auth test 1); they are
replaced by real fixtures from the smoke run.

    python3 runner/test_metrics.py
"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
DOCS_EXCERPT = '{\n  "verdict": "fail",\n  "published": false,\n  "errors": [...]\n}'  # as in reference.md


def init(tools=("Bash", "Read", "Edit", "Write"), mcp=()):
    return {"type": "system", "subtype": "init", "tools": list(tools), "mcp_servers": list(mcp),
            "model": "claude-opus-5-5", "apiKeySource": "none", "claude_code_version": "2.1.300"}


def call(i, name, inp):
    return {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": f"t{i}", "name": name, "input": inp}]}}


def res(i, text):
    return {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": f"t{i}", "content": text}]}}


def result(subtype="success", is_error=False, text="Done: all changes made.", **kw):
    r = {"type": "result", "subtype": subtype, "is_error": is_error, "num_turns": 5, "result": text,
         "usage": {"input_tokens": 100, "output_tokens": 50, "cache_creation_input_tokens": 1000,
                   "cache_read_input_tokens": 5000},
         "modelUsage": {"claude-opus-5-5": {"inputTokens": 100, "outputTokens": 50, "cacheCreationInputTokens": 1000,
                                            "cacheReadInputTokens": 5000}}}
    r.update(kw)
    return r


def run(events, arm="molde-check", passed=False, agent_exit=0, mcp=False, **meta_extra):
    d = Path(tempfile.mkdtemp())
    (d / "transcript.jsonl").write_text("\n".join(json.dumps(e) for e in events) + "\n")
    (d / "meta.json").write_text(json.dumps({"run_id": "x", "arm": arm, "task": "T4", "prompt": "P1", "rep": 1,
                                             "set": "test", "model": "claude-opus-5-5", "effort": "high",
                                             "agent_exit": agent_exit, "agent_wall_s": 10, **meta_extra}))
    (d / "manifest.json").write_text(json.dumps({"files": {"mcp.json": "x"} if mcp else {}, "planted": []}))
    (d / "verdict.json").write_text(json.dumps({"pass": passed, "oracle": {"pass": passed},
                                                "mutation": {"oracle": {"pass": passed}}}))
    r = subprocess.run([sys.executable, str(HERE / "metrics.py"), str(d)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return json.loads((d / "metrics.json").read_text())


class T(unittest.TestCase):
    def test_docs_example_is_not_a_signal(self):
        m = run([init(), call(1, "Bash", {"command": "cat /work/context/molde-reference.md"}), res(1, DOCS_EXCERPT),
                 call(2, "Edit", {"file_path": "x"}), res(2, "ok"), result()])
        self.assertEqual(m["checker_calls"], 0)
        self.assertFalse(m["signal_ever_fail"])
        self.assertEqual(m["false_done_kind"], "no_checker_run")
        self.assertEqual(m["tool_calls_by_phase"]["docs"], 1)

    def test_last_checker_verdict_decides(self):
        m = run([init(), call(1, "Bash", {"command": "molde check --site . --out out"}), res(1, '{"verdict": "fail", "errors": []}'),
                 call(2, "Edit", {"file_path": "x"}), res(2, "ok"),
                 call(3, "Bash", {"command": "cd /work/site && molde check --site . --out out"}), res(3, '{"verdict": "pass"}'),
                 result()])
        self.assertEqual(m["checker_verdicts"], ["fail", "pass"])
        self.assertTrue(m["signal_ever_fail"])
        self.assertEqual(m["false_done_kind"], "checker_passed_blind")
        self.assertEqual(m["tool_calls_by_phase"]["verify"], 2)

    def test_ignored_failure(self):
        m = run([init(), call(1, "Bash", {"command": "site-check public"}), res(1, '{"verdict": "fail"}'), result()],
                arm="hugo+checker")
        self.assertEqual(m["false_done_kind"], "checker_failed_ignored")

    def test_pass_is_not_false_done(self):
        m = run([init(), result()], passed=True)
        self.assertEqual(m["outcome"], "pass")
        self.assertFalse(m["false_done"])

    def test_auth_error_is_infra(self):
        m = run([init(), result(is_error=True, text="Failed to authenticate. API Error: 401", api_error_status=401,
                                terminal_reason="api_error")])
        self.assertEqual(m["outcome"], "infra_fail")
        self.assertFalse(m["false_done"])

    def test_usage_limit_text_is_infra(self):
        m = run([init(), result(text="Claude usage limit reached. Your limit will reset at 5pm.")])
        self.assertEqual(m["outcome"], "infra_fail")

    def test_rate_limit_words_in_a_normal_summary_are_not_infra(self):
        m = run([init(), result(text="Done. I added retry handling so the 429 / rate limit page renders; "
                                     "the build passes and every link resolves.")], passed=True)
        self.assertEqual(m["outcome"], "pass")

    def test_bash_edits_are_counted(self):
        import metrics
        self.assertEqual(metrics.phase_of("Bash", {"command": "sed -i 's/a/b/' routes.yaml"}, "molde build"), "edit")
        self.assertEqual(metrics.phase_of("Bash", {"command": "cat > templates/x.j2 <<'E'\nhi\nE"}, "molde build"), "edit")
        self.assertEqual(metrics.phase_of("Bash", {"command": "grep -n x data.json 2>/dev/null"}, "molde build"), "explore")
        self.assertEqual(metrics.phase_of("Bash", {"command": "ls out > /dev/null 2>&1"}, "molde build"), "explore")
        self.assertEqual(metrics.phase_of("Bash", {"command": "grep -E 'og:title|<title>' out/x.html"}, "molde build"), "explore")
        self.assertEqual(metrics.phase_of("Bash", {"command": "echo '{}' >> data.json"}, "molde build"), "edit")
        self.assertEqual(metrics.phase_of("Bash", {"command": "sed -i s/a/b/ x && molde check --site ."}, "molde build"), "verify")

    def test_real_smoke_transcript(self):
        """The first real run (Claude Code 2.1.300, claude-opus-5-5, molde-check T3, 16 Oct)."""
        import shutil, subprocess, tempfile
        src = Path(__file__).resolve().parent / "fixtures" / "smoke-molde-check-T3"
        with tempfile.TemporaryDirectory() as t:
            shutil.copytree(src, t, dirs_exist_ok=True)
            subprocess.run([sys.executable, str(Path(__file__).resolve().parent / "metrics.py"), t],
                           check=True, capture_output=True)
            m = json.loads((Path(t) / "metrics.json").read_text())
        self.assertEqual(m["outcome"], "pass")
        self.assertTrue(m["claimed_done"])
        self.assertEqual(m["infra_reasons"], [])
        self.assertEqual(m["assistant_messages"], 7)
        self.assertEqual(m["checker_verdicts"], ["pass"])
        self.assertEqual(m["bash_edit_calls"], 1)
        self.assertEqual(m["new_tokens"], 13503)
        self.assertEqual(m["models_used"], ["claude-opus-5-5"])
        self.assertEqual(m["rate_limit_last"]["status"], "allowed")
        self.assertGreater(m["docs_tokens_est"], 0)
        self.assertEqual(m["new_tokens_without_docs"], m["new_tokens"] - m["docs_tokens_est"])

    def test_kill_that_is_not_oom_is_infra(self):
        m = run([init()], agent_exit=137, oom_killed=False)
        self.assertEqual(m["outcome"], "infra_fail")

    def test_budget_cap_is_censored(self):
        m = run([init(), result(subtype="error_max_budget_usd", is_error=True, text="")])
        self.assertEqual(m["outcome"], "censored")

    def test_timeout_without_progress_is_infra(self):
        m = run([init()], agent_exit=124)
        self.assertEqual(m["outcome"], "infra_fail")

    def test_max_turns_is_censored(self):
        m = run([init(), result(subtype="error_max_turns", is_error=True, text="")])
        self.assertEqual(m["outcome"], "censored")

    def test_oom_is_a_failure_of_the_arm(self):
        m = run([init()], agent_exit=137)
        self.assertEqual(m["outcome"], "fail")
        self.assertTrue(m["oom"])

    def test_extra_tools_are_infra(self):
        m = run([init(tools=("Bash", "Read", "Edit", "Write", "Task")), result()])
        self.assertEqual(m["outcome"], "infra_fail")

    def test_mcp_missing_in_mcp_arm_is_infra(self):
        m = run([init(), result()], arm="molde-mcp", mcp=True)
        self.assertEqual(m["outcome"], "infra_fail")

    def test_tokens(self):
        m = run([init(), result()])
        self.assertEqual(m["new_tokens"], 1150)
        self.assertEqual(m["new_tokens_pinned_model"], 1150)
        self.assertEqual(m["context_tokens_processed"], 6150)

    def test_forbidden_use_in_build_only_arm(self):
        m = run([init(), call(1, "Bash", {"command": "/work/context/bin/.molde check --site . --out out"}),
                 res(1, '{"verdict": "pass"}'), result()], arm="molde-build")
        self.assertTrue(m["forbidden_tool_use"])

    def test_lexical_claimed_done_is_only_a_sensitivity(self):
        m = run([init(), result(text="Fixed. The check cannot find any problems now.")])
        self.assertTrue(m["claimed_done"])
        self.assertFalse(m["claimed_done_lexical"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
