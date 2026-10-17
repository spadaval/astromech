---
created_at: "2026-10-17T20:11:56.524818714+00:00"
id: "astromech-8qcz"
issue_type: "decision"
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
title: "S11 idle CPU budget: share of the machine or of one core?"
updated_at: "2026-10-17T20:11:56.524818714+00:00"
---

## Context

The WebView cost harness (astromech-y5mz) reports both and judges the machine share. On the 4-CPU Windows runner the window idles at 0.33% of the machine, which is 1.32% of one core: it passes by the first reading and fails by the second. On a 16-thread desktop the same load reads about 0.08% of the machine. Raised in review of astromech-y5mz (R4).

## Question

ARCHITECTURE §18.1 sets the app window's idle CPU budget at <= 1%. Is that 1% of the whole machine (Task Manager's reading) or 1% of one core?

## Options

- Machine share (current): matches Task Manager; lenient on many-core PCs.
- One core: host-independent; stricter.
