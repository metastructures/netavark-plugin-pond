## ADDED Requirements

### Requirement: Plugin validates and normalizes network configuration on create
The plugin SHALL validate that `bridge` and `vlan` options are present and non-empty on `create`. It SHALL validate that at least one subnet is provided. It SHALL derive the upstream interface name from `--option upstream=<name>` if present, otherwise from `crc32fast::hash(network_name)` truncated to 15 characters with a `pond` prefix. It SHALL return a normalized `Network` with `internal: true`, `dns_enabled: false`, and `ipv6_enabled: false`.

#### Scenario: Valid create with explicit upstream name
- **WHEN** `create` is called with options `bridge=ovsbr0`, `vlan=100`, `upstream=myapp0`, and a subnet
- **THEN** the plugin returns a `Network` with `network_interface: "myapp0"`, `internal: true`, `dns_enabled: false`

#### Scenario: Valid create with derived upstream name
- **WHEN** `create` is called with options `bridge=ovsbr0`, `vlan=100` and no `upstream` option, with network name `test-application`
- **THEN** the plugin returns a `Network` with a derived `network_interface` of the form `pond<8-hex-chars>` (max 15 chars total)

#### Scenario: Create fails when bridge option missing
- **WHEN** `create` is called without a `bridge` option
- **THEN** the plugin returns a JSON error `{"error": "..."}` and exits with code 1

#### Scenario: Create fails when vlan option missing
- **WHEN** `create` is called without a `vlan` option
- **THEN** the plugin returns a JSON error `{"error": "..."}` and exits with code 1

#### Scenario: Create fails when no subnet provided
- **WHEN** `create` is called with no subnets in the network config
- **THEN** the plugin returns a JSON error `{"error": "..."}` and exits with code 1

---

### Requirement: Plugin sets up network on infra container start
The plugin SHALL create and configure the veth pair, register the upstream end with OVS, configure the inner end inside the container netns, send an ARP request to the gateway to announce the new MAC-to-IP binding, and return a valid `StatusBlock` on `setup`. It SHALL be idempotent: if the upstream interface already exists, it SHALL return the existing `StatusBlock` without error and without sending an ARP request. The inner (container-side) interface SHALL be brought UP before the default route is installed; this ordering is required by the Linux kernel so that the connected route for the subnet is active when the gateway route is added.

#### Scenario: Successful setup
- **WHEN** `setup` is called with a valid netns path and a normalized `NetworkPluginExec`
- **THEN** the upstream veth exists in the host netns, connected to the OVS bridge
- **THEN** the inner veth exists in the container netns with the correct name, IP, and default route
- **THEN** an ARP request is sent from the inner interface to the gateway IP
- **THEN** the plugin returns a `StatusBlock` with `mac_address` and `subnets`

#### Scenario: Idempotent setup when upstream already exists
- **WHEN** `setup` is called and the upstream interface already exists in the host netns
- **THEN** the plugin returns the existing `StatusBlock` without creating a new veth pair, modifying OVS, or sending an ARP request

#### Scenario: Setup fails when ovs-vsctl is not available
- **WHEN** `setup` is called and `ovs-vsctl` is not found on PATH
- **THEN** the plugin returns a JSON error and exits with code 1, leaving no partial veth in the host netns

#### Scenario: Inner interface is UP before default route is installed
- **WHEN** `setup` is called with a valid subnet and gateway
- **THEN** the inner interface is in UP state at the time `add_route` executes
- **THEN** the default route via the configured gateway is installed without error

---

### Requirement: Plugin tears down network on pod stop
The plugin SHALL remove the upstream port from the OVS bridge and delete the veth pair on `teardown`. Deleting the upstream veth SHALL automatically remove the inner veth (kernel behavior). The plugin SHALL treat missing OVS port or missing veth as non-fatal (already cleaned up).

#### Scenario: Successful teardown
- **WHEN** `teardown` is called after a successful `setup`
- **THEN** the upstream veth no longer exists in the host netns
- **THEN** the OVS port is removed from the bridge

#### Scenario: Teardown is idempotent when already torn down
- **WHEN** `teardown` is called and the upstream veth does not exist
- **THEN** the plugin exits with code 0 without error
