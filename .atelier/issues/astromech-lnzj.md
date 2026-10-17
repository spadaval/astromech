---
created_at: "2026-10-17T19:22:54.264627539+00:00"
id: "astromech-lnzj"
issue_type: "bug"
labels:
- "alpha"
priority: "P2"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "todo"
title: "Check GuiFocus values for the left and right panels"
updated_at: "2026-10-17T19:22:54.264627539+00:00"
---

## Goal

Make sure "left panel" and "right panel" verify against the GuiFocus values the game really writes.

## Done when

GUI_LEFT and GUI_RIGHT in crates/astromech-status/src/status_file.rs match the game (checked in the game or against Frontier's journal docs), with a test.

## Context

status_file.rs has GUI_LEFT = 1 and GUI_RIGHT = 2. Frontier's journal documentation, as I recall it (not re-checked here), lists 1 = InternalPanel (right hand side) and 2 = ExternalPanel (left hand side). If that is right, "left panel" and "right panel" press the right keys but their verification waits for the other panel's value and logs "not confirmed". The mock game can't catch this because the harness uses the crate's values. Noticed while building the MVP criteria harness (astromech-08w8).
