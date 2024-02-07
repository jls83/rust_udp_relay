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
    #[arg(short, long, value_delimiter = ',')]
    addresses: Option<Vec<SocketAddrV4>>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    // TODO: Better error handling
    let speak_addresses = args.addresses.expect("Wrong speak_addresses config");

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);

    for speak_addr in speak_addresses {
        let mut buf: [u8; 1024] = [0; 1024];

        let tx = tx.clone();
        let mut rx = tx.subscribe();

        println!("Addr: {:?}", speak_addr);
        let sock = UdpSocket::bind(speak_addr)
            .await
            .expect("Error creating socket");

        let listen_sock = Arc::new(sock);
        let speak_sock = listen_sock.clone();

        tokio::spawn(async move {
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
