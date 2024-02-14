use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process;
use std::sync::Arc;

use env_logger::Env;
use ipnet::Ipv4Net;
use log::{debug, error, info, trace, warn};

use clap::Parser;

use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use tokio::io;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

const DATAGRAM_SIZE: usize = 65_535;
const TRANSMIT_PORT: u16 = 58371;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, required = true)]
    port: Option<u16>,

    #[arg(short, long, value_delimiter = ',', required = true)]
    receive_interfaces: Option<Vec<String>>,

    #[arg(short, long, value_delimiter = ',', required = true)]
    transmit_interfaces: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',')]
    block_nets: Option<Vec<Ipv4Net>>,

    #[arg(long, value_delimiter = ',')]
    allow_nets: Option<Vec<Ipv4Net>>,
}

fn get_interface_map() -> HashMap<String, NetworkInterface> {
    NetworkInterface::show()
        .unwrap()
        .iter()
        .map(|interface| (interface.name.to_string(), interface.clone()))
        .collect()
}

fn get_socket_addresses(
    interfaces: &Vec<String>,
    interface_map: &HashMap<String, NetworkInterface>,
    port: u16,
) -> Vec<SocketAddrV4> {
    interfaces
        .iter()
        .filter_map(|interface_name| match interface_map.get(interface_name) {
            Some(NetworkInterface { addr, .. }) => Some(addr),
            None => None,
        })
        .flat_map(|addrs| {
            addrs.iter().filter_map(|addr| match addr {
                Addr::V4(addr) => Some(SocketAddrV4::new(addr.ip, port)),
                _ => None,
            })
        })
        .collect()
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

#[tokio::main]
async fn main() -> io::Result<()> {
    // TODO: better env var names
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "trace")
        .write_style_or("MY_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    info!("Starting up");

    let interface_map: HashMap<String, NetworkInterface> = get_interface_map();

    debug!("Found {} interfaces", interface_map.len());

    let args = Args::parse();

    // TODO: Better error handling

    let port = args.port.expect("Wrong port config");

    let receive_interfaces = args
        .receive_interfaces
        .expect("Wrong receive interfaces config");

    let transmit_interfaces = args
        .transmit_interfaces
        .expect("Wrong transmit interfaces config");

    let block_nets = match args.block_nets {
        Some(block_nets) => block_nets,
        None => vec![],
    };

    let allow_nets = match args.allow_nets {
        Some(allow_nets) => allow_nets,
        None => vec![],
    };

    let receive_addresses = get_socket_addresses(&receive_interfaces, &interface_map, port);
    // TODO: read in transmit ports as well
    let transmit_addresses = get_socket_addresses(&transmit_interfaces, &interface_map, port + 1);

    if receive_addresses.len() == 0 {
        error!(
            "No interfaces to receive from. Tried {:?}",
            &receive_interfaces
        );
        process::exit(1);
    }

    if transmit_addresses.len() == 0 {
        error!(
            "No interfaces to transmit to. Tried {:?}",
            &transmit_interfaces
        );
        process::exit(1);
    }

    debug!("Receiving from {} interfaces", receive_addresses.len());
    debug!("Transmitting to {} interfaces", transmit_addresses.len());

    let transmit_addresses_set: HashSet<SocketAddrV4> =
        HashSet::from_iter(transmit_addresses.clone());

    let address_filter = AddressFilter::new(transmit_addresses_set, block_nets, allow_nets);
    let address_filter = Arc::new(address_filter);

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);
    trace!("Created broadcast channel");

    for receive_address in receive_addresses {
        let tx = tx.clone();

        let sock = UdpSocket::bind(receive_address)
            .await
            .expect("Error creating socket");
        info!("Listening on {:?}", receive_address);

        let receive_sock = Arc::new(sock);

        let address_filter = address_filter.clone();

        tokio::spawn(async move {
            loop {
                let mut buf: [u8; DATAGRAM_SIZE] = [0; DATAGRAM_SIZE];
                let (len, source_addr) = receive_sock.recv_from(&mut buf).await.unwrap();
                debug!("Read {} bytes from {:?}", len, source_addr);

                if let SocketAddr::V4(inner) = source_addr {
                    if address_filter.should_transmit(&inner) {
                        match tx.send((buf[..len].to_vec(), source_addr)) {
                            Ok(_) => trace!("Added packet to channel from {:?}", source_addr),
                            Err(_) => {
                                warn!("Error adding packet to channel from {:?}", source_addr);
                            }
                        }
                    }
                } else {
                    trace!("Ignoring non-IPv4 packet from {}", source_addr);
                }
            }
        });
    }

    // TODO: possible improvement - collection of open sockets?
    let transmit_sock_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), TRANSMIT_PORT);

    let transmit_sock = UdpSocket::bind(transmit_sock_addr)
        .await
        .expect("Could not bind transmit socket");
    info!("Transmitting on {:?}", transmit_sock_addr);

    let transmit_sock = Arc::new(transmit_sock);

    // // 1. Initial impl: single subscription
    // let mut rx = tx.subscribe();

    // while let Ok((buf, _source_addr)) = rx.recv().await {
    //     let buf = Arc::new(buf);

    //     for transmit_address in transmit_addresses.iter().cloned() {
    //         let transmit_sock = transmit_sock.clone();
    //         let buf = buf.clone();

    //         tokio::spawn(async move {
    //             match transmit_sock.send_to(&buf, &transmit_address).await {
    //                 Ok(n) => debug!("Sent {n} bytes to {transmit_address}"),
    //                 Err(_) => error!("Failed"),
    //             };
    //         });
    //     }
    // }

    // 2. hmm
    for transmit_address in transmit_addresses.iter().cloned() {
        let mut rx = tx.subscribe();
        let transmit_sock = transmit_sock.clone();

        tokio::spawn(async move {
            if let Ok((buf, _source_addr)) = rx.recv().await {
                match transmit_sock.send_to(&buf, &transmit_address).await {
                    Ok(n) => debug!("Sent {n} bytes to {transmit_address}"),
                    Err(_) => error!("Send failed to {:?}", transmit_address),
                }
            } else {
                // TODO: This isn't quite the correct error message.
                error!("Receive failed for {:?}", transmit_address);
            }
        });
    }

    // For version 1
    // Ok(())

    // For version 2
    loop {}
}
