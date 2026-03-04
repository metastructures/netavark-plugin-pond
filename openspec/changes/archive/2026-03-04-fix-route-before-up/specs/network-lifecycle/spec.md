## MODIFIED Requirements

### Requirement: Plugin sets up network on infra container start
The plugin SHALL create and configure the veth pair, register the upstream end with OVS, configure the inner end inside the container netns, and return a valid `StatusBlock` on `setup`. It SHALL be idempotent: if the upstream interface already exists, it SHALL return the existing `StatusBlock` without error. The inner (container-side) interface SHALL be brought UP before the default route is installed; this ordering is required by the Linux kernel so that the connected route for the subnet is active when the gateway route is added.

#### Scenario: Successful setup
- **WHEN** `setup` is called with a valid netns path and a normalized `NetworkPluginExec`
- **THEN** the upstream veth exists in the host netns, connected to the OVS bridge
- **THEN** the inner veth exists in the container netns with the correct name, IP, and default route
- **THEN** the plugin returns a `StatusBlock` with `mac_address` and `subnets`

#### Scenario: Idempotent setup when upstream already exists
- **WHEN** `setup` is called and the upstream interface already exists in the host netns
- **THEN** the plugin returns the existing `StatusBlock` without creating a new veth pair or modifying OVS

#### Scenario: Setup fails when ovs-vsctl is not available
- **WHEN** `setup` is called and `ovs-vsctl` is not found on PATH
- **THEN** the plugin returns a JSON error and exits with code 1, leaving no partial veth in the host netns

#### Scenario: Inner interface is UP before default route is installed
- **WHEN** `setup` is called with a valid subnet and gateway
- **THEN** the inner interface is in UP state at the time `add_route` executes
- **THEN** the default route via the configured gateway is installed without error
