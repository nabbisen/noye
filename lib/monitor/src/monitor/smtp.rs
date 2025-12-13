use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const SMTP_TIMEOUT: Duration = Duration::from_secs(4);

pub fn check_smtp(host: &str, port: usize) -> Result<u16, String> {
    let addr: String = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;

    stream
        .set_read_timeout(Some(SMTP_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(SMTP_TIMEOUT))
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;

    let banner = String::from_utf8_lossy(&buf[..n]);
    if !banner.starts_with("220") {
        return Err(format!("invalid banner: {}", banner));
    }

    stream
        .write_all(b"EHLO monitor\r\n")
        .map_err(|e| e.to_string())?;

    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    let reply = String::from_utf8_lossy(&buf[..n]);

    if !reply.starts_with("250") {
        return Err(format!("EHLO failed: {}", reply));
    }

    Ok(0)
}
