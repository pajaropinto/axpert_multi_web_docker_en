/// QPIRI — Query Device Rated Information
///
/// Response format (space-separated, 22+ fields):
///  0  grid_rating_voltage          (V)
///  1  grid_rating_current          (A)
///  2  ac_output_rating_voltage     (V)
///  3  ac_output_rating_frequency   (Hz)
///  4  ac_output_rating_current     (A)
///  5  ac_output_rating_apparent_power  (VA)
///  6  ac_output_rating_active_power    (W)
///  7  battery_rating_voltage           (V)
///  8  battery_recharge_voltage         (V)
///  9  battery_under_voltage            (V)
/// 10  battery_bulk_voltage             (V)
/// 11  battery_float_voltage            (V)
/// 12  battery_type     (0=AGM, 1=Flooded, 2=User)
/// 13  max_ac_charging_current     (A)
/// 14  max_charging_current        (A)
/// 15  input_voltage_range  (0=Appliance, 1=UPS)
/// 16  output_source_priority  (0=Utility, 1=Solar, 2=SBU)
/// 17  charger_source_priority (0=Utility, 1=Solar, 2=Solar+Utility, 3=Only solar)
/// 18  max_parallel_units
/// 19  machine_type   (00=Grid-tie, 01=Off-grid, 10=Hybrid)
/// 20  topology       (0=Transformerless, 1=Transformer)
/// 21  output_mode    (00=Single, 01=Parallel, 02=Phase1/3, 03=Phase2/3, 04=Phase3/3)
/// 22  battery_redischarge_voltage  (V)
/// 23  pv_ok_condition_parallel     (0=one unit, 1=all units)
/// 24  pv_power_balance             (0=PV max current, 1=PV max power)
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QpiriData {
    pub grid_rating_voltage: f64,
    pub grid_rating_current: f64,
    pub ac_output_rating_voltage: f64,
    pub ac_output_rating_frequency: f64,
    pub ac_output_rating_current: f64,
    pub ac_output_rating_apparent_power: i32,
    pub ac_output_rating_active_power: i32,
    pub battery_rating_voltage: f64,
    pub battery_recharge_voltage: f64,
    pub battery_under_voltage: f64,
    pub battery_bulk_voltage: f64,
    pub battery_float_voltage: f64,
    pub battery_type: i32,
    pub max_ac_charging_current: i32,
    pub max_charging_current: i32,
    pub input_voltage_range: i32,
    pub output_source_priority: i32,
    pub charger_source_priority: i32,
    pub max_parallel_units: i32,
    pub machine_type: String,
    pub topology: i32,
    pub output_mode: i32,
    pub battery_redischarge_voltage: f64,
    #[serde(default)]
    pub pv_ok_condition_parallel: Option<i32>,
    #[serde(default)]
    pub pv_power_balance: Option<i32>,
}

pub fn parse(payload: &str) -> Result<QpiriData> {
    let fields: Vec<&str> = payload.split_whitespace().collect();

    if fields.len() < 23 {
        bail!(
            "QPIRI: expected >=23 fields, got {} in: {}",
            fields.len(),
            payload
        );
    }

    Ok(QpiriData {
        grid_rating_voltage: pf(fields[0], "grid_rating_voltage")?,
        grid_rating_current: pf(fields[1], "grid_rating_current")?,
        ac_output_rating_voltage: pf(fields[2], "ac_output_rating_voltage")?,
        ac_output_rating_frequency: pf(fields[3], "ac_output_rating_frequency")?,
        ac_output_rating_current: pf(fields[4], "ac_output_rating_current")?,
        ac_output_rating_apparent_power: pi(fields[5], "ac_output_rating_apparent_power")?,
        ac_output_rating_active_power: pi(fields[6], "ac_output_rating_active_power")?,
        battery_rating_voltage: pf(fields[7], "battery_rating_voltage")?,
        battery_recharge_voltage: pf(fields[8], "battery_recharge_voltage")?,
        battery_under_voltage: pf(fields[9], "battery_under_voltage")?,
        battery_bulk_voltage: pf(fields[10], "battery_bulk_voltage")?,
        battery_float_voltage: pf(fields[11], "battery_float_voltage")?,
        battery_type: pi(fields[12], "battery_type")?,
        max_ac_charging_current: pi(fields[13], "max_ac_charging_current")?,
        max_charging_current: pi(fields[14], "max_charging_current")?,
        input_voltage_range: pi(fields[15], "input_voltage_range")?,
        output_source_priority: pi(fields[16], "output_source_priority")?,
        charger_source_priority: pi(fields[17], "charger_source_priority")?,
        max_parallel_units: pi(fields[18], "max_parallel_units")?,
        machine_type: fields[19].to_string(),
        topology: pi(fields[20], "topology")?,
        output_mode: pi(fields[21], "output_mode")?,
        battery_redischarge_voltage: pf(fields[22], "battery_redischarge_voltage")?,
        pv_ok_condition_parallel: fields.get(23).and_then(|s| s.parse().ok()),
        pv_power_balance: fields.get(24).and_then(|s| s.parse().ok()),
    })
}

#[inline]
fn pf(s: &str, field: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("QPIRI: cannot parse '{}' as f64 for {}", s, field))
}

#[inline]
fn pi(s: &str, field: &str) -> Result<i32> {
    s.parse::<i32>()
        .map_err(|_| anyhow::anyhow!("QPIRI: cannot parse '{}' as i32 for {}", s, field))
}
