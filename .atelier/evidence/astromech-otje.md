---
created_at: "2026-10-17T01:33:28.620226650+00:00"
id: "astromech-otje"
evidence_type: "test"
commit: "69b9058e699b4a709786b36e3a46b3520b521ff5"
result: "pass"
captured_at: "2026-10-17T01:32:42.134376691+00:00"
command: "bash -c 'cargo fmt --all --check && cargo clippy --workspace --all-targets -q -- -D warnings && cargo test --workspace -q 2>&1 | grep -E '\"'\"'test result'\"'\"' | grep -v '\"'\"' 0 failed'\"'\"' ; ! cargo test --workspace -q 2>&1 | grep -q FAILED && python3 scripts/check-deps.py && (cd app/src-tauri && cargo metadata --locked --offline --format-version 1 >/dev/null) && cd app && pnpm install --frozen-lockfile && pnpm lint && pnpm check && pnpm build && pnpm test'"
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
title: "Gates: cargo fmt --check, clippy -D warnings, cargo test --workspace, check-deps, src-tauri cargo metadata --locked --offline, and in app/ pnpm install --frozen-lockfile, lint, check, build, test (152 tests)."
updated_at: "2026-10-17T01:33:28.620489921+00:00"
---

## Summary

Gates: cargo fmt --check, clippy -D warnings, cargo test --workspace, check-deps, src-tauri cargo metadata --locked --offline, and in app/ pnpm install --frozen-lockfile, lint, check, build, test (152 tests).

## Command

```console
bash -c 'cargo fmt --all --check && cargo clippy --workspace --all-targets -q -- -D warnings && cargo test --workspace -q 2>&1 | grep -E '"'"'test result'"'"' | grep -v '"'"' 0 failed'"'"' ; ! cargo test --workspace -q 2>&1 | grep -q FAILED && python3 scripts/check-deps.py && (cd app/src-tauri && cargo metadata --locked --offline --format-version 1 >/dev/null) && cd app && pnpm install --frozen-lockfile && pnpm lint && pnpm check && pnpm build && pnpm test'
```

Exit status: 0

## Stdout

Bytes: 10093
Truncated: yes

```text
dependency rules OK (26 workspace crates)
Lockfile is up to date, resolution step is skipped
Already up to date

Done in 549ms using pnpm v10.34.6

> astromech-app@0.1.5 lint /home/supadava/projects/astromech-wt-task-astromech-x53n/app
> eslint .


> astromech-app@0.1.5 check /home/supadava/projects/astromech-wt-task-astromech-x53n/app
> svelte-check --tsconfig ./tsconfig.json --fail-on-warnings

1792200799050 START "/home/supadava/projects/astromech-wt-task-astromech-x53n/app"
1792200799052 COMPLETED 354 FILES 0 ERRORS 0 WARNINGS 0 FILES_WITH_PROBLEMS

> astromech-app@0.1.5 build /home/supadava/projects/astromech-wt-task-astromech-x53n/app
> vite build

vite v7.3.7 building client environment for production...
transforming...
✓ 303 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                                                          0.39 kB │ gzip:  0.27 kB
dist/assets/saira-semi-condensed-vietnamese-600-normal-CtOKQqyp.woff     5.85 kB
dist/assets/saira-semi-condensed-vietnamese-400-normal-DZeVTnup.woff     5.89 kB
dist/assets/saira-semi-condensed-vietnamese-500-normal-Dvrr_M0-.woff     5.90 kB
dist/assets/saira-semi-condensed-vietnamese-600-normal-BleKkWhQ.woff2    7.06 kB
dist/assets/saira-semi-condensed-vietnamese-400-normal-BjlVRgV2.woff2    7.08 kB
dist/assets/saira-semi-condensed-vietnamese-500-normal-DaSOOpQd.woff2    7.14 kB
dist/assets/share-tech-mono-latin-400-normal-15MgdvEM.woff              10.24 kB
dist/assets/share-tech-mono-latin-400-normal-CCqdbj5z.woff2             13.50 kB
dist/assets/saira-semi-condensed-latin-ext-400-normal-FBtzH8lb.woff     13.54 kB
dist/assets/saira-semi-condensed-latin-ext-600-normal-gxH8H1Hk.woff     13.56 kB
dist/assets/saira-semi-condensed-latin-ext-500-normal-CBunCqud.woff     13.70 kB
dist/assets/saira-semi-condensed-latin-ext-400-normal-z9rK9nq6.woff2    14.67 kB
dist/assets/saira-semi-condensed-latin-ext-600-normal-cD8iogYK.woff2    14.94 kB
dist/assets/saira-semi-condensed-latin-ext-500-normal-DcBBbpUL.woff2    15.00 kB
dist/assets/saira-semi-condensed-latin-600-normal-CyCQx_HP.woff         15.71 kB
dist/assets/saira-semi-condensed-latin-500-normal-DNw5c0PQ.woff         15.78 kB
dist/assets/saira-semi-condensed-latin-400-normal-GbyiKxUx.woff         15.86 kB
dist/assets/saira-semi-condensed-latin-600-normal-BlCmTINa.woff2        18.22 kB
dist/assets/saira-semi-condensed-latin-500-normal-OkPZo89y.woff2        18.26 kB
dist/assets/saira-semi-condensed-latin-400-normal-DBFPQfhT.woff2        18.33 kB
dist/assets/View-CKZXWWMe.css                                            0.40 kB │ gzip:  0.26 kB
dist/assets/View-GfvHuV02.css                                            0.40 kB │ gzip:  0.26 kB
dist/assets/View-B6pN-6rI.css                                            0.55 kB │ gzip:  0.31 kB
dist/assets/SystemChip-D3GM7gCR.css                                      0.61 kB │ gzip:  0.31 kB
dist/assets/View-DI0And5n.css                                            0.93 kB │ gzip:  0.37 kB
dist/assets/PresetPicker-D8iBmlpT.css                                    1.05 kB │ gzip:  0.41 kB
dist/assets/View-W1ScxR78.css                                            1.10 kB │ gzip:  0.42 kB
dist/assets/View-DruMCGqz.css                                            1.22 kB │ gzip:  0.45 kB
dist/assets/View-B1E7xiYe.css                                            1.43 kB │ gzip:  0.52 kB
dist/assets/View-ggjitJdN.css                                            1.43 kB │ gzip:  0.48 kB
dist/assets/View-CEILl0y0.css                                            1.56 kB │ gzip:  0.55 kB
dist/assets/View-DZ9WtIcy.css                                            1.67 kB │ gzip:  0.55 kB
dist/assets/View-BjzciPvw.css                                            1.74 kB │ gzip:  0.58 kB
dist/assets/View-CCvET0VO.css                                            1.92 kB │ gzip:  0.61 kB
dist/assets/View-BBLqak4E.css                                            1.93 kB │ gzip:  0.58 kB
dist/assets/View-D5pn8kgE.css
```

## Stderr

Bytes: 0
Truncated: no

```text
```

