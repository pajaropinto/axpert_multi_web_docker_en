/// Inverter configuration commands.
///
/// Each function builds the correct command string for the Voltronic protocol.
/// The caller is responsible for CRC-framing via protocol::build_command().
///
/// Returns None if the input value is invalid for the parameter.
use anyhow::{bail, Result};

// ─── Output / Grid ────────────────────────────────────────────────────────────

/// Output source priority.
/// "00"=Utility first, "01"=Solar first, "02"=SBU
pub fn cmd_output_source_priority(value: &str) -> Result<String> {
    match value {
        "00" | "01" | "02" => Ok(format!("POP{}", value)),
        _ => bail!("output_source_priority: invalid value '{}' (00/01/02)", value),
    }
}

/// AC input voltage range.
/// "00"=Appliance (wide), "01"=UPS (narrow)
pub fn cmd_ac_input_range(value: &str) -> Result<String> {
    match value {
        "00" | "01" => Ok(format!("PGR{}", value)),
        _ => bail!("ac_input_range: invalid value '{}' (00/01)", value),
    }
}

/// Output frequency (Hz).
/// "50" or "60"
pub fn cmd_output_frequency(value: &str) -> Result<String> {
    match value {
        "50" | "60" => Ok(format!("F{}", value)),
        _ => bail!("output_frequency: invalid value '{}' (50/60)", value),
    }
}

// ─── Battery / Charger ────────────────────────────────────────────────────────

/// Maximum total charger current (A).
/// Typical values: 10, 20, 30, 40, 50, 60, 80, 100 (inverter-model dependent)
pub fn cmd_max_charger_current(inv_id: u8, amps: &str) -> Result<String> {
    let a: u32 = amps
        .parse()
        .map_err(|_| anyhow::anyhow!("max_charger_current: not a number: '{}'", amps))?;
    if a > 200 {
        bail!("max_charger_current: value {} seems too high", a);
    }
    Ok(format!("MUCHGC{}{:03}", inv_id, a))
}

/// Charger source priority.
/// "00"=Solar first, "01"=Solar+Utility, "02"=Only solar
pub fn cmd_charger_priority(inv_id: u8, value: &str) -> Result<String> {
    match value {
        "00" | "01" | "02" => Ok(format!("PPCP{}{}", inv_id, value)),
        _ => bail!("charger_priority: invalid value '{}' (00/01/02)", value),
    }
}

/// Maximum battery discharge current (A).
pub fn cmd_max_discharge_current(amps: &str) -> Result<String> {
    let a: u32 = amps
        .parse()
        .map_err(|_| anyhow::anyhow!("max_discharge_current: not a number: '{}'", amps))?;
    if a > 999 {
        bail!("max_discharge_current: value {} too high (max 999)", a);
    }
    Ok(format!("PBATMAXDISC{:03}", a))
}

/// Battery equalization.
/// "0"=Disable, "1"=Enable
pub fn cmd_equalization(value: &str) -> Result<String> {
    match value {
        "0" | "1" => Ok(format!("PBEQE{}", value)),
        _ => bail!("equalization: invalid value '{}' (0/1)", value),
    }
}

// ─── Flags (PEx/PDx pattern) ──────────────────────────────────────────────────

/// Overload restart: "PEU"=Enable, "PDU"=Disable
pub fn cmd_overload_restart(value: &str) -> Result<String> {
    match value {
        "PEU" | "PDU" => Ok(value.to_string()),
        _ => bail!("overload_restart: use PEU or PDU"),
    }
}

/// Over-temperature restart: "PEV"=Enable, "PDV"=Disable
pub fn cmd_overtemp_restart(value: &str) -> Result<String> {
    match value {
        "PEV" | "PDV" => Ok(value.to_string()),
        _ => bail!("overtemp_restart: use PEV or PDV"),
    }
}

/// Buzzer: "PEA"=Enable, "PDA"=Disable
pub fn cmd_buzzer(value: &str) -> Result<String> {
    match value {
        "PEA" | "PDA" => Ok(value.to_string()),
        _ => bail!("buzzer: use PEA or PDA"),
    }
}

/// LCD auto-escape to main screen: "PEK"=Enable, "PDK"=Disable
pub fn cmd_lcd_escape(value: &str) -> Result<String> {
    match value {
        "PEK" | "PDK" => Ok(value.to_string()),
        _ => bail!("lcd_escape: use PEK or PDK"),
    }
}

/// LCD backlight: "PEX"=Enable, "PDX"=Disable
pub fn cmd_backlight(value: &str) -> Result<String> {
    match value {
        "PEX" | "PDX" => Ok(value.to_string()),
        _ => bail!("backlight: use PEX or PDX"),
    }
}

/// Grid loss alarm: "PEY"=Enable, "PDY"=Disable
pub fn cmd_grid_loss_alarm(value: &str) -> Result<String> {
    match value {
        "PEY" | "PDY" => Ok(value.to_string()),
        _ => bail!("grid_loss_alarm: use PEY or PDY"),
    }
}

/// Overload bypass: "PEB"=Enable, "PDB"=Disable
pub fn cmd_overload_bypass(value: &str) -> Result<String> {
    match value {
        "PEB" | "PDB" => Ok(value.to_string()),
        _ => bail!("overload_bypass: use PEB or PDB"),
    }
}

/// Fault code logging: "PEZ"=Enable, "PDZ"=Disable
pub fn cmd_fault_logging(value: &str) -> Result<String> {
    match value {
        "PEZ" | "PDZ" => Ok(value.to_string()),
        _ => bail!("fault_logging: use PEZ or PDZ"),
    }
}

// ─── Read commands ─────────────────────────────────────────────────────────────

/// Read commands used to verify current inverter state
pub const CMD_QID: &str = "QID";
pub const CMD_QPIGS: &str = "QPIGS";
pub const CMD_QPIGS2: &str = "QPIGS2";
pub const CMD_QFLAG: &str = "QFLAG";
pub const CMD_QPIRI: &str = "QPIRI";
pub const CMD_QPIWS: &str = "QPIWS";
pub const CMD_QMOD: &str = "QMOD";
