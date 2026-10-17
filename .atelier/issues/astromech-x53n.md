---
created_at: "2026-10-15T20:36:13.655103980+00:00"
id: "astromech-x53n"
issue_type: "task"
labels:
- "size-m"
workflow_branch:
  branch_base: main
  work_branch: task/astromech-x53n
priority: "P1"
relationships:
  blocks: []
  children: []
  attachments: []
  relates:
  - kind: "issue"
    id: "astromech-7q0z"
    type: "advances"
schema: "atelier.issue"
schema_version: 2
closed_at: "2026-10-17T01:33:45.603370548+00:00"
status: "done"
title: "Find a station with every module a build still needs"
updated_at: "2026-10-17T01:33:45.603370548+00:00"
---

## Goal

From a Builds plan, list stations ranked by how many of the still-needed modules they sell, then by distance (Inara's "all in one station").

## Done when

For a plan needing three modules and recorded responses, a station selling all three ranks above a closer one selling two.

## Context

Cross-link with the engineering module's Builds plans.

## Notes

- 2026-10-17 [note] Built in 69b9058. Plans only name blueprints for fitted modules; they don't swap modules (that's astromech-xw9t, backlog). So no plan has 'still-needed' modules as such. The Ships page uses what the plan names: the module fitted in each targeted slot (armour is sold per ship and isn't searched), described as 'for fitting this build to another ship'. The Done-when holds as a list of three named modules ranked from recorded Spansh responses (rc8d). Question for Sumeet: keep the plan-modules meaning, use the ship's whole loadout, or switch to planned-but-not-fitted once xw9t lands? Proof: rc8d (Done-when), aewy (component), ckhr (UI check), 1l3m (shell type-check), otje (gates). Open: independent review; shell compiled only on Windows CI.

## Close Reason

Stations ranked by how many of the plan's modules they sell, then nearest (module_list, stations_modules, Ships page). Plans don't swap modules yet (astromech-xw9t), so the list is the plan's targeted modules.
