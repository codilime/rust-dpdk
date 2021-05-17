extern crate etherparse;
extern crate pnet;
extern crate pnet_datalink;
extern crate smoltcp;

use std::net::Ipv4Addr;
use std::io::Cursor;

use pnet::packet::ethernet::MutableEthernetPacket;
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet_datalink::MacAddr;

use smoltcp::wire::EthernetAddress;
use smoltcp::wire::EthernetFrame;
use smoltcp::wire::Ipv4Packet;
use smoltcp::wire::Ipv4Address;

use etherparse::Ethernet2Header;
use etherparse::InternetSlice;
use etherparse::Ipv4Header;
use etherparse::PacketBuilder;
use etherparse::SlicedPacket;
use etherparse::TransportSlice;

pub fn nat_pnet(packet: &mut [u8]) {
    let mut ethernet_packet = MutableEthernetPacket::new(packet).unwrap();
    ethernet_packet.set_destination(MacAddr::new(100, 101, 102, 103, 104, 105));
    ethernet_packet.set_source(MacAddr::new(200, 201, 202, 203, 204, 205));
    let mut ip4_packet = MutableIpv4Packet::new(&mut packet[14..]).unwrap();
    ip4_packet.set_destination(Ipv4Addr::new(10, 0, 0, 1));
}

pub fn nat_smoltcp(packet: &mut [u8]) {
    let mut ethernet_packet = EthernetFrame::new_checked(packet).unwrap();
    ethernet_packet.set_dst_addr(EthernetAddress::from_bytes(&[100, 101, 102, 103, 104, 105]));
    ethernet_packet.set_src_addr(EthernetAddress::from_bytes(&[200, 201, 202, 203, 204, 205]));
    let mut ip4_packet = Ipv4Packet::new_checked(ethernet_packet.payload_mut()).unwrap();
    ip4_packet.set_dst_addr(Ipv4Address::new(10, 0, 0, 1));
}

pub fn nat_etherparse_fast_cursor(packet: &mut [u8]) {
    let mut read_cursor = Cursor::new(&packet);
    let mut header = Ethernet2Header::read(&mut read_cursor).unwrap();
    let mut ipv4header = Ipv4Header::read(&mut read_cursor).unwrap();
    header.destination = [100, 101, 102, 103, 104, 105];
    header.source = [200, 201, 202, 203, 204, 205];
    ipv4header.destination = [10, 0, 0, 1];
    let mut write_cursor = Cursor::new(packet);
    header.write(&mut write_cursor).unwrap();
    ipv4header.write_raw(&mut write_cursor).unwrap();
}

pub fn nat_etherparse_fast_slice(packet: &mut [u8]) {
    let (mut header, _) = Ethernet2Header::read_from_slice(packet).unwrap();
    header.destination = [100, 101, 102, 103, 104, 105];
    header.source = [200, 201, 202, 203, 204, 205];
    let mut ipv4_slice = header.write_to_slice(packet).unwrap();
    let (mut ipv4_header, _) = Ipv4Header::read_from_slice(ipv4_slice).unwrap();
    ipv4_header.destination = [10, 0, 0, 1];
    ipv4_header.write_raw(&mut ipv4_slice).unwrap();
}

pub fn nat_etherparse(packet: &mut [u8]) {
    let eth_src: [u8; 6] = [100, 101, 102, 103, 104, 105];
    let eth_dst: [u8; 6] = [200, 201, 202, 203, 204, 205];
    let sliced_packet = SlicedPacket::from_ethernet(packet).unwrap();
    let ip = match sliced_packet.ip.unwrap() {
        InternetSlice::Ipv4(ip4) => ip4,
        _ => {
            panic!()
        }
    };
    let mut ip_src: [u8; 4] = [0; 4];
    let ip_dst: [u8; 4] = [10, 0, 0, 1];
    ip_src.copy_from_slice(ip.source());
    let udp = match sliced_packet.transport.unwrap() {
        TransportSlice::Udp(udp) => udp,
        _ => {
            panic!()
        }
    };
    let payload: [u8; 0] = [0; 0];
    let builder = PacketBuilder::ethernet2(eth_src, eth_dst)
        .ipv4(ip_src, ip_dst, ip.ttl())
        .udp(udp.source_port(), udp.destination_port());

    let mut serialized = Vec::new();
    builder.write(&mut serialized, &payload).unwrap();
}
