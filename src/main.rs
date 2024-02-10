use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use clap::Parser;

use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use tokio::io;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

const DATAGRAM_SIZE: usize = 65_535;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    port: Option<u16>,

    #[arg(short, long, value_delimiter = ',')]
    receive_interfaces: Option<Vec<String>>,

    #[arg(short, long, value_delimiter = ',')]
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
    let interface_map: HashMap<String, NetworkInterface> = get_interface_map();

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
    let transmit_addresses = get_socket_addresses(transmit_interfaces, &interface_map, port + 1);

    if receive_addresses.len() == 0 {
        panic!("No interfaces to listen on");
    }

    let transmit_addresses_set: HashSet<SocketAddrV4> =
        HashSet::from_iter(transmit_addresses.clone());

    let transmit_addresses_set = Arc::new(transmit_addresses_set);

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);

    let transmit_sock_inner =
        UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 58371))
            .await
            .expect("Could not bind transmit socket");

    println!("Transmit Socket: {:?}", transmit_sock_inner);

    let transmit_sock = Arc::new(transmit_sock_inner);

    // listening
    for receive_address in receive_addresses {
        let tx = tx.clone();

        println!("Addr: {:?}", receive_address);
        let sock = UdpSocket::bind(receive_address)
            .await
            .expect("Error creating socket");

        let receive_sock = Arc::new(sock);

        let transmit_addresses_set = transmit_addresses_set.clone();

        tokio::spawn(async move {
            let mut buf: [u8; DATAGRAM_SIZE] = [0; DATAGRAM_SIZE];
            let (len, source_addr) = receive_sock.recv_from(&mut buf).await.unwrap();
            println!("Got {} bytes from {:?}", len, source_addr);
            // TODO: better comment
            // Avoid packet storms!

            let should_transmit = match source_addr {
                SocketAddr::V4(inner) => !transmit_addresses_set.contains(&inner),
                _ => true,
            };

            if should_transmit {
                tx.send((buf[..len].to_vec(), source_addr)).unwrap();
            }
        });
    }

    for transmit_address in transmit_addresses {
        let mut rx = tx.subscribe();

        println!("Addr: {:?}", transmit_address);

        let transmit_sock = transmit_sock.clone();

        tokio::spawn(async move {
            let (buf, _source_addr) = rx.recv().await.unwrap();
            println!("Sending to {:?}", transmit_address);
            let r = transmit_sock.send_to(&buf, transmit_address.clone()).await;

            match r {
                Ok(n) => println!("Sent {n} bytes"),
                Err(_) => println!("Failed"),
            };
        });
    }

    loop {}
}
