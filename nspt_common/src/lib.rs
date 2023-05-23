use rmp_serde::Serializer;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::io::prelude::*;
use std::mem::size_of;
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_os = "windows"))]
use std::os::unix::net::{UnixListener, UnixStream};
use std::str::FromStr;

pub const DEFAULT_SOCK_FILE: &str = "/tmp/nspt.sock";
pub const SERVER_PORT: u16 = 12845;
pub const SERVER_PORT_S: &str = "12845";
pub const TOTAL_SEND_NEG_BYTES: usize = 1024 * 1024 * 24; // 24 MB
pub const MIN_SEND_BYTES: usize = 1024 * 1024 * 24; // 24 MB
pub const BUF_SIZE: usize = 1024 << 6;
pub type ProtocolVer = u64;
pub const PROTOCOL_VER: ProtocolVer = 0x0000_0000_0000_0001;

#[derive(Debug)]
pub enum TestMode {
    Tcp,
    #[cfg(not(target_os = "windows"))]
    Unix,
}

impl FromStr for TestMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tcp" | "TCP" => Ok(TestMode::Tcp),
            #[cfg(not(target_os = "windows"))]
            "unix" | "UNIX" => Ok(TestMode::Unix),
            _ => Err(format!("Unkown Test Mode: {s}")),
        }
    }
}

pub trait ReadWriteStream: Read + Write + Send {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWriteStream + Send>>;
}

impl ReadWriteStream for TcpStream {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWriteStream + Send>> {
        self.try_clone()
            .map(|x| Box::new(x) as Box<dyn ReadWriteStream + Send>)
    }
}

#[cfg(not(target_os = "windows"))]
impl ReadWriteStream for UnixStream {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWriteStream + Send>> {
        self.try_clone()
            .map(|x| Box::new(x) as Box<dyn ReadWriteStream + Send>)
    }
}

pub trait Listener<'a> {
    fn accept(&self) -> std::io::Result<(Box<dyn ReadWriteStream + Send + 'a>, String)>;
}

impl<'a> Listener<'a> for TcpListener {
    fn accept(&self) -> std::io::Result<(Box<dyn ReadWriteStream + Send + 'a>, String)> {
        let (stream, addr) = TcpListener::accept(self)?;
        Ok((
            Box::new(stream) as Box<dyn ReadWriteStream + Send + 'a>,
            addr.to_string(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
impl<'a> Listener<'a> for UnixListener {
    fn accept(&self) -> std::io::Result<(Box<dyn ReadWriteStream + Send + 'a>, String)> {
        let (stream, addr) = UnixListener::accept(self)?;
        Ok((
            Box::new(stream) as Box<dyn ReadWriteStream + Send + 'a>,
            format!("{addr:?}"),
        ))
    }
}

fn find_next_power_of_two(n: u64) -> u64 {
    let mut power_of_two = 1;
    while power_of_two < n {
        power_of_two <<= 1;
    }
    power_of_two
}

pub fn get_transfer_size(bytes_per_ms: f64) -> usize {
    let bytes_per_sec = (bytes_per_ms * 1000.) as u64;

    let a = find_next_power_of_two(bytes_per_sec / 200) as usize;

    min(a, MIN_SEND_BYTES)
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NsptNegProtocol {
    ClientHello(ProtocolVer),
    ServerHello(ProtocolVer),
    SpeedNegotiation(bool), // true -> perform, false -> skip
    StartSpeedNegotiation,
    NotifyBufferSize(usize, u16), // unit buffer size, counts of test
    StartSpeedTest,
    EndOfSpeedTest,
    EndOfTransfer,
}

pub fn get_human_friendly_speed_str(bytes_per_ms: f64) -> String {
    let bytes_per_sec = bytes_per_ms * 1000.;
    let bits_per_sec = bytes_per_sec * 8.;
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
    } else if k_bits_per_sec as u64 != 0 {
        format!("{} Kb/s", k_bits_per_sec as u64)
    } else {
        format!("{} b/s", bits_per_sec as u64)
    }
}

pub fn get_human_friendly_data_size_str(bytes: u64) -> String {
    let k_bytes = bytes / 1024;
    let m_bytes = k_bytes / 1024;
    let g_bytes = m_bytes / 1024;

    if g_bytes != 0 {
        format!("{g_bytes} GB")
    } else if m_bytes != 0 {
        format!(" {m_bytes} MB")
    } else if k_bytes != 0 {
        format!("{k_bytes} KB")
    } else {
        format!("{bytes} B")
    }
}

#[derive(Debug)]
pub struct SerializedDataContainer {
    size: usize,
    data: Vec<u8>,
}

impl SerializedDataContainer {
    pub fn new(v: &[u8]) -> Self {
        Self {
            size: v.len(),
            data: v.to_owned(),
        }
    }

    pub fn to_one_vec(&self) -> Vec<u8> {
        let mut ret = vec![];

        ret.append(&mut self.size.to_le_bytes().to_vec());
        ret.append(&mut self.data.clone());

        ret
    }

    pub fn from_reader<T>(reader: &mut T) -> Result<Self, std::io::Error>
    where
        T: Read,
    {
        let mut size_buffer = [0; size_of::<usize>()];
        reader.read_exact(&mut size_buffer).and_then(|_| {
            let size = usize::from_le_bytes(size_buffer);
            let mut data = vec![];

            reader.take(size as u64).read_to_end(&mut data)?;

            Ok(Self { size, data })
        })
    }

    pub fn from_one_vec(v: Vec<u8>) -> Option<Self> {
        if v.len() >= size_of::<usize>() {
            let size = usize::from_le_bytes(
                v[0..size_of::<usize>()]
                    .try_into()
                    .expect("Failed to parse size of the data container"),
            );
            let data = v[size_of::<usize>()..size_of::<usize>() + size]
                .try_into()
                .expect("Failed to get data of the data container");

            Some(Self { size, data })
        } else {
            None
        }
    }

    pub fn from_serializable_data<T>(t: &T) -> Option<Self>
    where
        T: Serialize,
    {
        let mut data = vec![];
        t.serialize(&mut Serializer::new(&mut data)).ok().map(|_| {
            let size = data.len();
            Self { size, data }
        })
    }

    pub fn to_serializable_data<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        rmp_serde::from_slice(&self.data).ok()
    }
}
