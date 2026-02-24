use std::collections::HashMap;
use std::net::IpAddr;

use ipnet::IpNet;
use netavark::network::types;
use netavark::plugin::{Info, Plugin, PluginExec, API_VERSION};

use super::options::PondOptions;
use super::plumbing::{self, ProvisionParams};
use crate::constants::PLUGIN_VERSION;

/// The netavark driver for the `pond-netns` network type.
///
/// Implements the three lifecycle hooks that netavark calls when managing a
/// network of driver type `pond-netns`:
///
/// * [`create`][Plugin::create] — validate options and persist a normalized
///   `Network` object in the netavark state store.
/// * [`setup`][Plugin::setup] — provision the veth pair, configure the
///   container netns, and register the upstream end with OVS.
/// * [`teardown`][Plugin::teardown] — remove the OVS port and delete the veth.
///
/// See the [crate-level documentation](crate) for a high-level overview.
#[derive(Default)]
pub struct NetNsDriver {}

impl Plugin for NetNsDriver {
    /// Validate the network definition and return a normalized `Network`.
    ///
    /// Called by `podman network create`. No kernel resources are created at
    /// this point — provisioning happens in [`setup`][Self::setup].
    ///
    /// # What this does
    ///
    /// * Parses and validates all driver options via private `PondOptions::from_network`.
    /// * Verifies that at least one subnet is present.
    /// * Returns a normalized `Network` with:
    ///   - `internal = true` (OVS-connected pods are not expected to reach the
    ///     host's default route through this interface)
    ///   - `dns_enabled = false`
    ///   - `ipv6_enabled = false`
    ///   - `ipam_options` set to `host-local` so netavark assigns IPs from the
    ///     configured subnet
    ///   - `network_interface` set to the upstream veth name so [`teardown`][Self::teardown]
    ///     can recover it without re-parsing options
    ///
    /// # Errors
    ///
    /// Returns an error if required options (`bridge`, `vlan`) are missing or
    /// invalid, if `upstream` exceeds 15 characters, or if no subnet is given.
    fn create(
        &self,
        network: types::Network,
    ) -> Result<types::Network, Box<dyn std::error::Error>> {
        // Validate required options and subnet.
        let options = PondOptions::from_network(&network)?;

        if network
            .subnets
            .as_ref()
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            return Err("at least one subnet must be specified (use --subnet)".into());
        }

        Ok(types::Network {
            driver: network.driver,
            id: network.id,
            name: network.name,
            // network_interface carries the upstream veth name so teardown
            // can recover it without re-parsing options.
            network_interface: Some(options.upstream),
            options: network.options,
            internal: true,
            ipv6_enabled: false,
            dns_enabled: false,
            ipam_options: Some(HashMap::from([(
                "driver".to_string(),
                "host-local".to_string(),
            )])),
            subnets: network.subnets,
            routes: Some(vec![]),
            network_dns_servers: Some(vec![]),
        })
    }

    /// Provision networking for the infra container of a pod.
    ///
    /// Called by netavark when the infra (pause) container is started. In pod
    /// mode all other containers share the infra container's network namespace
    /// and do not trigger additional `setup` calls.
    ///
    /// # What this does (in order)
    ///
    /// 1. Creates a veth pair atomically: upstream end in the host netns,
    ///    inner end placed directly into `netns` with the name from
    ///    `PerNetworkOptions.interface_name` (e.g. `eth0`).
    /// 2. Assigns the IP address from `static_ips[0]` with the subnet prefix
    ///    length to the inner interface.
    /// 3. Adds a default IPv4 route via the configured gateway.
    /// 4. Brings both ends up.
    /// 5. Sets `net.ipv4.ip_unprivileged_port_start` inside the container
    ///    netns using `setns` + procfs write.
    /// 6. Disables TX checksumming, scatter-gather, and TSO on the inner
    ///    interface via `nsenter` + `ethtool` (required for correct behaviour
    ///    with most user-space data planes).
    /// 7. Adds the upstream interface to the OVS bridge as an access port with
    ///    the configured VLAN tag and `external_ids` metadata.
    ///
    /// # Idempotency
    ///
    /// If the upstream interface already exists the function logs a warning and
    /// returns the existing state. This guards against duplicate calls but
    /// should not occur in normal pod operation.
    ///
    /// # Errors
    ///
    /// Returns an error if any netlink operation, sysctl write, ethtool call,
    /// or `ovs-vsctl add-port` fails. On OVS failure the veth pair is deleted
    /// before the error is propagated.
    fn setup(
        &self,
        netns: String,
        opts: types::NetworkPluginExec,
    ) -> Result<types::StatusBlock, Box<dyn std::error::Error>> {
        let options = PondOptions::from_network(&opts.network)?;

        // Extract IP configuration from the network and per-container options.
        let subnets = opts
            .network
            .subnets
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or("no subnets configured in network")?;
        let first_subnet = &subnets[0];

        let gateway = first_subnet
            .gateway
            .ok_or("no gateway configured in subnet")?;
        let prefix_len = first_subnet.subnet.prefix_len();

        let static_ips = opts
            .network_options
            .static_ips
            .as_ref()
            .filter(|ips| !ips.is_empty())
            .ok_or(
                "no static IPs assigned — pond-netns requires pod mode \
                 (podman pod create --network <name>)",
            )?;
        let host_ip: IpAddr = static_ips[0];

        let host_ipnet =
            IpNet::new(host_ip, prefix_len).map_err(|e| format!("invalid IP/prefix: {}", e))?;

        let interface_name = opts.network_options.interface_name.clone();

        let params = ProvisionParams {
            options: &options,
            netns_path: &netns,
            network_id: &opts.network.id,
            network_name: &opts.network.name,
            interface_name: &interface_name,
            host_ipnet,
            gateway,
        };

        plumbing::provision(&params)
    }

    /// Remove the OVS port and delete the veth pair.
    ///
    /// Called by netavark when the pod is stopped or the network is removed.
    /// Both operations are non-fatal if the resource is already absent, making
    /// teardown safe to retry.
    ///
    /// The container netns may no longer be reachable at this point; deleting
    /// the upstream veth automatically removes the inner end as well.
    ///
    /// # Errors
    ///
    /// Returns an error only if the `del_link` netlink call fails for a reason
    /// other than the interface already being gone.
    fn teardown(
        &self,
        _netns: String,
        opts: types::NetworkPluginExec,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let options = PondOptions::from_network(&opts.network)?;
        plumbing::deprovision(&options)
    }
}

impl From<NetNsDriver> for PluginExec<NetNsDriver> {
    fn from(value: NetNsDriver) -> Self {
        let info = Info::new(PLUGIN_VERSION.to_owned(), API_VERSION.to_owned(), None);
        PluginExec::new(value, info)
    }
}
