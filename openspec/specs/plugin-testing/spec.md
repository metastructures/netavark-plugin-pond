### Requirement: Options parsing is fully covered by unit tests
The plugin SHALL have unit tests for every validation branch in
`PondOptions::from_network`, including missing required fields, out-of-range values,
over-length upstream names, and default-value application.

#### Scenario: Missing bridge option
- **WHEN** `from_network` is called with no `bridge` key in the options map
- **THEN** it SHALL return an error containing "missing required option: bridge"

#### Scenario: Missing vlan option
- **WHEN** `from_network` is called with no `vlan` key in the options map
- **THEN** it SHALL return an error containing "missing required option: vlan"

#### Scenario: Out-of-range vlan (zero)
- **WHEN** `from_network` is called with `vlan=0`
- **THEN** it SHALL return an error indicating vlan is out of range

#### Scenario: Out-of-range vlan (too high)
- **WHEN** `from_network` is called with `vlan=4095`
- **THEN** it SHALL return an error indicating vlan is out of range

#### Scenario: Non-numeric vlan
- **WHEN** `from_network` is called with `vlan=abc`
- **THEN** it SHALL return an error indicating the value is not a valid integer

#### Scenario: Upstream name too long
- **WHEN** `from_network` is called with an `upstream` value longer than 15 characters
- **THEN** it SHALL return an error referencing the IFNAMSIZ limit

#### Scenario: Defaults applied
- **WHEN** `from_network` is called with only `bridge` and `vlan` set
- **THEN** `min_port` SHALL be 1024, `mtu` SHALL be 1500, and `upstream` SHALL be
  derived from the network name

#### Scenario: All options explicit
- **WHEN** `from_network` is called with all options provided and valid
- **THEN** all fields SHALL reflect the provided values exactly

### Requirement: Upstream name derivation is deterministic and length-safe
`derive_upstream_name` SHALL produce the same output for the same input and SHALL
always produce a string of exactly 12 characters.

#### Scenario: Determinism
- **WHEN** `derive_upstream_name` is called twice with the same network name
- **THEN** both calls SHALL return identical strings

#### Scenario: Length invariant
- **WHEN** `derive_upstream_name` is called with any network name
- **THEN** the result SHALL be exactly 12 characters long

#### Scenario: Prefix
- **WHEN** `derive_upstream_name` is called with any network name
- **THEN** the result SHALL start with "pond"

### Requirement: MAC address extraction is unit-tested
`extract_mac` SHALL be tested with a synthetic `LinkMessage` containing a known
`LinkAttribute::Address` byte sequence.

#### Scenario: Address attribute present
- **WHEN** `extract_mac` is called with a `LinkMessage` whose attributes include
  a 6-byte `LinkAttribute::Address`
- **THEN** it SHALL return the hex-encoded MAC string

#### Scenario: Address attribute absent
- **WHEN** `extract_mac` is called with a `LinkMessage` with no `LinkAttribute::Address`
- **THEN** it SHALL return an empty string

### Requirement: OVS argument construction is unit-tested without spawning processes
The private arg-builder for `ovs_add_port` SHALL be tested to verify it produces
the correct `ovs-vsctl` argument list, including bridge, port, VLAN tag, vlan_mode,
and all `external_ids` metadata fields.

#### Scenario: add-port arguments
- **WHEN** the OVS add-port arg-builder is called with known parameters
- **THEN** the returned argument list SHALL contain `add-port`, the bridge name,
  the upstream name, `tag=<vlan>`, `vlan_mode=access`, and all three `external_ids`
  entries (`network_id`, `network_name`, `driver=pond-netns`)

### Requirement: ethtool argument construction is unit-tested without spawning processes
The private arg-builder for `disable_offloads` SHALL be tested to verify it produces
the correct `nsenter` + `ethtool` argument list.

#### Scenario: ethtool offload arguments
- **WHEN** the ethtool arg-builder is called with a netns path and interface name
- **THEN** the returned argument list SHALL contain `--net=<path>`, `ethtool`,
  `--offload`, the interface name, and `tx off sg off tso off`

### Requirement: sysctl value formatting is unit-tested
The private value-formatter for `set_min_port` SHALL be tested to ensure it
produces the correct string written to procfs.

#### Scenario: Port value format
- **WHEN** the port formatter is called with port value 1024
- **THEN** it SHALL return the string `"1024"`

#### Scenario: Zero port value
- **WHEN** the port formatter is called with port value 0
- **THEN** it SHALL return the string `"0"`

### Requirement: Extracted helpers carry testability doc-comments
Every private function extracted solely to enable unit testing SHALL carry a
`// Extracted for unit-testability:` comment, and its caller SHALL carry a
reference comment pointing to the helper.

#### Scenario: Helper comment present
- **WHEN** a developer reads a extracted arg-builder function
- **THEN** a `// Extracted for unit-testability:` comment SHALL appear directly
  above the `fn` signature explaining why the split exists

#### Scenario: Caller comment present
- **WHEN** a developer reads a function that delegates to an extracted helper
- **THEN** a `// See <helper_name> for testable argument construction.` comment
  SHALL appear in the function body near the delegation call
