## Context

Each time a pod restarts, the veth pair is recreated and the kernel assigns a new MAC address to the inner interface. The IP address remains the same (host-local IPAM). The gateway holds a stale ARP cache entry (old MAC → same IP) and continues forwarding packets to the non-existent MAC until the entry expires — typically 5–30 minutes.

An ARP request sent from the container netns after `ovs_add_port` completes is the correct mechanism. The frame carries the pod's new MAC as `sender_hw` and asks for the gateway's MAC (`target_ip = gateway_ip`). This works correctly in all OVS configurations:

- **OVS with ARP responder** for the gateway IP: OVS intercepts the request, replies on behalf of the gateway, but records `sender_hw = new_mac` in its MAC learning table. The pod gets the gateway MAC and upstream routing resumes immediately.
- **OVS without ARP responder**: OVS floods the request to all ports in the VLAN; the gateway receives a legitimate ARP request from `new_mac`, updates its ARP cache, and replies. Standard protocol behavior — no filtering applies.

A GARP-style request (sender_ip = target_ip = pod_ip) was considered but rejected: OVS ARP responders are keyed on target_ip and may intercept and absorb such a frame before it reaches the physical gateway port. Asking for the gateway IP avoids this entirely.

The plugin is single-threaded, which makes `setns`-based netns entry safe. The pattern is already established in `tuning::set_min_port`.

## Goals / Non-Goals

**Goals:**
- Send one ARP request from the inner veth at the end of every new `setup` (not the idempotent path), **after** `ovs_add_port`, asking for the gateway MAC.
- Use `pnet_datalink` + `pnet_packet` sub-crates for L2 channel and frame construction; `nix::sched::setns` for netns entry.
- Add no new binary dependencies.
- Treat send failure as non-fatal: log a warning and return the `StatusBlock` normally.
- Provide a unit test for ARP frame byte layout.

**Non-Goals:**
- IPv6 Neighbor Advertisement.
- Repeated retransmission (configurable count deferred to a follow-up).
- ARP announcement on the idempotent path (no MAC change occurred).

## Decisions

### 1. Library: `pnet_datalink` + `pnet_packet` sub-crates

`pnet_datalink` opens an `AF_PACKET / SOCK_RAW` channel on a named interface; `pnet_packet` constructs the Ethernet and ARP layers. Using the sub-crates (not the umbrella `pnet`) keeps the dependency footprint minimal.

After `setns` into the container netns, `pnet_datalink::interfaces()` enumerates only that netns's interfaces and `pnet_datalink::channel()` opens the socket bound within that netns automatically. No additional nix features are required beyond `sched` (already present).

Alternatives considered:
- `arping` subprocess via `nsenter` — zero Rust deps but adds a binary dependency; inconsistent with the in-process setns approach.
- Manual `nix` raw socket — no new crates but requires hand-assembling a 42-byte frame; contradicts the preference for library reuse.

### 2. ARP frame: op=1 (request), target_ip = gateway_ip from config

```
Ethernet: dst=ff:ff:ff:ff:ff:ff  src=<inner_mac>  ethertype=0x0806
ARP:      htype=1  ptype=0x0800  hlen=6  plen=4  op=1
          sender_hw=<inner_mac>  sender_ip=<pod_ip>
          target_hw=00:00:00:00:00:00  target_ip=<gateway_ip>
```

Using the real gateway IP as `target_ip` ensures the request is processed correctly by every OVS configuration (with or without ARP responder), propagates through to the physical gateway when needed, and is never filtered by anti-spoofing rules because `sender_mac` matches the actual source interface.

### 3. New module `src/net_ns/arp.rs`

A dedicated `arp` module follows the convention of `tuning.rs` and keeps `plumbing.rs` focused on orchestration. Public API:

```rust
pub fn send_arp_request(
    netns_path: &str,
    iface: &str,
    sender_mac: [u8; 6],
    sender_ip: std::net::Ipv4Addr,
    target_ip: std::net::Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error>>
```

Internally: save host netns fd → `setns` into container netns → find interface by name via `pnet_datalink::interfaces()` → open channel → build and send frame → `setns` back (always, even on error).

A pure-function helper `build_arp_frame(sender_mac, sender_ip, target_ip) -> Vec<u8>` is extracted for unit-testability, mirroring the `build_ethtool_args` / `build_ovs_add_port_args` pattern.

### 4. Call site: after `ovs_add_port`, non-fatal, before `StatusBlock` return

The upstream veth must be connected to the OVS bridge before sending so that OVS can forward the frame. A failure here is logged as a warning — the `StatusBlock` is returned regardless.

Final `provision` sequence tail:
```
ovs_add_port → send_arp_request (non-fatal warning on error) → return StatusBlock
```

## Risks / Trade-offs

- **`CAP_NET_RAW` required for `AF_PACKET`** → rootful Podman always grants this. Rootless Podman with a user namespace may not; `pnet_datalink::channel()` returns an error logged as a warning. Connectivity is unaffected (ARP cache expires naturally).
- **OVS port-security drop** → an overly strict port-security ACL could still drop the frame. Non-fatal design handles it gracefully.
- **Single frame, no retransmission** → a lost frame leaves the stale cache until natural expiry. A configurable `arp_announce_count` option is a natural follow-up.

## Open Questions

- Should retransmission count be a plugin option (e.g. `arp_announce_count`, default 1)?
