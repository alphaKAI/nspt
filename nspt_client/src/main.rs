#[cfg(not(target_os = "windows"))]
use nspt_common::DEFAULT_SOCK_FILE;
use nspt_common::{
    get_human_friendly_data_size_str, get_human_friendly_speed_str, get_transfer_size,
    NsptNegProtocol, ReadWriteStream, SerializedDataContainer, TestMode, BUF_SIZE, PROTOCOL_VER,
    SERVER_PORT_S, TOTAL_SEND_NEG_BYTES,
};
use rand::RngCore;
use std::net::TcpStream;
#[cfg(not(target_os = "windows"))]
use std::os::unix::net::UnixStream;
use std::{io::prelude::*, io::Write, mem::size_of};
use structopt::StructOpt;

fn do_speed_test<T>(server_stream: &mut T, transfer_size: usize) -> f64
where
    T: Read + Write,
{
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut rng = rand::thread_rng();
    for i in 0..BUF_SIZE / size_of::<u32>() {
        let bytes: &[u8; 4] = &rng.next_u32().to_le_bytes();
        buf[i] = bytes[0];
        buf[i + 1] = bytes[1];
        buf[i + 2] = bytes[2];
        buf[i + 3] = bytes[3];
    }

    println!("Start speed test!");
    let mut total: usize = 0;
    let prog = transfer_size / BUF_SIZE / 10;
    let mut parcent = 0;
    let mut count = 0;

    let mut stdout = std::io::stdout();

    let start = chrono::Local::now();
    while total < transfer_size {
        if count % prog == 0 {
            if count > 0 {
                print!("...");
            }
            print!("{}%", parcent * 10);
            let _ = stdout.flush();
            parcent += 1;
        }

        server_stream.write_all(&buf).unwrap();
        total += BUF_SIZE;
        count += 1;
    }
    let end = chrono::Local::now();
    println!();

    let elapse = (end - start).num_milliseconds(); //.to_std().unwrap().as_millis();
    let bytes_per_ms = transfer_size as f64 / elapse as f64;

    println!(
        " -> Finish Data Transfer! speed: {}",
        get_human_friendly_speed_str(bytes_per_ms)
    );

    bytes_per_ms
}

fn do_test(
    mut server_stream: &mut Box<dyn ReadWriteStream + Send>,
    test_times: u16,
    transfer_bytes: Option<usize>,
) {
    println!("Start exchanging Hello message.");
    {
        // Exchange Hello
        let (version_matched, server_proto_ver) =
            if let NsptNegProtocol::ServerHello(server_proto_ver) =
                SerializedDataContainer::from_reader(&mut server_stream)
                    .unwrap()
                    .to_serializable_data::<NsptNegProtocol>()
                    .unwrap()
            {
                (server_proto_ver == PROTOCOL_VER, server_proto_ver)
            } else {
                panic!("Protocol err")
            };

        server_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(&NsptNegProtocol::ClientHello(
                    PROTOCOL_VER,
                ))
                .unwrap()
                .to_one_vec(),
            )
            .expect("Failed to send ClientHello");
        if !version_matched {
            panic!("Protocol version mismatched! this proto-ver: {PROTOCOL_VER} but server proto-ver: {server_proto_ver}");
        }
    }
    println!(" -> End exchanging Hello message.");

    let transfer_size = if let Some(transfer_bytes) = transfer_bytes {
        server_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(
                    &NsptNegProtocol::SpeedNegotiation(false),
                )
                .unwrap()
                .to_one_vec(),
            )
            .expect("Failed to send SpeedNegotiation");

        transfer_bytes
    } else {
        // Determin amount of transfer size
        let mut neg_test_buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut rng = rand::thread_rng();
        for i in 0..BUF_SIZE / size_of::<u32>() {
            let bytes: &[u8; 4] = &rng.next_u32().to_le_bytes();
            neg_test_buf[i] = bytes[0];
            neg_test_buf[i + 1] = bytes[1];
            neg_test_buf[i + 2] = bytes[2];
            neg_test_buf[i + 3] = bytes[3];
        }

        server_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(
                    &NsptNegProtocol::SpeedNegotiation(true),
                )
                .unwrap()
                .to_one_vec(),
            )
            .expect("Failed to send SpeedNegotiation");

        server_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(
                    &NsptNegProtocol::StartSpeedNegotiation,
                )
                .unwrap()
                .to_one_vec(),
            )
            .expect("Failed to send StartSpeedNegotiation");

        if let NsptNegProtocol::StartSpeedNegotiation =
            SerializedDataContainer::from_reader(&mut server_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
            let mut total: usize = 0;

            println!("Start small speed test for negotiation...");
            let start = chrono::Local::now();

            while total < TOTAL_SEND_NEG_BYTES {
                server_stream.write_all(&neg_test_buf).unwrap();
                total += BUF_SIZE;
            }
            let end = chrono::Local::now();

            println!(" -> End of data transfer...");

            let elapse = (end - start).num_milliseconds(); //.to_std().unwrap().as_millis();
            let bytes_per_ms = TOTAL_SEND_NEG_BYTES as f64 / elapse as f64;

            get_transfer_size(bytes_per_ms)
        } else {
            panic!("Protocol err")
        }
    };

    println!(
        "[Condition] transfer_size: {}, test_times: {test_times}",
        get_human_friendly_data_size_str(transfer_size as u64)
    );

    let total = {
        server_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(
                    &NsptNegProtocol::NotifyBufferSize(transfer_size, test_times),
                )
                .unwrap()
                .to_one_vec(),
            )
            .expect("Failed to send NotifyBufferSize");

        if let NsptNegProtocol::StartSpeedTest =
            SerializedDataContainer::from_reader(&mut server_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
            let mut total = 0.;

            for _ in 0..test_times {
                total += do_speed_test(&mut server_stream, transfer_size);
            }

            server_stream
                .write_all(
                    &SerializedDataContainer::from_serializable_data(
                        &NsptNegProtocol::EndOfTransfer,
                    )
                    .unwrap()
                    .to_one_vec(),
                )
                .expect("Failed to send EndOfTransfer");

            total
        } else {
            panic!("Protocol err")
        }
    };

    {
        if let NsptNegProtocol::EndOfSpeedTest =
            SerializedDataContainer::from_reader(&mut server_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
            println!(
                "average: {}",
                get_human_friendly_speed_str(total / (test_times as f64))
            );
        } else {
            panic!("Protocol err")
        }
    }
}

const DEFAULT_SERVER_IP: &str = "127.0.0.1";

#[derive(Debug, StructOpt)]
#[structopt(name = "nspt_server", about = "Network Speed Test Server.")]
struct NsptClientArg {
    #[structopt(short = "i", long, default_value = DEFAULT_SERVER_IP)]
    server_ip: String,
    #[cfg(not(target_os = "windows"))]
    #[structopt(short = "s", long, default_value = DEFAULT_SOCK_FILE)]
    server_sock: String,
    #[structopt(short = "p", long, default_value = SERVER_PORT_S)]
    server_port: u16,
    #[structopt(short = "m", long, default_value = "TCP", parse(try_from_str))]
    test_mode: TestMode,
    #[structopt(short, long, default_value = "10")]
    test_times: u16,
    #[structopt(short = "d", long)]
    transfer_bytes: Option<usize>,
}

fn main() {
    let nspt_client_arg = NsptClientArg::from_args();

    let (mut server_stream, server_addr): (Box<dyn ReadWriteStream + Send>, String) =
        match nspt_client_arg.test_mode {
            TestMode::Tcp => {
                let server_addr = format!(
                    "{}:{}",
                    nspt_client_arg.server_ip, nspt_client_arg.server_port
                );
                (
                    Box::new(TcpStream::connect(&server_addr).unwrap()),
                    server_addr,
                )
            }
            #[cfg(not(target_os = "windows"))]
            TestMode::Unix => (
                Box::new(UnixStream::connect(&nspt_client_arg.server_sock).unwrap()),
                nspt_client_arg.server_sock,
            ),
        };
    println!("Server addr is: {server_addr}");

    println!("Connection is Established!");
    do_test(
        &mut server_stream,
        nspt_client_arg.test_times,
        nspt_client_arg.transfer_bytes,
    );
}
