use std::collections::HashMap;
use std::net::IpAddr;
use std::os::fd::{AsFd, AsRawFd};
use std::process::Command;

use ipnet::IpNet;
use netavark::network::{
    core_utils::{open_netlink_sockets, CoreUtils},
    netlink_route, types,
};
use netlink_packet_route::link::{InfoData, InfoKind, InfoVeth, LinkAttribute, LinkMessage};

use super::options::PondOptions;
use super::tuning;

/// All parameters needed to fully provision one pod network.
pub struct ProvisionParams<'a> {
    pub options: &'a PondOptions,
    /// Path to the infra container's network namespace (e.g. /proc/<pid>/ns/net).
    pub netns_path: &'a str,
    pub network_id: &'a str,
    pub network_name: &'a str,
    /// Interface name inside the container (from PerNetworkOptions.interface_name).
    pub interface_name: &'a str,
    /// Host IP with prefix length, e.g. 10.1.0.2/29.
    pub host_ipnet: IpNet,
    /// Default gateway for the container.
    pub gateway: IpAddr,
}

/// Create the veth pair, configure the container netns, and register the
/// upstream end with OVS. Returns a StatusBlock for netavark.
pub fn provision(
    params: &ProvisionParams,
) -> Result<types::StatusBlock, Box<dyn std::error::Error>> {
    let (mut host, mut container) = open_netlink_sockets(params.netns_path)?;

    // Idempotency: if upstream already exists return existing state.
    if host
        .netlink
        .get_link(netlink_route::LinkID::Name(params.options.upstream.clone()))
        .is_ok()
    {
        eprintln!(
            "pond-netns: {} already exists — returning existing state",
            params.options.upstream
        );
        return existing_status_block(&mut container, params);
    }

    // --- Create veth pair ---
    // The peer (container side) is created directly inside the container netns.
    let container_fd = container.file.as_fd().as_raw_fd();

    let mut peer_msg = LinkMessage::default();
    peer_msg
        .attributes
        .push(LinkAttribute::IfName(params.interface_name.to_string()));
    peer_msg
        .attributes
        .push(LinkAttribute::Mtu(params.options.mtu));
    peer_msg
        .attributes
        .push(LinkAttribute::NetNsFd(container_fd));

    let mut host_opts =
        netlink_route::CreateLinkOptions::new(params.options.upstream.clone(), InfoKind::Veth);
    host_opts.mtu = params.options.mtu;
    host_opts.info_data = Some(InfoData::Veth(InfoVeth::Peer(peer_msg)));

    host.netlink
        .create_link(host_opts)
        .map_err(|e| format!("create veth pair '{}': {}", params.options.upstream, e))?;

    // --- Configure container-side interface ---
    let inner = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("get inner veth '{}': {}", params.interface_name, e))?;

    container
        .netlink
        .add_addr(inner.header.index, &params.host_ipnet)
        .map_err(|e| {
            format!(
                "add address {} to {}: {}",
                params.host_ipnet, params.interface_name, e
            )
        })?;

    // Default route — IPv4 only (IPv6 is a non-goal).
    match (params.host_ipnet, params.gateway) {
        (IpNet::V4(_), IpAddr::V4(gw)) => {
            let dest: ipnet::Ipv4Net = "0.0.0.0/0".parse().unwrap();
            container
                .netlink
                .add_route(&netlink_route::Route::Ipv4 {
                    dest,
                    gw,
                    metric: None,
                })
                .map_err(|e| format!("add default route via {}: {}", gw, e))?;
        }
        _ => return Err("IPv6 is not supported by this driver".into()),
    }

    container
        .netlink
        .set_up(netlink_route::LinkID::ID(inner.header.index))
        .map_err(|e| format!("set {} up: {}", params.interface_name, e))?;

    // --- Read inner MAC after it's up ---
    let inner_up = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("re-read inner veth: {}", e))?;
    let mac_address = extract_mac(&inner_up);

    // --- Bring upstream up ---
    host.netlink
        .set_up(netlink_route::LinkID::Name(params.options.upstream.clone()))
        .map_err(|e| format!("set {} up: {}", params.options.upstream, e))?;

    // --- Tuning (sysctl + ethtool) ---
    // Done before OVS so that if OVS fails we only need to clean up the veth.
    tuning::set_min_port(params.netns_path, params.options.min_port)
        .map_err(|e| format!("sysctl ip_unprivileged_port_start: {}", e))?;

    tuning::disable_offloads(params.netns_path, params.interface_name)
        .map_err(|e| format!("ethtool offload disable: {}", e))?;

    // --- OVS: add-port with VLAN and metadata ---
    // On failure, clean up the veth before returning the error.
    if let Err(e) = ovs_add_port(params) {
        let _ = host
            .netlink
            .del_link(netlink_route::LinkID::Name(params.options.upstream.clone()));
        return Err(e);
    }

    // --- Build StatusBlock ---
    let net_addr = types::NetAddress {
        gateway: Some(params.gateway),
        ipnet: params.host_ipnet,
    };
    let interface = types::NetInterface {
        mac_address,
        subnets: Some(vec![net_addr]),
    };
    let mut interfaces = HashMap::new();
    interfaces.insert(params.interface_name.to_string(), interface);

    Ok(types::StatusBlock {
        dns_server_ips: None,
        dns_search_domains: None,
        interfaces: Some(interfaces),
    })
}

/// Remove the upstream port from OVS and delete the veth pair.
/// Both operations are non-fatal if the resource is already gone.
pub fn deprovision(options: &PondOptions) -> Result<(), Box<dyn std::error::Error>> {
    // OVS del-port (non-fatal if port already absent).
    let ovs_out = Command::new("ovs-vsctl")
        .args(["del-port", &options.bridge, &options.upstream])
        .output();
    match ovs_out {
        Ok(out) if !out.status.success() => {
            eprintln!(
                "pond-netns: ovs-vsctl del-port warning (may already be gone): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            eprintln!("pond-netns: ovs-vsctl unavailable during teardown: {}", e);
        }
        _ => {}
    }

    // Open a host-side netlink socket by passing the host netns as both sides.
    // We only use the host socket; the container netns may no longer be reachable.
    let (mut host, _) = open_netlink_sockets("/proc/self/ns/net")?;

    match host
        .netlink
        .del_link(netlink_route::LinkID::Name(options.upstream.clone()))
    {
        Ok(_) => {}
        Err(e) => {
            let msg = format!("{}", e);
            if !msg.contains("No such device") && !msg.contains("ENODEV") {
                return Err(format!("del veth '{}': {}", options.upstream, msg).into());
            }
            // Interface already gone — idempotent teardown.
        }
    }

    Ok(())
}

// --- helpers ---

fn ovs_add_port(params: &ProvisionParams) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("ovs-vsctl")
        .args([
            "add-port",
            &params.options.bridge,
            &params.options.upstream,
            "--",
            "set",
            "port",
            &params.options.upstream,
            &format!("tag={}", params.options.vlan),
            "vlan_mode=access",
            "--",
            "set",
            "interface",
            &params.options.upstream,
            &format!("external_ids:network_id={}", params.network_id),
            &format!("external_ids:network_name={}", params.network_name),
            "external_ids:driver=pond-netns",
        ])
        .output()
        .map_err(|e| format!("ovs-vsctl not found: {}", e))?;

    if !out.status.success() {
        return Err(format!(
            "ovs-vsctl add-port failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(())
}

fn extract_mac(link: &netlink_packet_route::link::LinkMessage) -> String {
    for attr in &link.attributes {
        if let LinkAttribute::Address(addr) = attr {
            return CoreUtils::encode_address_to_hex(addr);
        }
    }
    String::new()
}

/// Build a StatusBlock from the existing (already provisioned) state.
fn existing_status_block(
    container: &mut netavark::network::core_utils::NamespaceOptions,
    params: &ProvisionParams,
) -> Result<types::StatusBlock, Box<dyn std::error::Error>> {
    let inner = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("get existing inner veth '{}': {}", params.interface_name, e))?;
    let mac_address = extract_mac(&inner);

    let net_addr = types::NetAddress {
        gateway: Some(params.gateway),
        ipnet: params.host_ipnet,
    };
    let interface = types::NetInterface {
        mac_address,
        subnets: Some(vec![net_addr]),
    };
    let mut interfaces = HashMap::new();
    interfaces.insert(params.interface_name.to_string(), interface);

    Ok(types::StatusBlock {
        dns_server_ips: None,
        dns_search_domains: None,
        interfaces: Some(interfaces),
    })
}
