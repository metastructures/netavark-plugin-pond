## ADDED Requirements

### Requirement: Upstream veth is registered as a VLAN-tagged access port on the OVS bridge
On `setup`, the plugin SHALL add the upstream veth to the pre-existing OVS bridge using `ovs-vsctl add-port`. The port SHALL be configured as `vlan_mode=access` with the `tag` set to the `vlan` option value. The plugin SHALL NOT create the OVS bridge itself.

#### Scenario: Port added with correct VLAN tag
- **WHEN** `setup` is called with `bridge=ovsbr0` and `vlan=100`
- **THEN** `ovs-vsctl` adds the upstream interface to `ovsbr0` with `tag=100` and `vlan_mode=access`

#### Scenario: Setup fails when bridge does not exist
- **WHEN** `setup` is called with a `bridge` value that does not exist in OVS
- **THEN** the plugin returns a JSON error propagating the `ovs-vsctl` failure and exits with code 1

---

### Requirement: OVS port carries metadata for troubleshooting
On `setup`, the plugin SHALL set `external_ids` on the OVS interface record to include `network_id`, `network_name`, and `driver` fields. These fields SHALL be set atomically with the `add-port` call using a single `ovs-vsctl` invocation.

#### Scenario: Metadata present after setup
- **WHEN** `setup` completes successfully
- **THEN** `ovs-vsctl get interface <upstream> external_ids` returns a map containing `network_id`, `network_name`, and `driver=pond-netns`

---

### Requirement: Upstream port is removed from OVS on teardown
On `teardown`, the plugin SHALL remove the upstream interface from the OVS bridge using `ovs-vsctl del-port`. If the port does not exist in OVS (already removed or never added), the plugin SHALL treat this as non-fatal and continue to veth deletion.

#### Scenario: Port removed on teardown
- **WHEN** `teardown` is called after a successful `setup`
- **THEN** the upstream interface is no longer listed as a port on the OVS bridge

#### Scenario: Teardown continues when OVS port already absent
- **WHEN** `teardown` is called and the upstream interface is not an OVS port
- **THEN** the plugin continues and attempts veth deletion without returning an error for the missing OVS port
