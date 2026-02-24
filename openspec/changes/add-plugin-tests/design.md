## Context

`pond-netns` has three internal modules with testable logic:

- `options.rs` — pure parsing/validation from a `HashMap<String,String>`; no I/O
- `plumbing.rs` — netlink calls + two `Command` invocations (`ovs-vsctl`, helper functions)
- `tuning.rs` — two `Command` invocations (`nsenter`+`ethtool`, `setns`+procfs write)

External-command calls (`ovs-vsctl`, `nsenter`, `ethtool`) and netlink sockets cannot
be exercised without root + live hardware. Integration tests for those are explicitly
out of scope per user guidance.

## Goals / Non-Goals

**Goals:**
- Unit-test all validation branches in `PondOptions::from_network`
- Unit-test `derive_upstream_name` (determinism, max-length)
- Unit-test `extract_mac` with synthetic `LinkMessage` data
- Unit-test the *argument construction* of `ovs_add_port`, `disable_offloads`,
  and `set_min_port` without actually spawning processes
- Tests runnable by any developer with `cargo test` (no root, no OVS, no netlink)

**Non-Goals:**
- Integration tests that require root or a live OVS bridge
- Mocking the netlink socket or the `provision` / `deprovision` full flows
- Testing the `driver.rs` Plugin trait (covered by integration-level tests elsewhere)
- 100 % code coverage

## Decisions

### D1 — Command-builder extraction over trait injection

**Chosen:** Extract a private `build_ovs_add_port_args` / `build_ovs_del_port_args` /
`build_ethtool_args` function that returns `Vec<String>` (or `Vec<&str>`). The actual
`Command::new(…).args(…).output()` call in the public function delegates to the
builder. Tests assert on the returned argument vector; no process is spawned.

**Alternatives considered:**
- *Trait injection* (`CommandRunner` trait, mock impl): more powerful but adds boilerplate
  and a trait visible in the public API. Overkill for argument-list verification.
- *`std::env::var`-based override* (e.g., `POND_OVS_VSCTL=/usr/bin/echo`): works for
  smoke tests but still requires spawning a process and is harder to assert on structure.

Builder extraction keeps changes minimal and confined to private helpers.

### D2 — `set_min_port` tested via argument/path inspection, not execution

`set_min_port` uses `nix::sched::setns`, which requires `CAP_SYS_ADMIN`. Extracting
`sysctl_path()` (returns `/proc/sys/net/ipv4/ip_unprivileged_port_start`) and
`format_port_value(port)` (returns `"1024\n"`) as trivial private helpers gives
meaningful tests without any privilege. The `setns` call itself is not unit-tested.

### D3 — Tests live in `#[cfg(test)]` modules in the same file

Keeps each module self-contained. No separate `tests/` directory needed for unit tests.
A `tests/` integration-test directory remains available for future root-requiring tests
guarded by `#[ignore]` or a cargo feature flag.

### D4 — No new runtime dependencies

`crc32fast` is already a dependency (used by `derive_upstream_name`). Synthetic
`LinkMessage` construction uses types already imported from `netlink_packet_route`.
No `mockall`, `wiremock`, or other test-framework crates are added.

### D5 — Extracted helpers carry doc-comments explaining the testability split

Any private function extracted solely to enable unit testing (e.g. `build_ovs_add_port_args`,
`build_ethtool_args`, `sysctl_path`, `format_port_value`) must carry a `// Extracted for
unit-testability: <one-line reason>` comment directly above the `fn` signature. This makes
the intent clear to future readers who might otherwise wonder why a trivial two-line helper
exists instead of being inlined.

The corresponding caller (`ovs_add_port`, `disable_offloads`, `set_min_port`) gets a matching
`// See <helper_name> for testable argument construction.` comment so the split is traceable
in both directions.

## Risks / Trade-offs

- **Refactor surface**: Extracting arg-builder helpers changes the internal structure of
  `plumbing.rs` and `tuning.rs`. The public API is unchanged; risk is low.
- **Partial coverage**: `provision()` and `deprovision()` full flows are not tested.
  This is acceptable given the no-root constraint; the design is explicit about this.
- **LinkMessage construction fragility**: Tests that build synthetic `LinkMessage` structs
  are coupled to `netlink_packet_route` internals. If that crate changes its types these
  tests may need updating. Mitigation: keep those tests minimal (one happy-path case).
