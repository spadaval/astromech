---
created_at: "2026-10-17T16:54:23.553683589+00:00"
id: "astromech-5qh0"
issue_type: "task"
labels:
- "alpha"
- "size-m"
workflow_branch:
  branch_base: main
  work_branch: task/astromech-5qh0
priority: "P2"
relationships:
  blocks: []
  children: []
  attachments: []
  relates: []
schema: "atelier.issue"
schema_version: 2
status: "in_progress"
title: "Agent walk of every app screen"
updated_at: "2026-10-17T21:26:36.026313933+00:00"
---

## Goal

Before Sumeet's walk (astromech-snqt), an agent screenshots every page and state against the example fixture and fixes what is visibly broken: overflow, empty states, loading and error states, narrow windows.

## Done when

A note lists every page with ok or the issue filed or fixed for it, with the screenshot set attached as evidence.

## Context

`scripts/app-shots.sh` and `scripts/ui` (headless Chrome via Puppeteer) run the app without Tauri.

## Notes

- 2026-10-17 [note] Walk results (fixture; core ready/empty/new; 1400 and 900 px). Full PNG set (uncommitted): /home/supadava/projects/astromech-wt-walk/docs/evidence/astromech-5qh0/full. Committed before/after/states: docs/evidence/astromech-5qh0/.
  FIXED (55b4782): keybinds, presets, keytester at 900 px (horizontal overflow); keybinds empty (Loading forever); presets empty (no message) and preset picker (blank box); route plot field alignment and colour; station Check again styling; odyssey Held only checkbox colour; header and margins under 1100 px.
  OK: controller, engineering ships, materials, on foot, engineers, outfitting, shipyard, services, route (empty, plotted, 900/1400), trade, colonisation, exploration, exobiology (incl. Elite not running), powerplay, logbook, session, mining, station, settings (callouts, prospector thresholds), data sharing, diagnostics, voice tester (learned phrases), setup wizard (all 6 steps, empty/new, 900/1400), command palette, bind dialog. Console 404 is favicon.ico in the preview only.
  FILED: astromech-x81p (preview cannot show empty/loading/error for fixture-backed pages, so those states were NOT seen: engineering, materials, logbook, session, mining, station, trade, colonisation, exploration, powerplay, data sharing, diagnostics); astromech-ak62 (header wraps at 900, Powerplay gap).
  Frontend checks after the change: pnpm lint, check, build, test (216 pass), scripts/check-guide.py (0 problems).
