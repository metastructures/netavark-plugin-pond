## Why

Each time a pod restarts, `setup` destroys and recreates the veth pair, which causes the kernel to assign a **new MAC address** to the inner interface while the **IP address remains the same**. Any device on the same L2 broadcast domain — routers, switches, peer containers — holds a stale ARP cache entry mapping that IP to the previous MAC. Until the cached entry expires (typically 5–30 minutes), traffic destined for the pod is delivered to a non-existent MAC, causing silent packet loss. Sending an ARP frame at the end of `setup` announces the new MAC-to-IP binding immediately, eliminating this window of unreachability.

## What Changes

- After the inner veth is configured and UP, the plugin sends an ARP frame from inside the container netns to notify upstream devices of the new MAC-to-IP binding.
- Two approaches are candidates; the choice affects blast radius and filtering risk (see below).
- Failure to send is treated as **non-fatal**: the plugin logs a warning and continues — connectivity will self-heal once the upstream ARP cache expires.

### Approach A — Gratuitous ARP broadcast (GARP)

An unsolicited ARP reply/request sent to the Ethernet broadcast address (`ff:ff:ff:ff:ff:ff`), with sender IP = target IP = the pod's own IP. All ARP-capable devices in the broadcast domain receive and update their caches.

- **Pros**: updates every device at once; standard mechanism used by bonding, macvlan, live migration.
- **Cons**: OVS port-security rules or anti-ARP-spoofing ACLs may drop unsolicited ARP frames whose source MAC did not exist before. Some managed switches silently discard them.

### Approach B — Unicast ARP request to the gateway

A standard ARP request (`who has <gateway_ip>?`) sent as an Ethernet broadcast, but targeting the configured gateway IP rather than announcing self. As a side-effect of any ARP request, the sender's MAC is recorded in the ARP cache of every device that processes the request — including the gateway. OVS also learns the new MAC from the outgoing frame.

- **Pros**: indistinguishable from a normal first-contact ARP; unlikely to be filtered since it is a legitimate request; gateway updates its cache as a standard protocol side-effect.
- **Cons**: only directly updates the gateway's cache; other peers on the segment learn the new MAC only when they see subsequent traffic from the pod (which is typically fine since the gateway is the critical hop for external traffic).

**Recommended: Approach B.** The gateway is the only ARP cache that matters for external reachability, and a regular ARP request to it is the least invasive, most compatible mechanism. GARP broadcast should be a follow-up option if intra-segment peer-to-pod traffic is also a concern.

## Capabilities

### New Capabilities
- `arp-announce`: Send an ARP frame from the inner veth interface at the end of `setup` to notify the gateway of the new MAC-to-IP binding.

### Modified Capabilities
- `network-lifecycle`: The `setup` phase gains an additional step (ARP announcement) at the end of the inner-interface configuration sequence.

## Impact

- **Code**: `src/net_ns/plumbing.rs` — `provision` function; new `src/net_ns/arp.rs` module with frame construction and raw socket logic.
- **Dependencies**: Requires raw socket access (`AF_PACKET / SOCK_RAW`) inside the container netns. The existing `nix` crate gains the `net` feature to provide `SockaddrLl`; no new crates needed.
- **Permissions**: Sending a raw Ethernet frame requires `CAP_NET_RAW` inside the netns. Rootful Podman always grants this; rootless Podman may not — handled gracefully as a non-fatal warning.
- **Testing**: Unit test for ARP frame byte layout; integration/manual test verifying that the gateway's ARP cache reflects the new MAC after `setup`.
