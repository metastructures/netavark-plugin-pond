## ADDED Requirements

### Requirement: Inner veth is correctly named and assigned in the container netns
On `setup`, the plugin SHALL move the inner veth peer into the container netns, rename it to `opts.network_options.interface_name` (e.g. `eth0`), assign the IP address from the first subnet in the network config, add a default route via the subnet gateway, and bring the interface up. These operations SHALL use netlink via `netavark::network::netlink::Socket`.

#### Scenario: Inner interface configured with correct name and IP
- **WHEN** `setup` is called with `interface_name: "eth0"` and subnet `10.1.0.2/29` with gateway `10.1.0.1`
- **THEN** a network interface named `eth0` exists in the container netns with address `10.1.0.2/29`
- **THEN** a default route via `10.1.0.1` exists in the container netns

#### Scenario: Inner interface is up after setup
- **WHEN** `setup` completes successfully
- **THEN** the inner interface is in the UP state inside the container netns

---

### Requirement: Unprivileged port minimum is configured inside the container netns
On `setup`, the plugin SHALL set `net.ipv4.ip_unprivileged_port_start` inside the container netns to the value of the `min_port` option (default: `1024`). It SHALL do this by entering the container netns using `nix::sched::setns`, writing to `/proc/sys/net/ipv4/ip_unprivileged_port_start`, and returning to the host netns.

#### Scenario: Default min_port applied
- **WHEN** `setup` is called without a `min_port` option
- **THEN** `net.ipv4.ip_unprivileged_port_start` inside the container netns is set to `1024`

#### Scenario: Custom min_port applied
- **WHEN** `setup` is called with `min_port=80`
- **THEN** `net.ipv4.ip_unprivileged_port_start` inside the container netns is set to `80`

---

### Requirement: TX offloads are disabled on the inner interface
On `setup`, the plugin SHALL disable `tx`, `sg`, and `tso` offloads on the inner interface inside the container netns using `nsenter --net=<netns_path> ethtool --offload <iface> tx off sg off tso off`. This is required for correct behavior with OVS-DPDK datapaths that do not support software offloads on the veth.

#### Scenario: Offloads disabled after setup
- **WHEN** `setup` completes successfully
- **THEN** `ethtool -k <inner>` inside the container netns shows `tx-checksumming`, `scatter-gather`, and `tcp-segmentation-offload` as off (or not supported/unchangeable, which is also acceptable)

#### Scenario: Setup fails if ethtool binary is not available
- **WHEN** `setup` is called and `ethtool` is not found on PATH
- **THEN** the plugin returns a JSON error and exits with code 1

---

### Requirement: StatusBlock reflects the configured interface
On `setup`, the plugin SHALL return a `StatusBlock` with the `interfaces` map keyed by `interface_name`. The `mac_address` field SHALL be the MAC of the inner veth after it has been moved into the container netns. The `subnets` list SHALL contain the assigned `ipnet` and `gateway`.

#### Scenario: StatusBlock has correct interface entry
- **WHEN** `setup` completes with `interface_name: "eth0"`, IP `10.1.0.2/29`, gateway `10.1.0.1`
- **THEN** the returned JSON contains `{"interfaces": {"eth0": {"mac_address": "<mac>", "subnets": [{"ipnet": "10.1.0.2/29", "gateway": "10.1.0.1"}]}}}`
