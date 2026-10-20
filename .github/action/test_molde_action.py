#!/usr/bin/env python3
"""Tests for the action's script: python3 .github/action/test_molde_action.py

The rendering is checked on real reports (molde verify on a fixture); the install on a
fake release, so no network is needed.
"""
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(__file__))
import molde_action as A  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIXTURES = os.path.join(ROOT, "tests", "verify", "fixtures", "built")
MOLDE = os.environ.get("MOLDE_BIN", os.path.join(ROOT, "target", "debug", "molde"))


def verify(name, *args):
    out = subprocess.run([MOLDE, "verify", os.path.join(FIXTURES, name), *args], capture_output=True, text=True)
    return json.loads(out.stdout)


@unittest.skipUnless(os.path.exists(MOLDE), "needs a built molde (cargo build), or MOLDE_BIN")
class Render(unittest.TestCase):
    def test_a_pass(self):
        text = A.render(verify("hugo-realistic", "--base-url", "https://example.org/"), "verify")
        self.assertIn(A.MARKER, text)
        self.assertIn("✅ **pass**", text)
        self.assertIn("19 indexable", text)
        self.assertIn("warnings (these never fail)", text)

    def test_a_fail_names_the_check_the_page_and_what_to_do(self):
        text = A.render(verify("hugo-realistic-basepath", "--base-url", "https://user.github.io/repo/"), "verify")
        self.assertIn("❌ **fail**: 1 error", text)
        self.assertIn("| `links.outside_base` | 1 |", text)
        self.assertIn("/posts/my-first-post/", text)
        self.assertIn("What to do", text)
        self.assertNotIn("\n| `links.outside_base` | index.html | attribute: href; broken_href: /posts/my-first-post/; target: /posts/my-first-post/; count: 21; pages:", text)

    def test_skipped_checks_carry_the_suggestion(self):
        text = A.render(verify("hugo-realistic-basepath"), "verify")
        self.assertIn("run with --base-url https://user.github.io/repo/", text)


class Unit(unittest.TestCase):
    def test_cells_are_table_safe_and_short(self):
        self.assertEqual(A.cell("a|b\nc"), "a&#124;b c")
        self.assertTrue(A.cell("x" * 500).endswith("…"))

    def test_long_reports_are_cut_below_the_comment_limit(self):
        report = {"verdict": "fail", "errors": [{"check": "links.internal", "page": f"p{i}.html", "hint": "h" * 50}
                                                for i in range(5000)]}
        text = A.render(report, "check")
        self.assertLess(len(text), 65_536)
        self.assertIn("and 4970 more", text)

    def test_a_cut_report_closes_what_it_left_open(self):
        report = {"verdict": "fail", "errors": [{"check": "a", "page": "p", "hint": "h"}],
                  "warnings": [{"check": "w", "page": f"p{i}"} for i in range(30)]}
        with mock.patch.object(A, "MAX_CHARS", 300):
            text = A.render(report, "verify")
        self.assertEqual(text.count("<details>"), text.count("</details>"), text)
        self.assertIn("(cut: the full report is in the job)", text)

    def test_annotations_cannot_end_the_command(self):
        self.assertEqual(A.annotation("a\nb%c"), "a%0Ab%25c")
        self.assertEqual(A.annotation("molde a:b,c", prop=True), "molde a%3Ab%2Cc")

    def test_the_comment_is_found_past_the_first_page(self):
        bot = {"login": A.BOT}
        pages = {1: [{"id": i, "body": "other", "user": bot} for i in range(100)],
                 2: [{"id": 7, "body": A.marker(), "user": bot}]}
        calls = []

        def api(url, token, data=None, method=None):
            calls.append((url, method))
            if method:
                return None
            if url.endswith("/user"):
                raise OSError("403")
            return pages.get(int(url.rsplit("page=", 1)[1]), [])

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"pull_request": {"number": 5}}, f)
        env = {"GITHUB_EVENT_PATH": f.name, "GITHUB_REPOSITORY": "o/r", "MOLDE_COMMENT": "true"}
        with mock.patch.dict(os.environ, env), mock.patch.object(A, "api", api):
            A.comment("text", "token")
        os.unlink(f.name)
        self.assertEqual(calls[-1], ("https://api.github.com/repos/o/r/issues/comments/7", "PATCH"), calls)

    def test_someone_elses_comment_with_the_marker_is_never_edited(self):
        calls = []

        def api(url, token, data=None, method=None):
            calls.append((url, method))
            if url.endswith("/user"):
                raise OSError("403")
            if method:
                return None
            return [{"id": 9, "body": A.marker() + " fake pass", "user": {"login": "attacker"}}]

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"pull_request": {"number": 5}}, f)
        env = {"GITHUB_EVENT_PATH": f.name, "GITHUB_REPOSITORY": "o/r", "MOLDE_COMMENT": "true"}
        with mock.patch.dict(os.environ, env), mock.patch.object(A, "api", api):
            A.comment("text", "token")
        os.unlink(f.name)
        self.assertEqual(calls[-1], ("https://api.github.com/repos/o/r/issues/5/comments", "POST"), calls)

    def test_report_content_cannot_reach_markdown_or_the_runner(self):
        evil = "x\r::error::forged\n</details>## Pass ![i](https://e.example/p.png) @team #1 a\\|b"
        report = {"verdict": "fail", "errors": [{"check": "links.internal", "page": evil, "hint": evil}],
                  "warnings": [{"check": evil, "page": evil}], "skipped": [{"checks": evil, "reason": evil}],
                  "pages": {evil: 1}}
        text = A.render(report, "check")
        for bad in ["\r", "::error::forged\n", "</details>##", "![i]", "@team", " #1", "\\|b"]:
            self.assertNotIn(bad, text.replace(A.marker(), ""), bad)
        self.assertEqual(text.count("<details>"), text.count("</details>"))

    def run_main(self, report, code=0, extra=None):
        with tempfile.TemporaryDirectory() as tmp:
            fake = os.path.join(tmp, "molde")
            with open(fake, "w") as f:
                f.write(f"#!/bin/sh\ncat <<'EOF'\n{json.dumps(report)}\nEOF\nexit {code}\n")
            os.chmod(fake, 0o755)
            env = {"MOLDE_PATH": fake, "MOLDE_COMMAND": "check", "MOLDE_ARGS": "", "RUNNER_TEMP": tmp,
                   "MOLDE_FAIL_ON_ERROR": "true", "MOLDE_COMMENT": "false", "GITHUB_OUTPUT": os.path.join(tmp, "out"),
                   "GITHUB_STEP_SUMMARY": os.path.join(tmp, "sum"), **(extra or {})}
            with mock.patch.dict(os.environ, env), mock.patch("sys.stdout", new_callable=io.StringIO) as out:
                try:
                    A.main()
                    status = 0
                except SystemExit as e:
                    status = e.code
            with open(os.path.join(tmp, "out")) if os.path.exists(os.path.join(tmp, "out")) else io.StringIO() as f:
                outputs = f.read()
            return status, out.getvalue(), outputs

    def test_the_verdict_and_exit_code_decide_not_only_the_errors(self):
        self.assertEqual(self.run_main({"verdict": "pass", "errors": []})[0], 0)
        self.assertEqual(self.run_main({"verdict": "fail"})[0], 1)
        self.assertEqual(self.run_main({"verdict": "pass", "errors": []}, code=1)[0], 1)
        status, _, outputs = self.run_main({"verdict": "pass\nerrors=0", "errors": []})
        self.assertEqual(status, 1)
        self.assertIn("verdict=fail\n", outputs)

    def test_the_printed_report_cannot_run_workflow_commands(self):
        _, out, _ = self.run_main({"verdict": "fail", "errors": [{"check": "a", "page": "p\n::add-mask::x"}]})
        start = out.index("::stop-commands::")
        token = out[start:].split("\n", 1)[0].removeprefix("::stop-commands::")
        self.assertGreaterEqual(len(token), 32)
        self.assertLess(start, out.index("::add-mask::"))
        self.assertLess(out.index("::add-mask::"), out.index(f"::{token}::"))
        self.assertIn(f"::{token}::", out)

    def test_boolean_inputs_reject_typos(self):
        for v in ["True", "FALSE"]:
            with mock.patch.dict(os.environ, {"MOLDE_FAIL_ON_ERROR": v}):
                A.flag("MOLDE_FAIL_ON_ERROR", "true")
        for v in ["1", "yes", "ture"]:
            with mock.patch.dict(os.environ, {"MOLDE_FAIL_ON_ERROR": v}):
                with self.assertRaises(SystemExit):
                    A.flag("MOLDE_FAIL_ON_ERROR", "true")

    def test_command_lines(self):
        with mock.patch.dict(os.environ, {"MOLDE_COMMAND": "verify", "MOLDE_DIR": "public",
                                          "MOLDE_BASE_URL": "https://x.example/", "MOLDE_ARGS": "--ignore '/api/**' --unique"}):
            self.assertEqual(A.command_line("molde"),
                             ["molde", "verify", "public", "--base-url", "https://x.example/", "--ignore", "/api/**", "--unique"])
        with mock.patch.dict(os.environ, {"MOLDE_COMMAND": "check", "MOLDE_SITE": "s", "MOLDE_OUT": "o", "MOLDE_ARGS": ""}):
            self.assertEqual(A.command_line("molde"), ["molde", "check", "--site", "s", "--out", "o"])
        with mock.patch.dict(os.environ, {"MOLDE_COMMAND": "verify", "MOLDE_DIR": ""}):
            with self.assertRaises(SystemExit):
                A.command_line("molde")

    def test_version_comes_from_the_input_then_the_pinned_tag(self):
        with mock.patch.dict(os.environ, {"MOLDE_VERSION": "v0.2.0", "MOLDE_ACTION_REF": "v9.9.9"}):
            self.assertEqual(A.resolve_version("o/r"), "0.2.0")
        with mock.patch.dict(os.environ, {"MOLDE_VERSION": "", "MOLDE_ACTION_REF": "v0.3.1"}):
            self.assertEqual(A.resolve_version("o/r"), "0.3.1")
        # a pin that names no release never falls back to the latest silently
        for ref in ["0123456789abcdef0123456789abcdef01234567", "main", "v0"]:
            with mock.patch.dict(os.environ, {"MOLDE_VERSION": "", "MOLDE_ACTION_REF": ref}):
                with self.assertRaises(SystemExit):
                    A.resolve_version("o/r")
        with mock.patch.dict(os.environ, {"MOLDE_VERSION": "../../x"}):
            with self.assertRaises(SystemExit):
                A.resolve_version("o/r")

    def test_install_checks_the_archive_against_sha256sums(self):
        name = f"molde-0.2.0-{A.target()}"
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w:gz") as t:
            data = b"#!/bin/sh\necho molde\n"
            info = tarfile.TarInfo(f"{name}/molde")
            info.size = len(data)
            t.addfile(info, io.BytesIO(data))
        archive = buf.getvalue()
        good = f"{hashlib.sha256(archive).hexdigest()}  {name}.tar.gz\n".encode()

        def fake(sums):
            def urlopen(url, timeout=0):
                return io.BytesIO(sums if url.endswith("SHA256SUMS") else archive)
            return urlopen
        with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(os.environ, {"MOLDE_VERSION": "0.2.0", "RUNNER_TEMP": tmp}):
            with mock.patch("urllib.request.urlopen", fake(good)):
                path = A.install()
                self.assertTrue(os.access(path, os.X_OK))
            with mock.patch("urllib.request.urlopen", fake(b"0" * 64 + f"  {name}.tar.gz\n".encode())):
                with self.assertRaises(SystemExit):
                    A.install()


if __name__ == "__main__":
    unittest.main(verbosity=1)
