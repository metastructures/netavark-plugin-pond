## 1. Dependencies and module structure

- [ ] 1.1 Add `nix` (with `sched` feature) and `crc32fast` to `Cargo.toml`
- [ ] 1.2 Create `src/net_ns/options.rs` with `PondOptions` struct and `parse_options(network: &Network) -> Result<PondOptions>` function
- [ ] 1.3 Create `src/net_ns/plumbing.rs` with empty `provision` and `deprovision` function signatures
- [ ] 1.4 Create `src/net_ns/tuning.rs` with empty `disable_offloads` and `set_min_port` function signatures
- [ ] 1.5 Update `src/net_ns/mod.rs` to declare and re-export all new modules

## 2. Options parsing and validation

- [ ] 2.1 Implement `PondOptions` struct fields: `bridge: String`, `vlan: u16`, `upstream: String`, `min_port: u16`, `mtu: u32`
- [ ] 2.2 Implement `parse_options`: extract and validate `bridge` (required), `vlan` (required, parse as u16), `upstream` (optional, derive via crc32fast if absent), `min_port` (default 1024), `mtu` (default 1500)
- [ ] 2.3 Implement upstream name derivation: `pond` + first 8 hex chars of `crc32fast::hash(name.as_bytes())`, assert total length ≤ 15 chars
- [ ] 2.4 Implement `create` hook in `driver.rs`: parse options, validate subnet present, return normalized `Network` with `internal: true`, `dns_enabled: false`, `ipv6_enabled: false`, `network_interface` set to derived upstream name

## 3. Veth pair creation (plumbing::provision — part 1)

- [ ] 3.1 In `provision(options, netns_path)`: open host and container netlink sockets via `open_netlink_sockets(netns_path)`
- [ ] 3.2 Idempotency check: call `host_nl.get_link(Name(upstream))`, if Ok return early with existing StatusBlock
- [ ] 3.3 Create veth pair on host: `host_nl.create_link(CreateLinkOptions { name: upstream, kind: InfoKind::Veth, ... })` — peer created as `<upstream>p`
- [ ] 3.4 Get peer link index: `host_nl.get_link(Name("<upstream>p"))`
- [ ] 3.5 Move peer into container netns: `host_nl.set_link_ns(peer.header.index, container_netns_fd)`

## 4. Inner interface configuration (plumbing::provision — part 2)

- [ ] 4.1 Rename peer inside container netns: `container_nl.set_link_name(peer.index, interface_name)`
- [ ] 4.2 Assign IP address: `container_nl.add_addr(inner.index, subnet_with_host_ip)`
- [ ] 4.3 Add default route: `container_nl.add_route(Route::Ipv4 { dest: 0.0.0.0/0, gw: gateway, metric: None })`
- [ ] 4.4 Bring inner interface up: `container_nl.set_up(LinkID::Name(interface_name))`
- [ ] 4.5 Read inner MAC address: `container_nl.get_link(Name(interface_name))` → extract `LinkAttribute::Address`

## 5. Host-side and OVS wiring (plumbing::provision — part 3)

- [ ] 5.1 Bring upstream interface up: `host_nl.set_up(LinkID::Name(upstream))`
- [ ] 5.2 Implement OVS add-port: `ovs-vsctl add-port <bridge> <upstream> -- set port <upstream> tag=<vlan> vlan_mode=access -- set interface <upstream> external_ids:network_id=<id> external_ids:network_name=<name> external_ids:driver=pond-netns`
- [ ] 5.3 Handle OVS command failure: propagate stderr as error, attempt veth cleanup before returning error

## 6. Tuning (tuning.rs)

- [ ] 6.1 Implement `disable_offloads(netns_path, iface)`: run `nsenter --net=<netns_path> ethtool --offload <iface> tx off sg off tso off` via `Command`, propagate non-zero exit as error
- [ ] 6.2 Implement `set_min_port(netns_path, port)`: open container netns fd, `nix::sched::setns` into it, write `port.to_string()` to `/proc/sys/net/ipv4/ip_unprivileged_port_start`, `setns` back to host netns (save host netns fd first via `/proc/self/ns/net`)
- [ ] 6.3 Call `tuning::set_min_port` and `tuning::disable_offloads` from `provision` after inner interface is up

## 7. StatusBlock and setup hook

- [ ] 7.1 Build `StatusBlock`: `interfaces: { interface_name => NetInterface { mac_address, subnets: [{ ipnet: subnet_with_host_ip, gateway }] } }`
- [ ] 7.2 Implement `setup` hook in `driver.rs`: parse options, call `plumbing::provision`, return StatusBlock
- [ ] 7.3 Add pod-only guard in `setup`: if `opts.network_options.static_ips` is None and no `port_mappings`, allow; document the pod-only intent in the error path for standalone containers

## 8. Teardown (plumbing::deprovision)

- [ ] 8.1 Implement `deprovision(options)`: open host netlink socket
- [ ] 8.2 OVS del-port: run `ovs-vsctl del-port <bridge> <upstream>`, treat exit code 1 with "no such port" as non-fatal
- [ ] 8.3 Delete upstream veth: `host_nl.del_link(Name(upstream))`, treat `ENODEV` as non-fatal (already gone)
- [ ] 8.4 Implement `teardown` hook in `driver.rs`: parse options, call `plumbing::deprovision`

## 9. Update examples and README

- [ ] 9.1 Update `examples/create.json` with realistic options: `bridge`, `vlan`, correct `network_interface` field
- [ ] 9.2 Update `examples/setup.json` and `examples/teardown.json` to match the updated network config
- [ ] 9.3 Update README.md: add usage section showing `podman network create --driver pond-netns --option bridge=ovsbr0 --option vlan=100 --subnet ...` and note which systemd ExecStartPre lines are replaced

## 10. Build verification

- [ ] 10.1 Run `cargo build` and resolve all compile errors
- [ ] 10.2 Run `cargo clippy` and resolve warnings
- [ ] 10.3 Manually test `create` subcommand: `echo '<create.json>' | ./target/debug/pond-netns create`
- [ ] 10.4 Manually test `info` subcommand: `./target/debug/pond-netns info`
