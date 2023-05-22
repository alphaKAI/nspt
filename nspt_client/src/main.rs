use nspt_common::{BUF_SIZE, SERVER_PORT, TOTAL_SEND_BYTES};
use rand::RngCore;
use std::env::args;
use std::net::TcpStream;
use std::{io::prelude::*, mem::size_of};

fn get_human_friendly_speed_str(bytes_per_ms: f64) -> String {
    let bytes_per_sec = bytes_per_ms * 1000.;
    let k_bytes_per_sec = bytes_per_sec / 1024.;
    let k_bits_per_sec = k_bytes_per_sec * 8.;
    let m_bytes_per_sec = k_bytes_per_sec / 1024.;
    let m_bits_per_sec = m_bytes_per_sec * 8.;
    let g_bytes_per_sec = m_bytes_per_sec / 1024.;
    let g_bits_per_sec = g_bytes_per_sec * 8.;

    if g_bits_per_sec as u64 != 0 {
        format!("{} Gb/s", g_bits_per_sec as u64)
    } else if m_bits_per_sec as u64 != 0 {
        format!(" {} Mb/s", m_bits_per_sec as u64)
    } else {
        format!("{} Kb/s", k_bits_per_sec as u64)
    }
}

fn start_speed_test(server_ip: &str) -> f64 {
    let mut server_stream = TcpStream::connect(format!("{server_ip}:{SERVER_PORT}")).unwrap();
    println!("client is ready for testing");

    // need negotiation
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut rng = rand::thread_rng();
    for i in 0..BUF_SIZE / size_of::<u32>() {
        let bytes: &[u8; 4] = &rng.next_u32().to_le_bytes();
        buf[i] = bytes[0];
        buf[i + 1] = bytes[1];
        buf[i + 2] = bytes[2];
        buf[i + 3] = bytes[3];
    }

    let mut total: usize = 0;

    println!("Start speed test!");
    let start = chrono::Local::now();

    while total < TOTAL_SEND_BYTES {
        let n = server_stream.write(&buf).unwrap();
        total += n;
    }
    let end = chrono::Local::now();

    let elapse = (end - start).num_milliseconds(); //.to_std().unwrap().as_millis();
    let bytes_per_ms = TOTAL_SEND_BYTES as f64 / elapse as f64;

    println!(
        "Finish Data Transfer! speed: {}",
        get_human_friendly_speed_str(bytes_per_ms)
    );

    bytes_per_ms
}

const DEFAULT_SERVER_IP: &str = "127.0.0.1";

fn main() {
    let args: &[String] = &args().collect::<Vec<_>>()[1..];
    let server_ip = if args.len() != 1 || (args.len() == 1 && &args[0] == "localhost") {
        DEFAULT_SERVER_IP
    } else {
        &args[0]
    };
    println!("Server IP is: {server_ip}");

    let times = 10;
    let mut total = 0.;

    for _ in 0..times {
        total += start_speed_test(server_ip);
    }

    println!("average: {}", get_human_friendly_speed_str(total / (times as f64)));
}
