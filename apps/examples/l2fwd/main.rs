extern crate getopts;
extern crate libc;
extern crate nix;
extern crate pretty_env_logger;
extern crate rte;

use std::clone::Clone;
use std::env;
use std::io;
use std::io::prelude::*;
use std::mem;
use std::path::Path;
use std::process;
use std::str::FromStr;

use rte::ethdev::{EthDevice, EthDeviceInfo, TxBuffer};
use rte::ffi::RTE_MAX_ETHPORTS;
use rte::lcore::RTE_MAX_LCORE;
use rte::ether::RTE_ETHER_ADDR_LEN;
use rte::memory::AsMutRef;
use rte::*;

const EXIT_FAILURE: i32 = -1;

const MAX_PKT_BURST: usize = 32;

const BURST_TX_DRAIN_US: u64 = 100;
const US_PER_S: u64 = 1000000;

const MAX_RX_QUEUE_PER_LCORE: u32 = 16;

// A tsc-based timer responsible for triggering statistics printout
const TIMER_MILLISECOND: i64 = 2000000; /* around 1ms at 2 Ghz */
const MAX_TIMER_PERIOD: u32 = 86400; /* 1 day max */

const NB_MBUF: u32 = 2048;

// Configurable number of RX/TX ring descriptors

const RTE_TEST_RX_DESC_DEFAULT: u16 = 128;
const RTE_TEST_TX_DESC_DEFAULT: u16 = 512;

struct ForwardDesc {
    src_port: ethdev::PortId,
    dst_port: ethdev::PortId,
}

struct LcoreQueueConf {
    forward_desc_nb: u32,
    forward_desc_list: [ForwardDesc; MAX_RX_QUEUE_PER_LCORE as usize],
}

struct Conf {
    rxd_nb: u16,
    txd_nb: u16,

    tx_buffs: [ethdev::RawTxBufferPtr; RTE_MAX_ETHPORTS as usize],
    port_eth_addrs: [[u8; RTE_ETHER_ADDR_LEN]; RTE_MAX_ETHPORTS as usize],
    queue_conf: [LcoreQueueConf; RTE_MAX_LCORE as usize],
}

impl Default for Conf {
    fn default() -> Self {
        let mut conf: Self = unsafe { mem::zeroed() };

        conf.rxd_nb = RTE_TEST_RX_DESC_DEFAULT;
        conf.txd_nb = RTE_TEST_TX_DESC_DEFAULT;

        return conf;
    }
}

// display usage
fn print_usage(program: &String, opts: getopts::Options) -> ! {
    let brief = format!("Usage: {} [EAL options] -- [options]", program);

    print!("{}", opts.usage(&brief));

    process::exit(-1);
}

// Parse the argument given in the command line of the application
fn parse_args(args: &Vec<String>) -> (u32, u32, u32) {
    let mut opts = getopts::Options::new();
    let program = args[0].clone();

    opts.optopt("p", "", "hexadecimal bitmask of ports to configure", "PORTMASK");
    opts.optopt("q", "", "number of queue (=ports) per lcore (default is 1)", "NQ");
    opts.optopt(
        "T",
        "",
        "statistics will be refreshed each PERIOD seconds (0 to disable, 10 default, \
         86400 maximum)",
        "PERIOD",
    );
    opts.optflag("h", "help", "print this help menu");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(err) => {
            println!("Invalid L2FWD arguments, {}", err);

            print_usage(&program, opts);
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
    }

    let mut enabled_port_mask: u32 = 0; // mask of enabled ports
    let mut rx_queue_per_lcore: u32 = 1;
    let mut timer_period_seconds: u32 = 10; // default period is 10 seconds

    if let Some(arg) = matches.opt_str("p") {
        match u32::from_str_radix(arg.as_str(), 16) {
            Ok(mask) if mask != 0 => enabled_port_mask = mask,
            _ => {
                println!("invalid portmask, {}", arg);

                print_usage(&program, opts);
            }
        }
    }

    if let Some(arg) = matches.opt_str("q") {
        match u32::from_str(arg.as_str()) {
            Ok(n) if 0 < n && n < MAX_RX_QUEUE_PER_LCORE => rx_queue_per_lcore = n,
            _ => {
                println!("invalid queue number, {}", arg);

                print_usage(&program, opts);
            }
        }
    }

    if let Some(arg) = matches.opt_str("T") {
        match u32::from_str(arg.as_str()) {
            Ok(t) if 0 < t && t < MAX_TIMER_PERIOD => timer_period_seconds = t,
            _ => {
                println!("invalid timer period, {}", arg);

                print_usage(&program, opts);
            }
        }
    }

    (enabled_port_mask, rx_queue_per_lcore, timer_period_seconds)
}

fn prepare_args(args: &mut Vec<String>) -> (Vec<String>, Vec<String>) {
    let program = String::from(Path::new(&args[0]).file_name().unwrap().to_str().unwrap());

    if let Some(pos) = args.iter().position(|arg| arg == "--") {
        let (eal_args, opt_args) = args.split_at_mut(pos);

        opt_args[0] = program;

        (eal_args.to_vec(), opt_args.to_vec())
    } else {
        (args[..1].to_vec(), args.clone())
    }
}

// Check the link status of all ports in up to 9s, and print them finally
fn check_all_ports_link_status(enabled_devices: &Vec<ethdev::PortId>) {
    print!("Checking link status");

    const CHECK_INTERVAL: u32 = 100;
    const MAX_CHECK_TIME: usize = 90;

    for _ in 0..MAX_CHECK_TIME {
        // if unsafe { l2fwd_force_quit != 0 } {
        //     break;
        // }

        if enabled_devices.iter().all(|dev| dev.link_nowait().up) {
            break;
        }

        delay_ms(CHECK_INTERVAL);

        print!(".");

        io::stdout().flush().unwrap();
    }

    println!("Done:");

    for dev in enabled_devices {
        let link = dev.link();

        if link.up {
            println!(
                "  Port {} Link Up - speed {} Mbps - {}",
                dev.portid(),
                link.speed,
                if link.duplex { "full-duplex" } else { "half-duplex" }
            )
        } else {
            println!("  Port {} Link Down", dev.portid());
        }
    }
}

fn l2fwd_mac_updating(m: &mut mbuf::MBuf, dst_port: &ethdev::PortId, shared_conf: &Conf) {
    let dst_ptr: std::ptr::NonNull<[u8; RTE_ETHER_ADDR_LEN]> = m.mtod();
    let src_ptr: std::ptr::NonNull<[u8; RTE_ETHER_ADDR_LEN]> = m.mtod_offset(6);
    unsafe {
        *dst_ptr.as_ptr() = [2, 0, 0, 0, 0, *dst_port as u8];
        *src_ptr.as_ptr() = shared_conf.port_eth_addrs[*dst_port as usize];
    }
}

fn l2fwd_simple_forward(m: &mut mbuf::MBuf, dst_port: &ethdev::PortId, shared_conf: &Conf) {
    l2fwd_mac_updating(m, dst_port, shared_conf);
    let sent = dst_port.tx_buffer(0, shared_conf.tx_buffs[*dst_port as usize], m);
    if sent > 0 {
        println!("automatically flushed {} packets to port {}", sent, dst_port);
    }
}

fn l2fwd_launch_one_lcore(conf: Option<&Conf>) -> i32 {
    let lcore_id = lcore::current().unwrap();
    let shared_conf = &conf.unwrap();
    let local_conf = &shared_conf.queue_conf[*lcore_id as usize];
    let forward_descs = &local_conf.forward_desc_list[..local_conf.forward_desc_nb as usize];

    if local_conf.forward_desc_nb == 0 {
        println!("lcore {} has nothing to do", lcore_id);
        return 0;
    }

    println!("entering main loop on lcore {}", lcore_id);

    for ForwardDesc {src_port, dst_port} in forward_descs {
        println!(" -- lcoreid={} src_port={} dst_port={}", lcore_id, src_port, dst_port);
    }

    let mut prev_tsc = 0;
    let drain_tsc = (get_tsc_hz() + US_PER_S - 1) / US_PER_S *
            BURST_TX_DRAIN_US;

    loop {
        let cur_tsc = rdtsc();
        let diff_tsc = cur_tsc - prev_tsc;
        if diff_tsc > drain_tsc {
            for ForwardDesc {src_port: _, dst_port} in forward_descs {
                let sent = dst_port.tx_buffer_flush(0, shared_conf.tx_buffs[*dst_port as usize]);
                if sent > 0 {
                    println!("manually flushed {} packets to port {}", sent, dst_port);
                }
            }
            prev_tsc = cur_tsc;
        }
        for ForwardDesc {src_port, dst_port} in forward_descs {
            let mut mbufs: [Option<mbuf::MBuf>; MAX_PKT_BURST] = Default::default();
            let recv_nb = src_port.rx_burst(0, &mut mbufs);
            if recv_nb > 0 {
                println!("received {} from port {}", recv_nb, src_port);
            }
            for m_opt in &mut mbufs[..recv_nb] {
                if let Some(m) = m_opt {
                    l2fwd_simple_forward(m, dst_port, shared_conf);
                }
            }
        }
        // std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn main() {
    let mut args: Vec<String> = env::args().collect();

    let (eal_args, opt_args) = prepare_args(&mut args);

    println!("eal args: {:?}, l2fwd args: {:?}", eal_args, opt_args);

    let (enabled_port_mask, rx_queue_per_lcore, timer_period_seconds) = parse_args(&opt_args);

    let timer_period_seconds = timer_period_seconds as i64 * TIMER_MILLISECOND * 1000;

    println!("enabled_port_mask: {:?}, rx_queue_per_lcore: {:?}, timer_period_seconds {:?}",
        enabled_port_mask, rx_queue_per_lcore, timer_period_seconds);

    // init EAL
    eal::init(&eal_args).expect("fail to initial EAL");

    // create the mbuf pool
    let mut l2fwd_pktmbuf_pool = mbuf::pool_create(
        "mbuf_pool",
        NB_MBUF,
        32,
        0,
        mbuf::RTE_MBUF_DEFAULT_BUF_SIZE as u16,
        rte::socket_id() as i32,
    )
    .unwrap();

    let enabled_devices: Vec<ethdev::PortId> = ethdev::devices()
        .filter(|dev| ((1 << dev.portid()) & enabled_port_mask) != 0)
        .collect();

    if enabled_devices.is_empty() {
        eal::exit(EXIT_FAILURE, "All available ports are disabled. Please set portmask.\n");
    }

    let mut last_port = 0;
    let mut nb_ports_in_mask = 0;

    // Each logical core is assigned a dedicated TX queue on each port.
    let mut l2fwd_dst_ports = [0u16; RTE_MAX_ETHPORTS as usize];
    for dev in &enabled_devices {
        let portid = dev.portid();

        if (nb_ports_in_mask % 2) != 0 {
            l2fwd_dst_ports[portid as usize] = last_port as u16;
            l2fwd_dst_ports[last_port as usize] = portid as u16;
        } else {
            last_port = portid;
        }

        nb_ports_in_mask += 1;

        let info = dev.info();

        println!("found port #{} with `{}` drive", portid, info.driver_name());
    }

    if (nb_ports_in_mask % 2) != 0 {
        println!("Notice: odd number of ports in portmask.");

        l2fwd_dst_ports[last_port as usize] = last_port as u16;
    }

    let mut conf = Conf::default();

    let mut rx_lcore_id = lcore::id(0);

    // Initialize the port/queue configuration of each logical core
    for dev in &enabled_devices {
        let portid = dev.portid();

        loop {
            if let Some(id) = rx_lcore_id.next() {
                if conf.queue_conf[*rx_lcore_id as usize].forward_desc_nb == rx_queue_per_lcore {
                    rx_lcore_id = id
                }
            }

            break;
        }

        // Assigned a new logical core in the loop above.
        let qconf = &mut conf.queue_conf[*rx_lcore_id as usize];

        qconf.forward_desc_list[qconf.forward_desc_nb as usize] = ForwardDesc {
            src_port: portid,
            dst_port: l2fwd_dst_ports[portid as usize]
        };
        qconf.forward_desc_nb += 1;

        println!("Lcore {}: RX port {}", rx_lcore_id, portid);
    }

    let port_conf = ethdev::EthConf::default();

    // Initialise each port
    for dev in &enabled_devices {
        let portid = dev.portid() as usize;

        // init port
        print!("Initializing port {}... ", portid);

        dev.configure(1, 1, &port_conf)
            .expect(&format!("fail to configure device: port={}", portid));

        let mac_addr = dev.mac_addr();

        conf.port_eth_addrs[portid] = *mac_addr.octets();

        // init one RX queue
        dev.rx_queue_setup(0, conf.rxd_nb, None, &mut l2fwd_pktmbuf_pool)
            .expect(&format!("fail to setup device rx queue: port={}", portid));

        // init one TX queue on each port
        dev.tx_queue_setup(0, conf.txd_nb, None)
            .expect(&format!("fail to setup device tx queue: port={}", portid));

        // Initialize TX buffers
        let buf = ethdev::alloc_buffer(MAX_PKT_BURST, dev.socket_id())
            .as_mut_ref()
            .expect(&format!("fail to allocate buffer for tx: port={}", portid));

        buf.count_err_packets()
            .expect(&format!("failt to set error callback for tx buffer: port={}", portid));

        conf.tx_buffs[portid] = buf;

        // Start device
        dev.start().expect(&format!("fail to start device: port={}", portid));

        println!("Done: ");

        dev.promiscuous_enable();

        println!(
            "  Port {}, MAC address: {} (promiscuous {})",
            portid,
            mac_addr,
            if dev.is_promiscuous_enabled() { "enabled" } else { "disabled" }
        );
    }

    check_all_ports_link_status(&enabled_devices);

    lcore::foreach_slave(|lcore_id| {
        launch::remote_launch(l2fwd_launch_one_lcore, Some(&conf), lcore_id).expect("Cannot launch task");
    });
    l2fwd_launch_one_lcore(Some(&conf));
    // TODO: why causes segfault?
    // launch::mp_remote_launch(l2fwd_launch_one_lcore, Some(&conf), false).unwrap();

    launch::mp_wait_lcore();

    for dev in &enabled_devices {
        print!("Closing port {}...", dev.portid());
        dev.stop();
        dev.close();
        println!(" Done");

        if let Some(buf) = (conf.tx_buffs[dev.portid() as usize]).as_mut_ref() {
            buf.free();
        }
    }

    println!("Bye...");
}
