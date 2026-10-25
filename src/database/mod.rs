/// Persistent storage using redb (pure-Rust embedded ACID database).
///
/// Tables:
///   users           → username → JSON UserRecord
///   energy_daily    → "{inv_id}:{metric}" → f64  (reset at midnight)
///   energy_total    → "{inv_id}:{metric}_total" → f64  (lifetime accumulator)
///
/// Energy counters are accumulated in memory and written to disk by `flush()`
/// (once a minute and on shutdown), to keep disk writes low.
///   meta            → string key → string value  (last_reset_date, etc.)
use anyhow::{Context, Result};
use chrono::Local;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

const USERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("users");
const ENERGY_DAILY_TABLE: TableDefinition<&str, f64> = TableDefinition::new("energy_daily");
const ENERGY_TOTAL_TABLE: TableDefinition<&str, f64> = TableDefinition::new("energy_total");
const META_TABLE: TableDefinition<&str, &str> = TableDefinition::new("meta");

pub const DB_PATH: &str = "data/axpert.db";

// ─── User management ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub username: String,
    pub password_hash: String,
    pub role: String, // "admin" | "user"
}

// ─── Energy snapshots (sent to MQTT / WebSocket) ──────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InverterEnergyData {
    pub inv_id: u8,
    pub energy_pv1_today: f64,
    pub energy_pv2_today: f64,
    pub energy_pv_today: f64,
    pub energy_ac_output_today: f64,
    pub energy_battery_charged_today: f64,
    pub energy_battery_discharged_today: f64,
    /// Energy imported from the grid (estimated from the power balance)
    pub energy_grid_import_today: f64,
    /// Energy sent from PV into the battery (PV charging power)
    pub energy_pv_to_battery_today: f64,
    /// AC output reactive energy (varh)
    pub energy_ac_output_reactive_today: f64,
    pub energy_pv1_total: f64,
    pub energy_pv2_total: f64,
    pub energy_pv_total: f64,
    pub energy_ac_output_total: f64,
    pub energy_battery_charged_total: f64,
    pub energy_battery_discharged_total: f64,
    pub energy_grid_import_total: f64,
    pub energy_pv_to_battery_total: f64,
    pub energy_ac_output_reactive_total: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemEnergyData {
    // ── Wh values (raw accumulators) ──────────────────────────────────────────
    pub total_energy_pv_today: f64,
    pub total_energy_pv1_today: f64,
    pub total_energy_pv2_today: f64,
    pub total_energy_ac_output_today: f64,
    pub total_energy_battery_charged_today: f64,
    pub total_energy_battery_discharged_today: f64,
    pub total_energy_grid_import_today: f64,
    pub total_energy_pv_to_battery_today: f64,
    pub total_energy_ac_output_reactive_today: f64,
    pub total_energy_pv_total: f64,
    pub total_energy_pv1_total: f64,
    pub total_energy_pv2_total: f64,
    pub total_energy_ac_output_total: f64,
    pub total_energy_battery_charged_total: f64,
    pub total_energy_battery_discharged_total: f64,
    pub total_energy_grid_import_total: f64,
    pub total_energy_pv_to_battery_total: f64,
    pub total_energy_ac_output_reactive_total: f64,
    pub system_battery_cycles: f64,
    pub co2_saved_today_kg: f64,
    pub co2_saved_total_kg: f64,

    // ── kWh values (derived: Wh / 1000, ready for HA energy dashboard) ────────
    /// Total PV production today (kWh)
    pub total_pv_production_today_kwh: f64,
    /// Total PV production lifetime (kWh)
    pub total_pv_production_total_kwh: f64,
    /// Total installation consumption today — AC output delivered to loads (kWh)
    pub total_consumption_today_kwh: f64,
    /// Total installation consumption lifetime (kWh)
    pub total_consumption_total_kwh: f64,
    /// Total energy charged into batteries today (kWh)
    pub total_battery_charged_today_kwh: f64,
    /// Total energy charged into batteries lifetime (kWh)
    pub total_battery_charged_total_kwh: f64,
    /// Total energy discharged from batteries today (kWh)
    pub total_battery_discharged_today_kwh: f64,
    /// Total energy discharged from batteries lifetime (kWh)
    pub total_battery_discharged_total_kwh: f64,
    /// Energy imported from the grid today, estimated (kWh)
    pub total_grid_import_today_kwh: f64,
    /// Energy imported from the grid lifetime, estimated (kWh)
    pub total_grid_import_total_kwh: f64,
    /// Energy sent from PV into the batteries today (kWh)
    pub total_pv_to_battery_today_kwh: f64,
    /// Energy sent from PV into the batteries lifetime (kWh)
    pub total_pv_to_battery_total_kwh: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnergySnapshot {
    pub inverter_energy: Vec<InverterEnergyData>,
    pub system: SystemEnergyData,
}

// ─── Database wrapper ──────────────────────────────────────────────────────────

pub struct Db {
    inner: Database,
    cache: std::sync::Mutex<EnergyCache>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap_or_else(|| std::path::Path::new(".")))
            .context("Cannot create data directory")?;

        let db = Database::create(path)
            .with_context(|| format!("Cannot open database: {}", path))?;

        // Create tables if they don't exist
        {
            let tx = db.begin_write()?;
            tx.open_table(USERS_TABLE)?;
            tx.open_table(ENERGY_DAILY_TABLE)?;
            tx.open_table(ENERGY_TOTAL_TABLE)?;
            tx.open_table(META_TABLE)?;
            tx.commit()?;
        }

        info!("Database opened: {}", path);
        let db_wrapper = Self { inner: db, cache: Default::default() };

        // Seed default admin user if no users exist
        if db_wrapper.count_users()? == 0 {
            let hash = bcrypt::hash("admin", bcrypt::DEFAULT_COST)
                .context("Cannot hash default password")?;
            db_wrapper.save_user(&UserRecord {
                username: "admin".to_string(),
                password_hash: hash,
                role: "admin".to_string(),
            })?;
            info!("Default user 'admin' created (password: admin)");
        }

        Ok(db_wrapper)
    }

    // ─── User operations ───────────────────────────────────────────────────────

    pub fn save_user(&self, user: &UserRecord) -> Result<()> {
        let json = serde_json::to_string(user)?;
        let tx = self.inner.begin_write()?;
        {
            let mut table = tx.open_table(USERS_TABLE)?;
            table.insert(user.username.as_str(), json.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_user(&self, username: &str) -> Result<Option<UserRecord>> {
        let tx = self.inner.begin_read()?;
        let table = tx.open_table(USERS_TABLE)?;
        match table.get(username)? {
            Some(v) => {
                let record: UserRecord = serde_json::from_str(v.value())?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    pub fn list_users(&self) -> Result<Vec<UserRecord>> {
        let tx = self.inner.begin_read()?;
        let table = tx.open_table(USERS_TABLE)?;
        let mut users = Vec::new();
        for result in table.iter()? {
            let (_, v) = result?;
            let record: UserRecord = serde_json::from_str(v.value())?;
            users.push(record);
        }
        Ok(users)
    }

    pub fn delete_user(&self, username: &str) -> Result<bool> {
        let tx = self.inner.begin_write()?;
        let removed = {
            let mut table = tx.open_table(USERS_TABLE)?;
            let result = table.remove(username)?;
            result.is_some()
        };
        tx.commit()?;
        Ok(removed)
    }

    fn count_users(&self) -> Result<usize> {
        let tx = self.inner.begin_read()?;
        let table = tx.open_table(USERS_TABLE)?;
        Ok(table.len()? as usize)
    }

    pub fn verify_user(&self, username: &str, password: &str) -> Result<Option<UserRecord>> {
        if let Some(user) = self.get_user(username)? {
            if bcrypt::verify(password, &user.password_hash).unwrap_or(false) {
                return Ok(Some(user));
            }
        }
        Ok(None)
    }

    pub fn change_password(&self, username: &str, new_password: &str) -> Result<bool> {
        if let Some(mut user) = self.get_user(username)? {
            user.password_hash =
                bcrypt::hash(new_password, bcrypt::DEFAULT_COST).context("Cannot hash password")?;
            self.save_user(&user)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ─── Energy counters (in-memory cache, flushed periodically) ──────────────

    fn daily_key(inv_id: u8, metric: &str) -> String {
        format!("{}:{}", inv_id, metric)
    }

    /// Lifetime counters use the daily key plus a "_total" suffix
    fn total_key(inv_id: u8, metric: &str) -> String {
        format!("{}:{}_total", inv_id, metric)
    }

    /// Current value of a counter: from the cache, loaded from disk on first use.
    fn counter(&self, kind: Counter, key: &str) -> Result<f64> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(v) = cache.map(kind).get(key) {
            return Ok(*v);
        }
        let tx = self.inner.begin_read()?;
        let table = tx.open_table(kind.table())?;
        let v = table.get(key)?.map(|v| v.value()).unwrap_or(0.0);
        cache.map(kind).insert(key.to_string(), v);
        Ok(v)
    }

    fn add_counter(&self, kind: Counter, key: &str, delta: f64) -> Result<()> {
        let v = self.counter(kind, key)?;
        let mut cache = self.cache.lock().unwrap();
        cache.map(kind).insert(key.to_string(), v + delta);
        cache.dirty = true;
        Ok(())
    }

    /// Set counters to zero on disk right away and in the cache.
    fn zero_counters(&self, kind: Counter, keys: &[String]) -> Result<()> {
        let tx = self.inner.begin_write()?;
        {
            let mut table = tx.open_table(kind.table())?;
            for key in keys {
                table.insert(key.as_str(), 0.0)?;
            }
        }
        tx.commit()?;
        let mut cache = self.cache.lock().unwrap();
        for key in keys {
            cache.map(kind).insert(key.clone(), 0.0);
        }
        Ok(())
    }

    /// Write all cached energy counters to disk in one transaction.
    /// Called periodically and on shutdown; a power cut loses at most one period.
    pub fn flush(&self) -> Result<()> {
        let mut cache = self.cache.lock().unwrap();
        if !cache.dirty {
            return Ok(());
        }
        let tx = self.inner.begin_write()?;
        {
            let mut daily = tx.open_table(ENERGY_DAILY_TABLE)?;
            for (k, v) in &cache.daily {
                daily.insert(k.as_str(), *v)?;
            }
            let mut total = tx.open_table(ENERGY_TOTAL_TABLE)?;
            for (k, v) in &cache.total {
                total.insert(k.as_str(), *v)?;
            }
        }
        tx.commit()?;
        cache.dirty = false;
        Ok(())
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let tx = self.inner.begin_read()?;
        let table = tx.open_table(META_TABLE)?;
        Ok(table.get(key)?.map(|v| v.value().to_string()))
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let tx = self.inner.begin_write()?;
        {
            let mut table = tx.open_table(META_TABLE)?;
            table.insert(key, value)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn daily_keys(inv_ids: &[u8]) -> Vec<String> {
        inv_ids
            .iter()
            .flat_map(|&id| INV_METRICS.iter().map(move |m| Self::daily_key(id, m)))
            .collect()
    }

    // ─── Daily reset ───────────────────────────────────────────────────────────

    /// Check if a daily reset is needed (new day). If so, reset all daily counters.
    pub fn maybe_reset_daily(&self, inv_ids: &[u8]) -> Result<bool> {
        let today = Local::now().date_naive().to_string();
        if self.get_meta("last_reset_date")?.as_deref() == Some(&today) {
            return Ok(false);
        }
        info!("New day detected — resetting daily energy counters");
        self.zero_counters(Counter::Daily, &Self::daily_keys(inv_ids))?;
        self.set_meta("last_reset_date", &today)?;
        Ok(true)
    }

    // ─── Energy accumulation ───────────────────────────────────────────────────

    /// Accumulate energy from inverter data over a time delta (in memory).
    /// delta_secs: seconds since last call.
    pub fn accumulate(
        &self,
        inv: &crate::inverter::InverterData,
        delta_secs: f64,
    ) -> Result<InverterEnergyData> {
        if !inv.online {
            return self.read_inverter_energy(inv.id);
        }

        let hours = delta_secs / 3600.0;
        let calc = inv.calculated.clone().unwrap_or_default();
        let (ac_out_w, bat_charge_w, bat_discharge_w, pv1_charge_w) = inv
            .qpigs
            .as_ref()
            .map(|q| {
                (
                    q.ac_output_active_power as f64,
                    q.battery_charging_current as f64 * q.battery_voltage,
                    q.battery_discharge_current as f64 * q.battery_voltage,
                    q.pv1_charging_power as f64,
                )
            })
            .unwrap_or_default();
        let pv2_charge_w = inv
            .qpigs2
            .as_ref()
            .map(|q2| q2.pv2_charging_power as f64)
            .unwrap_or(0.0);

        // Same order as INV_METRICS
        let powers: [f64; INV_METRICS.len()] = [
            calc.pv1_input_power,
            calc.pv2_input_power,
            calc.pv1_input_power + calc.pv2_input_power,
            ac_out_w,
            bat_charge_w,
            bat_discharge_w,
            calc.grid_input_power_estimate,
            pv1_charge_w + pv2_charge_w,
            calc.ac_output_reactive_power,
        ];

        for (metric, power) in INV_METRICS.iter().zip(powers) {
            let delta = power.max(0.0) * hours;
            self.add_counter(Counter::Daily, &Self::daily_key(inv.id, metric), delta)?;
            self.add_counter(Counter::Total, &Self::total_key(inv.id, metric), delta)?;
        }

        self.read_inverter_energy(inv.id)
    }

    pub fn read_inverter_energy(&self, id: u8) -> Result<InverterEnergyData> {
        let today = |m: &str| self.counter(Counter::Daily, &Self::daily_key(id, m));
        let lifetime = |m: &str| self.counter(Counter::Total, &Self::total_key(id, m));
        Ok(InverterEnergyData {
            inv_id: id,
            energy_pv1_today: today("energy_pv1")?,
            energy_pv2_today: today("energy_pv2")?,
            energy_pv_today: today("energy_pv")?,
            energy_ac_output_today: today("energy_ac_output")?,
            energy_battery_charged_today: today("energy_bat_charge")?,
            energy_battery_discharged_today: today("energy_bat_discharge")?,
            energy_grid_import_today: today("energy_grid_import")?,
            energy_pv_to_battery_today: today("energy_pv_to_battery")?,
            energy_ac_output_reactive_today: today("energy_ac_output_reactive")?,
            energy_pv1_total: lifetime("energy_pv1")?,
            energy_pv2_total: lifetime("energy_pv2")?,
            energy_pv_total: lifetime("energy_pv")?,
            energy_ac_output_total: lifetime("energy_ac_output")?,
            energy_battery_charged_total: lifetime("energy_bat_charge")?,
            energy_battery_discharged_total: lifetime("energy_bat_discharge")?,
            energy_grid_import_total: lifetime("energy_grid_import")?,
            energy_pv_to_battery_total: lifetime("energy_pv_to_battery")?,
            energy_ac_output_reactive_total: lifetime("energy_ac_output_reactive")?,
        })
    }

    /// Sum per-inverter energies into system values.
    /// `battery_capacity_wh`: nominal bank energy, for equivalent full cycles.
    pub fn accumulate_system(
        &self,
        inv_energies: &[InverterEnergyData],
        co2_factor: f64,
        battery_capacity_wh: f64,
    ) -> Result<SystemEnergyData> {
        // Sum per-inverter today values
        let pv_today: f64 = inv_energies.iter().map(|e| e.energy_pv_today).sum();
        let pv1_today: f64 = inv_energies.iter().map(|e| e.energy_pv1_today).sum();
        let pv2_today: f64 = inv_energies.iter().map(|e| e.energy_pv2_today).sum();
        let ac_today: f64 = inv_energies.iter().map(|e| e.energy_ac_output_today).sum();
        let bc_today: f64 = inv_energies.iter().map(|e| e.energy_battery_charged_today).sum();
        let bd_today: f64 = inv_energies.iter().map(|e| e.energy_battery_discharged_today).sum();
        let pv_total: f64 = inv_energies.iter().map(|e| e.energy_pv_total).sum();
        let ac_total: f64 = inv_energies.iter().map(|e| e.energy_ac_output_total).sum();

        let bc_total: f64 = inv_energies.iter().map(|e| e.energy_battery_charged_total).sum();
        let bd_total: f64 = inv_energies.iter().map(|e| e.energy_battery_discharged_total).sum();
        let pv1_total: f64 = inv_energies.iter().map(|e| e.energy_pv1_total).sum();
        let pv2_total: f64 = inv_energies.iter().map(|e| e.energy_pv2_total).sum();
        let grid_today: f64 = inv_energies.iter().map(|e| e.energy_grid_import_today).sum();
        let grid_total: f64 = inv_energies.iter().map(|e| e.energy_grid_import_total).sum();
        let pvbat_today: f64 = inv_energies.iter().map(|e| e.energy_pv_to_battery_today).sum();
        let pvbat_total: f64 = inv_energies.iter().map(|e| e.energy_pv_to_battery_total).sum();
        let react_today: f64 = inv_energies.iter().map(|e| e.energy_ac_output_reactive_today).sum();
        let react_total: f64 = inv_energies.iter().map(|e| e.energy_ac_output_reactive_total).sum();

        let co2_today = pv_today / 1000.0 * co2_factor;
        let co2_total = pv_total / 1000.0 * co2_factor;

        // Equivalent full cycles: lifetime discharged energy / bank capacity
        let cycles = if battery_capacity_wh > 0.0 { bd_total / battery_capacity_wh } else { 0.0 };

        Ok(SystemEnergyData {
            // Wh accumulators
            total_energy_pv_today: pv_today,
            total_energy_pv1_today: pv1_today,
            total_energy_pv2_today: pv2_today,
            total_energy_ac_output_today: ac_today,
            total_energy_battery_charged_today: bc_today,
            total_energy_battery_discharged_today: bd_today,
            total_energy_grid_import_today: grid_today,
            total_energy_pv_to_battery_today: pvbat_today,
            total_energy_ac_output_reactive_today: react_today,
            total_energy_pv_total: pv_total,
            total_energy_pv1_total: pv1_total,
            total_energy_pv2_total: pv2_total,
            total_energy_ac_output_total: ac_total,
            total_energy_battery_charged_total: bc_total,
            total_energy_battery_discharged_total: bd_total,
            total_energy_grid_import_total: grid_total,
            total_energy_pv_to_battery_total: pvbat_total,
            total_energy_ac_output_reactive_total: react_total,
            system_battery_cycles: round2(cycles),
            co2_saved_today_kg: round2(co2_today),
            co2_saved_total_kg: round2(co2_total),
            // kWh derived fields (Wh / 1000, rounded to 2 decimals)
            total_pv_production_today_kwh:        round2(pv_today  / 1000.0),
            total_pv_production_total_kwh:        round2(pv_total  / 1000.0),
            total_consumption_today_kwh:          round2(ac_today  / 1000.0),
            total_consumption_total_kwh:          round2(ac_total  / 1000.0),
            total_battery_charged_today_kwh:      round2(bc_today  / 1000.0),
            total_battery_charged_total_kwh:      round2(bc_total  / 1000.0),
            total_battery_discharged_today_kwh:   round2(bd_today  / 1000.0),
            total_battery_discharged_total_kwh:   round2(bd_total  / 1000.0),
            total_grid_import_today_kwh:          round2(grid_today / 1000.0),
            total_grid_import_total_kwh:          round2(grid_total / 1000.0),
            total_pv_to_battery_today_kwh:        round2(pvbat_today / 1000.0),
            total_pv_to_battery_total_kwh:        round2(pvbat_total / 1000.0),
        })
    }


    /// Reset all daily energy counters for all inverters.
    pub fn reset_daily(&self, inv_ids: &[u8]) -> Result<()> {
        self.zero_counters(Counter::Daily, &Self::daily_keys(inv_ids))?;
        let today = Local::now().date_naive().to_string();
        self.set_meta("last_reset_date", &today)?;
        info!("Daily energy counters reset manually");
        Ok(())
    }

    /// Reset all energy counters (daily + lifetime) for all inverters.
    /// Battery cycles derive from the lifetime discharge, so they reset too.
    pub fn reset_all(&self, inv_ids: &[u8]) -> Result<()> {
        self.reset_daily(inv_ids)?;
        let keys: Vec<String> = inv_ids
            .iter()
            .flat_map(|&id| INV_METRICS.iter().map(move |m| Self::total_key(id, m)))
            .collect();
        self.zero_counters(Counter::Total, &keys)?;
        info!("All energy counters reset manually");
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Counter {
    Daily,
    Total,
}

impl Counter {
    fn table(self) -> TableDefinition<'static, &'static str, f64> {
        match self {
            Counter::Daily => ENERGY_DAILY_TABLE,
            Counter::Total => ENERGY_TOTAL_TABLE,
        }
    }
}

/// Energy counters kept in memory between flushes.
#[derive(Default)]
struct EnergyCache {
    daily: HashMap<String, f64>,
    total: HashMap<String, f64>,
    dirty: bool,
}

impl EnergyCache {
    fn map(&mut self, kind: Counter) -> &mut HashMap<String, f64> {
        match kind {
            Counter::Daily => &mut self.daily,
            Counter::Total => &mut self.total,
        }
    }
}

/// Per-inverter energy counters (Wh, varh for reactive). Daily key "{id}:{metric}",
/// lifetime key "{id}:{metric}_total".
const INV_METRICS: [&str; 9] = [
    "energy_pv1",
    "energy_pv2",
    "energy_pv",
    "energy_ac_output",
    "energy_bat_charge",
    "energy_bat_discharge",
    "energy_grid_import",
    "energy_pv_to_battery",
    "energy_ac_output_reactive",
];

#[inline]
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Thread-safe database handle
pub type SharedDb = Arc<Mutex<Db>>;

pub fn open_shared(path: &str) -> Result<SharedDb> {
    let db = Db::open(path)?;
    Ok(Arc::new(Mutex::new(db)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inverter::{InverterData, QpigsData, Qpigs2Data};

    fn temp_db() -> (Db, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("axpert-test-{}.db", uuid::Uuid::new_v4()));
        (Db::open(path.to_str().unwrap()).unwrap(), path)
    }

    fn online_inverter() -> InverterData {
        let mut inv = InverterData {
            id: 1,
            online: true,
            qpigs: Some(QpigsData {
                grid_voltage: 230.0,
                ac_output_apparent_power: 1250,
                ac_output_active_power: 1000,
                battery_voltage: 50.0,
                battery_charging_current: 4,
                pv1_input_voltage: 200.0,
                pv1_input_current: 4.0, // 800 W
                pv1_charging_power: 150,
                ..Default::default()
            }),
            qpigs2: Some(Qpigs2Data {
                pv2_input_voltage: 100.0,
                pv2_input_current: 1.0, // 100 W
                pv2_charging_power: 50,
            }),
            ..Default::default()
        };
        inv.compute_calculated(100.0);
        inv
    }

    #[test]
    fn accumulates_one_hour_of_every_metric() {
        let (db, path) = temp_db();
        // A counter written by an older version, under the same lifetime key
        let tx = db.inner.begin_write().unwrap();
        tx.open_table(ENERGY_TOTAL_TABLE).unwrap().insert("1:energy_pv1_total", 5000.0).unwrap();
        tx.commit().unwrap();

        let e = db.accumulate(&online_inverter(), 3600.0).unwrap();
        assert_eq!(e.energy_pv1_today, 800.0);
        assert_eq!(e.energy_pv2_today, 100.0);
        assert_eq!(e.energy_pv_today, 900.0);
        assert_eq!(e.energy_ac_output_today, 1000.0);
        assert_eq!(e.energy_battery_charged_today, 200.0);
        assert_eq!(e.energy_battery_discharged_today, 0.0);
        // Load 1000 W + battery 200 W − PV 900 W = 300 W from the grid
        assert_eq!(e.energy_grid_import_today, 300.0);
        assert_eq!(e.energy_pv_to_battery_today, 200.0);
        assert_eq!(e.energy_ac_output_reactive_today, 750.0); // √(1250² − 1000²)
        assert_eq!(e.energy_pv1_total, 5800.0); // old value kept and increased
        assert_eq!(e.energy_grid_import_total, 300.0);

        let sys = db.accumulate_system(&[e], 0.233, 10_000.0).unwrap();
        assert_eq!(sys.total_grid_import_today_kwh, 0.3);
        assert_eq!(sys.total_pv_to_battery_total_kwh, 0.2);
        assert_eq!(sys.total_energy_pv1_total, 5800.0);

        db.reset_all(&[1]).unwrap();
        let e = db.read_inverter_energy(1).unwrap();
        assert_eq!(e.energy_grid_import_today, 0.0);
        assert_eq!(e.energy_grid_import_total, 0.0);
        assert_eq!(e.energy_pv1_total, 0.0);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn counters_survive_flush_and_cycles_are_equivalent() {
        let (db, path) = temp_db();
        let mut inv = online_inverter();
        // Discharge 20 A at 50 V = 1000 W for 5 hours = 5000 Wh
        inv.qpigs.as_mut().unwrap().battery_charging_current = 0;
        inv.qpigs.as_mut().unwrap().battery_discharge_current = 20;
        let e = db.accumulate(&inv, 5.0 * 3600.0).unwrap();
        assert_eq!(e.energy_battery_discharged_total, 5000.0);

        // 5000 Wh discharged from a 10 kWh bank = 0.5 equivalent cycles
        let sys = db.accumulate_system(&[e], 0.233, 10_000.0).unwrap();
        assert_eq!(sys.system_battery_cycles, 0.5);

        // Not on disk until flushed; reopened database sees flushed values
        db.flush().unwrap();
        drop(db);
        let db = Db::open(path.to_str().unwrap()).unwrap();
        assert_eq!(db.read_inverter_energy(1).unwrap().energy_battery_discharged_total, 5000.0);

        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

