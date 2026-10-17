---
created_at: "2026-10-17T21:32:03.895164138+00:00"
id: "astromech-pd3h"
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
status: "todo"
title: "Presets and controller preset picker hide failed reloads"
updated_at: "2026-10-17T21:32:03.895164138+00:00"
---

## Goal

A failed binds reload is visible wherever the preset list is shown.

## Done when

On Presets, a successful action followed by a failed reload (run() in presets/View.svelte) shows the error, not only the success message over the old list. The preset picker on the controller page shows binds.error when the load failed.

## Context

Found in the re-review of PR #140 (R1, R2); both predate it. Related states are covered by app/src/lib/ui/emptystates.test.ts.
