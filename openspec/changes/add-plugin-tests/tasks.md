## 1. Refactor plumbing.rs for testability

- [x] 1.1 Extract `build_ovs_add_port_args(params: &ProvisionParams) -> Vec<String>` from `ovs_add_port`; add `// Extracted for unit-testability` comment
- [x] 1.2 Update `ovs_add_port` to delegate to `build_ovs_add_port_args`; add `// See build_ovs_add_port_args` comment at call site
- [x] 1.3 Verify `cargo build` still passes after refactor

## 2. Refactor tuning.rs for testability

- [x] 2.1 Extract `build_ethtool_args(netns_path: &str, iface: &str) -> Vec<String>` from `disable_offloads`; add `// Extracted for unit-testability` comment
- [x] 2.2 Update `disable_offloads` to delegate to `build_ethtool_args`; add `// See build_ethtool_args` comment at call site
- [x] 2.3 Extract `format_port_value(port: u16) -> String` from `set_min_port`; add `// Extracted for unit-testability` comment
- [x] 2.4 Update `set_min_port` to use `format_port_value`; add `// See format_port_value` comment at call site
- [x] 2.5 Verify `cargo build` still passes after refactor

## 3. Unit tests — options.rs

- [x] 3.1 Add `#[cfg(test)] mod tests` block to `options.rs`
- [x] 3.2 Test: missing `bridge` option returns error containing "missing required option: bridge"
- [x] 3.3 Test: missing `vlan` option returns error containing "missing required option: vlan"
- [x] 3.4 Test: `vlan=0` returns out-of-range error
- [x] 3.5 Test: `vlan=4095` returns out-of-range error
- [x] 3.6 Test: `vlan=abc` returns non-numeric error
- [x] 3.7 Test: `upstream` longer than 15 chars returns IFNAMSIZ error
- [x] 3.8 Test: only `bridge` + `vlan` provided → `min_port=1024`, `mtu=1500`, `upstream` derived
- [x] 3.9 Test: all options explicit → all fields match provided values
- [x] 3.10 Test `derive_upstream_name`: same input produces same output (determinism)
- [x] 3.11 Test `derive_upstream_name`: result is always exactly 12 characters
- [x] 3.12 Test `derive_upstream_name`: result starts with "pond"

## 4. Unit tests — plumbing.rs

- [x] 4.1 Add `#[cfg(test)] mod tests` block to `plumbing.rs`
- [x] 4.2 Test `extract_mac`: `LinkMessage` with a 6-byte `LinkAttribute::Address` returns correct hex MAC
- [x] 4.3 Test `extract_mac`: `LinkMessage` with no address attribute returns empty string
- [x] 4.4 Test `build_ovs_add_port_args`: result contains `add-port`, bridge name, upstream name
- [x] 4.5 Test `build_ovs_add_port_args`: result contains `tag=<vlan>` and `vlan_mode=access`
- [x] 4.6 Test `build_ovs_add_port_args`: result contains all three `external_ids` entries

## 5. Unit tests — tuning.rs

- [x] 5.1 Add `#[cfg(test)] mod tests` block to `tuning.rs`
- [x] 5.2 Test `build_ethtool_args`: result contains `--net=<path>`, `ethtool`, `--offload`, iface name
- [x] 5.3 Test `build_ethtool_args`: result contains `tx`, `off`, `sg`, `off`, `tso`, `off`
- [x] 5.4 Test `format_port_value(1024)` returns `"1024"`
- [x] 5.5 Test `format_port_value(0)` returns `"0"`

## 6. Verify

- [x] 6.1 Run `cargo test` and confirm all new tests pass
- [x] 6.2 Run `cargo clippy` and confirm no new warnings
