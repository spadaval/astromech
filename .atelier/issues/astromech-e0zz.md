---
created_at: "2026-10-17T19:22:54.126798893+00:00"
id: "astromech-e0zz"
issue_type: "bug"
labels:
- "alpha"
priority: "P1"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "todo"
title: "Push-to-talk release is acted on about 210 ms late, leaving no margin in the 400 ms action-start budget"
updated_at: "2026-10-17T19:22:54.126798893+00:00"
---

## Goal

Give the action-start budget (ARCHITECTURE §11, MVP criterion 1: keypress begins <= 400 ms p95 after push-to-talk release) real margin.

## Done when

The MVP criteria harness (scripts/mvp_criteria.py) reports the action-start estimate with a clear margin under 400 ms on the Windows runner, or the budget is consciously re-set with a recorded decision.

## Context

Found by the MVP criteria harness (astromech-08w8) against the mock game on the Windows runner. With a keyboard push-to-talk key, the core handles the release p50 208-210 ms, p95 218-225 ms after it (three runs). The 150 ms debounce (controls.rs `Mapper::flush`) is only checked on the controls thread's 100 ms idle tick, so with no further input the release is reported about 200 ms late. A HOTAS that streams reports may flush it sooner. session.rs then sleeps post_roll - debounce (50 ms) and waits for the next mic callback before speech-to-text. With S9's STT p95 (104 ms) and the measured command-to-key-down p95 (11-16 ms), the estimate came out 403.7 ms (FAIL), 398.0 and 394.6 ms (PASS): no margin. Options include flushing the debounce on a timer at its deadline instead of the next idle tick, a shorter debounce for keyboard and mouse, and overlapping the post-roll with speech-to-text start.
