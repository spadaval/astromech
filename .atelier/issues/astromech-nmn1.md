---
created_at: "2026-10-17T20:34:12.139111763+00:00"
id: "astromech-nmn1"
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
title: "Onboarding smoke test: run it on PRs that change the shared frontend code it relies on"
updated_at: "2026-10-17T20:34:12.139111763+00:00"
---

## Goal

The Release build's PR paths cover the onboarding wizard, the voice tester, App.svelte and the mock game, but not shared frontend code the wizard relies on (app/src/lib/store.svelte.ts, the binding capture in app/src/lib/, shared styles). Decide whether to add them, given that every such PR then runs the Windows release build.

## Done when

The paths are widened, or the choice not to is noted in release.yml.

## Context

Re-review of astromech-flru (N1).
