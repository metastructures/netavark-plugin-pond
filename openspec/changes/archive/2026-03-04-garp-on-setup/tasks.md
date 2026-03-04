## 1. Dependencies

- [x] 1.1 Add `pnet_datalink` and `pnet_packet` to `Cargo.toml` dependencies

## 2. ARP Module

- [x] 2.1 Create `src/net_ns/arp.rs` with `build_arp_frame(sender_mac, sender_ip, target_ip) -> Vec<u8>` returning a correctly laid-out 42-byte Ethernet+ARP frame
- [x] 2.2 Implement `send_arp_request(netns_path, iface, sender_mac, sender_ip, target_ip)` using the `setns`-enter/work/restore pattern from `tuning::set_min_port`, finding the interface via `pnet_datalink::interfaces()` and sending via `pnet_datalink::channel()`
- [x] 2.3 Register `mod arp` in `src/net_ns/mod.rs`

## 3. Integration in Provision

- [x] 3.1 Call `arp::send_arp_request` in `plumbing::provision` after `ovs_add_port` succeeds, passing `netns_path`, `interface_name`, `inner_mac`, `host_ipnet` IPv4 address, and `gateway` IPv4 address
- [x] 3.2 Treat the call as non-fatal: log a warning with `eprintln!` on error and continue to `StatusBlock` return

## 4. Tests

- [x] 4.1 Unit test `build_arp_frame`: assert Ethernet dst is broadcast, ethertype is `0x0806`, ARP op is `0x0001`, sender MAC/IP and target IP bytes are at correct offsets
- [x] 4.2 Unit test `build_arp_frame`: assert target MAC bytes are all zero
