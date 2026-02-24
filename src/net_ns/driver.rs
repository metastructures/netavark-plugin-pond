use std::collections::HashMap;
use std::net::IpAddr;

use ipnet::IpNet;
use netavark::network::types;
use netavark::plugin::{Info, Plugin, PluginExec, API_VERSION};

use super::options::PondOptions;
use super::plumbing::{self, ProvisionParams};
use crate::constants::PLUGIN_VERSION;

#[derive(Default)]
pub struct NetNsDriver {}

impl Plugin for NetNsDriver {
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
