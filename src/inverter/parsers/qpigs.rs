/// QPIGS — Query Device General Status
///
/// Response format (space-separated):
/// BBB.B CC.C DDD.D EE.E FFFF GGGGG HHH III JJ.JJ KKK OOO TTTT EEEE UUU.U WW.WW PPPPP <status1_8bits> QQ VV MMMM <status2_3bits>
///
/// Indices:
///  0  grid_voltage          (V)
///  1  grid_frequency        (Hz)
///  2  ac_output_voltage     (V)
///  3  ac_output_frequency   (Hz)
///  4  ac_output_apparent_power   (VA)
///  5  ac_output_active_power     (W)
///  6  output_load_percent        (%)
///  7  bus_voltage                (V)
///  8  battery_voltage            (V)
///  9  battery_charging_current   (A)
/// 10  battery_capacity           (%)  [SOC]
/// 11  heatsink_temperature       (°C)
/// 12  pv1_input_current          (A)
/// 13  pv1_input_voltage          (V)
/// 14  battery_voltage_scc        (V)
/// 15  battery_discharge_current  (A)
/// 16  device_status_1            (8-char binary string b7..b0)
/// 17  battery_voltage_offset_fan (10mV units)
/// 18  eeprom_version
/// 19  pv1_charging_power         (W)
/// 20  device_status_2            (3-char binary string bits 8-10)
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QpigsData {
    // Grid
    pub grid_voltage: f64,
    pub grid_frequency: f64,

    // AC Output
    pub ac_output_voltage: f64,
    pub ac_output_frequency: f64,
    pub ac_output_apparent_power: i32,
    pub ac_output_active_power: i32,
    pub output_load_percent: i32,

    // Bus
    pub bus_voltage: i32,

    // Battery
    pub battery_voltage: f64,
    pub battery_charging_current: i32,
    pub battery_capacity: i32,
    pub battery_voltage_scc: f64,
    pub battery_discharge_current: i32,
    pub battery_voltage_offset_fan: i32, // ×10mV

    // Temperature
    pub heatsink_temperature: i32,

    // PV1
    pub pv1_input_current: f64,
    pub pv1_input_voltage: f64,
    pub pv1_charging_power: i32,

    // Misc
    pub eeprom_version: i32,

    // Device Status 1 bits (0=off/normal, 1=on/active)
    // String: b7 b6 b5 b4 b3 b2 b1 b0  (index 0 = b7, index 7 = b0)
    pub status_sbu_priority: u8,     // b7: SBU priority version added
    pub status_config_changed: u8,   // b6: Configuration changed
    pub status_scc_firmware_updated: u8, // b5: SCC firmware version updated
    pub status_load_on: u8,          // b4: Load is on
    pub status_battery_steady: u8,   // b3: Battery voltage steady while charging
    pub status_charging: u8,         // b2: Charging on (any source)
    pub status_scc_charging: u8,     // b1: SCC (solar) charging on
    pub status_ac_charging: u8,      // b0: AC (grid) charging on

    // Device Status 2 bits, string: b10 b9 b8
    pub status_charging_to_float: u8,  // b10: Charging to floating mode
    pub status_switch_on: u8,          // b9: Switch on
    pub status_dustproof_installed: u8, // b8: Dustproof installed
}

pub fn parse(payload: &str) -> Result<QpigsData> {
    let fields: Vec<&str> = payload.split_whitespace().collect();

    if fields.len() < 21 {
        bail!(
            "QPIGS: expected >=21 fields, got {} in: {}",
            fields.len(),
            payload
        );
    }

    // Status 1: 8-char binary string
    let st1 = fields[16];
    if st1.len() < 8 {
        bail!("QPIGS: device_status_1 too short: '{}'", st1);
    }
    let s1: Vec<u8> = st1.chars().map(|c| if c == '1' { 1 } else { 0 }).collect();

    // Status 2: 3-char binary string
    let st2 = fields[20];
    let s2: Vec<u8> = st2.chars().map(|c| if c == '1' { 1 } else { 0 }).collect();
    let s2_0 = s2.first().copied().unwrap_or(0);
    let s2_1 = s2.get(1).copied().unwrap_or(0);
    let s2_2 = s2.get(2).copied().unwrap_or(0);

    Ok(QpigsData {
        grid_voltage: pf(fields[0], "grid_voltage")?,
        grid_frequency: pf(fields[1], "grid_frequency")?,
        ac_output_voltage: pf(fields[2], "ac_output_voltage")?,
        ac_output_frequency: pf(fields[3], "ac_output_frequency")?,
        ac_output_apparent_power: pi(fields[4], "ac_output_apparent_power")?,
        ac_output_active_power: pi(fields[5], "ac_output_active_power")?,
        output_load_percent: pi(fields[6], "output_load_percent")?,
        bus_voltage: pi(fields[7], "bus_voltage")?,
        battery_voltage: pf(fields[8], "battery_voltage")?,
        battery_charging_current: pi(fields[9], "battery_charging_current")?,
        battery_capacity: pi(fields[10], "battery_capacity")?,
        heatsink_temperature: pi(fields[11], "heatsink_temperature")?,
        pv1_input_current: pf(fields[12], "pv1_input_current")?,
        pv1_input_voltage: pf(fields[13], "pv1_input_voltage")?,
        battery_voltage_scc: pf(fields[14], "battery_voltage_scc")?,
        battery_discharge_current: pi(fields[15], "battery_discharge_current")?,
        battery_voltage_offset_fan: pi(fields[17], "battery_voltage_offset_fan")?,
        eeprom_version: pi(fields[18], "eeprom_version")?,
        pv1_charging_power: pi(fields[19], "pv1_charging_power")?,
        // Status 1 bits, left to right = b7 to b0
        status_sbu_priority: s1[0],
        status_config_changed: s1[1],
        status_scc_firmware_updated: s1[2],
        status_load_on: s1[3],
        status_battery_steady: s1[4],
        status_charging: s1[5],
        status_scc_charging: s1[6],
        status_ac_charging: s1[7],
        // Status 2 bits, left to right = b10 to b8
        status_charging_to_float: s2_0,
        status_switch_on: s2_1,
        status_dustproof_installed: s2_2,
    })
}

#[inline]
fn pf(s: &str, field: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("QPIGS: cannot parse '{}' as f64 for field {}", s, field))
}

#[inline]
fn pi(s: &str, field: &str) -> Result<i32> {
    s.parse::<i32>()
        .map_err(|_| anyhow::anyhow!("QPIGS: cannot parse '{}' as i32 for field {}", s, field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bits_follow_the_protocol_order() {
        // b7..b0 = 00010110: load on (b4), charging (b2) with SCC (b1), no AC charging
        let d = parse(
            "230.0 50.0 230.0 50.0 0161 0108 003 405 54.00 012 100 0035 0003 340.5 54.10 00000 \
             00010110 00 00 00650 010",
        )
        .unwrap();
        assert_eq!(d.status_sbu_priority, 0);
        assert_eq!(d.status_config_changed, 0);
        assert_eq!(d.status_scc_firmware_updated, 0);
        assert_eq!(d.status_load_on, 1);
        assert_eq!(d.status_battery_steady, 0);
        assert_eq!(d.status_charging, 1);
        assert_eq!(d.status_scc_charging, 1);
        assert_eq!(d.status_ac_charging, 0);
        // b10 b9 b8 = 010: switched on
        assert_eq!(d.status_charging_to_float, 0);
        assert_eq!(d.status_switch_on, 1);
        assert_eq!(d.status_dustproof_installed, 0);
    }
}
