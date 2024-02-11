use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use env_logger::Env;
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
}

fn get_interface_map() -> HashMap<String, NetworkInterface> {
    NetworkInterface::show()
        .unwrap()
        .iter()
        .map(|interface| (interface.name.to_string(), interface.clone()))
        .collect()
}

fn get_socket_addresses(
    interfaces: Vec<String>,
    interface_map: &HashMap<String, NetworkInterface>,
    port: u16,
) -> Vec<SocketAddrV4> {
    interfaces
        .iter()
        .flat_map(|interface_name| {
            interface_map
                .get(interface_name)
                .unwrap()
                .addr
                .iter()
                .filter_map(|x| match x {
                    Addr::V4(s) => Some(SocketAddrV4::new(s.ip, port)),
                    _ => None,
                })
        })
        .collect()
}

#[tokio::main]
async fn main() -> io::Result<()> {
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

    let receive_addresses = get_socket_addresses(receive_interfaces, &interface_map, port);
    // TODO: read in transmit ports as well
    let transmit_addresses = get_socket_addresses(transmit_interfaces, &interface_map, port + 1);

    if receive_addresses.len() == 0 {
        // TODO: error, exit code 1
        panic!("No interfaces to listen on");
    }

    if transmit_addresses.len() == 0 {
        // TODO: error, exit code 1
        panic!("No interfaces to transmit to");
    }

    debug!("Receiving from {} interfaces", receive_addresses.len());
    debug!("Transmitting to {} interfaces", transmit_addresses.len());

    let transmit_addresses_set: HashSet<SocketAddrV4> =
        HashSet::from_iter(transmit_addresses.clone());

    let transmit_addresses_set = Arc::new(transmit_addresses_set);

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);
    trace!("Created broadcast channel");

    for receive_address in receive_addresses {
        let tx = tx.clone();

        info!("Listening on {:?}", receive_address);
        let sock = UdpSocket::bind(receive_address)
            .await
            .expect("Error creating socket");

        let receive_sock = Arc::new(sock);

        let transmit_addresses_set = transmit_addresses_set.clone();

        tokio::spawn(async move {
            loop {
                let mut buf: [u8; DATAGRAM_SIZE] = [0; DATAGRAM_SIZE];
                let (len, source_addr) = receive_sock.recv_from(&mut buf).await.unwrap();
                debug!("Read {} bytes from {:?}", len, source_addr);
                // TODO: better comment
                // Avoid packet storms!

                let should_transmit = match source_addr {
                    SocketAddr::V4(inner) => !transmit_addresses_set.contains(&inner),
                    _ => true,
                };

                if should_transmit {
                    tx.send((buf[..len].to_vec(), source_addr)).unwrap();
                } else {
                    trace!("Ignoring packet from {}", source_addr);
                }
            }
        });
    }

    let mut rx = tx.subscribe();

    let transmit_sock_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), TRANSMIT_PORT);

    let transmit_sock_inner = UdpSocket::bind(transmit_sock_addr)
        .await
        .expect("Could not bind transmit socket");

    info!("Transmitting on {:?}", transmit_sock_addr);

    let transmit_sock = Arc::new(transmit_sock_inner);

    while let Ok((buf, _source_addr)) = rx.recv().await {
        for transmit_address in transmit_addresses.as_slice() {
            match transmit_sock.send_to(&buf, &transmit_address).await {
                Ok(n) => debug!("Sent {n} bytes to {transmit_address}"),
                Err(_) => error!("Failed"),
            };
        }
    }

    Ok(())
}
