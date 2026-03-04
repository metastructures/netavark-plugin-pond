use std::net::Ipv4Addr;

use nix::sched::CloneFlags;
use pnet_datalink::{MacAddr, NetworkInterface};
use pnet_packet::arp::{ArpHardwareTypes, ArpOperations, MutableArpPacket};
use pnet_packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet_packet::MutablePacket;

const ETHERNET_HEADER_LEN: usize = 14;
const ARP_PACKET_LEN: usize = 28;
const FRAME_LEN: usize = ETHERNET_HEADER_LEN + ARP_PACKET_LEN;

const BROADCAST: MacAddr = MacAddr(0xff, 0xff, 0xff, 0xff, 0xff, 0xff);
const ZERO_MAC: MacAddr = MacAddr(0x00, 0x00, 0x00, 0x00, 0x00, 0x00);

/// Build a 42-byte Ethernet+ARP request frame.
///
/// The frame asks "who has `target_ip`?" with `sender_mac`/`sender_ip` as the
/// requester. Sending this frame causes every L2-aware device that receives it
/// — including OVS and the upstream router — to record `sender_mac` as the
/// MAC for `sender_ip` in their ARP cache, immediately replacing any stale
/// entry left from a previous veth incarnation.
pub fn build_arp_frame(sender_mac: [u8; 6], sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> Vec<u8> {
    let mut buf = vec![0u8; FRAME_LEN];
    let mac = MacAddr(
        sender_mac[0],
        sender_mac[1],
        sender_mac[2],
        sender_mac[3],
        sender_mac[4],
        sender_mac[5],
    );

    // --- Ethernet header ---
    let mut eth = MutableEthernetPacket::new(&mut buf).expect("buffer sized correctly");
    eth.set_destination(BROADCAST);
    eth.set_source(mac);
    eth.set_ethertype(EtherTypes::Arp);

    // --- ARP payload ---
    let mut arp = MutableArpPacket::new(eth.payload_mut()).expect("payload sized correctly");
    arp.set_hardware_type(ArpHardwareTypes::Ethernet);
    arp.set_protocol_type(EtherTypes::Ipv4);
    arp.set_hw_addr_len(6);
    arp.set_proto_addr_len(4);
    arp.set_operation(ArpOperations::Request);
    arp.set_sender_hw_addr(mac);
    arp.set_sender_proto_addr(sender_ip);
    arp.set_target_hw_addr(ZERO_MAC);
    arp.set_target_proto_addr(target_ip);

    buf
}

/// Send an ARP request from `iface` inside the network namespace at
/// `netns_path`, asking for `target_ip`.
///
/// Uses the same setns-enter / work / restore pattern as
/// [`tuning::set_min_port`][super::tuning::set_min_port]: saves the host netns
/// fd, enters the container netns, opens an `AF_PACKET` channel via
/// `pnet_datalink`, sends the frame, then unconditionally restores the host
/// netns before returning.
///
/// Callers should treat errors as non-fatal warnings — connectivity is
/// unaffected if the frame cannot be sent (natural ARP expiry is the fallback).
///
/// # Errors
///
/// Returns an error if the netns files cannot be opened, `setns` fails,
/// the interface is not found inside the container netns, the `AF_PACKET`
/// channel cannot be created (e.g. `EPERM` when `CAP_NET_RAW` is absent), or
/// the send fails.
pub fn send_arp_request(
    netns_path: &str,
    iface: &str,
    sender_mac: [u8; 6],
    sender_ip: Ipv4Addr,
    target_ip: Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error>> {
    let host_ns =
        std::fs::File::open("/proc/self/ns/net").map_err(|e| format!("open host netns: {}", e))?;
    let container_ns = std::fs::File::open(netns_path)
        .map_err(|e| format!("open container netns {}: {}", netns_path, e))?;

    nix::sched::setns(&container_ns, CloneFlags::CLONE_NEWNET)
        .map_err(|e| format!("setns into container netns: {}", e))?;

    let result = send_in_netns(iface, sender_mac, sender_ip, target_ip);

    // Always restore host netns.
    let _ = nix::sched::setns(&host_ns, CloneFlags::CLONE_NEWNET);

    result
}

fn send_in_netns(
    iface: &str,
    sender_mac: [u8; 6],
    sender_ip: Ipv4Addr,
    target_ip: Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error>> {
    let interface = pnet_datalink::interfaces()
        .into_iter()
        .find(|i: &NetworkInterface| i.name == iface)
        .ok_or_else(|| format!("interface '{}' not found in container netns", iface))?;

    let (mut tx, _rx) = match pnet_datalink::channel(&interface, Default::default())? {
        pnet_datalink::Channel::Ethernet(tx, rx) => (tx, rx),
        _ => return Err("unexpected channel type from pnet_datalink".into()),
    };

    let frame = build_arp_frame(sender_mac, sender_ip, target_ip);
    tx.send_to(&frame, None)
        .ok_or("pnet_datalink send returned None")?
        .map_err(|e| format!("ARP send failed: {}", e).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Vec<u8> {
        build_arp_frame(
            [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
            "10.1.0.2".parse().unwrap(),
            "10.1.0.1".parse().unwrap(),
        )
    }

    #[test]
    fn arp_frame_length_is_42() {
        assert_eq!(frame().len(), 42);
    }

    #[test]
    fn ethernet_dst_is_broadcast() {
        let f = frame();
        assert_eq!(&f[0..6], &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn ethernet_src_is_sender_mac() {
        let f = frame();
        assert_eq!(&f[6..12], &[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    }

    #[test]
    fn ethertype_is_arp() {
        let f = frame();
        assert_eq!(&f[12..14], &[0x08, 0x06]);
    }

    #[test]
    fn arp_op_is_request() {
        let f = frame();
        // ARP op is at offset 20–21 (14 eth + 6 htype/ptype/hlen/plen).
        assert_eq!(&f[20..22], &[0x00, 0x01]);
    }

    #[test]
    fn arp_sender_mac_matches() {
        let f = frame();
        // Sender HW addr: offset 22–27.
        assert_eq!(&f[22..28], &[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    }

    #[test]
    fn arp_sender_ip_matches() {
        let f = frame();
        // Sender proto addr: offset 28–31.
        assert_eq!(&f[28..32], &[10, 1, 0, 2]);
    }

    #[test]
    fn arp_target_mac_is_zero() {
        let f = frame();
        // Target HW addr: offset 32–37.
        assert_eq!(&f[32..38], &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn arp_target_ip_matches_gateway() {
        let f = frame();
        // Target proto addr: offset 38–41.
        assert_eq!(&f[38..42], &[10, 1, 0, 1]);
    }
}
