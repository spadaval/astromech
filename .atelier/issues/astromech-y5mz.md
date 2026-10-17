---
created_at: "2026-10-15T20:36:13.657732240+00:00"
id: "astromech-y5mz"
issue_type: "spike"
labels:
- "alpha"
- "size-s"
workflow_branch:
  branch_base: main
  work_branch: spike/astromech-y5mz
priority: "P1"
relationships:
  blocks:
  - kind: "issue"
    id: "astromech-7ck3"
  children: []
  attachments: []
  relates:
  - kind: "issue"
    id: "astromech-610m"
    type: "advances"
schema: "atelier.issue"
schema_version: 2
status: "in_progress"
title: "Build the WebView2 cost harness"
updated_at: "2026-10-17T20:36:24.051783242+00:00"
---

## Goal

A harness that measures the app window's cost: memory idle and in use, frame-time impact on another process, and cold start, against the budgets in spike S11 (< 250 MB idle, no frame-time impact, < 2 s cold start).

## Done when

One command runs the measurements and writes a report against each budget. It's built and tested on Linux against WebKitGTK (evidence); its first WebView2 run is on Windows in astromech-7ck3.

## Context

Matters more as the window gets more modules. Spike names aren't record IDs.

## Notes

- 2026-10-17 [note] Handoff (Claude): PR #132 ready for review. scripts/webview_cost.py and the App window cost workflow (WebKitGTK reported, WebView2 gating). Windows runner: idle 141.6 MB, CPU 0.33% of the machine (1.32% of one core), cold start 0.65 s with the core running and 0.648 s without; start after a core crash 2.67 s (finding astromech-k8yk). Linux: 289 MB (over, not gating). Independent review done, FIX NOW items fixed and re-reviewed; awaiting Sumeet's approval. For Sumeet: decision astromech-8qcz (CPU basis). Deferred: astromech-wj8v, astromech-e7t5. Frame time next to Elite stays with astromech-7ck3.
