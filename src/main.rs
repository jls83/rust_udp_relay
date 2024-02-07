use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4};
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
    // TODO: Ipv4Addr, Port separate
    // #[arg(short, long, value_delimiter = ',')]
    // addresses: Option<Vec<SocketAddrV4>>,
    #[arg(short, long)]
    port: Option<u16>,

    #[arg(short, long, value_delimiter = ',')]
    interfaces: Option<Vec<String>>,
}

fn get_interface_map() -> HashMap<String, NetworkInterface> {
    NetworkInterface::show()
        .unwrap()
        .iter()
        .map(|interface| (interface.name.to_string(), interface.clone()))
        .collect()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let interface_map: HashMap<String, NetworkInterface> = get_interface_map();

    let args = Args::parse();

    // TODO: Better error handling

    let port = args.port.expect("Wrong port config");
    let interfaces = args.interfaces.expect("Wrong interfaces config");

    let speak_addresses: Vec<SocketAddrV4> = interfaces
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
        .collect();

    if speak_addresses.len() == 0 {
        panic!("No interfaces to listen on");
    }

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);

    for speak_addr in speak_addresses {
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        println!("Addr: {:?}", speak_addr);
        let sock = UdpSocket::bind(speak_addr)
            .await
            .expect("Error creating socket");

        let listen_sock = Arc::new(sock);
        let speak_sock = listen_sock.clone();

        tokio::spawn(async move {
            let mut buf: [u8; DATAGRAM_SIZE] = [0; DATAGRAM_SIZE];
            let (len, source_addr) = listen_sock.recv_from(&mut buf).await.unwrap();
            println!("Got {} bytes from {:?}", len, source_addr);
            // TODO: better comment
            // Avoid packet storms!
            if source_addr != SocketAddr::V4(speak_addr) {
                tx.send((buf[..len].to_vec(), source_addr)).unwrap();
            }
        });

        tokio::spawn(async move {
            let (foo, _) = rx.recv().await.unwrap();
            println!("Sending to {:?}", speak_addr);
            speak_sock.send_to(&foo, speak_addr.clone()).await.unwrap();
        });
    }

    loop {}
}
