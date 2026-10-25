/// QFLAG — Query Device Flag Status
///
/// Response format: ExxxDxxx
///   E = Enable prefix, followed by enabled flag letters
///   D = Disable prefix, followed by disabled flag letters
///
/// Example: "EakxyDbjuvz"
///   Enabled:  a, k, x, y
///   Disabled: b, j, u, v, z
///
/// Flag definitions:
///   a = Buzzer
///   b = Overload bypass
///   j = Power saving
///   k = LCD timeout / escape to main screen
///   u = Overload restart
///   v = Over-temperature restart
///   x = Backlight
///   y = Alarm on primary source interrupt
///   z = Fault code record
///
/// Published values: 0 = Disable, 1 = Enable
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QflagData {
    pub flag_buzzer: u8,               // a
    pub flag_overload_bypass: u8,      // b
    pub flag_power_saving: u8,         // j
    pub flag_lcd_timeout_escape: u8,   // k
    pub flag_overload_restart: u8,     // u
    pub flag_overtemp_restart: u8,     // v
    pub flag_backlight: u8,            // x
    pub flag_alarm_primary_input: u8,  // y
    pub flag_fault_code_record: u8,    // z
}

pub fn parse(payload: &str) -> Result<QflagData> {
    // Find 'E' and 'D' positions
    let e_pos = payload.find('E');
    let d_pos = payload.find('D');

    if e_pos.is_none() && d_pos.is_none() {
        bail!("QFLAG: neither 'E' nor 'D' found in payload: '{}'", payload);
    }

    let enabled: &str = match (e_pos, d_pos) {
        (Some(e), Some(d)) if e < d => &payload[e + 1..d],
        (Some(e), None) => &payload[e + 1..],
        _ => "",
    };

    let disabled: &str = match d_pos {
        Some(d) => &payload[d + 1..],
        None => "",
    };

    Ok(QflagData {
        flag_buzzer: flag_val('a', enabled, disabled),
        flag_overload_bypass: flag_val('b', enabled, disabled),
        flag_power_saving: flag_val('j', enabled, disabled),
        flag_lcd_timeout_escape: flag_val('k', enabled, disabled),
        flag_overload_restart: flag_val('u', enabled, disabled),
        flag_overtemp_restart: flag_val('v', enabled, disabled),
        flag_backlight: flag_val('x', enabled, disabled),
        flag_alarm_primary_input: flag_val('y', enabled, disabled),
        flag_fault_code_record: flag_val('z', enabled, disabled),
    })
}

fn flag_val(letter: char, enabled: &str, disabled: &str) -> u8 {
    if enabled.contains(letter) {
        1
    } else if disabled.contains(letter) {
        0
    } else {
        // Not mentioned — default to disabled
        0
    }
}
