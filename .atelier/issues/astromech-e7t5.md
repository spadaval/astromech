---
created_at: "2026-10-17T20:34:33.499386010+00:00"
id: "astromech-e7t5"
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
title: "WebView cost harness: confirm the page really changed"
updated_at: "2026-10-17T20:34:33.499386010+00:00"
---

## Goal

The harness checks that the window received each page a second start asked for (the shell's log), not that the frontend navigated. Have the frontend report the page it shows (for example a log line through a command, or the title) and count those.

## Done when

A broken route handler in the frontend fails the harness's page-change check.

## Context

Re-review of astromech-y5mz (N1).
