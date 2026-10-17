---
created_at: "2026-10-17T20:10:24.648177355+00:00"
id: "astromech-c9i7"
issue_type: "task"
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
status: "backlog"
title: "Decide whether the onboarding smoke test gates tagged releases"
updated_at: "2026-10-17T20:10:24.648177355+00:00"
---

## Goal

Decide whether a failing onboarding smoke test (scripts/onboarding_smoke.py, Release build) should stop a tagged release from publishing.

## Done when

The Release build either keeps the step as a gate for tags (after a record of green runs) or runs it with continue-on-error on tags, and the choice is noted in the workflow.

## Context

Review of astromech-flru (R2). The step runs after the installer artifact is uploaded, but publish needs the installer job to pass, so a flaky run (window timing, foreground rules, fixed DevTools port) would hold up a release. It has one green PR run so far.
