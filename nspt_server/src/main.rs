use nspt_common::{BUF_SIZE, SERVER_PORT, TOTAL_SEND_BYTES};
use std::io::prelude::*;
use std::net::TcpListener;

fn main() {
    let listner = TcpListener::bind(format!("0.0.0.0:{SERVER_PORT}")).unwrap();

    loop {
        println!("server is ready for to be connected");

        let (mut client_stream, client_addr) = listner.accept().unwrap();
        println!("New client({client_addr:?}) connected!");

        // need negotiation
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut total: usize = 0;

        println!("Start speed test!");

        while total < TOTAL_SEND_BYTES {
            let n = client_stream.read(&mut buf).unwrap();
            if n == 0 {
                println!("[Fatal] Connection closed unexpectedly!!!!");
                break;
            }
            total += n;
        }

        println!("Finish Data Transfer!");

        println!("Hello, world!");
    }
}
