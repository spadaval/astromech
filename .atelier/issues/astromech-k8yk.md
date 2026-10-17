---
created_at: "2026-10-17T19:59:11.181360982+00:00"
id: "astromech-k8yk"
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
title: "App window takes about 2 s longer to open after a core crash"
updated_at: "2026-10-17T19:59:11.181360982+00:00"
---

## Goal

Opening the app window after the core crashed (or was killed) should be as quick as any other start.

## Done when

The WebView cost harness's start after a core crash (scripts/webview_cost.py) is within a few hundred ms of the normal cold start on the Windows runner.

## Context

Measured by the WebView cost harness (astromech-y5mz) on the Windows runner. The window's first page load comes 0.42 s after start when the core is running, but 2.44 s after a start that finds the core gone with its core.json left behind (on Linux, 0.71 s either way). The shell's setup calls `core_alive()`, which connects to the port in the stale core.json. On Windows a connect to a closed localhost port takes about 2 s (SYN retries), and setup runs before the window's event loop, so the page waits. A crashed core is the case the window's crash restart (V1-DESIGN §7.7) exists for. Options: check the endpoint's pid is alive before connecting, use a short connect timeout for localhost, or move the check off the setup path.
