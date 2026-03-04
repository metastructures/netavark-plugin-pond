## ADDED Requirements

### Requirement: Plugin sends an ARP request to the gateway after setup
On `setup`, after the upstream veth has been registered with the OVS bridge, the plugin SHALL send a broadcast ARP request from the inner veth interface inside the container netns. The request SHALL use the inner interface's MAC as `sender_hw`, the assigned pod IP as `sender_ip`, and the configured subnet gateway as `target_ip`. This causes all devices on the L2 segment — including OVS and the gateway — to associate the new MAC with the pod IP immediately, without waiting for natural ARP cache expiry.

#### Scenario: ARP request is sent after successful OVS registration
- **WHEN** `setup` completes `ovs_add_port` successfully
- **THEN** the plugin sends a broadcast ARP request frame from the inner interface with `sender_hw=<inner_mac>`, `sender_ip=<pod_ip>`, `target_ip=<gateway_ip>` before returning the `StatusBlock`

#### Scenario: ARP send failure is non-fatal
- **WHEN** the ARP request cannot be sent (e.g. `CAP_NET_RAW` is unavailable in the netns)
- **THEN** the plugin logs a warning and returns the `StatusBlock` successfully without error

#### Scenario: ARP is not sent on idempotent setup path
- **WHEN** `setup` is called and the upstream interface already exists (idempotent path)
- **THEN** no ARP request is sent
