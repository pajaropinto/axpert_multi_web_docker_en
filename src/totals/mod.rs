/// System-wide totals and aggregations across all inverters.
use crate::config::BatteryConfig;
use crate::inverter::InverterData;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemTotals {
    // ── Battery ────────────────────────────────────────────────────────────────
    /// Average battery voltage across all online inverters (V)
    pub battery_voltage_avg: f64,
    /// Average battery SOC across all online inverters (%)
    pub battery_soc_avg: f64,
    /// Sum of all charging currents (A)
    pub battery_charging_current_total: f64,
    /// Sum of all discharge currents (A)
    pub battery_discharge_current_total: f64,
    /// Net battery current: positive=charging, negative=discharging (A)
    pub battery_real_charge_total: f64,
    /// Net battery power (W)
    pub battery_real_power_total: f64,

    // ── PV Solar ───────────────────────────────────────────────────────────────
    /// Total PV1 power across all inverters (W)
    pub pv1_power_total: f64,
    /// Total PV2 power across all inverters (W)
    pub pv2_power_total: f64,
    /// Total PV power (PV1+PV2, all inverters) (W)
    pub pv_power_total: f64,
    /// Total PV current (PV1+PV2, all inverters) (A)
    pub pv_input_current_total: f64,

    // ── AC Output ──────────────────────────────────────────────────────────────
    /// Sum of AC apparent power (VA)
    pub ac_output_apparent_power_total: f64,
    /// Sum of AC active power (W)
    pub ac_output_active_power_total: f64,
    /// Sum of AC reactive power (VAR)
    pub ac_output_reactive_power_total: f64,
    /// Average output load percentage (%)
    pub output_load_percent_avg: f64,

    // ── Grid ───────────────────────────────────────────────────────────────────
    /// Average grid voltage (V)
    pub grid_voltage_avg: f64,
    /// Average grid frequency (Hz)
    pub grid_frequency_avg: f64,
    /// Sum of per-inverter grid input power estimates (W)
    pub grid_input_power_estimate_total: f64,

    // ── Temperature ────────────────────────────────────────────────────────────
    /// Highest heatsink temperature across all inverters (°C)
    pub heatsink_temperature_max: f64,
    /// Average heatsink temperature (°C)
    pub heatsink_temperature_avg: f64,

    // ── System ─────────────────────────────────────────────────────────────────
    /// Number of inverters currently online
    pub inverters_online: u8,
    /// System power factor (total active / total apparent)
    pub system_power_factor: f64,
    /// Solar self-sufficiency today: share of today's consumption not taken
    /// from the grid, 1 − grid_import_today / consumption_today (%).
    /// Set from the energy counters by the aggregator (see `self_sufficiency`).
    pub solar_self_sufficiency_percent: f64,
    /// Consolidated system status: 0=normal, 1=warning, 2=fault
    pub system_status: u8,

    // ── Battery time estimates (system-level) ─────────────────────────────────
    /// Runtime until the safety margin is reached at the current discharge (minutes)
    pub battery_time_remaining_min: Option<f64>,
    /// Time to 100 % SOC at the current charging rate (minutes)
    pub battery_time_to_full_min: Option<f64>,
}

/// Compute system totals from all inverter data.
/// Sums and averages only use online inverters that returned QPIGS data.
pub fn compute(inverters: &[InverterData], battery: &BatteryConfig) -> SystemTotals {
    let online: Vec<&InverterData> = inverters
        .iter()
        .filter(|i| i.online && i.qpigs.is_some())
        .collect();
    let count = online.len() as f64;

    if online.is_empty() {
        return SystemTotals {
            system_status: 2, // all inverters offline = fault
            ..Default::default()
        };
    }

    // Accumulators
    let mut bat_v_sum = 0.0f64;
    let mut bat_soc_sum = 0.0f64;
    let mut bat_charge_sum = 0.0f64;
    let mut bat_discharge_sum = 0.0f64;
    let mut pv1_power_sum = 0.0f64;
    let mut pv2_power_sum = 0.0f64;
    let mut pv_current_sum = 0.0f64;
    let mut grid_estimate_sum = 0.0f64;
    let mut ac_apparent_sum = 0.0f64;
    let mut ac_active_sum = 0.0f64;
    let mut ac_reactive_sum = 0.0f64;
    let mut load_pct_sum = 0.0f64;
    let mut grid_v_sum = 0.0f64;
    let mut grid_f_sum = 0.0f64;
    let mut temp_max = f64::NEG_INFINITY;
    let mut temp_sum = 0.0f64;
    let mut status_max: u8 = 0;

    for inv in &online {
        status_max = status_max.max(inv.inverter_status);

        if let Some(ref q) = inv.qpigs {
            bat_v_sum += q.battery_voltage;
            bat_soc_sum += q.battery_capacity as f64;
            bat_charge_sum += q.battery_charging_current as f64;
            bat_discharge_sum += q.battery_discharge_current as f64;
            ac_apparent_sum += q.ac_output_apparent_power as f64;
            ac_active_sum += q.ac_output_active_power as f64;
            load_pct_sum += q.output_load_percent as f64;
            grid_v_sum += q.grid_voltage;
            grid_f_sum += q.grid_frequency;
            temp_sum += q.heatsink_temperature as f64;
            temp_max = temp_max.max(q.heatsink_temperature as f64);
        }

        if let Some(ref c) = inv.calculated {
            pv1_power_sum += c.pv1_input_power;
            pv2_power_sum += c.pv2_input_power;
            ac_reactive_sum += c.ac_output_reactive_power;
            pv_current_sum += c.pv_total_input_current;
            grid_estimate_sum += c.grid_input_power_estimate;
        }
    }

    let bat_v_avg = bat_v_sum / count;
    let bat_soc_avg = bat_soc_sum / count;
    let net_charge = bat_charge_sum - bat_discharge_sum;
    let net_power = net_charge * bat_v_avg;
    let pv_total = pv1_power_sum + pv2_power_sum;

    let pf = if ac_apparent_sum > 0.0 {
        ac_active_sum / ac_apparent_sum
    } else {
        1.0
    };

    let temp_max_val = if temp_max == f64::NEG_INFINITY {
        0.0
    } else {
        temp_max
    };

    // Battery time estimates (one shared bank). In parallel one inverter may
    // charge from its PV while another discharges, so the net bank current
    // decides: the usable charge is what is above the safety margin.
    let time_remaining = if net_charge < 0.0 {
        let usable_ah =
            battery.capacity_ah * (bat_soc_avg - battery.safety_margin_percent).max(0.0) / 100.0;
        Some(usable_ah / -net_charge * 60.0)
    } else {
        None
    };

    let time_to_full = if net_charge > 0.0 {
        let missing_ah = battery.capacity_ah * (100.0 - bat_soc_avg).max(0.0) / 100.0;
        Some(missing_ah / net_charge * 60.0)
    } else {
        None
    };

    // If not all inverters are online, at least warn
    let inv_count_all = inverters.len() as u8;
    let inv_online = online.len() as u8;
    if inv_online < inv_count_all && status_max < 1 {
        status_max = 1; // at least warning if some are offline
    }

    SystemTotals {
        battery_voltage_avg: round2(bat_v_avg),
        battery_soc_avg: round2(bat_soc_avg),
        battery_charging_current_total: round2(bat_charge_sum),
        battery_discharge_current_total: round2(bat_discharge_sum),
        battery_real_charge_total: round2(net_charge),
        battery_real_power_total: round2(net_power),
        pv1_power_total: round2(pv1_power_sum),
        pv2_power_total: round2(pv2_power_sum),
        pv_power_total: round2(pv_total),
        pv_input_current_total: round2(pv_current_sum),
        ac_output_apparent_power_total: round2(ac_apparent_sum),
        ac_output_active_power_total: round2(ac_active_sum),
        ac_output_reactive_power_total: round2(ac_reactive_sum),
        output_load_percent_avg: round2(load_pct_sum / count),
        grid_voltage_avg: round2(grid_v_sum / count),
        grid_frequency_avg: round2(grid_f_sum / count),
        grid_input_power_estimate_total: round2(grid_estimate_sum),
        heatsink_temperature_max: round2(temp_max_val),
        heatsink_temperature_avg: round2(temp_sum / count),
        inverters_online: inv_online,
        system_power_factor: round2(pf),
        solar_self_sufficiency_percent: 0.0,
        system_status: status_max,
        battery_time_remaining_min: time_remaining.map(round2),
        battery_time_to_full_min: time_to_full.map(round2),
    }
}

/// Today's solar self-sufficiency (%) from the energy counters (Wh).
pub fn self_sufficiency(consumption_today: f64, grid_import_today: f64) -> f64 {
    if consumption_today <= 0.0 {
        return 100.0;
    }
    round2(((1.0 - grid_import_today / consumption_today) * 100.0).clamp(0.0, 100.0))
}

#[inline]
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inverter::{QpigsData, Qpigs2Data};

    #[allow(clippy::too_many_arguments)]
    fn inverter(
        id: u8,
        grid_v: f64,
        apparent: i32,
        active: i32,
        charge: i32,
        discharge: i32,
        pv1: (f64, f64),
        pv2: Option<(f64, f64)>,
    ) -> InverterData {
        let mut inv = InverterData {
            id,
            online: true,
            qpigs: Some(QpigsData {
                grid_voltage: grid_v,
                grid_frequency: 50.0,
                ac_output_apparent_power: apparent,
                ac_output_active_power: active,
                battery_voltage: 52.0,
                battery_charging_current: charge,
                battery_discharge_current: discharge,
                battery_capacity: 80,
                pv1_input_voltage: pv1.0,
                pv1_input_current: pv1.1,
                status_switch_on: 1,
                ..Default::default()
            }),
            qpigs2: pv2.map(|(v, i)| Qpigs2Data {
                pv2_input_voltage: v,
                pv2_input_current: i,
                ..Default::default()
            }),
            ..Default::default()
        };
        inv.compute_calculated(100.0);
        inv
    }

    fn battery() -> BatteryConfig {
        BatteryConfig { capacity_ah: 160.0, safety_margin_percent: 20.0, co2_factor_kg_per_kwh: 0.233 }
    }

    #[test]
    fn per_inverter_pv_current_grid_estimate_and_power_factor() {
        // 2000 W of PV covers the 1000 W load and the 520 W battery charge: no grid
        let a = inverter(1, 230.0, 1200, 1000, 10, 0, (300.0, 5.0), Some((250.0, 2.0)));
        let c = a.calculated.as_ref().unwrap();
        assert_eq!(c.pv_total_input_current, 7.0);
        assert_eq!(c.grid_input_power_estimate, 0.0);
        assert_eq!(c.power_factor, 0.83); // 1000 / 1200, rounded

        // No PV, idle battery: the whole load comes from the grid
        let b = inverter(2, 230.0, 2000, 1800, 0, 0, (0.0, 0.0), None);
        assert_eq!(b.calculated.as_ref().unwrap().grid_input_power_estimate, 1800.0);

        // No grid voltage: estimate is zero whatever the balance says
        let c = inverter(3, 0.0, 2000, 1800, 0, 0, (0.0, 0.0), None);
        assert_eq!(c.calculated.as_ref().unwrap().grid_input_power_estimate, 0.0);
    }

    #[test]
    fn system_totals() {
        let invs = [
            inverter(1, 230.0, 1200, 1000, 10, 0, (300.0, 5.0), Some((250.0, 2.0))),
            inverter(2, 230.0, 2000, 1800, 0, 0, (0.0, 0.0), None),
        ];
        let t = compute(&invs, &battery());
        assert_eq!(t.pv_input_current_total, 7.0);
        assert_eq!(t.pv_power_total, 2000.0);
        assert_eq!(t.grid_input_power_estimate_total, 1800.0);
        assert_eq!(t.ac_output_active_power_total, 2800.0);
        assert_eq!(t.ac_output_apparent_power_total, 3200.0);
        assert_eq!(t.system_power_factor, 0.88); // 2800 / 3200 = 0.875
        assert_eq!(t.battery_real_charge_total, 10.0);
        assert_eq!(t.battery_real_power_total, 520.0);
        assert_eq!(t.grid_voltage_avg, 230.0);
    }

    #[test]
    fn normal_qpigs_status_bits_do_not_raise_warning() {
        let invs = [inverter(1, 230.0, 1000, 900, 0, 0, (0.0, 0.0), None)];
        assert_eq!(invs[0].inverter_status, 0);
        assert_eq!(compute(&invs, &battery()).system_status, 0);
    }

    #[test]
    fn battery_times_use_shared_bank_and_safety_margin() {
        // SOC 80 %, margin 20 % → 60 % of 160 Ah = 96 Ah usable; 2 × 10 A discharge
        let invs = [
            inverter(1, 230.0, 500, 400, 0, 10, (0.0, 0.0), None),
            inverter(2, 230.0, 500, 400, 0, 10, (0.0, 0.0), None),
        ];
        let t = compute(&invs, &battery());
        assert_eq!(t.battery_time_remaining_min, Some(288.0)); // 96 Ah / 20 A
        assert_eq!(t.battery_time_to_full_min, None);

        // Charging 10 A: 20 % of 160 Ah missing = 32 Ah
        let invs = [inverter(1, 230.0, 500, 400, 10, 0, (0.0, 0.0), None)];
        let t = compute(&invs, &battery());
        assert_eq!(t.battery_time_to_full_min, Some(192.0));
        assert_eq!(t.battery_time_remaining_min, None);
    }

    #[test]
    fn self_sufficiency_from_daily_energy() {
        assert_eq!(self_sufficiency(1000.0, 300.0), 70.0);
        assert_eq!(self_sufficiency(1000.0, 0.0), 100.0);
        assert_eq!(self_sufficiency(0.0, 0.0), 100.0);
    }

    #[test]
    fn qmod_battery_mode_efficiency_and_fault() {
        use crate::inverter::parsers::qmod;
        let mut inv = inverter(1, 230.0, 2000, 1800, 0, 0, (0.0, 0.0), None);

        // Line mode, 90 % efficiency: 1800 W load / 0.9 from the grid
        inv.qmod = Some(qmod::parse("L").unwrap());
        inv.compute_calculated(90.0);
        assert_eq!(inv.calculated.as_ref().unwrap().grid_input_power_estimate, 2000.0);

        // Battery mode: the grid supplies nothing even if its voltage is present
        inv.qmod = Some(qmod::parse("B").unwrap());
        inv.compute_calculated(90.0);
        assert_eq!(inv.calculated.as_ref().unwrap().grid_input_power_estimate, 0.0);

        // Fault mode raises the status to fault
        inv.qmod = Some(qmod::parse("F").unwrap());
        inv.compute_calculated(90.0);
        assert_eq!(inv.inverter_status, 2);
        assert_eq!(compute(&[inv], &battery()).system_status, 2);
    }

    #[test]
    fn parallel_mixed_charge_and_discharge_uses_net_current() {
        // Inverter 1 charges 5 A from its PV, inverter 2 discharges 25 A:
        // the bank loses 20 A net → 96 Ah usable / 20 A = 288 min
        let invs = [
            inverter(1, 230.0, 500, 400, 5, 0, (300.0, 2.0), None),
            inverter(2, 230.0, 1500, 1300, 0, 25, (0.0, 0.0), None),
        ];
        let t = compute(&invs, &battery());
        assert_eq!(t.battery_time_remaining_min, Some(288.0));
        assert_eq!(t.battery_time_to_full_min, None);
    }

    #[test]
    fn inverter_without_data_does_not_distort_averages() {
        let silent = InverterData { id: 3, online: true, ..Default::default() };
        let invs = [
            inverter(1, 230.0, 500, 400, 0, 0, (0.0, 0.0), None),
            inverter(2, 230.0, 500, 400, 0, 0, (0.0, 0.0), None),
            silent,
        ];
        let t = compute(&invs, &battery());
        assert_eq!(t.battery_voltage_avg, 52.0);
        assert_eq!(t.battery_soc_avg, 80.0);
        assert_eq!(t.grid_voltage_avg, 230.0);
        assert_eq!(t.inverters_online, 2);
        assert_eq!(t.system_status, 1); // one inverter not answering → warning
    }
}

