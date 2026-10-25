use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Response to QID — Device serial number
/// Raw payload example: `92439200012345`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QidData {
    pub serial_number: String,
}

pub fn parse(payload: &str) -> Result<QidData> {
    let serial = payload.trim();
    if serial.is_empty() {
        bail!("QID: empty serial number");
    }
    Ok(QidData {
        serial_number: serial.to_string(),
    })
}
