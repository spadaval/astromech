#!/usr/bin/env python3
"""Tests for the WebView cost harness's judging (scripts/webview_cost.py). Runs anywhere:
python3 scripts/test_webview_cost.py"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import webview_cost as w  # noqa: E402


def results(idle_mb=120.0, idle_cpu=0.2, later=(0.9, 1.1, 1.0), first=2.6, no_core=1.4):
    win = {"seconds": 30, "uss_mb_mean": idle_mb - 5, "uss_mb_max": idle_mb, "cpu_core_pct": idle_cpu * 4,
           "cpu_machine_pct": idle_cpu, "processes": 5, "names": ["astromech-shell", "WebKitWebProcess"],
           "webview_seen": True, "window_alive": True}
    use = dict(win, uss_mb_max=idle_mb + 40, cpu_machine_pct=8.0, switches=15, switch_exit_codes=[0],
               extra_windows=0, pages_received=15)
    return {"cold_start": {"first_s": first, "later_s": list(later), "in_process_ms": [800, 700, 650, 690],
                           "core_not_running_s": no_core, "after_core_crash_s": 2.7},
            "idle": win, "in_use": use, "core_mb": 12.0}


def verdicts(r):
    return {c["id"]: c["pass"] for c in w.judge(r)}


class Judge(unittest.TestCase):
    def test_within_budget(self):
        self.assertEqual(verdicts(results()),
                         {"idle_memory": True, "idle_cpu": True, "cold_start": True, "page_changes": True})

    def test_each_budget_can_fail(self):
        self.assertFalse(verdicts(results(idle_mb=250.0))["idle_memory"])
        self.assertTrue(verdicts(results(idle_mb=249.9))["idle_memory"])
        self.assertFalse(verdicts(results(idle_cpu=1.01))["idle_cpu"])
        self.assertTrue(verdicts(results(idle_cpu=1.0))["idle_cpu"])
        self.assertFalse(verdicts(results(later=(1.9, 2.0, 2.4)))["cold_start"])

    def test_page_changes_must_reach_the_window(self):
        for key, bad in (("extra_windows", 2), ("pages_received", 14), ("switch_exit_codes", [0, 1]),
                         ("window_alive", False)):
            r = results()
            r["in_use"][key] = bad
            self.assertFalse(verdicts(r)["page_changes"], key)

    def test_nothing_measured_is_not_a_pass(self):
        for key, bad in (("window_alive", False), ("webview_seen", False), ("uss_mb_max", 3.0)):
            r = results()
            r["idle"][key] = bad
            v = verdicts(r)
            self.assertFalse(v["idle_memory"] or v["idle_cpu"], key)

    def test_a_start_with_the_core_not_running_counts(self):
        self.assertFalse(verdicts(results(no_core=2.1))["cold_start"])

    def test_cold_start_judges_the_later_starts(self):
        # The first start makes the WebView profile, once per install.
        self.assertTrue(verdicts(results(first=5.0))["cold_start"])
        self.assertFalse(verdicts(results(later=()))["cold_start"])

    def test_report_renders(self):
        rep = {"webview": "WebKitGTK", "host": "test", "commit": "abc", "started": "now",
               "budgets": w.judge(results(idle_mb=300.0)), "results": results(idle_mb=300.0)}
        md = w.markdown(rep)
        self.assertEqual(md.count("**FAIL**"), 1)
        self.assertIn("astromech-7ck3", md)


if __name__ == "__main__":
    unittest.main()
