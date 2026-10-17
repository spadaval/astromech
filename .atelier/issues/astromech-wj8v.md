---
created_at: "2026-10-17T20:11:56.602835344+00:00"
id: "astromech-wj8v"
issue_type: "task"
labels:
- "alpha"
priority: "P3"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "backlog"
title: "WebView cost harness: measure with a played commander's data"
updated_at: "2026-10-17T20:11:56.602835344+00:00"
---

## Goal

The WebView cost harness (scripts/webview_cost.py) measures a fresh profile: no Elite Bindings folder, journals or star map, so the keybinds page and the other lists are short. Seed synthetic fixtures (a full binds file, a journal history, a populated star map) so the idle and in-use numbers reflect a played commander.

## Done when

The harness's idle page shows a long list, and the report says what data it ran with.

## Context

Review of astromech-y5mz (R7).
