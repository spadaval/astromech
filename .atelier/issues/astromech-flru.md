---
created_at: "2026-10-17T16:54:23.485268262+00:00"
id: "astromech-flru"
issue_type: "task"
labels:
- "alpha"
- "size-m"
workflow_branch:
  branch_base: main
  work_branch: task/astromech-flru
priority: "P1"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "in_progress"
title: "Onboarding smoke test to a mock gear-down on Windows CI"
updated_at: "2026-10-17T20:36:22.894954354+00:00"
---

## Goal

Prove the fresh-install path without the game: on the Windows runner, install silently, drive the onboarding wizard, and reach a verified gear-down against `mockgame`, with timings.

## Done when

A CI job (or script run on the Windows runner) reports install-to-gear-down time and passes; the run is linked as evidence.

## Context

`.github/workflows/release.yml` already does a silent per-user install, start, status, quit and uninstall. Voice can't be spoken on the runner: feed a recorded or rendered 'gear down' clip through the core's test input instead of a microphone. astromech-we0f (a timed human run) still follows.

## Notes

- 2026-10-17 [note] Handoff (Claude): PR #124 ready for review. The onboarding smoke test runs in the Release build after the installer smoke test: fresh silent install, real wizard over DevTools (shell flag --webview-debug-port), PTT and kill switch bound by pressing keys, mock game as EliteDangerous64.exe, gear down run live from the voice tester and confirmed in Status.json. Green: 14.93 s, 10.35 s and 9.12 s install to verified gear-down (budget 300 s). Fault run (mock drops the press) fails as designed. Independent review done, FIX NOW items fixed and re-reviewed; awaiting Sumeet's approval. Deferred: astromech-c9i7 (tag gating), astromech-0sls, astromech-nmn1. Not covered: speech, mic and calibration steps, the timed human run (astromech-we0f).
