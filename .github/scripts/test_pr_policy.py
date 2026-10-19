#!/usr/bin/env python3
"""Unit tests for pr_policy.problems(): python3 .github/scripts/test_pr_policy.py"""
import unittest

from pr_policy import problems

CHECK = ["src/check/links.rs"]
STRICTER = ["- A site with such a link to a missing page now fails: fix the link."]


class T(unittest.TestCase):
    def ok(self, *args):
        self.assertEqual(problems(*args), [])

    def bad(self, *args):
        self.assertTrue(problems(*args), args)

    def test_titles(self):
        self.ok("feat(cli): molde guide", ["src/main.rs"], [], set())
        self.ok("bench(agent-cost): round 2A", ["docs/bench/x.md"], [], set())
        self.ok("build(deps): bump serde_json from 1.0.151 to 1.0.154", ["Cargo.lock"], [], set())
        self.ok("molde 0.1.0", ["Cargo.toml", "CHANGELOG.md"], [], set())
        self.ok("molde 0.2.0-rc.1", [], [], set())
        self.bad("Update readme", [], [], set())
        self.bad("docs: no scope", [], [], set())
        self.bad("wip(check): unknown type", [], [], set())

    def test_check_changes_need_the_changelog(self):
        self.bad("fix(check): x", CHECK, [], set())
        self.bad("feat(check): x", CHECK, [], set())
        self.ok("fix(check): x", CHECK + ["CHANGELOG.md"], ["- fixed"], set())
        self.ok("refactor(check): x", CHECK, [], set())  # no behaviour change, no entry
        self.ok("perf(check): x", CHECK, [], set())

    def test_new_check_ids_are_feat(self):
        self.bad("fix(check): x", CHECK + ["CHANGELOG.md"], ["- New check"], {"og.duplicate"})
        self.ok("feat(check): x", CHECK + ["CHANGELOG.md"], ["- New check"], {"og.duplicate"})
        self.ok("fix(check)!: x", CHECK + ["CHANGELOG.md"], ["- New check"], {"og.duplicate"})

    def test_a_stricter_check_is_feat(self):
        # what #28 did: a fix whose entry says sites now fail
        self.bad("fix(check): links written absolute", CHECK + ["CHANGELOG.md"], STRICTER, set())
        self.ok("feat(check): links written absolute", CHECK + ["CHANGELOG.md"], STRICTER, set())
        self.ok("fix(check): x", CHECK + ["CHANGELOG.md"], ["- A correct site no longer fails."], set())


if __name__ == "__main__":
    unittest.main(verbosity=1)
