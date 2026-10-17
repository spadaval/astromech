---
created_at: "2026-10-17T01:32:28.682552788+00:00"
id: "astromech-rc8d"
evidence_type: "test"
commit: "69b9058e699b4a709786b36e3a46b3520b521ff5"
result: "pass"
captured_at: "2026-10-17T01:32:28.347353583+00:00"
command: "cargo test -p astromech-galaxy module_list"
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
title: "Done-when: from recorded Spansh pages (5A FSD, 4A distributor, 0A booster near Sol, 16 Oct 2026), G6G-8KJ selling all three (5.95 ly) ranks above Abraham Lincoln selling two (0 ly); also one search per module plus an all-of search, the all-of search finding Bugrov Hub (32 ly) when carriers are excluded, same-name stations kept apart, filters re-checked, unsold modules named. The ranking test fails when ranked by distance alone (checked by mutation)."
updated_at: "2026-10-17T01:32:28.683323526+00:00"
---

## Summary

Done-when: from recorded Spansh pages (5A FSD, 4A distributor, 0A booster near Sol, 16 Oct 2026), G6G-8KJ selling all three (5.95 ly) ranks above Abraham Lincoln selling two (0 ly); also one search per module plus an all-of search, the all-of search finding Bugrov Hub (32 ly) when carriers are excluded, same-name stations kept apart, filters re-checked, unsold modules named. The ranking test fails when ranked by distance alone (checked by mutation).

## Command

```console
cargo test -p astromech-galaxy module_list
```

Exit status: 0

## Stdout

Bytes: 908
Truncated: no

```text

running 7 tests
test module_list::tests::nothing_listed_asks_nothing ... ok
test module_list::tests::a_station_selling_all_three_ranks_above_a_closer_one_selling_two ... ok
test module_list::tests::a_module_no_station_sells_is_named ... ok
test module_list::tests::stations_sharing_a_name_in_different_systems_stay_apart ... ok
test module_list::tests::the_all_of_search_finds_all_in_one_stations_past_each_modules_nearest ... ok
test module_list::tests::one_search_per_module_and_one_for_all_of_them ... ok
test module_list::tests::filters_are_rechecked_and_the_limit_applies ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 0.05s


running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s


running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s
```

## Stderr

Bytes: 311
Truncated: no

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.12s
     Running unittests src/lib.rs (target/debug/deps/astromech_galaxy-a08adfc363780b79)
     Running tests/powerplay.rs (target/debug/deps/powerplay-e2f36cae0368588c)
     Running tests/sites.rs (target/debug/deps/sites-01039a139ce9070c)
```

