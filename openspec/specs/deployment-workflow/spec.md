## ADDED Requirements

### Requirement: Plugin deployment is documented for podman quadlets
The project SHALL provide a `DEPLOYMENT.md` that documents how to deploy a pod using the `pond-netns` driver via podman quadlet files and systemd. The document SHALL cover the three required quadlet file types (`.network`, `.pod`, `.container`), the systemd service dependency chain, and the correct field values needed for the plugin to function.

#### Scenario: Operator follows DEPLOYMENT.md to create a working network quadlet
- **WHEN** an operator creates a `.network` quadlet file with `Driver=pond-netns`, a `Subnet=`, a `Gateway=`, and `Options=bridge=<name>` and `Options=vlan=<id>`
- **THEN** `systemctl start <name>-network.service` succeeds and `podman network inspect <name>` shows the correct subnet, gateway, and `ipam_options: {driver: host-local}`

#### Scenario: Operator follows DEPLOYMENT.md to create a working pod quadlet
- **WHEN** an operator creates a `.pod` quadlet file with `Network=<name>.network` (optionally with `:ip=<addr>`) and the pod service has `Requires=` and `After=` on the network service
- **THEN** `systemctl start <name>-pod.service` starts the pod and the infra container reaches Running state

#### Scenario: Operator follows DEPLOYMENT.md to create a working container quadlet
- **WHEN** an operator creates a `.container` quadlet file with `Pod=<name>.pod` and no conflicting `Network=` directive
- **THEN** the container starts inside the pod and shares the pod's network namespace

---

### Requirement: Manual plugin invocation is documented
`DEPLOYMENT.md` SHALL document how to invoke the `pond-netns` binary directly with JSON payloads, independent of podman and systemd, for diagnostic and development purposes.

#### Scenario: Operator manually tests the create phase
- **WHEN** an operator pipes a valid `Network` JSON object to `pond-netns create`
- **THEN** the plugin prints a normalized `Network` JSON to stdout and exits 0

#### Scenario: Operator manually tests the setup phase
- **WHEN** an operator creates a network namespace with `ip netns add`, constructs a valid `NetworkPluginExec` JSON with a subnet and static IP, and pipes it to `pond-netns setup /run/netns/<name>`
- **THEN** the plugin configures the veth and prints a `StatusBlock` JSON to stdout

#### Scenario: Operator manually tests the teardown phase
- **WHEN** an operator pipes the same `NetworkPluginExec` JSON to `pond-netns teardown /run/netns/<name>`
- **THEN** the veth is deleted and the plugin exits 0

---

### Requirement: Operational troubleshooting is documented
`DEPLOYMENT.md` SHALL include a troubleshooting section covering the failure modes discovered during live testing.

#### Scenario: Operator recovers from missing Gateway= in network quadlet
- **WHEN** a `.network` quadlet omits `Gateway=` and pod start fails
- **THEN** the operator can identify the cause from the documented symptom and resolution (add `Gateway=` and recreate the network)

#### Scenario: Operator recovers from stale IPAM state
- **WHEN** a pod fails to start with an IPAM error after a previously incomplete teardown
- **THEN** the operator can follow the documented steps (`podman network rm` + restart network service) to clear the stale state and retry
