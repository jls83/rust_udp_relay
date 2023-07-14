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
    listen_address: SocketAddrV4,
    speak_address: SocketAddrV4,
    // NOTE: Handle the "num_args" downstream OR figure out the `Append` junk.
    #[arg(short, long, value_delimiter = ',')]
    other_stuff: Option<Vec<SocketAddrV4>>,
    // listen_port: u16,
    // speak_port: u16,
}

fn main() {
    let args = Args::parse();

    println!("{:?}", args.other_stuff);

    // let listen_address = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), args.listen_port);
    // let speak_address = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), args.speak_port);

    let listen_sock = match create_socket(args.listen_address) {
        Ok(listen_sock) => listen_sock,
        Err(_) => panic!("Error creating socket"),
    };

    loop {
        let mut buf: [u8; 1024] = [0; 1024];

        listen_sock
            .recv_from(&mut buf)
            .expect("Reading from buffer failed");

        println!("{}", String::from_utf8((&buf).to_vec()).unwrap());

        listen_sock
            .send_to(&buf, args.speak_address)
            .expect("Send to port failed");
    }
}
