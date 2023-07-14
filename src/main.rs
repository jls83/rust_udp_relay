use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;

use clap::Parser;

use tokio::io;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // TODO: Ipv4Addr, Port separate
    listen_address: SocketAddrV4,
    #[arg(short, long, value_delimiter = ',')]
    speak_addresses: Option<Vec<SocketAddrV4>>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    // TODO: Better error handling
    let speak_addresses = args.speak_addresses.expect("Wrong speak_addresses config");

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);

    // speak_addresses.iter().for_each(|&speak_addr| {
    for speak_addr in speak_addresses {
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        println!("Addr: {:?}", speak_addr);
        let sock = UdpSocket::bind(speak_addr)
            .await
            .expect("Error creating socket");

        let listen_sock = Arc::new(sock);
        let speak_sock = listen_sock.clone();

        let mut buf: [u8; 1024] = [0; 1024];

        tokio::spawn(async move {
            loop {

                let (len, source_addr) = listen_sock.recv_from(&mut buf).await.unwrap();
                // TODO: better comment
                // Avoid packet storms!
                if source_addr == SocketAddr::V4(speak_addr) {
                    continue;
                }
                tx.send((buf[..len].to_vec(), source_addr)).unwrap();
            }
        });

        tokio::spawn(async move {
            loop {
                let (foo, _) = rx.recv().await.unwrap();
                speak_sock.send_to(&foo, speak_addr.clone()).await.unwrap();
            }
        });
    };
    // });

    loop {}
}
