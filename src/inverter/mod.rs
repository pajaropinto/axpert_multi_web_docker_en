pub mod config_cmds;
pub mod manager;
pub mod parsers;
pub mod protocol;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use parsers::{
    qflag::QflagData, qmod::QmodData, qpigs::QpigsData, qpigs2::Qpigs2Data, qpiri::QpiriData,
    qpiws::QpiwsData,
};

/// All data collected from one inverter in a single polling cycle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InverterData {
    pub id: u8,
    pub name: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub online: bool,

    // Set once at startup from QID
    pub serial: Option<String>,

    // Per-cycle query results
    pub qpigs: Option<QpigsData>,
    pub qpigs2: Option<Qpigs2Data>,
    pub qflag: Option<QflagData>,
    pub qpiri: Option<QpiriData>,
    pub qpiws: Option<QpiwsData>,
    pub qmod: Option<QmodData>,

    // Derived/calculated values
    pub calculated: Option<CalculatedData>,

    /// Consolidated status across all warnings+status bits:
    /// 0=normal, 1=warning, 2=fault
    pub inverter_status: u8,

    /// Battery runtime at the current discharge (minutes). The battery bank is
    /// shared, so this is the system value (set by the aggregator).
    pub battery_time_remaining_min: Option<f64>,

    /// Time to full charge at the current charge rate (minutes), system value.
    pub battery_time_to_full_min: Option<f64>,
}

/// Below this grid voltage the grid is considered absent (V).
const GRID_PRESENT_MIN_VOLTAGE: f64 = 100.0;

/// Values derived from QPIGS + QPIGS2 data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalculatedData {
    /// PV1 power = pv1_input_voltage × pv1_input_current (W)
    pub pv1_input_power: f64,
    /// PV2 power = pv2_input_voltage × pv2_input_current (W)
    pub pv2_input_power: f64,
    /// Total PV power for this inverter (W)
    pub pv_total_power: f64,
    /// Total PV current = pv1_input_current + pv2_input_current (A)
    pub pv_total_input_current: f64,
    /// Estimated grid input power from the energy balance:
    /// (AC output active + battery charge power − PV power) / efficiency
    /// (W, 0 when there is no grid or the inverter runs from battery)
    pub grid_input_power_estimate: f64,
    /// Net battery current: positive=charging, negative=discharging (A)
    pub battery_real_charge_current: f64,
    /// Net battery power: positive=charging, negative=discharging (W)
    pub battery_real_power: f64,
    /// Reactive power Q = sqrt(S² - P²) (VAR)
    pub ac_output_reactive_power: f64,
    /// Power factor (0-1)
    pub power_factor: f64,
}

impl InverterData {
    /// Compute calculated fields from QPIGS/QPIGS2/QMOD data.
    /// `efficiency_percent`: inverter efficiency used for the grid estimate.
    pub fn compute_calculated(&mut self, efficiency_percent: f64) {
        if let Some(ref q) = self.qpigs {
            let pv1_power = q.pv1_input_voltage * q.pv1_input_current;
            let pv2_power = self
                .qpigs2
                .as_ref()
                .map(|q2| q2.pv2_input_voltage * q2.pv2_input_current)
                .unwrap_or(0.0);

            let charge = q.battery_charging_current as f64;
            let discharge = q.battery_discharge_current as f64;
            let net_current = charge - discharge;
            let bat_power = net_current * q.battery_voltage;

            let s = q.ac_output_apparent_power as f64;
            let p = q.ac_output_active_power as f64;
            let reactive = (s * s - p * p).max(0.0).sqrt();
            let pf = if s > 0.0 { p / s } else { 1.0 };

            let pv_total = pv1_power + pv2_power;
            let pv_current = q.pv1_input_current
                + self
                    .qpigs2
                    .as_ref()
                    .map(|q2| q2.pv2_input_current)
                    .unwrap_or(0.0);

            // Whatever the load and the battery take that PV does not cover
            // must come from the grid, plus the conversion losses. In battery
            // mode (QMOD = B) or fault the grid supplies nothing.
            let off_grid = self
                .qmod
                .as_ref()
                .map(|m| m.is_battery() || m.is_fault())
                .unwrap_or(false);
            let grid_estimate = if q.grid_voltage >= GRID_PRESENT_MIN_VOLTAGE && !off_grid {
                (p + bat_power - pv_total).max(0.0) / (efficiency_percent / 100.0)
            } else {
                0.0
            };

            self.calculated = Some(CalculatedData {
                pv1_input_power: round2(pv1_power),
                pv2_input_power: round2(pv2_power),
                pv_total_power: round2(pv_total),
                pv_total_input_current: round2(pv_current),
                grid_input_power_estimate: round2(grid_estimate),
                battery_real_charge_current: round2(net_current),
                battery_real_power: round2(bat_power),
                ac_output_reactive_power: round2(reactive),
                power_factor: round2(pf),
            });

            // Consolidated inverter_status comes from the QPIWS warning bits,
            // and fault mode (QMOD = F). QPIGS status bits (switch on, float
            // charging, config changed) describe normal operation.
            let warn_status = self
                .qpiws
                .as_ref()
                .map(|w| w.inverter_status)
                .unwrap_or(0);
            let mode_status = if self.qmod.as_ref().map(|m| m.is_fault()).unwrap_or(false) {
                2
            } else {
                0
            };
            self.inverter_status = warn_status.max(mode_status);
        }
    }
}

#[inline]
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
