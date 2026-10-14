#!/usr/bin/env python3
"""Tests of the pure rules of check-health.py. Run: python3 tools/test-check-health.py"""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("check_health", Path(__file__).with_name("check-health.py"))
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)


class Rules(unittest.TestCase):
    def test_an_answer_is_sorted_ok_challenge_or_fail(self):
        self.assertEqual(health.classify(200, {}, "<title>Weeb Central</title>", "weeb central"), "ok")
        self.assertEqual(health.classify(200, {}, "something else", "weeb central"), "fail", "the page lacks its text")
        self.assertEqual(health.classify(200, {}, "anything", ""), "ok", "no text expected")
        self.assertEqual(health.classify(403, {"cf-mitigated": "challenge"}, "Just a moment", "x"), "challenge")
        self.assertEqual(health.classify(500, {}, "", "x"), "fail")
        self.assertEqual(health.classify(None, {}, "", "x"), "fail", "no answer at all")

    def test_one_failure_is_not_broken_but_two_in_a_row_are(self):
        state = health.next_state(("active", 0), "fail")
        self.assertEqual(state, ("active", 1))
        state = health.next_state(state, "fail")
        self.assertEqual(state, ("broken", 2))

    def test_an_ok_clears_it_and_a_challenge_changes_nothing(self):
        self.assertEqual(health.next_state(("broken", 3), "ok"), ("active", 0))
        self.assertEqual(health.next_state(("broken", 3), "challenge"), ("broken", 3))
        self.assertEqual(health.next_state(("active", 1), "challenge"), ("active", 1))


if __name__ == "__main__":
    unittest.main()
