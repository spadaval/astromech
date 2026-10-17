---
created_at: "2026-10-17T01:32:31.618075351+00:00"
id: "astromech-aewy"
evidence_type: "test"
commit: "69b9058e699b4a709786b36e3a46b3520b521ff5"
result: "pass"
captured_at: "2026-10-17T01:32:28.781367113+00:00"
command: "bash -c 'cd app && pnpm vitest run src/modules/engineering/Build.test.ts src/lib/stations.test.ts src/lib/outfitting.test.ts'"
exit_status: "0"
target:
  kind: "issue"
  id: "astromech-x53n"
  role: "validates"
relationships:
  blocks: []
  children: []
  attachments:
  - kind: "issue"
    id: "astromech-x53n"
    role: "validates"
  relates: []
schema: "atelier.evidence"
schema_version: 2
status: "recorded"
title: "Component tests: the Ships page lists the plan's modules in slot order (armour not searched) and its where-to-buy rows put Stronghold Carrier (3 of 3, 4.4 ly) above Abraham Lincoln (2 of 3, 0 ly, no 6A Thrusters); dropping a module from the plan re-searches without it; no plan, no section. Preview narrowing, filters, unavailable warning and labels in stations.test.ts."
updated_at: "2026-10-17T01:32:31.618380681+00:00"
---

## Summary

Component tests: the Ships page lists the plan's modules in slot order (armour not searched) and its where-to-buy rows put Stronghold Carrier (3 of 3, 4.4 ly) above Abraham Lincoln (2 of 3, 0 ly, no 6A Thrusters); dropping a module from the plan re-searches without it; no plan, no section. Preview narrowing, filters, unavailable warning and labels in stations.test.ts.

## Command

```console
bash -c 'cd app && pnpm vitest run src/modules/engineering/Build.test.ts src/lib/stations.test.ts src/lib/outfitting.test.ts'
```

Exit status: 0

## Stdout

Bytes: 238
Truncated: no

```text

 RUN  v5.0.5 /home/supadava/projects/astromech-wt-task-astromech-x53n/app


 Test Files  3 passed (3)
      Tests  27 passed (27)
   Start at  21:32:29
   Duration  1.96s (environment 42%, tests 37%, setup 10%, transform 9%, import 1%)
```

## Stderr

Bytes: 0
Truncated: no

```text
```

