//! A small FTP client, enough to drive the chunk list.
//!
//! IDM fetches a file over FTP with the same chunk list it uses for HTTP:
//! every chunk opens its own data connection, asks for the file with `RETR`,
//! reads the bytes of its range and closes the connection. A chunk that does
//! not start at the first byte asks the server to start there with `REST`.
//!
//! Evidence: `FTPConnection::StartGetChunk`, `InsInChunksList` and
//! `SendRetr` (assertion strings in `IDMan.exe`). See `ANALYSIS.md`.

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use crate::transfer::ByteStream;

#[cfg(test)]
pub mod server;

/// The greeting the server sends when the control connection is open.
const GREETING: &str = "220";

/// An FTP control connection.
pub struct Ftp {
    reader: BufReader<ReadHalf<TcpStream>>,
    writer: WriteHalf<TcpStream>,
    user: String,
    password: String,
}

impl Ftp {
    /// Open the control connection and log in.
    pub async fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
    ) -> Result<Self, String> {
        let stream = TcpStream::connect((host, port))
            .await
            .map_err(|e| format!("cannot reach the server: {e}"))?;
        let (reader, writer) = tokio::io::split(stream);

        let mut ftp = Ftp {
            reader: BufReader::new(reader),
            writer,
            user: user.to_string(),
            password: password.to_string(),
        };

        let greeting = ftp.read_reply().await?;
        if !greeting.starts_with(GREETING) {
            return Err(format!("the server refused the connection: {greeting}"));
        }

        ftp.login().await?;
        ftp.set_binary().await?;
        Ok(ftp)
    }

    /// Send a command and read its reply.
    async fn command(&mut self, command: &str) -> Result<String, String> {
        self.writer
            .write_all(format!("{command}\r\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.writer.flush().await.map_err(|e| e.to_string())?;
        self.read_reply().await
    }

    /// Read one reply, which may run over several lines.
    async fn read_reply(&mut self) -> Result<String, String> {
        let mut reply = String::new();
        loop {
            let mut line = String::new();
            let read = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;
            if read == 0 {
                return Err("the server closed the connection".to_string());
            }
            let more = line.as_bytes().get(3) == Some(&b'-');
            reply.push_str(&line);
            if !more {
                break;
            }
        }
        Ok(reply)
    }

    /// `USER` and `PASS`.
    async fn login(&mut self) -> Result<(), String> {
        let reply = self.command(&format!("USER {}", self.user)).await?;
        if reply.starts_with("230") {
            // The server let the user in without a password.
            return Ok(());
        }
        if !reply.starts_with("331") {
            return Err(format!("the server refused the user: {reply}"));
        }
        let reply = self.command(&format!("PASS {}", self.password)).await?;
        if reply.starts_with("230") || reply.starts_with("202") {
            return Ok(());
        }
        Err(format!("the server refused the password: {reply}"))
    }

    /// Ask for the file in binary form.
    async fn set_binary(&mut self) -> Result<(), String> {
        let reply = self.command("TYPE I").await?;
        if reply.starts_with("200") {
            return Ok(());
        }
        Err(format!("the server refused the binary mode: {reply}"))
    }

    /// The address of the data connection the server offers.
    async fn passive(&mut self) -> Result<(String, u16), String> {
        let reply = self.command("PASV").await?;
        if !reply.starts_with("227") {
            return Err(format!("the server refused the passive mode: {reply}"));
        }
        let open = reply
            .find('(')
            .ok_or_else(|| format!("the server sent no address: {reply}"))?;
        let close = reply[open + 1..]
            .find(')')
            .ok_or_else(|| format!("the server sent no address: {reply}"))?;
        let numbers: Vec<u16> = reply[open + 1..open + 1 + close]
            .split(',')
            .filter_map(|part| part.trim().parse().ok())
            .collect();
        if numbers.len() != 6 {
            return Err(format!("the server sent no address: {reply}"));
        }
        let host = format!(
            "{}.{}.{}.{}",
            numbers[0], numbers[1], numbers[2], numbers[3]
        );
        let port = numbers[4] * 256 + numbers[5];
        Ok((host, port))
    }
}

/// How big the file is, in bytes.
pub async fn size(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    path: &str,
) -> Result<u64, String> {
    let mut ftp = Ftp::connect(host, port, user, password).await?;
    let reply = ftp.command(&format!("SIZE {path}")).await?;
    if !reply.starts_with("213") {
        return Err(format!("the server did not tell the size: {reply}"));
    }
    reply[3..]
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("the server sent a size we cannot read: {reply}"))
}

/// Open a transfer that yields the bytes of a range.
///
/// `to` of `None` means "to the end of the file". The control connection
/// stays open while the bytes are read: the server ends the transfer on the
/// control channel, and closing it too early would cut the transfer short.
pub async fn open_range(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    path: &str,
    from: u64,
    to: Option<u64>,
) -> Result<ByteStream, String> {
    let mut ftp = Ftp::connect(host, port, user, password).await?;

    let (data_host, data_port) = ftp.passive().await?;
    let data = TcpStream::connect((data_host, data_port))
        .await
        .map_err(|e| format!("cannot open the data connection: {e}"))?;

    // A range that does not start at the first byte asks the server to start
    // there.
    if from > 0 {
        let reply = ftp.command(&format!("REST {from}")).await?;
        if !reply.starts_with("350") {
            return Err(format!(
                "the server cannot start the file at an offset: {reply}"
            ));
        }
    }

    let reply = ftp.command(&format!("RETR {path}")).await?;
    if !(reply.starts_with("150") || reply.starts_with("125")) {
        return Err(format!("the server refused to send the file: {reply}"));
    }

    let remaining = to.map(|to| to - from + 1);
    let stream = futures_util::stream::unfold(
        (data, ftp, remaining),
        |(mut data, ftp, remaining)| async move {
            if remaining == Some(0) {
                return None;
            }
            let mut buffer = vec![0u8; 16 * 1024];
            let read = match data.read(&mut buffer).await {
                Ok(read) => read,
                Err(_) => return None,
            };
            if read == 0 {
                return None;
            }
            buffer.truncate(read);

            // The server sends the file from the offset to its end; only the
            // bytes of the range are ours.
            let taken = match remaining {
                Some(left) => buffer.len().min(left as usize),
                None => buffer.len(),
            };
            buffer.truncate(taken);
            let remaining = remaining.map(|left| left - taken as u64);

            Some((Ok(buffer), (data, ftp, remaining)))
        },
    );

    Ok(Box::pin(stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn sample_data() -> Vec<u8> {
        (0..4096u32).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test]
    async fn the_size_of_the_file_is_read() {
        let data = sample_data();
        let server = server::TestServer::start(data.clone());

        let size = size("127.0.0.1", server.port, "user", "secret", server::PATH)
            .await
            .expect("size");

        assert_eq!(size, data.len() as u64);
    }

    #[tokio::test]
    async fn a_range_is_read_from_the_server() {
        let data = sample_data();
        let server = server::TestServer::start(data.clone());

        let mut stream = open_range(
            "127.0.0.1",
            server.port,
            "user",
            "secret",
            server::PATH,
            0,
            Some(1023),
        )
        .await
        .expect("range");

        let mut got = Vec::new();
        while let Some(item) = stream.next().await {
            got.extend(item.expect("bytes"));
        }

        assert_eq!(got, data[..1024].to_vec());
    }

    #[tokio::test]
    async fn a_range_that_starts_later_is_read_from_that_offset() {
        let data = sample_data();
        let server = server::TestServer::start(data.clone());

        let mut stream = open_range(
            "127.0.0.1",
            server.port,
            "user",
            "secret",
            server::PATH,
            2048,
            Some(3071),
        )
        .await
        .expect("range");

        let mut got = Vec::new();
        while let Some(item) = stream.next().await {
            got.extend(item.expect("bytes"));
        }

        assert_eq!(got, data[2048..3072].to_vec());
    }

    #[tokio::test]
    async fn a_range_to_the_end_is_read() {
        let data = sample_data();
        let server = server::TestServer::start(data.clone());

        let mut stream = open_range(
            "127.0.0.1",
            server.port,
            "user",
            "secret",
            server::PATH,
            4000,
            None,
        )
        .await
        .expect("range");

        let mut got = Vec::new();
        while let Some(item) = stream.next().await {
            got.extend(item.expect("bytes"));
        }

        assert_eq!(got, data[4000..].to_vec());
    }

    #[tokio::test]
    async fn a_server_that_is_not_there_is_reported() {
        let error = size("127.0.0.1", 1, "user", "secret", server::PATH)
            .await
            .expect_err("must fail");
        assert!(!error.is_empty());
    }
}
