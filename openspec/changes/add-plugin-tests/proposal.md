## Why

The `pond-netns` plugin has no automated tests. Adding unit tests for the pure-logic
modules (`options`, `plumbing` helpers, `tuning` argument-building) increases confidence
in correctness without requiring root, OVS, or a live container runtime.

## What Changes

- Add `#[cfg(test)]` unit test modules inside `options.rs`, `plumbing.rs`, and `tuning.rs`
- Test `PondOptions::from_network` validation paths (missing fields, out-of-range vlan,
  too-long upstream name, defaults)
- Test `derive_upstream_name` determinism and length bounds
- Test `extract_mac` helper with synthetic `LinkMessage` data
- Test `ovs_add_port` argument construction by wrapping `Command` in a thin
  injectable trait (or by making the function return the argv list for inspection)
- Test `set_min_port` and `disable_offloads` argument construction similarly

## Capabilities

### New Capabilities

- `plugin-testing`: Unit-test coverage for options parsing, netlink helpers, and
  external-command argument construction in the pond-netns plugin

### Modified Capabilities

<!-- none -->

## Impact

- `src/net_ns/options.rs` — test module added
- `src/net_ns/plumbing.rs` — `extract_mac` and `ovs_add_port` made testable
  (extract argv or use a command-builder abstraction)
- `src/net_ns/tuning.rs` — argument-building extracted so it can be tested without
  actually invoking `nsenter`/`ethtool`/`sysctl`
- No new runtime dependencies; test-only helpers live under `#[cfg(test)]`
