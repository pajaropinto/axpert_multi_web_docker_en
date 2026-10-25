/// QMOD — Query Device Mode
///
/// Response: a single letter
///   P = Power on      S = Standby       L = Line (grid)
///   B = Battery       F = Fault         H = Power saving
///   D = Shutdown      Y = Bypass        E = ECO
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct QmodData {
    /// Raw mode letter as returned by the inverter
    pub mode_code: String,
    /// Human-readable mode
    pub mode_name: String,
}

impl QmodData {
    pub fn is_battery(&self) -> bool {
        self.mode_code == "B"
    }
    pub fn is_fault(&self) -> bool {
        self.mode_code == "F"
    }
}

pub fn parse(payload: &str) -> Result<QmodData> {
    let code = payload.trim();
    let name = match code {
        "P" => "Power On",
        "S" => "Standby",
        "L" => "Line",
        "B" => "Battery",
        "F" => "Fault",
        "H" => "Power Saving",
        "D" => "Shutdown",
        "Y" => "Bypass",
        "E" => "ECO",
        _ => bail!("QMOD: unknown mode '{}'", code),
    };
    Ok(QmodData {
        mode_code: code.to_string(),
        mode_name: name.to_string(),
    })
}
