//! A small FTP server, for the tests.
//!
//! It speaks the part of the protocol the engine uses: the greeting, `USER`,
//! `PASS`, `TYPE`, `SIZE`, `PASV`, `REST`, `RETR` and `QUIT`. The file it
//! serves is in memory.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// A server that serves one file.
pub struct TestServer {
    pub port: u16,
}

impl TestServer {
    /// Start the server in a thread. It stops when the process stops.
    pub fn start(data: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let data = data.clone();
                std::thread::spawn(move || {
                    let _ = serve(stream, data);
                });
            }
        });

        Self { port }
    }
}

/// The address of the file on the server.
pub const PATH: &str = "/file.bin";

fn serve(mut control: TcpStream, data: Vec<u8>) -> std::io::Result<()> {
    let mut reader = control.try_clone()?;
    let mut buffer = [0u8; 1024];
    let mut offset = 0u64;

    control.write_all(b"220 ready\r\n")?;
    control.flush()?;

    loop {
        let command = match read_command(&mut reader, &mut buffer) {
            Some(command) => command,
            None => return Ok(()),
        };
        let name = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();

        match name.as_str() {
            "USER" => control.write_all(b"331 need a password\r\n")?,
            "PASS" => control.write_all(b"230 logged in\r\n")?,
            "TYPE" => control.write_all(b"200 binary\r\n")?,
            "SIZE" => control.write_all(format!("213 {}\r\n", data.len()).as_bytes())?,
            "REST" => {
                let wanted = command
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                offset = wanted.min(data.len() as u64);
                control.write_all(format!("350 restarting at {offset}\r\n").as_bytes())?;
            }
            "PASV" => {
                // The client opens the data connection on the port we offer,
                // then asks for the file. `REST` may come first.
                let data_listener = TcpListener::bind("127.0.0.1:0")?;
                let data_port = data_listener.local_addr()?.port();
                let parts = format!("127,0,0,1,{},{}", data_port / 256, data_port % 256);
                control.write_all(format!("227 entering passive mode ({parts})\r\n").as_bytes())?;
                control.flush()?;

                loop {
                    let Some(command) = read_command(&mut reader, &mut buffer) else {
                        return Ok(());
                    };
                    let name = command
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase();

                    if name == "REST" {
                        let wanted = command
                            .split_whitespace()
                            .nth(1)
                            .and_then(|value| value.parse::<u64>().ok())
                            .unwrap_or(0);
                        offset = wanted.min(data.len() as u64);
                        control.write_all(format!("350 restarting at {offset}\r\n").as_bytes())?;
                        control.flush()?;
                        continue;
                    }

                    if name == "RETR" {
                        control.write_all(b"150 opening the data connection\r\n")?;
                        control.flush()?;

                        let (mut data_stream, _) = data_listener.accept()?;
                        let start = offset as usize;
                        let slice = &data[start.min(data.len())..];
                        data_stream.write_all(slice)?;
                        data_stream.flush()?;
                        drop(data_stream);

                        control.write_all(b"226 transfer complete\r\n")?;
                        control.flush()?;
                        break;
                    }

                    control.write_all(b"500 unknown command\r\n")?;
                    control.flush()?;
                }
            }
            "QUIT" => {
                control.write_all(b"221 bye\r\n")?;
                return Ok(());
            }
            _ => control.write_all(b"500 unknown command\r\n")?,
        }

        control.flush()?;
    }
}

/// Read one command from the control connection.
fn read_command(reader: &mut TcpStream, buffer: &mut [u8; 1024]) -> Option<String> {
    let read = reader.read(buffer).ok()?;
    if read == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buffer[..read]).trim().to_string())
}
