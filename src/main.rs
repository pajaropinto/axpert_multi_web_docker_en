mod config;
mod database;
mod inverter;
mod mqtt;
mod totals;
mod web;

use crate::config::watcher::CONFIG_PATH;
use crate::database::open_shared;
use crate::inverter::InverterData;
use crate::mqtt::publisher::SystemSnapshot;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{sleep, sleep_until, Duration, Instant};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // ── Load initial config first so we can use the configured log level ───────
    let initial_config = config::AppConfig::load(CONFIG_PATH).unwrap_or_else(|e| {
        // Can't log yet — print to stderr directly
        eprintln!("FATAL: cannot load {}: {}", CONFIG_PATH, e);
        std::process::exit(1);
    });

    // Initialise logging once with the configured level
    let filter = EnvFilter::try_new(&initial_config.system.log_level)
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    info!("=== Axpert Multi-Inverter Monitor v{} ===", env!("CARGO_PKG_VERSION"));

    let web_port = initial_config.web.port;

    // ── Open database ──────────────────────────────────────────────────────────
    let db = open_shared(database::DB_PATH)?;

    // ── Shared config (updated on hot-reload) ─────────────────────────────────
    let shared_config = Arc::new(RwLock::new(initial_config.clone()));

    // ── Channels ───────────────────────────────────────────────────────────────
    // Config updates: watcher → all tasks
    // Keep one standing receiver so the channel stays alive.
    let (config_tx, _config_rx_standing) = broadcast::channel::<config::AppConfig>(8);

    // Inverter data: each inverter manager → aggregator
    let (inv_data_tx, inv_data_rx) = mpsc::channel::<InverterData>(64);

    // System snapshot: aggregator → MQTT publisher + WebSocket clients
    // Keep one standing receiver so the channel stays alive when no WS clients are connected.
    let (snapshot_tx, _snapshot_rx_standing) = broadcast::channel::<SystemSnapshot>(8);

    // ── Spawn config watcher ───────────────────────────────────────────────────
    {
        let config_tx_w = config_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = config::watcher::watch_config(config_tx_w).await {
                error!("Config watcher failed: {}", e);
            }
        });

        // Mirror config changes into shared_config
        let mut config_rx_mirror = config_tx.subscribe();
        let shared_config_mirror = Arc::clone(&shared_config);
        tokio::spawn(async move {
            loop {
                match config_rx_mirror.recv().await {
                    Ok(new_cfg) => *shared_config_mirror.write().await = new_cfg,
                    // Missed some updates: the next one carries the full config
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // ── Spawn per-inverter polling tasks (follows config changes) ──────────────
    {
        let config_tx = config_tx.clone();
        let init_cfg = initial_config.clone();
        tokio::spawn(async move {
            run_inverter_supervisor(init_cfg, config_tx, inv_data_tx).await;
        });
    }

    // ── Shared latest snapshot for WebSocket clients ───────────────────────────
    let latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::new(RwLock::new(None));

    // ── Spawn aggregator task ──────────────────────────────────────────────────
    {
        let snapshot_tx = snapshot_tx.clone();
        let latest_snapshot = Arc::clone(&latest_snapshot);
        let shared_config = Arc::clone(&shared_config);
        let db = Arc::clone(&db);

        tokio::spawn(async move {
            run_aggregator(
                inv_data_rx,  // mpsc::Receiver moved into aggregator
                snapshot_tx,
                latest_snapshot,
                shared_config,
                db,
            )
            .await;
        });
    }

    // ── Spawn MQTT publisher task ──────────────────────────────────────────────
    {
        let mqtt_rx = snapshot_tx.subscribe();
        let cfg_rx = config_tx.subscribe();
        let init_cfg = initial_config.clone();

        tokio::spawn(async move {
            mqtt::publisher::run(init_cfg, cfg_rx, mqtt_rx).await;
        });
    }

    // ── Spawn session cleanup task ─────────────────────────────────────────────
    let sessions = web::auth::new_session_store();
    {
        let sessions_clone = Arc::clone(&sessions);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(300)).await;
                web::auth::purge_expired_sessions(&sessions_clone).await;
            }
        });
    }

    // ── Start web server, until a stop signal arrives ──────────────────────────
    let web_state = web::AppState {
        config: Arc::clone(&shared_config),
        db: Arc::clone(&db),
        sessions,
        latest_snapshot: Arc::clone(&latest_snapshot),
        snapshot_tx: snapshot_tx.clone(),
    };

    tokio::select! {
        result = web::start(web_state, web_port) => result?,
        _ = shutdown_signal() => info!("Stop signal received — shutting down"),
    }

    // Save the energy counters still in memory
    if let Err(e) = db.lock().await.flush() {
        error!("Cannot save energy counters on shutdown: {}", e);
    }
    Ok(())
}

/// Resolves on SIGTERM (docker stop) or Ctrl-C.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("cannot install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}

/// Keeps one polling task per active inverter, starting and stopping tasks
/// when inverters are added, removed, enabled or disabled in the config.
async fn run_inverter_supervisor(
    initial_config: config::AppConfig,
    config_tx: broadcast::Sender<config::AppConfig>,
    data_tx: mpsc::Sender<InverterData>,
) {
    let mut config_rx = config_tx.subscribe();
    let mut tasks: HashMap<u8, JoinHandle<()>> = HashMap::new();
    let mut cfg = initial_config;

    loop {
        let active: HashSet<u8> = cfg.active_inverters().iter().map(|i| i.id).collect();

        tasks.retain(|id, task| {
            let keep = active.contains(id);
            if !keep {
                info!("[Inv {}] Removed from config — polling stopped", id);
                task.abort();
            }
            keep
        });

        for inv_cfg in cfg.active_inverters() {
            if tasks.contains_key(&inv_cfg.id) {
                continue;
            }
            if let Err(e) = config::ensure_inverter_param_file(inv_cfg.id) {
                warn!("[Inv {}] {}", inv_cfg.id, e);
            }
            let inv_cfg = inv_cfg.clone();
            let id = inv_cfg.id;
            let cfg_rx = config_tx.subscribe();
            let data_tx = data_tx.clone();
            let init_cfg = cfg.clone();
            tasks.insert(
                id,
                tokio::spawn(async move {
                    inverter::manager::run(inv_cfg, cfg_rx, data_tx, init_cfg).await;
                }),
            );
        }

        cfg = loop {
            match config_rx.recv().await {
                Ok(new_cfg) => break new_cfg,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            }
        };
    }
}

/// Polling data older than this is treated as offline (task stuck or stopped).
const STALE_AFTER: Duration = Duration::from_secs(60);
/// How often energy counters are written to disk.
const ENERGY_FLUSH_EVERY: Duration = Duration::from_secs(60);
/// Battery nominal voltage when no inverter has reported QPIRI yet (V).
const DEFAULT_BATTERY_VOLTAGE: f64 = 48.0;
/// Longest gap integrated as energy, in cycles (minimum 30 s). After a pause
/// (host suspended, container paused) the last power is not assumed to have
/// lasted the whole pause.
const MAX_INTEGRATION_CYCLES: u64 = 3;
const MIN_INTEGRATION_GAP: Duration = Duration::from_secs(30);

/// Aggregates InverterData from all polling tasks.
///
/// - Every time an inverter reports: rebuilds the snapshot for the web UI.
/// - Every `delay_between_cycles_ms`: accumulates energy, publishes the
///   snapshot to MQTT (`end_of_cycle`), and flushes energy to disk once a minute.
async fn run_aggregator(
    mut inv_data_rx: mpsc::Receiver<InverterData>,
    snapshot_tx: broadcast::Sender<SystemSnapshot>,
    latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    shared_config: Arc<RwLock<config::AppConfig>>,
    db: database::SharedDb,
) {
    let mut inverter_map: HashMap<u8, (InverterData, Instant)> = HashMap::new();
    let mut energy = database::EnergySnapshot::default();
    let mut energy_loaded = false;
    let mut last_accumulate = Instant::now();
    let mut last_flush = Instant::now();
    let mut next_cycle = Instant::now();

    loop {
        let cfg = shared_config.read().await.clone();

        // Forget inverters that are no longer configured
        let active: HashSet<u8> = cfg.active_inverters().iter().map(|i| i.id).collect();
        inverter_map.retain(|id, _| active.contains(id));

        let end_of_cycle = tokio::select! {
            msg = inv_data_rx.recv() => {
                match msg {
                    Some(data) => {
                        if active.contains(&data.id) {
                            inverter_map.insert(data.id, (data, Instant::now()));
                        }
                        false
                    }
                    None => break, // all polling tasks gone
                }
            }
            _ = sleep_until(next_cycle) => true,
        };

        if inverter_map.is_empty() {
            if end_of_cycle {
                next_cycle = Instant::now() + Duration::from_millis(cfg.system.delay_between_cycles_ms);
                last_accumulate = Instant::now(); // no data: nothing to integrate
            }
            continue;
        }

        let mut inverters: Vec<InverterData> = inverter_map
            .values()
            .map(|(data, received)| {
                let mut data = data.clone();
                if received.elapsed() > STALE_AFTER {
                    data.online = false;
                }
                data
            })
            .collect();
        inverters.sort_by_key(|i| i.id);

        if end_of_cycle {
            next_cycle = Instant::now() + Duration::from_millis(cfg.system.delay_between_cycles_ms);
            let max_gap = Duration::from_millis(
                cfg.system.delay_between_cycles_ms * MAX_INTEGRATION_CYCLES,
            )
            .max(MIN_INTEGRATION_GAP);
            let elapsed = last_accumulate.elapsed();
            if elapsed > max_gap {
                warn!(
                    "{:.0} s since the last energy update — only {:.0} s counted",
                    elapsed.as_secs_f64(),
                    max_gap.as_secs_f64()
                );
            }
            let delta_secs = elapsed.min(max_gap).as_secs_f64();
            last_accumulate = Instant::now();
            energy = accumulate_energy(&db, &inverters, delta_secs, &cfg).await;
            energy_loaded = true;

            if last_flush.elapsed() >= ENERGY_FLUSH_EVERY {
                last_flush = Instant::now();
                if let Err(e) = db.lock().await.flush() {
                    error!("Cannot save energy counters: {}", e);
                }
            }
        } else if !energy_loaded {
            // First data after start: show the stored counters instead of zeros
            energy = accumulate_energy(&db, &inverters, 0.0, &cfg).await;
            energy_loaded = true;
        }

        let snapshot = build_snapshot(inverters, energy.clone(), &cfg, end_of_cycle);
        *latest_snapshot.write().await = Some(snapshot.clone());
        let _ = snapshot_tx.send(snapshot);
    }
}

/// Adds the energy of the last `delta_secs` to the counters (in memory).
async fn accumulate_energy(
    db: &database::SharedDb,
    inverters: &[InverterData],
    delta_secs: f64,
    cfg: &config::AppConfig,
) -> database::EnergySnapshot {
    let db = db.lock().await;

    let ids: Vec<u8> = cfg.active_inverters().iter().map(|i| i.id).collect();
    if let Err(e) = db.maybe_reset_daily(&ids) {
        warn!("Daily reset check failed: {}", e);
    }

    // Every configured inverter counts in the system totals, also those that
    // have not reported yet (just started): their stored counters are used,
    // so lifetime totals never go down (HA would take it as a meter reset).
    let mut inv_energies = Vec::new();
    for inv_cfg in cfg.active_inverters() {
        let result = match inverters.iter().find(|i| i.id == inv_cfg.id) {
            Some(inv) => db.accumulate(inv, delta_secs),
            None => db.read_inverter_energy(inv_cfg.id),
        };
        match result {
            Ok(e) => inv_energies.push(e),
            Err(e) => warn!("Energy accumulation error for inv {}: {}", inv_cfg.id, e),
        }
    }

    let battery_v = inverters
        .iter()
        .filter_map(|i| i.qpiri.as_ref().map(|q| q.battery_rating_voltage))
        .find(|v| *v > 0.0)
        .unwrap_or(DEFAULT_BATTERY_VOLTAGE);
    let system = db
        .accumulate_system(
            &inv_energies,
            cfg.battery.co2_factor_kg_per_kwh,
            cfg.battery.capacity_ah * battery_v,
        )
        .unwrap_or_default();

    database::EnergySnapshot {
        inverter_energy: inv_energies,
        system,
    }
}

fn build_snapshot(
    mut inverters: Vec<InverterData>,
    energy: database::EnergySnapshot,
    cfg: &config::AppConfig,
    end_of_cycle: bool,
) -> SystemSnapshot {
    let mut totals = totals::compute(&inverters, &cfg.battery);
    totals.solar_self_sufficiency_percent = totals::self_sufficiency(
        energy.system.total_energy_ac_output_today,
        energy.system.total_energy_grid_import_today,
    );

    // One shared battery bank: every inverter shows the system battery times
    for inv in &mut inverters {
        inv.battery_time_remaining_min = totals.battery_time_remaining_min;
        inv.battery_time_to_full_min = totals.battery_time_to_full_min;
    }

    SystemSnapshot {
        inverters,
        totals,
        energy,
        end_of_cycle,
    }
}
