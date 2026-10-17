---
created_at: "2026-10-17T19:22:54.539314492+00:00"
id: "astromech-3vjv"
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
title: "MVP criteria harness: kill switch during more kinds of actions"
updated_at: "2026-10-17T19:22:54.539314492+00:00"
---

## Goal

The kill-switch phase of scripts/mvp_criteria.py kills only lights (key plus modifier) and gear presses. Also kill during the paste (Ctrl+V, then Enter: Enter must not be sent) and during a pending confirmation, and check that more than verification is cancelled.

## Done when

The harness reports the kill time and cancellation for each kind, against the 50 ms criterion.

## Context

Review of astromech-08w8 (R4, deferred).
