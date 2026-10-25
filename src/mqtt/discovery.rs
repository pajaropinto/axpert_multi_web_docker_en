/// Home Assistant MQTT autodiscovery payload builder.
///
/// For each sensor, we publish a config message to:
///   homeassistant/sensor/{device_id}/{object_id}/config
///
/// And states to:
///   {topic_prefix}/inv{id}/{object_id}   (per-inverter)
///   {topic_prefix}/totals/{object_id}    (system totals)
///   {topic_prefix}/energy/{object_id}    (energy accumulators)
use serde_json::json;

pub struct SensorMeta {
    pub object_id: &'static str,
    pub name: &'static str,
    pub unit: Option<&'static str>,
    pub device_class: Option<&'static str>,
    pub state_class: Option<&'static str>,
    pub icon: Option<&'static str>,
    pub component: &'static str, // "sensor" | "binary_sensor"
}

// ─── Per-inverter sensor definitions ─────────────────────────────────────────

pub fn inverter_sensors() -> Vec<SensorMeta> {
    vec![
        // QPIGS — Grid
        SensorMeta { object_id: "grid_voltage", name: "Grid Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:transmission-tower"), component: "sensor" },
        SensorMeta { object_id: "grid_frequency", name: "Grid Frequency", unit: Some("Hz"), device_class: Some("frequency"), state_class: Some("measurement"), icon: Some("mdi:sine-wave"), component: "sensor" },
        // QPIGS — AC Output
        SensorMeta { object_id: "ac_output_voltage", name: "AC Output Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "ac_output_frequency", name: "AC Output Frequency", unit: Some("Hz"), device_class: Some("frequency"), state_class: Some("measurement"), icon: Some("mdi:sine-wave"), component: "sensor" },
        SensorMeta { object_id: "ac_output_apparent_power", name: "AC Output Apparent Power", unit: Some("VA"), device_class: Some("apparent_power"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "ac_output_active_power", name: "AC Output Active Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "ac_output_reactive_power", name: "AC Output Reactive Power", unit: Some("var"), device_class: Some("reactive_power"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "output_load_percent", name: "Output Load", unit: Some("%"), device_class: None, state_class: Some("measurement"), icon: Some("mdi:gauge"), component: "sensor" },
        SensorMeta { object_id: "power_factor", name: "Power Factor", unit: None, device_class: Some("power_factor"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "inverter_mode", name: "Inverter Mode", unit: None, device_class: None, state_class: None, icon: Some("mdi:state-machine"), component: "sensor" },
        SensorMeta { object_id: "grid_input_power_estimate", name: "Grid Estimate Input Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "pv_total_input_current", name: "PV Input Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        // QPIGS — Bus
        SensorMeta { object_id: "bus_voltage", name: "Bus Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:flash"), component: "sensor" },
        // QPIGS — Battery
        SensorMeta { object_id: "battery_voltage", name: "Battery Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:battery"), component: "sensor" },
        SensorMeta { object_id: "battery_voltage_scc", name: "Battery Voltage (SCC)", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:battery-charging"), component: "sensor" },
        SensorMeta { object_id: "battery_charging_current", name: "Battery Charging Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:current-dc"), component: "sensor" },
        SensorMeta { object_id: "battery_discharge_current", name: "Battery Discharge Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:current-dc"), component: "sensor" },
        SensorMeta { object_id: "battery_real_charge_current", name: "Battery Net Charge Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "battery_real_power", name: "Battery Net Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:battery-charging-outline"), component: "sensor" },
        SensorMeta { object_id: "battery_capacity", name: "Battery SOC", unit: Some("%"), device_class: Some("battery"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "battery_voltage_offset_fan", name: "Battery Voltage Offset Fan", unit: Some("mV"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:fan"), component: "sensor" },
        // QPIGS — Temperature
        SensorMeta { object_id: "heatsink_temperature", name: "Heatsink Temperature", unit: Some("°C"), device_class: Some("temperature"), state_class: Some("measurement"), icon: None, component: "sensor" },
        // QPIGS — PV1
        SensorMeta { object_id: "pv1_input_voltage", name: "PV1 Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv1_input_current", name: "PV1 Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv1_input_power", name: "PV1 Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv1_charging_power", name: "PV1 Charging Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel-large"), component: "sensor" },
        // QPIGS2 — PV2
        SensorMeta { object_id: "pv2_input_voltage", name: "PV2 Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv2_input_current", name: "PV2 Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv2_input_power", name: "PV2 Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv2_charging_power", name: "PV2 Charging Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel-large"), component: "sensor" },
        // QPIGS — Misc
        SensorMeta { object_id: "eeprom_version", name: "EEPROM Version", unit: None, device_class: None, state_class: None, icon: Some("mdi:chip"), component: "sensor" },
        // Battery time estimates
        SensorMeta { object_id: "battery_time_remaining_min", name: "Battery Time Remaining", unit: Some("min"), device_class: Some("duration"), state_class: Some("measurement"), icon: Some("mdi:battery-clock"), component: "sensor" },
        SensorMeta { object_id: "battery_time_to_full_min", name: "Battery Time to Full", unit: Some("min"), device_class: Some("duration"), state_class: Some("measurement"), icon: Some("mdi:battery-clock-outline"), component: "sensor" },
        // Consolidated inverter status
        SensorMeta { object_id: "inverter_status", name: "Inverter Status", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:information"), component: "sensor" },
        // QPIGS Status bits (0/1)
        SensorMeta { object_id: "status_ac_charging", name: "AC Charging", unit: None, device_class: Some("battery_charging"), state_class: None, icon: None, component: "binary_sensor" },
        SensorMeta { object_id: "status_scc_charging", name: "Solar Charging", unit: None, device_class: Some("battery_charging"), state_class: None, icon: Some("mdi:solar-power"), component: "binary_sensor" },
        SensorMeta { object_id: "status_load_on", name: "Load On", unit: None, device_class: Some("power"), state_class: None, icon: Some("mdi:power"), component: "binary_sensor" },
        SensorMeta { object_id: "status_battery_steady", name: "Battery Steady", unit: None, device_class: None, state_class: None, icon: Some("mdi:battery-check"), component: "binary_sensor" },
        SensorMeta { object_id: "status_charging", name: "Charging Active", unit: None, device_class: Some("battery_charging"), state_class: None, icon: None, component: "binary_sensor" },
        SensorMeta { object_id: "status_config_changed", name: "Config Changed", unit: None, device_class: None, state_class: None, icon: Some("mdi:cog-sync"), component: "binary_sensor" },
        SensorMeta { object_id: "status_charging_to_float", name: "Charging to Float", unit: None, device_class: None, state_class: None, icon: Some("mdi:battery-charging-100"), component: "binary_sensor" },
        // QFLAG bits (0/1)
        SensorMeta { object_id: "flag_buzzer", name: "Buzzer", unit: None, device_class: None, state_class: None, icon: Some("mdi:volume-high"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_overload_bypass", name: "Overload Bypass", unit: None, device_class: None, state_class: None, icon: Some("mdi:shield-check"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_power_saving", name: "Power Saving", unit: None, device_class: None, state_class: None, icon: Some("mdi:leaf"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_lcd_timeout_escape", name: "LCD Timeout Escape", unit: None, device_class: None, state_class: None, icon: Some("mdi:monitor"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_overload_restart", name: "Overload Auto-Restart", unit: None, device_class: None, state_class: None, icon: Some("mdi:restart"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_overtemp_restart", name: "Overtemp Auto-Restart", unit: None, device_class: None, state_class: None, icon: Some("mdi:restart-alert"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_backlight", name: "LCD Backlight", unit: None, device_class: None, state_class: None, icon: Some("mdi:brightness-5"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_alarm_primary_input", name: "Alarm Primary Input", unit: None, device_class: None, state_class: None, icon: Some("mdi:bell"), component: "binary_sensor" },
        SensorMeta { object_id: "flag_fault_code_record", name: "Fault Code Record", unit: None, device_class: None, state_class: None, icon: Some("mdi:file-alert"), component: "binary_sensor" },
        // QPIWS warnings (0/1/2 severity)
        SensorMeta { object_id: "warn_pv_loss", name: "Warning PV Loss", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:solar-panel-large"), component: "sensor" },
        SensorMeta { object_id: "warn_inverter_fault", name: "Warning Inverter Fault", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert-circle"), component: "sensor" },
        SensorMeta { object_id: "warn_bus_over", name: "Warning Bus Over", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_bus_under", name: "Warning Bus Under", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_line_fail", name: "Warning Line Fail", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:transmission-tower-off"), component: "sensor" },
        SensorMeta { object_id: "warn_over_temperature", name: "Warning Over Temperature", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:thermometer-alert"), component: "sensor" },
        SensorMeta { object_id: "warn_fan_locked", name: "Warning Fan Locked", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:fan-alert"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_low_alarm", name: "Warning Battery Low", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:battery-alert"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_under_shutdown", name: "Warning Battery Under Shutdown", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:battery-remove"), component: "sensor" },
        SensorMeta { object_id: "warn_overload", name: "Warning Overload", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:gauge-full"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_voltage_high", name: "Warning Battery Voltage High", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:battery-plus"), component: "sensor" },
        SensorMeta { object_id: "warn_inverter_voltage_low", name: "Warning Inverter Voltage Low", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:flash-alert"), component: "sensor" },
        SensorMeta { object_id: "warn_inverter_voltage_high", name: "Warning Inverter Voltage High", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:flash-alert"), component: "sensor" },
        SensorMeta { object_id: "warn_power_limit", name: "Warning Power Limit", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:speedometer-slow"), component: "sensor" },
        SensorMeta { object_id: "warn_pv_voltage_high", name: "Warning PV Voltage High", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_too_low_charge", name: "Warning Battery Too Low to Charge", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:battery-alert-variant"), component: "sensor" },
        SensorMeta { object_id: "warn_bus_soft_fail", name: "Warning Bus Soft Fail", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_opv_short", name: "Warning Output Short", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_derating", name: "Warning Battery Derating", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_eeprom_fault", name: "Warning EEPROM Fault", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_inverter_over_current", name: "Warning Inverter Over Current", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_inverter_soft_fail", name: "Warning Inverter Soft Fail", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_self_test_fail", name: "Warning Self Test Fail", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_op_dc_voltage_over", name: "Warning DC Voltage in AC Output", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_bat_open", name: "Warning Battery Open", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_current_sensor_fail", name: "Warning Current Sensor Fail", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_battery_short", name: "Warning Battery Short", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_mppt_overload_fault", name: "Warning MPPT Overload Fault", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "warn_mppt_overload_warning", name: "Warning MPPT Overload", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:alert"), component: "sensor" },
        SensorMeta { object_id: "pv_total_power", name: "PV Total Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "serial_number", name: "Serial Number", unit: None, device_class: None, state_class: None, icon: Some("mdi:identifier"), component: "sensor" },
        SensorMeta { object_id: "charger_source_priority", name: "Charger Source Priority", unit: None, device_class: None, state_class: None, icon: Some("mdi:battery-charging-wireless"), component: "sensor" },
        SensorMeta { object_id: "max_ac_charging_current", name: "Max AC Charging Current", unit: Some("A"), device_class: Some("current"), state_class: None, icon: Some("mdi:current-ac"), component: "sensor" },
        SensorMeta { object_id: "output_mode", name: "Output Mode", unit: None, device_class: None, state_class: None, icon: Some("mdi:sine-wave"), component: "sensor" },
    ]
}

// ─── System totals sensor definitions ────────────────────────────────────────

pub fn totals_sensors() -> Vec<SensorMeta> {
    vec![
        SensorMeta { object_id: "battery_voltage_avg", name: "Total System Battery Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:battery"), component: "sensor" },
        SensorMeta { object_id: "battery_soc_avg", name: "Total System Battery SOC", unit: Some("%"), device_class: Some("battery"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "battery_charging_current_total", name: "Total System Battery Charging Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:current-dc"), component: "sensor" },
        SensorMeta { object_id: "battery_discharge_current_total", name: "Total System Battery Discharge Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:current-dc"), component: "sensor" },
        SensorMeta { object_id: "battery_real_charge_total", name: "Total System Battery Real Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:battery-arrow-up-outline"), component: "sensor" },
        SensorMeta { object_id: "battery_real_power_total", name: "Total System Battery Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:battery-charging-outline"), component: "sensor" },
        SensorMeta { object_id: "pv1_power_total", name: "System PV1 Power Total", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv2_power_total", name: "System PV2 Power Total", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "pv_power_total", name: "Total System PV Input Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "pv_input_current_total", name: "Total System PV Input Current", unit: Some("A"), device_class: Some("current"), state_class: Some("measurement"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "ac_output_active_power_total", name: "Total System AC Output Active Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "ac_output_apparent_power_total", name: "Total System AC Output Apparent Power", unit: Some("VA"), device_class: Some("apparent_power"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "ac_output_reactive_power_total", name: "Total System AC Output Reactive Power", unit: Some("var"), device_class: Some("reactive_power"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "output_load_percent_avg", name: "Total System Load Percentage", unit: Some("%"), device_class: None, state_class: Some("measurement"), icon: Some("mdi:gauge"), component: "sensor" },
        SensorMeta { object_id: "grid_voltage_avg", name: "Total System Grid Input Voltage", unit: Some("V"), device_class: Some("voltage"), state_class: Some("measurement"), icon: Some("mdi:transmission-tower"), component: "sensor" },
        SensorMeta { object_id: "grid_frequency_avg", name: "Total System Grid Input Frequency", unit: Some("Hz"), device_class: Some("frequency"), state_class: Some("measurement"), icon: Some("mdi:sine-wave"), component: "sensor" },
        SensorMeta { object_id: "grid_input_power_estimate_total", name: "Total System Grid Estimate Input Power", unit: Some("W"), device_class: Some("power"), state_class: Some("measurement"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "heatsink_temperature_max", name: "System Heatsink Temp (max)", unit: Some("°C"), device_class: Some("temperature"), state_class: Some("measurement"), icon: Some("mdi:thermometer-chevron-up"), component: "sensor" },
        SensorMeta { object_id: "heatsink_temperature_avg", name: "System Heatsink Temp (avg)", unit: Some("°C"), device_class: Some("temperature"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "inverters_online", name: "Inverters Online", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:server-network"), component: "sensor" },
        SensorMeta { object_id: "system_power_factor", name: "System Power Factor", unit: None, device_class: Some("power_factor"), state_class: Some("measurement"), icon: None, component: "sensor" },
        SensorMeta { object_id: "solar_self_sufficiency_percent", name: "Solar Self-Sufficiency Today", unit: Some("%"), device_class: None, state_class: Some("measurement"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "system_status", name: "System General Status", unit: None, device_class: None, state_class: Some("measurement"), icon: Some("mdi:shield-check"), component: "sensor" },
        SensorMeta { object_id: "battery_time_remaining_min", name: "System Battery Time Remaining", unit: Some("min"), device_class: Some("duration"), state_class: Some("measurement"), icon: Some("mdi:battery-clock"), component: "sensor" },
        SensorMeta { object_id: "battery_time_to_full_min", name: "System Battery Time to Full", unit: Some("min"), device_class: Some("duration"), state_class: Some("measurement"), icon: Some("mdi:battery-clock-outline"), component: "sensor" },
    ]
}

// ─── Energy sensor definitions ────────────────────────────────────────────────

pub fn energy_sensors_per_inv() -> Vec<SensorMeta> {
    vec![
        SensorMeta { object_id: "energy_pv1_today", name: "PV1 Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "energy_pv2_today", name: "PV2 Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "energy_pv_today", name: "PV Total Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "energy_ac_output_today", name: "AC Output Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "energy_battery_charged_today", name: "Battery Charged Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "energy_battery_discharged_today", name: "Battery Discharged Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "energy_grid_import_today", name: "Grid Import Energy Today (est.)", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "energy_pv_to_battery_today", name: "PV to Battery Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "energy_ac_output_reactive_today", name: "AC Output Reactive Energy Today", unit: Some("varh"), device_class: None, state_class: Some("total_increasing"), icon: Some("mdi:sine-wave"), component: "sensor" },
        SensorMeta { object_id: "energy_pv1_total", name: "PV1 Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "energy_pv2_total", name: "PV2 Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "energy_pv_total", name: "PV Total Energy Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "energy_ac_output_total", name: "AC Output Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "energy_battery_charged_total", name: "Battery Charged Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "energy_battery_discharged_total", name: "Battery Discharged Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "energy_grid_import_total", name: "Grid Import Energy Total (est.)", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "energy_pv_to_battery_total", name: "PV to Battery Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "energy_ac_output_reactive_total", name: "AC Output Reactive Energy Total", unit: Some("varh"), device_class: None, state_class: Some("total_increasing"), icon: Some("mdi:sine-wave"), component: "sensor" },
    ]
}

pub fn energy_sensors_system() -> Vec<SensorMeta> {
    vec![
        SensorMeta { object_id: "total_energy_pv_today", name: "System PV Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv1_today", name: "System PV1 Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv2_today", name: "System PV2 Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "total_energy_ac_output_today", name: "System AC Output Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "total_energy_battery_charged_today", name: "System Battery Charged Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "total_energy_battery_discharged_today", name: "System Battery Discharged Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv_total", name: "System PV Energy Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "total_energy_ac_output_total", name: "System AC Output Energy Total", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:power-plug"), component: "sensor" },
        SensorMeta { object_id: "total_energy_grid_import_today", name: "System Grid Import Energy Today (est.)", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv_to_battery_today", name: "System PV to Battery Energy Today", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "total_energy_ac_output_reactive_today", name: "System AC Output Reactive Energy Today", unit: Some("varh"), device_class: None, state_class: Some("total_increasing"), icon: Some("mdi:sine-wave"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv1_total", name: "System PV1 Energy Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv2_total", name: "System PV2 Energy Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-panel"), component: "sensor" },
        SensorMeta { object_id: "total_energy_battery_charged_total", name: "System Battery Charged Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "total_energy_battery_discharged_total", name: "System Battery Discharged Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "total_energy_grid_import_total", name: "System Grid Import Energy Lifetime (est.)", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "total_energy_pv_to_battery_total", name: "System PV to Battery Energy Lifetime", unit: Some("Wh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "total_energy_ac_output_reactive_total", name: "System AC Output Reactive Energy Lifetime", unit: Some("varh"), device_class: None, state_class: Some("total_increasing"), icon: Some("mdi:sine-wave"), component: "sensor" },
        SensorMeta { object_id: "system_battery_cycles", name: "System Battery Cycles (equivalent)", unit: None, device_class: None, state_class: Some("total_increasing"), icon: Some("mdi:battery-sync"), component: "sensor" },
        SensorMeta { object_id: "co2_saved_today_kg", name: "CO2 Saved Today", unit: Some("kg"), device_class: Some("weight"), state_class: Some("total_increasing"), icon: Some("mdi:leaf"), component: "sensor" },
        SensorMeta { object_id: "co2_saved_total_kg", name: "CO2 Saved Lifetime", unit: Some("kg"), device_class: Some("weight"), state_class: Some("total_increasing"), icon: Some("mdi:leaf"), component: "sensor" },
        // ── kWh energy sensors (HA Energy dashboard compatible) ──────────────
        SensorMeta { object_id: "total_pv_production_today_kwh", name: "PV Production Today", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "total_pv_production_total_kwh", name: "PV Production Lifetime", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power-variant"), component: "sensor" },
        SensorMeta { object_id: "total_consumption_today_kwh", name: "Installation Consumption Today", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:home-lightning-bolt"), component: "sensor" },
        SensorMeta { object_id: "total_consumption_total_kwh", name: "Installation Consumption Lifetime", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:home-lightning-bolt"), component: "sensor" },
        SensorMeta { object_id: "total_battery_charged_today_kwh", name: "Battery Charged Today", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "total_battery_charged_total_kwh", name: "Battery Charged Lifetime", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-up"), component: "sensor" },
        SensorMeta { object_id: "total_battery_discharged_today_kwh", name: "Battery Discharged Today", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "total_battery_discharged_total_kwh", name: "Battery Discharged Lifetime", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:battery-arrow-down"), component: "sensor" },
        SensorMeta { object_id: "total_grid_import_today_kwh", name: "Grid Import Today (est.)", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "total_grid_import_total_kwh", name: "Grid Import Lifetime (est.)", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:transmission-tower-import"), component: "sensor" },
        SensorMeta { object_id: "total_pv_to_battery_today_kwh", name: "PV to Battery Today", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
        SensorMeta { object_id: "total_pv_to_battery_total_kwh", name: "PV to Battery Lifetime", unit: Some("kWh"), device_class: Some("energy"), state_class: Some("total_increasing"), icon: Some("mdi:solar-power"), component: "sensor" },
    ]
}

// ─── Discovery payload builder ────────────────────────────────────────────────

pub struct DiscoveryContext<'a> {
    pub ha_prefix: &'a str,
    pub topic_prefix: &'a str,
    pub device_id: &'a str,
    pub device_name: &'a str,
    pub manufacturer: &'a str,
    pub model: &'a str,
    /// Availability topics ("online"/"offline"); the entity is available only
    /// when all of them are online.
    pub availability: &'a [String],
}

pub fn build_discovery_payload(ctx: &DiscoveryContext, meta: &SensorMeta) -> (String, String) {
    let state_topic = format!("{}/{}", ctx.topic_prefix, meta.object_id);
    let unique_id = format!("{}_{}", ctx.device_id, meta.object_id);

    let mut payload = json!({
        "name": meta.name,
        "unique_id": unique_id,
        "state_topic": state_topic,
        "device": {
            "identifiers": [ctx.device_id],
            "name": ctx.device_name,
            "manufacturer": ctx.manufacturer,
            "model": ctx.model,
        }
    });

    if !ctx.availability.is_empty() {
        payload["availability"] =
            json!(ctx.availability.iter().map(|t| json!({ "topic": t })).collect::<Vec<_>>());
        payload["availability_mode"] = json!("all");
    }
    if let Some(unit) = meta.unit {
        payload["unit_of_measurement"] = json!(unit);
    }
    if let Some(dc) = meta.device_class {
        payload["device_class"] = json!(dc);
    }
    if let Some(sc) = meta.state_class {
        payload["state_class"] = json!(sc);
    }
    if let Some(icon) = meta.icon {
        payload["icon"] = json!(icon);
    }

    // Binary sensors use ON/OFF
    if meta.component == "binary_sensor" {
        payload["payload_on"] = json!("1");
        payload["payload_off"] = json!("0");
    }

    let config_topic = format!(
        "{}/{}/{}/{}/config",
        ctx.ha_prefix, meta.component, ctx.device_id, meta.object_id
    );

    (config_topic, payload.to_string())
}
