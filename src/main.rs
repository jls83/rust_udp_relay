use std::io;
use std::net::{UdpSocket, Ipv4Addr, SocketAddrV4};

use clap::{ArgAction, Parser};

// fn create_socket(listen_address: &str, listen_port: i32) -> io::Result<UdpSocket> {
fn create_socket(listen_address: SocketAddrV4) -> io::Result<UdpSocket> {
    UdpSocket::bind(listen_address)
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // TODO: Ipv4Addr, Port separate
    listen_address: SocketAddrV4,
    #[arg(short, long, value_delimiter = ',')]
    speak_addresses: Option<Vec<SocketAddrV4>>,
}

fn main() {
    let args = Args::parse();

    let listen_sock = match create_socket(args.listen_address) {
        Ok(listen_sock) => listen_sock,
        Err(_) => panic!("Error creating socket"),
    };

    let speak_addresses = match args.speak_addresses {
        Some(speak_addresses) => speak_addresses,
        // TODO: Better error handling
        None => panic!("Wrong speak_addresses config"),
    };

    loop {
        let mut buf: [u8; 1024] = [0; 1024];

        listen_sock
            .recv_from(&mut buf)
            .expect("Reading from buffer failed");

        println!("{}", String::from_utf8((&buf).to_vec()).unwrap());

        speak_addresses.iter().for_each(|speak_address| {
            listen_sock
                .send_to(&buf, speak_address)
                .expect("Send to port failed");
        });
    }
}
