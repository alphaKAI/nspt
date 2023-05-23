use log::{info, trace};
#[cfg(not(target_os = "windows"))]
use nspt_common::DEFAULT_SOCK_FILE;
use nspt_common::{
    get_human_friendly_data_size_str, Listener, NsptNegProtocol, ReadWriteStream,
    SerializedDataContainer, TestMode, BUF_SIZE, PROTOCOL_VER, SERVER_PORT_S, TOTAL_SEND_NEG_BYTES,
};
use std::env;
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_os = "windows"))]
use std::{fs, os::unix::net::UnixListener, path::Path};
use structopt::StructOpt;

#[allow(dead_code)]
fn packet_peeker(stream: TcpStream) {
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    loop {
        let n = stream.peek(&mut buf).unwrap_or(0);
        trace!("[PEEK] {buf:?}");
        if n == 0 {
            break;
        }
    }
}

fn do_test(
    mut client_stream: &mut Box<dyn ReadWriteStream + Send>,
    test_stream: &mut Box<dyn ReadWriteStream + Send>,
    client_addr: String,
) {
    info!("New client({client_addr}) connected!");

    {
        // Exchange Hello Message - Negotiation
        client_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(&NsptNegProtocol::ServerHello(
                    PROTOCOL_VER,
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();

        let proto_ver_matched = if let NsptNegProtocol::ClientHello(client_proto_ver) =
            SerializedDataContainer::from_reader(&mut client_stream)
                .unwrap()
                .to_serializable_data()
                .unwrap()
        {
            client_proto_ver == PROTOCOL_VER
        } else {
            panic!("Protocol Err");
        };

        if !proto_ver_matched {
            info!("Negotiation failed... reset connection.");
            return;
        }
    }

    {
        // Determine transfer buffer size

        if let NsptNegProtocol::SpeedNegotiation(is_required) =
            SerializedDataContainer::from_reader(&mut client_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
            if is_required {
                if let NsptNegProtocol::StartSpeedNegotiation =
                    SerializedDataContainer::from_reader(&mut client_stream)
                        .unwrap()
                        .to_serializable_data::<NsptNegProtocol>()
                        .unwrap()
                {
                    client_stream
                        .write_all(
                            &SerializedDataContainer::from_serializable_data(
                                &NsptNegProtocol::StartSpeedNegotiation,
                            )
                            .unwrap()
                            .to_one_vec(),
                        )
                        .expect("Failed to send ClientHello");

                    let mut neg_test_buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
                    let mut total: usize = 0;

                    info!("Start to determin unit size of test.");

                    while total < TOTAL_SEND_NEG_BYTES {
                        client_stream.read_exact(&mut neg_test_buf).unwrap();
                        total += BUF_SIZE;
                    }

                    info!("End determining unit size of test.");
                } else {
                    panic!("Protocol err")
                }
            }
        } else {
            panic!("Protocol err")
        }
    }

    // Receive transfer size from client
    let (transfer_size, test_times) = {
        if let NsptNegProtocol::NotifyBufferSize(transfer_size, test_times) =
            SerializedDataContainer::from_reader(&mut client_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
            info!(
                "transfer_size: {}, test_times: {test_times}",
                get_human_friendly_data_size_str(transfer_size as u64)
            );

            (transfer_size, test_times)
        } else {
            panic!("Protocol err")
        }
    };

    {
        // Speed Test Main
        client_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(&NsptNegProtocol::StartSpeedTest)
                    .unwrap()
                    .to_one_vec(),
            )
            .expect("Failed to send ClientHello");

        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];

        for round in 0..test_times {
            info!("Start transsfer data unit for speed testing - round {round}");

            let mut next_read_size = BUF_SIZE;
            let mut remain = transfer_size;

            while remain > 0 {
                let n = test_stream.read(&mut buf).unwrap();

                if n == 0 {
                    info!("Connection is closed unexpectely");
                    return;
                }

                if n == BUF_SIZE {
                    next_read_size = BUF_SIZE;
                } else if n != BUF_SIZE {
                    if n == next_read_size {
                        next_read_size = BUF_SIZE;
                    } else {
                        next_read_size = BUF_SIZE - n;
                    }
                }

                remain -= n;
            }

            info!("Finish Data Unit Transfer");
        }
    }

    {
        // End of Test.
        if let NsptNegProtocol::EndOfTransfer =
            SerializedDataContainer::from_reader(&mut client_stream)
                .unwrap()
                .to_serializable_data::<NsptNegProtocol>()
                .unwrap()
        {
        } else {
            panic!("Protocol err")
        }

        client_stream
            .write_all(
                &SerializedDataContainer::from_serializable_data(&NsptNegProtocol::EndOfSpeedTest)
                    .unwrap()
                    .to_one_vec(),
            )
            .expect("Failed to send ClientHello");
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "nspt_server", about = "Network Speed Test Server.")]
struct NsptServerArg {
    #[structopt(short = "m", long, default_value = "TCP", parse(try_from_str))]
    test_mode: TestMode,
    #[structopt(short = "p", long, default_value = SERVER_PORT_S)]
    server_port: u16,
    #[cfg(not(target_os = "windows"))]
    #[structopt(short = "s", long, default_value = DEFAULT_SOCK_FILE)]
    server_sock: String,
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let nspt_server_args = NsptServerArg::from_args();

    let (listner, server_addr): (Box<dyn Listener>, String) = match nspt_server_args.test_mode {
        TestMode::Tcp => {
            let addr = format!("0.0.0.0:{}", nspt_server_args.server_port);
            (Box::new(TcpListener::bind(&addr).unwrap()), addr)
        }
        #[cfg(not(target_os = "windows"))]
        TestMode::Unix => {
            let sockfile = Path::new(&nspt_server_args.server_sock);
            if sockfile.exists() {
                fs::remove_file(sockfile).unwrap();
            }
            (
                Box::new(UnixListener::bind(sockfile).unwrap()),
                nspt_server_args.server_sock,
            )
        }
    };

    loop {
        info!(" *** Server is ready for to be connected *** ");
        info!(
            "Test Mode: {:?}, Protocol Version: {PROTOCOL_VER:#04x}",
            nspt_server_args.test_mode
        );
        info!("Waiting a connection from client with {server_addr}");
        let (mut client_stream, client_addr) = listner.accept().unwrap();

        let mut test_stream = client_stream.try_clone().unwrap();

        do_test(
            &mut client_stream,
            &mut test_stream,
            format!("{client_addr:?}"),
        );
    }
}
