use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process;
use std::sync::Arc;

use ipnet::Ipv4Net;
use log::{debug, error, info, trace, warn};

use clap::Parser;

use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use tokio::io;
use tokio::net::UdpSocket;
use tokio::sync::broadcast::{self, Sender};

const BUFFER_SIZE: usize = 4096 + 20 + 8;
const TRANSMIT_PORT: u16 = 58371;
const CHANNEL_SIZE: usize = 2 << 7;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, required = true)]
    port: u16,

    #[arg(short, long, value_delimiter = ',', required = true)]
    receive_interfaces: Vec<String>,

    #[arg(short, long, value_delimiter = ',', required = true)]
    transmit_interfaces: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    block_nets: Vec<Ipv4Net>,

    #[arg(long, value_delimiter = ',')]
    allow_nets: Vec<Ipv4Net>,

    #[command(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

fn get_interface_map() -> HashMap<String, NetworkInterface> {
    let interface_map: HashMap<String, NetworkInterface> = NetworkInterface::show()
        .unwrap()
        .iter()
        .map(|interface| (interface.name.to_string(), interface.clone()))
        .collect();

    debug!("Found {} interfaces", interface_map.len());

    interface_map
}

fn get_socket_addresses(
    interfaces: &[String],
    interface_map: &HashMap<String, NetworkInterface>,
    port: u16,
) -> Option<Vec<SocketAddrV4>> {
    let addrs: Vec<SocketAddrV4> = interfaces
        .iter()
        .filter_map(|interface_name| {
            interface_map
                .get(interface_name)
                .map(|NetworkInterface { addr, .. }| addr)
        })
        .flat_map(|addrs| {
            addrs.iter().filter_map(|addr| match addr {
                Addr::V4(addr) => Some(SocketAddrV4::new(addr.ip, port)),
                _ => None,
            })
        })
        .collect();

    match addrs.len() {
        0 => None,
        _ => Some(addrs),
    }
}

struct AddressFilter {
    transmit_addresses_set: HashSet<SocketAddrV4>,
    block_nets: Vec<Ipv4Net>,
    allow_nets: Vec<Ipv4Net>,
}

impl AddressFilter {
    fn new(
        transmit_addresses_set: HashSet<SocketAddrV4>,
        block_nets: Vec<Ipv4Net>,
        allow_nets: Vec<Ipv4Net>,
    ) -> Self {
        // TODO: only log if non-zero?
        debug!("Blocking packets from {} subnets", block_nets.len());
        debug!("Allowing packets from {} subnets", allow_nets.len());
        Self {
            transmit_addresses_set,
            block_nets,
            allow_nets,
        }
    }

    fn should_transmit(&self, socket_addr: &SocketAddrV4) -> bool {
        let storm_check = self.transmit_addresses_set.contains(socket_addr);

        let in_block_net = self
            .block_nets
            .iter()
            .any(|net| net.contains(socket_addr.ip()));

        let in_allow_net = self
            .allow_nets
            .iter()
            .any(|net| net.contains(socket_addr.ip()));

        // TODO: Check semantics of this.
        let res = !storm_check && (!in_block_net || in_allow_net);

        if !res {
            trace!("Not transmitting packet from {:?}", socket_addr);
        }

        res
    }
}

async fn receive_handler(
    tx: Sender<(Vec<u8>, SocketAddr)>,
    receive_sock: Arc<UdpSocket>,
    address_filter: Arc<AddressFilter>,
) {
    let mut buf: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let (len, source_addr) = receive_sock.recv_from(&mut buf).await.unwrap();
    debug!("Read {} bytes from {:?}", len, source_addr);

    if let SocketAddr::V4(inner) = source_addr {
        if address_filter.should_transmit(&inner) {
            match tx.send((buf[..len].to_vec(), source_addr)) {
                Ok(_) => trace!("Added packet to channel from {:?}", source_addr),
                Err(e) => {
                    warn!(
                        "Error adding packet to channel from {:?} {:?}",
                        source_addr, e
                    );
                }
            }
        }
    } else {
        trace!("Ignoring non-IPv4 packet from {}", source_addr);
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    env_logger::Builder::new()
        .filter_level(args.verbose.log_level_filter())
        .init();

    info!("Starting up");

    let interface_map: HashMap<String, NetworkInterface> = get_interface_map();

    let receive_addresses =
        match get_socket_addresses(&args.receive_interfaces, &interface_map, args.port) {
            Some(addrs) => addrs,
            None => {
                error!(
                    "No interfaces to receive from. Tried {:?}",
                    &args.receive_interfaces
                );
                process::exit(1);
            }
        };

    // TODO: Pull this from args
    let transmit_ports: Vec<u16> = (1..3).map(|i| args.port + i).collect();

    let transmit_addresses: Vec<SocketAddrV4> = transmit_ports
        .iter()
        .flat_map(|transmit_port| {
            match get_socket_addresses(&args.transmit_interfaces, &interface_map, *transmit_port) {
                Some(addrs) => addrs,
                None => {
                    error!(
                        "No interfaces to transmit to. Tried {:?}",
                        &args.transmit_interfaces
                    );
                    process::exit(1);
                }
            }
        })
        .collect();

    debug!("Receiving from {} interfaces", receive_addresses.len());
    debug!("Transmitting to {} interfaces", transmit_addresses.len());

    let transmit_addresses_set: HashSet<SocketAddrV4> =
        HashSet::from_iter(transmit_addresses.iter().cloned());

    let address_filter =
        AddressFilter::new(transmit_addresses_set, args.block_nets, args.allow_nets);
    let address_filter = Arc::new(address_filter);

    // TODO: consider channel size here
    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(CHANNEL_SIZE);
    trace!("Created broadcast channel");

    // Set up incoming packet receivers. We bind a `UdpSocket` per-address in the
    // `receive_addresses` collection. Note that the `tx` within the loop refers to the
    // input-side of the `broadcast::channel` created above.
    for receive_address in receive_addresses {
        let receive_sock = UdpSocket::bind(receive_address)
            .await
            .expect("Error creating socket");
        info!("Listening on {:?}", receive_address);

        let receive_sock = Arc::new(receive_sock);
        let tx = tx.clone();
        let address_filter = address_filter.clone();

        tokio::spawn(async move {
            let tx = tx.clone();
            loop {
                receive_handler(tx.clone(), receive_sock.clone(), address_filter.clone()).await
            }
        });
    }

    // Set up outing packet transmitters. Here, we bind a single `UdpSocket` on
    // localhost:TRANSMIT_PORT, then use the `send_to` method to sent to the apprpriate
    // `transmit_address`. Note that the `rx` within the loop refers to the output-side
    // of the `broadcast::channel` created above.
    //
    // TODO: possible improvement - collection of open sockets?
    let transmit_sock_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), TRANSMIT_PORT);

    let transmit_sock = UdpSocket::bind(transmit_sock_addr)
        .await
        .expect("Could not bind transmit socket");
    info!("Transmitting on {:?}", transmit_sock_addr);

    let transmit_sock = Arc::new(transmit_sock);
    let transmit_addresses = Arc::new(transmit_addresses);

    let mut rx = tx.subscribe();

    loop {
        match rx.recv().await {
            Ok((buf, _source_addr)) => {
                let transmit_sock = transmit_sock.clone();
                let transmit_addresses = transmit_addresses.clone();

                tokio::spawn(async move {
                    for transmit_address in transmit_addresses.iter() {
                        match transmit_sock.send_to(&buf, transmit_address).await {
                            Ok(n) => debug!("Sent {n} bytes to {transmit_address}"),
                            Err(e) => error!("Send failed to {:?}, {:?}", transmit_address, e),
                        }
                    }
                });
            }
            Err(e) => {
                // TODO: This isn't quite the correct error message.
                // TODO: check for `Lagged` message
                error!("Receive failed: {:?}", e);
            }
        }
    }
}

#[tokio::test]
async fn blah() {
    assert!(true);
}
