---
created_at: "2026-10-17T16:54:23.691494692+00:00"
id: "astromech-08w8"
issue_type: "task"
labels:
- "alpha"
- "size-m"
workflow_branch:
  branch_base: main
  work_branch: task/astromech-08w8
priority: "P1"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "in_progress"
title: "MVP criteria harness: mock-game trials, soak, kill switch, action latency"
updated_at: "2026-10-17T18:40:01.176914224+00:00"
---

## Goal

Measure the game-free MVP criteria for astromech-05k2 on the Windows runner against `mockgame`: 200 trials per T1 tool at ≥ 99% verified success, a 4 h soak replay with zero unintended keypresses (`keys.log`), kill switch releasing all keys within 50 ms, and action start ≤ 400 ms p95 after push-to-talk release.

## Done when

One command runs all four and writes a report against each threshold; the report from a Windows run is attached to astromech-05k2 as evidence.

## Context

`crates/astromech-core/examples/mockgame.rs` toggles Status.json flags on keypress with a configurable delay and dropped presses, and logs every key. Device classes beyond keyboard (mouse 4/5, HOTAS, gamepad) need real hardware and stay with the human check.
