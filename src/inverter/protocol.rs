/// Voltronic/Axpert serial protocol over TCP.
///
/// Frame format:  <CMD><CRC_HI><CRC_LO><CR>
/// Response:      (<DATA><CRC_HI><CRC_LO><CR>
///
/// CRC: CRC-16/XMODEM  polynomial 0x1021, init 0x0000, no reflection.
/// Voltronic firmware then bumps any CRC byte equal to '(' (0x28), CR (0x0D)
/// or LF (0x0A) by one, so the CRC never looks like a frame delimiter.
use anyhow::{bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Compute Voltronic CRC-16 for the given bytes.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC as sent on the wire by Voltronic devices (delimiter bytes bumped).
pub fn voltronic_crc(data: &[u8]) -> u16 {
    let bump = |b: u8| if matches!(b, 0x28 | 0x0D | 0x0A) { b + 1 } else { b };
    let crc = crc16(data);
    ((bump((crc >> 8) as u8) as u16) << 8) | bump((crc & 0xFF) as u8) as u16
}

/// Build a complete framed command: ASCII bytes + 2-byte CRC + 0x0D
pub fn build_command(cmd: &str) -> Vec<u8> {
    let bytes = cmd.as_bytes();
    let crc = voltronic_crc(bytes);
    let mut frame = Vec::with_capacity(bytes.len() + 3);
    frame.extend_from_slice(bytes);
    frame.push((crc >> 8) as u8);
    frame.push((crc & 0xFF) as u8);
    frame.push(0x0D);
    frame
}

/// Open a TCP connection with timeout.
pub async fn connect(ip: &str, port: u16) -> Result<TcpStream> {
    let addr = format!("{}:{}", ip, port);
    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| anyhow::anyhow!("Connect timeout to {}", addr))?
        .map_err(|e| anyhow::anyhow!("TCP connect error {}: {}", addr, e))?;
    Ok(stream)
}

/// Send a command and receive the response.
/// Returns the inner payload (strips leading '(' and trailing CRC+CR).
/// Validates the response CRC.
pub async fn send_command(stream: &mut TcpStream, cmd: &str) -> Result<String> {
    // Flush any stale data
    flush_socket(stream).await;

    let frame = build_command(cmd);
    stream.write_all(&frame).await?;

    // Read until 0x0D (CR)
    let raw = timeout(READ_TIMEOUT, read_until_cr(stream))
        .await
        .map_err(|_| anyhow::anyhow!("Read timeout for command {}", cmd))??;

    parse_response(&raw, cmd)
}

/// Read bytes until 0x0D is received, with a max of 2048 bytes.
async fn read_until_cr(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await?;
        if byte[0] == 0x0D {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > 2048 {
            bail!("Response too large (>2048 bytes)");
        }
    }
    Ok(buf)
}

/// Parse and validate the raw response bytes (without the trailing CR).
/// Returns the payload string without the leading '(' and without the 2-byte CRC.
fn parse_response(raw: &[u8], cmd: &str) -> Result<String> {
    if raw.len() < 3 {
        bail!("Response too short for command {}", cmd);
    }

    // Validate CRC: last 2 bytes are CRC of everything before them
    let payload_with_crc = raw;
    let data_part = &payload_with_crc[..payload_with_crc.len() - 2];
    let recv_crc = ((payload_with_crc[payload_with_crc.len() - 2] as u16) << 8)
        | payload_with_crc[payload_with_crc.len() - 1] as u16;
    let calc_crc = voltronic_crc(data_part);

    // Accept the plain CRC too, for firmwares that do not bump delimiter bytes
    if recv_crc != calc_crc && recv_crc != crc16(data_part) {
        bail!(
            "CRC mismatch for {}: received {:04X}, calculated {:04X}",
            cmd, recv_crc, calc_crc
        );
    }

    // Strip leading '(' if present
    let payload = if data_part.first() == Some(&b'(') {
        &data_part[1..]
    } else {
        data_part
    };

    String::from_utf8(payload.to_vec())
        .map_err(|e| anyhow::anyhow!("Non-UTF8 response for {}: {}", cmd, e))
}

/// Attempt to drain any pending data on the socket (non-blocking style).
async fn flush_socket(stream: &mut TcpStream) {
    let mut buf = [0u8; 256];
    let _ = timeout(Duration::from_millis(50), stream.read(&mut buf)).await;
}

/// Check if a response is ACK (command accepted) or NAK (rejected).
pub enum AckResult {
    Ack,
    Nak,
    Other(String),
}

pub fn parse_ack(response: &str) -> AckResult {
    if response.starts_with("ACK") {
        AckResult::Ack
    } else if response.starts_with("NAK") {
        AckResult::Nak
    } else {
        AckResult::Other(response.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpigs_frame_matches_known_bytes() {
        assert_eq!(build_command("QPIGS"), b"QPIGS\xB7\xA9\r".to_vec());
    }

    #[test]
    fn crc_never_contains_frame_delimiters() {
        // Find payloads whose plain CRC contains a delimiter byte
        for i in 0..5000 {
            let data = format!("(TEST {}", i);
            let crc = voltronic_crc(data.as_bytes());
            for b in [(crc >> 8) as u8, (crc & 0xFF) as u8] {
                assert!(!matches!(b, 0x28 | 0x0D | 0x0A), "{} -> {:04X}", data, crc);
            }
            // A response framed with the bumped CRC is accepted
            let mut raw = data.clone().into_bytes();
            raw.push((crc >> 8) as u8);
            raw.push((crc & 0xFF) as u8);
            assert_eq!(parse_response(&raw, "TEST").unwrap(), &data[1..]);
        }
    }
}
