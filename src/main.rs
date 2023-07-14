use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use tokio::io;
use tokio::net::UdpSocket;

use clap::{ArgAction, Parser};
use tokio::sync::{mpsc, broadcast};
use tokio::sync::mpsc::Sender;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // TODO: Ipv4Addr, Port separate
    listen_address: SocketAddrV4,
    #[arg(short, long, value_delimiter = ',')]
    speak_addresses: Option<Vec<SocketAddrV4>>,
}

#[tokio::main]
async fn main() -> io::Result<()>{
    let args = Args::parse();

    let listen_sock = UdpSocket::bind(args.listen_address).await
        .expect("Error creating socket");

    let r = Arc::new(listen_sock);

    // TODO: Better error handling
    let speak_addresses = args.speak_addresses
        .expect("Wrong speak_addresses config");

    let (tx, _rx) = broadcast::channel::<(Vec<u8>, SocketAddr)>(32);

    speak_addresses.iter().for_each(|&speak_addr| {
        let mut rx = tx.subscribe();
        let r = r.clone();
        tokio::spawn(async move {
            loop {
                let (foo, _) = rx.recv().await.unwrap();
                r.send_to(&foo, speak_addr.clone()).await.unwrap();
            }
        });
    });

    let mut buf: [u8; 1024] = [0; 1024];

    loop {
        let (len, addr) = r.recv_from(&mut buf).await?;
        tx.send((buf[..len].to_vec(), addr)).unwrap();
    }
}
