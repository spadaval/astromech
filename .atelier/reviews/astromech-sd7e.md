---
schema: atelier.review
schema_version: 1
id: astromech-sd7e
title: 'R1: the action-start estimate is mostly constants (S9, config) yet gates the exit status; runner jitter flipped it 403.7 FAIL / 398.0 PASS. Report it with its margin and don''t gate on it; document the up-to-100 ms mic wait. Reading constants from the config can be deferred.'
target: astromech-08w8
identity: independent reviewer subagent (sonnet, agent-factory review)
commit: b26b3d26d5b2d0452779ad9977b68d589a97ce3d
created_at: 2026-10-17T19:23:18.635407919Z
event:
  event: finding
  recommendation: fix_now
---

R1: the action-start estimate is mostly constants (S9, config) yet gates the exit status; runner jitter flipped it 403.7 FAIL / 398.0 PASS. Report it with its margin and don't gate on it; document the up-to-100 ms mic wait. Reading constants from the config can be deferred.
