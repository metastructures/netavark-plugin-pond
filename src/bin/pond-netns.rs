//! # pond-netns
//!
//! A [netavark](https://github.com/containers/netavark) network plugin that wires
//! a Podman pod's infra-container network namespace to a pre-existing
//! [Open vSwitch](https://www.openvswitch.org/) bridge using a kernel veth pair.
//!
//! ## Network model
//!
//! * **One veth pair per network.** The upstream (host-side) end is added to the OVS
//!   bridge as an access port with a configured VLAN tag. The inner (container-side)
//!   end lives in the pod's shared infra-container network namespace.
//! * **Pod-only.** `setup` is called exactly once — for the infra container. All other
//!   containers in the pod inherit its network namespace through the OCI runtime and
//!   do not trigger additional netavark calls.
//! * **IPv4 only.** IPv6 is explicitly out of scope.
//! * **OVS bridge is pre-existing.** The plugin does not create or manage OVS bridges.
//!
//! ## Runtime dependencies
//!
//! The following host binaries must be present at pod-start time:
//!
//! | Binary | Purpose |
//! |--------|---------|
//! | `ovs-vsctl` | Add/remove the upstream veth from the OVS bridge |
//! | `nsenter` | Enter the container netns for ethtool invocation |
//! | `ethtool` | Disable TX/SG/TSO offloads on the inner interface |
//!
//! ## Quick start
//!
//! ```bash
//! podman network create \
//!   --driver pond-netns \
//!   --subnet 10.1.0.0/29 \
//!   --gateway 10.1.0.1 \
//!   --option bridge=ovsbr0 \
//!   --option vlan=100 \
//!   my-pod-network
//!
//! podman pod create --network my-pod-network mypod
//! ```

use netavark::plugin::PluginExec;
use netavark_plugin_pond::NetNsDriver;

pub fn main() {
    let plugin: PluginExec<NetNsDriver> = NetNsDriver::default().into();
    plugin.exec();
}
