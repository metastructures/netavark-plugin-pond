## Why

The systemd unit file in README.md sets up a pod network using raw `ip` commands and `ovs-vsctl` calls in `ExecStartPre`/`ExecStopPost` directives. This is fragile, hard to maintain, and bypasses podman's network lifecycle. A native netavark plugin replaces all of this with a single `podman network create --driver pond-netns` invocation that integrates cleanly with podman's create/setup/teardown lifecycle.

## What Changes

- **New binary**: `pond-netns` — a netavark plugin binary implementing the `Plugin` trait (`create`, `setup`, `teardown`)
- **`create`**: validates required options (`bridge`, `vlan`), derives the upstream veth name, normalizes and returns the `Network` config
- **`setup`**: creates a veth pair, moves the inner end into the infra container's netns, configures it (IP, default route, sysctl, ethtool offloads), brings up the upstream end, and registers it as a VLAN-tagged access port on the pre-existing OVS bridge with troubleshooting metadata
- **`teardown`**: removes the upstream port from OVS and deletes the veth pair
- **New modules**: `options.rs` (option parsing/validation), `plumbing.rs` (netlink + OVS operations), `tuning.rs` (sysctl + ethtool via binaries)
- **Dependency additions**: `nix` (sysctl via setns), `crc32fast` (upstream name derivation)

## Capabilities

### New Capabilities

- `network-lifecycle`: Manage the full create/setup/teardown lifecycle of a pod network through the netavark plugin API, replacing the systemd unit's ExecStartPre/ExecStopPost shell commands
- `ovs-integration`: Wire the host-side veth to a pre-existing OVS bridge as a VLAN-tagged access port with metadata for troubleshooting, using `ovs-vsctl`
- `netns-configuration`: Configure the container-side veth inside the infra container's netns: IP assignment, default route, sysctl unprivileged port minimum, ethtool offload disabling

### Modified Capabilities

## Impact

- **Replaces**: all `ExecStartPre`/`ExecStopPost` lines in the systemd unit in README.md
- **Requires on host**: `ovs-vsctl` (OVS package), `ethtool`, `nsenter` (util-linux) — same trust tier, already present on OVS-DPDK deployments
- **Netavark version**: `1.10.3` (current `Cargo.toml`); uses `netavark::network::netlink::Socket` and `open_netlink_sockets`
- **Runs as**: root (required for netlink and netns operations)
- **OVS bridge**: must be pre-created by the operator — not managed by the plugin
- **Pod mode only**: standalone containers attaching to this network are rejected with a clear error
