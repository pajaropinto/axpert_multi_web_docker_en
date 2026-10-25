/// QPIGS2 — Query Device General Status 2 (Extended, PV2 data)
///
/// Response format (space-separated), Axpert MAX protocol:
/// BB.B CCC.C DDDDD
///
///  0  pv2_input_current   (A)
///  1  pv2_input_voltage   (V)
///  2  pv2_charging_power  (W)
///
/// Some documents and firmwares give voltage first. A working PV string
/// always has a higher voltage (tens to hundreds of V) than current (tens of
/// A at most), so the larger of the first two values is taken as the voltage:
/// this reads both orders correctly. With no PV both are ~0 either way.
///
/// If the inverter returns additional fields they are ignored; fewer fields
/// fail the parse and the manager leaves PV2 empty.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Qpigs2Data {
    pub pv2_input_voltage: f64,
    pub pv2_input_current: f64,
    pub pv2_charging_power: i32,
}

pub fn parse(payload: &str) -> Result<Qpigs2Data> {
    let fields: Vec<&str> = payload.split_whitespace().collect();

    if fields.len() < 3 {
        bail!(
            "QPIGS2: expected >=3 fields, got {} in: {}",
            fields.len(),
            payload
        );
    }

    let a = pf(fields[0], "pv2 field 0")?;
    let b = pf(fields[1], "pv2 field 1")?;
    let (current, voltage) = if a <= b { (a, b) } else { (b, a) };

    Ok(Qpigs2Data {
        pv2_input_voltage: voltage,
        pv2_input_current: current,
        pv2_charging_power: pi(fields[2], "pv2_charging_power")?,
    })
}

#[inline]
fn pf(s: &str, field: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("QPIGS2: cannot parse '{}' as f64 for {}", s, field))
}

#[inline]
fn pi(s: &str, field: &str) -> Result<i32> {
    s.parse::<i32>()
        .map_err(|_| anyhow::anyhow!("QPIGS2: cannot parse '{}' as i32 for {}", s, field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_first_and_voltage_first_firmwares() {
        for payload in ["03.1 327.3 01026", "327.3 03.1 01026"] {
            let d = parse(payload).unwrap();
            assert_eq!(d.pv2_input_current, 3.1, "{}", payload);
            assert_eq!(d.pv2_input_voltage, 327.3, "{}", payload);
            assert_eq!(d.pv2_charging_power, 1026);
        }
        // Voltage present, no current (MPPT idle) — both orders
        for payload in ["00.0 350.0 00000", "350.0 00.0 00000"] {
            let d = parse(payload).unwrap();
            assert_eq!((d.pv2_input_current, d.pv2_input_voltage), (0.0, 350.0));
        }
    }
}
