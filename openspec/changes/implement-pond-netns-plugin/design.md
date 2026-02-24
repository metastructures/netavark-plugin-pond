## Context

netavark plugins are standalone binaries invoked by netavark (called by podman) via three subcommands: `create`, `setup`, and `teardown`. The plugin reads JSON from stdin and writes JSON to stdout. It runs as root and must perform all kernel operations itself — netavark passes it no open netlink sockets (the `_netlink_sockets` parameter in `PluginDriver::setup` is discarded).

In podman pod mode, `setup` is called exactly once per network per pod — for the infra (pause) container. Sidecar containers join the infra container's network namespace via the OCI runtime (`CLONE_NEWNET` is not called for them). This means the plugin receives a single `netns_path` pointing to the infra container's netns, which all pod containers share. There is no `delete` subcommand in the current netavark plugin API; teardown is the terminal lifecycle event.

The current `driver.rs` stub is partially wired: the `Plugin` trait impl exists, `open_netlink_sockets` is used correctly, and the binary entrypoint is correct. The implementation of `setup` and `teardown` is incomplete and the `create` logic is incorrect (hardcodes interface names, ignores options).

## Goals / Non-Goals

**Goals:**
- Implement `create`: validate required options, derive upstream interface name, return normalized `Network`
- Implement `setup`: full veth creation, netns configuration, OVS registration
- Implement `teardown`: OVS deregistration and veth deletion
- Structure code so the `provision`/`deprovision` logic can be trivially moved to `create`/`delete` if netavark adds those subcommands upstream
- Reject standalone (non-pod) container setup with a clear error

**Non-Goals:**
- Managing the OVS bridge itself (pre-existing, operator responsibility)
- IPAM beyond using the subnet already in the `Network` config (netavark's `host-local` driver handles allocation)
- IPv6 (out of scope for initial implementation)
- Named kernel netns in `/var/run/netns/` (anonymous infra container netns is sufficient)
- Firewall / nftables rules (future work)

## Decisions

### D1: One veth pair per network, not per container

**Decision**: A single veth pair is created during `setup` and shared by all pod containers via the infra netns.

**Rationale**: All containers in a pod share the infra container's netns. The plugin is called only once (for infra). Each OVS port corresponds to one pod network, which maps cleanly to one VLAN-tagged access port. Creating per-container veths would require a separate bridge or more complex OVS topology.

**Alternative considered**: per-container veth pairs with a Linux bridge in the pod netns. Rejected — introduces Linux bridge, contradicts the goal of OVS-only L2 forwarding.

---

### D2: External binaries for OVS and ethtool

**Decision**: Use `ovs-vsctl` via `std::process::Command` for OVS port management, and `nsenter` + `ethtool` for offload disabling inside the container netns.

**Rationale**: No trustworthy Rust OVSDB crate exists. Implementing OVSDB JSON-RPC directly is ~500 LOC of protocol code. For ethtool, `ETHTOOL_SFEATURES` via ioctl requires ~150–200 LOC of unsafe Rust with variable-length structs — significant maintenance burden for a tuning call. Both `ovs-vsctl` and `ethtool` come from well-audited system packages (same trust tier as the OVS bridge they depend on). Supply chain risk of unknown crates outweighs the benefit of removing binary dependencies that are already required by the deployment.

**Alternative considered**: `ethtool` via `nix` ioctl. Rejected — high unsafe code surface for minimal gain.

---

### D3: sysctl via `nix::sched::setns` + procfs

**Decision**: To set `net.ipv4.ip_unprivileged_port_start` inside the container netns, enter the netns using `nix::sched::setns`, write to `/proc/sys/net/ipv4/ip_unprivileged_port_start`, then return to the host netns.

**Rationale**: The plugin binary is single-threaded (no async runtime). `setns` on a single-threaded process is safe and correct. Writing to `/proc/sys/...` from within the netns is the standard approach. The `nix` crate is one of the most trusted crates in the ecosystem and is already an indirect dependency of `netavark`.

**Alternative considered**: `nsenter` + `sysctl` binary. Rejected — adds a binary dependency for something that's trivial with `nix` and requires no unsafe code beyond `setns` itself.

---

### D4: Upstream interface name derivation

**Decision**: Use `--option upstream=<name>` if provided (Strategy 3). Fall back to `pond` + first 8 hex chars of `crc32fast::hash(network_name.as_bytes())`, truncated to 15 chars (Linux `IFNAMSIZ` limit).

**Rationale**: Explicit naming gives operators control and matches the ergonomics of the systemd unit (`POD_LINK_UPSTREAM`). The derived fallback is deterministic (same name on re-create with same network name), short, and unique enough for typical deployments. `crc32fast` is a minimal, audited crate with no unsafe code beyond SIMD intrinsics.

**Alternative considered**: First N chars of network ID. Rejected — IDs are assigned at create time and may not be stable across network recreation.

---

### D5: Code structure for future refactoring

**Decision**: Separate `options.rs`, `plumbing.rs`, and `tuning.rs` modules with a thin `driver.rs` that wires them together.

**Rationale**: When netavark adds `create`/`delete` subcommands, `plumbing::provision` maps directly to `create` and `plumbing::deprovision` maps to `delete`. Today they are called from `setup`/`teardown` with idempotency checks. The separation makes this a rename + move, not a refactor.

```
driver.rs       Plugin trait impl, wires everything
options.rs      Parse + validate Network.options HashMap → typed PondOptions struct
plumbing.rs     provision(options, netns_path) / deprovision(options)
                  — pure netlink + ovs-vsctl, no state
tuning.rs       disable_offloads(netns_path, iface) / set_min_port(netns_path, port)
                  — nsenter+ethtool binary, nix setns+procfs
netlink.rs      (reserved) plugin-specific netlink helpers if needed
```

---

### D6: Idempotency

**Decision**: In `setup`, check if the upstream veth already exists via `get_link(Name(upstream))`. If it does, log a warning and return the existing `StatusBlock` rather than failing.

**Rationale**: In normal pod mode this never happens (option C confirmed from netavark source). But defensive idempotency costs nothing and prevents hard failures on unexpected retry scenarios.

---

### D7: Pod-only enforcement

**Decision**: Detect non-pod usage by checking whether the network was configured with `internal: true` and no port mappings. If port mappings are present on a network that would normally be pod-internal, return an error.

**Note**: This is a best-effort heuristic. A more reliable signal would be a container name pattern (`-infra` suffix) but that is fragile. For now, document the pod-only intent clearly in the error message and in the README.

## Risks / Trade-offs

**[Risk] netavark calls setup more than once for edge cases** → Idempotency check (D6) handles this. The upstream veth existence is the ground truth.

**[Risk] ovs-vsctl not on PATH at plugin invocation time** → Plugin fails with a clear error from `Command::status()`. Document the dependency in README. Deployment environments running OVS-DPDK will always have this.

**[Risk] Veth peer temporary name collision** → Use `<upstream>p` as the temp peer name. If a stale `<upstream>p` exists (crash during setup), teardown's `del_link` will clean it up. setup's idempotency check catches the full-setup case.

**[Risk] netavark API changes in future versions** → Plugin depends on the published `netavark = "1.10.3"` crate. The plugin API (stdin/stdout JSON protocol) is versioned (`API_VERSION = "1.0.0"`). Types are stable. Breakage would require an explicit version bump.

**[Risk] sysctl write fails in restricted environments** → Return error with context. The sysctl is optional behavior (the pod still works without it); a future flag could make it best-effort.
