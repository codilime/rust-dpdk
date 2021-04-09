use anyhow::Context;
use dpdk::arrayvec::ArrayVec;
use dpdk::eal::{self, Eal, LCoreId, Port, TxQ};
use log::{info, warn};
use smoltcp::wire::{EthernetAddress, EthernetFrame};
use structopt::StructOpt;

use std::env;

mod utils;

type PacketMeta = ();
type RxQ = eal::RxQ<PacketMeta>;
type Packet<'pool> = eal::Packet<'pool, PacketMeta>;

#[derive(Debug, StructOpt)]
#[structopt(usage = "l2fwd [EAL OPTIONS] -- [OPTIONS]\n    l2fwd [EAL OPTIONS]")]
#[structopt(after_help = "Note: To print EAL help message, run: l2fwd -h --")]
struct Opt {
    /// hexadecimal bitmask of ports to configure, no mask → all ports
    #[structopt(short, long, parse(try_from_str = utils::parse_hex), name = "PORTMASK")]
    portmask: Option<u64>,

    /// number of queues per lcore
    #[structopt(short, long, default_value = "1", name = "NQ")]
    queues_per_lcore: usize,

    /// statistics refresh period in seconds, 0 to disable
    #[structopt(short = "T", long, default_value = "10", name = "PERIOD")]
    stats_period: u32,
}

fn main() -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new().init().unwrap();

    let mut args: Vec<String> = env::args().collect();
    if matches!(&*args, [_, h] if matches!(&**h,  "-h" | "--help" | "-V" | "--version")) {
        // Print application help instead of EAL's by default
        Opt::from_iter(args);
        unreachable!();
    }
    let eal = Eal::new(&mut args).context("initializing EAL")?;
    let opt = Opt::from_iter(args);

    let lcores = eal.lcores();
    let portswq: Vec<PortWithQueues> = eal
        .ports()?
        .into_iter()
        .filter(|port| match opt.portmask {
            None => true,
            Some(mask) => ((1 << port.port_id()) & mask) != 0,
        })
        .map(|port| {
            let (port, (rxqs, txqs)) = port.init(1, 1, None);
            info!("found port #{} ", port.port_id());
            PortWithQueues {
                port,
                rx: rxqs.into_iter().nth(0).unwrap(),
                tx: txqs.into_iter().nth(0).unwrap(),
            }
        })
        .collect();

    let ports: Vec<Port> = portswq.iter().map(|p| p.port.clone()).collect();

    anyhow::ensure!(!ports.is_empty(), "no enabled ports");
    info!("{} enabled lcores and {} ports", lcores.len(), ports.len());

    let fwds = pair_ports(portswq);
    let assigned_fwds = assign_work(lcores, fwds, &opt);

    for port in &ports {
        port.set_promiscuous(true);
        port.start()
            .with_context(|| format!("starting port {}", port.port_id()))?;
    }

    dpdk::thread::scope(|scope| {
        for (lcore, fwds) in assigned_fwds {
            lcore.launch(scope, |id| forward_loop(id, fwds));
        }
    })
    .map_err(|err| anyhow::anyhow!("{:?}", err))
    .context("lcore failed")?;

    Ok(())
}

struct ForwardDesc {
    src: RxQ,
    dst: TxQ<'static>,
}

struct PortWithQueues {
    port: Port,
    rx: RxQ,
    tx: TxQ<'static>,
}

fn pair_ports(ports: Vec<PortWithQueues>) -> Vec<ForwardDesc> {
    let mut fwds = Vec::with_capacity(ports.len());

    let mut ports = ports.into_iter();
    while let Some(port1) = ports.next() {
        match ports.next() {
            Some(port2) => {
                fwds.push(ForwardDesc {
                    src: port1.rx,
                    dst: port2.tx,
                });
                fwds.push(ForwardDesc {
                    src: port2.rx,
                    dst: port1.tx,
                });
            }
            None => {
                warn!("odd number of ports, last one will forward to itself");
                fwds.push(ForwardDesc {
                    src: port1.rx,
                    dst: port1.tx,
                });
            }
        }
    }

    fwds
}

fn assign_work(
    lcores: Vec<LCoreId>,
    fwds: Vec<ForwardDesc>,
    opt: &Opt,
) -> Vec<(LCoreId, Vec<ForwardDesc>)> {
    let mut lcore_fwds = Vec::new();

    let mut fwds = fwds.into_iter();
    while !fwds.as_slice().is_empty() {
        let local_fwds: Vec<_> = fwds.by_ref().take(opt.queues_per_lcore).collect();
        if lcore_fwds.len() < lcores.len() {
            lcore_fwds.push(local_fwds);
        } else {
            warn!("not enough lcores, last one will have more queues");
            lcore_fwds.last_mut().unwrap().extend(local_fwds);
        }
    }

    lcores.into_iter().zip(lcore_fwds).collect()
}

fn forward_loop(lcore: LCoreId, fwds: Vec<ForwardDesc>) {
    // TODO impl buffering and flush timer

    info!("entering main loop on lcore {}", lcore);
    for fwd in &fwds {
        println!(
            " -- lcoreid={}, src_port={}, dst_port={}",
            lcore,
            fwd.src.port().port_id(),
            fwd.dst.port().port_id(),
        );
    }

    // We need to split rxs and txses into separate variables, as txs borrow from rxes (more
    // precisely, from their mpools). And Rust doesn't understand "self-referential" structs.
    let (srcs, mut dsts): (Vec<RxQ>, Vec<TxQ>) =
        fwds.into_iter().map(|fwd| (fwd.src, fwd.dst)).unzip();

    // Cache src macs. They are cheap to access (function call + memcpy),
    // but that's still _some_ cost.
    let src_macs: Vec<_> = dsts.iter().map(|dst| dst.port().mac_addr()).collect();

    let dst_macs: Vec<_> = dsts
        .iter()
        .map(|dst| get_fake_dst_mac(dst.port()))
        .collect();

    let mut bufs: Vec<ArrayVec<Packet, MAX_PKT_BURST>> =
        srcs.iter().map(|_| ArrayVec::new()).collect();

    loop {
        for (src, dst, src_mac, dst_mac, buf) in
            itertools::izip!(&srcs, &mut dsts, &src_macs, &dst_macs, &mut bufs)
        {
            let len_before_rx = buf.len();
            src.rx(buf);

            for pkt in &mut buf[len_before_rx..] {
                set_macs(pkt, *src_mac, *dst_mac);
            }

            dst.tx(buf);
        }
    }
}

const MAX_PKT_BURST: usize = 32;

fn set_macs(pkt: &mut Packet, src_mac: [u8; 6], dst_mac: [u8; 6]) {
    let mut eth = match EthernetFrame::new_checked(pkt.data_mut()) {
        Ok(eth) => eth,
        Err(_) => {
            warn!("packet too short");
            return;
        }
    };
    eth.set_src_addr(EthernetAddress(src_mac));
    eth.set_dst_addr(EthernetAddress(dst_mac));
}

fn get_fake_dst_mac(port: &Port) -> [u8; 6] {
    [2, 0, 0, 0, 0, port.port_id() as u8]
}
