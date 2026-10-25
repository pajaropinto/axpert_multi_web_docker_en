/// MQTT publisher.
///
/// On every (re)connection: publishes HA autodiscovery configs (retained).
/// Each cycle: publishes individual sensor state values (retained).
/// Config changes apply live: broker/credential changes reconnect, and
/// topic/name/inverter changes republish discovery on the same connection.
use crate::config::AppConfig;
use crate::database::EnergySnapshot;
use crate::inverter::InverterData;
use crate::mqtt::discovery::{
    self, build_discovery_payload, DiscoveryContext,
};
use crate::totals::SystemTotals;
use anyhow::{anyhow, Result};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, info, warn};

/// Payload for a value that does not apply right now (e.g. battery time while
/// not discharging): Home Assistant shows the sensor as "unknown".
const NO_VALUE: &str = "None";

/// A publish batch that takes longer than this means the connection is stuck.
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(
    initial_config: AppConfig,
    mut config_rx: broadcast::Receiver<AppConfig>,
    mut data_rx: broadcast::Receiver<SystemSnapshot>,
) {
    let mut cfg = initial_config;

    loop {
        match connect_and_run(&mut cfg, &mut config_rx, &mut data_rx).await {
            // Connection settings changed: reconnect right away with the new ones
            Ok(()) => {}
            Err(e) => {
                error!("MQTT error: {} — reconnecting in 5s", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// App availability topic: "online" while connected, "offline" (last will)
/// when the app stops or loses the connection.
fn status_topic(cfg: &AppConfig) -> String {
    format!("{}/status", cfg.mqtt.topic_prefix)
}

/// Per-inverter availability topic: "online" while the inverter answers.
fn inverter_availability_topic(cfg: &AppConfig, inv_id: u8) -> String {
    format!("{}/inv{:02}/availability", cfg.mqtt.topic_prefix, inv_id)
}

/// Settings that require a new connection to the broker (the last will
/// topic depends on the topic prefix).
fn connection_changed(old: &AppConfig, new: &AppConfig) -> bool {
    old.mqtt.topic_prefix != new.mqtt.topic_prefix
        || old.mqtt.broker_ip != new.mqtt.broker_ip
        || old.mqtt.broker_port != new.mqtt.broker_port
        || old.mqtt.user != new.mqtt.user
        || old.mqtt.password != new.mqtt.password
}

/// Settings that change the HA discovery payloads (topics, device names, inverters).
fn discovery_changed(old: &AppConfig, new: &AppConfig) -> bool {
    old.mqtt.ha_discovery_prefix != new.mqtt.ha_discovery_prefix
        || old.active_inverters() != new.active_inverters()
}

/// Runs one broker connection. Returns Ok(()) when the connection settings
/// changed (`cfg` already holds the new ones), Err on a publish failure.
async fn connect_and_run(
    cfg: &mut AppConfig,
    config_rx: &mut broadcast::Receiver<AppConfig>,
    data_rx: &mut broadcast::Receiver<SystemSnapshot>,
) -> Result<()> {
    let client_id = format!("axpert-monitor-{}", std::process::id());
    let mut opts = MqttOptions::new(&client_id, &cfg.mqtt.broker_ip, cfg.mqtt.broker_port);
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_clean_session(true);

    if !cfg.mqtt.user.is_empty() {
        opts.set_credentials(&cfg.mqtt.user, &cfg.mqtt.password);
    }
    opts.set_last_will(LastWill::new(status_topic(cfg), "offline", QoS::AtLeastOnce, true));

    let (client, mut eventloop) = AsyncClient::new(opts, 100);
    info!(
        "MQTT connecting to {}:{}",
        cfg.mqtt.broker_ip, cfg.mqtt.broker_port
    );

    // Poll the event loop in the background and report connection state
    let (connected_tx, mut connected_rx) = watch::channel(false);
    let poller = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    info!("MQTT connected");
                    let _ = connected_tx.send(true);
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("MQTT eventloop: {}", e);
                    let _ = connected_tx.send(false);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    let result = loop {
        tokio::select! {
            // (Re)connected: publish discovery, the broker may have lost retained messages
            changed = connected_rx.changed() => {
                if changed.is_err() {
                    break Err(anyhow!("MQTT event loop stopped"));
                }
                if *connected_rx.borrow_and_update() {
                    if let Err(e) = timed(publish_discovery(cfg, &client)).await {
                        break Err(e);
                    }
                    if let Err(e) = client
                        .publish(status_topic(cfg), QoS::AtLeastOnce, true, "online")
                        .await
                    {
                        break Err(e.into());
                    }
                }
            }

            msg = config_rx.recv() => match msg {
                Ok(new_cfg) => {
                    if connection_changed(cfg, &new_cfg) {
                        info!(
                            "MQTT settings changed — reconnecting to {}:{}",
                            new_cfg.mqtt.broker_ip, new_cfg.mqtt.broker_port
                        );
                        *cfg = new_cfg;
                        break Ok(());
                    }
                    let rediscover = discovery_changed(cfg, &new_cfg);
                    let old_cfg = std::mem::replace(cfg, new_cfg);
                    if rediscover && *connected_rx.borrow() {
                        info!("MQTT topics or inverter names changed — republishing discovery");
                        if let Err(e) = timed(remove_inverter_discovery(&old_cfg, cfg, &client)).await {
                            break Err(e);
                        }
                        if let Err(e) = timed(publish_discovery(cfg, &client)).await {
                            break Err(e);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    break Err(anyhow!("config channel closed"));
                }
            },

            msg = data_rx.recv() => match msg {
                // Live values are not queued while disconnected: they would be stale.
                // Only end-of-cycle snapshots are published (delay_between_cycles_ms).
                Ok(snapshot) => {
                    if snapshot.end_of_cycle && *connected_rx.borrow() {
                        if let Err(e) = timed(publish_snapshot(cfg, &client, &snapshot)).await {
                            break Err(e);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("MQTT publisher lagged {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break Err(anyhow!("snapshot channel closed"));
                }
            },
        }
    };

    let _ = client.try_disconnect();
    poller.abort();
    result
}

async fn timed(fut: impl std::future::Future<Output = Result<()>>) -> Result<()> {
    tokio::time::timeout(PUBLISH_TIMEOUT, fut)
        .await
        .map_err(|_| anyhow!("publish timed out"))?
}

/// Publish all HA autodiscovery config messages (retained).
async fn publish_discovery(cfg: &AppConfig, client: &AsyncClient) -> Result<()> {
    info!("Publishing HA autodiscovery configs...");

    let app_only = [status_topic(cfg)];

    // Per-inverter discovery
    for inv in cfg.active_inverters() {
        // Live values: unavailable when the app or this inverter is down.
        // Energy counters: only depend on the app.
        let live = [status_topic(cfg), inverter_availability_topic(cfg, inv.id)];
        let device_id = format!("axpert_inv{:02}", inv.id);
        let state_base = format!("{}/inv{:02}", cfg.mqtt.topic_prefix, inv.id);

        let ctx = DiscoveryContext {
            ha_prefix: &cfg.mqtt.ha_discovery_prefix,
            topic_prefix: &state_base,
            device_id: &device_id,
            device_name: &inv.ha_device_name,
            manufacturer: "Voltronic",
            model: "Axpert",
            availability: &live,
        };

        for meta in discovery::inverter_sensors() {
            let (topic, payload) = build_discovery_payload(&ctx, &meta);
            client
                .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
                .await?;
        }

        // Per-inverter energy sensors
        let energy_ctx = DiscoveryContext {
            ha_prefix: &cfg.mqtt.ha_discovery_prefix,
            topic_prefix: &format!("{}/energy/inv{:02}", cfg.mqtt.topic_prefix, inv.id),
            device_id: &device_id,
            device_name: &inv.ha_device_name,
            manufacturer: "Voltronic",
            model: "Axpert",
            availability: &app_only,
        };
        for meta in discovery::energy_sensors_per_inv() {
            let (topic, payload) = build_discovery_payload(&energy_ctx, &meta);
            client
                .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
                .await?;
        }
    }

    // System totals discovery
    let sys_device_id = "axpert_system";
    let totals_ctx = DiscoveryContext {
        ha_prefix: &cfg.mqtt.ha_discovery_prefix,
        topic_prefix: &format!("{}/totals", cfg.mqtt.topic_prefix),
        device_id: sys_device_id,
        device_name: "Axpert System",
        manufacturer: "Voltronic",
        model: "Multi-Inverter",
        availability: &app_only,
    };
    for meta in discovery::totals_sensors() {
        let (topic, payload) = build_discovery_payload(&totals_ctx, &meta);
        client
            .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
            .await?;
    }

    // Energy system totals
    let energy_sys_ctx = DiscoveryContext {
        ha_prefix: &cfg.mqtt.ha_discovery_prefix,
        topic_prefix: &format!("{}/energy/system", cfg.mqtt.topic_prefix),
        device_id: sys_device_id,
        device_name: "Axpert System",
        manufacturer: "Voltronic",
        model: "Multi-Inverter",
        availability: &app_only,
    };
    for meta in discovery::energy_sensors_system() {
        let (topic, payload) = build_discovery_payload(&energy_sys_ctx, &meta);
        client
            .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
            .await?;
    }

    info!("HA autodiscovery published");
    Ok(())
}

/// Inverters active in `old` but not in `new`: delete their HA entities
/// (empty retained config) and mark them offline.
async fn remove_inverter_discovery(old: &AppConfig, new: &AppConfig, client: &AsyncClient) -> Result<()> {
    let kept: Vec<u8> = new.active_inverters().iter().map(|i| i.id).collect();
    for inv in old.active_inverters() {
        if kept.contains(&inv.id) {
            continue;
        }
        info!("Removing HA entities of inverter {}", inv.id);
        let device_id = format!("axpert_inv{:02}", inv.id);
        for meta in discovery::inverter_sensors().iter().chain(discovery::energy_sensors_per_inv().iter()) {
            let topic = format!(
                "{}/{}/{}/{}/config",
                old.mqtt.ha_discovery_prefix, meta.component, device_id, meta.object_id
            );
            client.publish(topic, QoS::AtLeastOnce, true, "").await?;
        }
        client
            .publish(inverter_availability_topic(old, inv.id), QoS::AtLeastOnce, true, "offline")
            .await?;
    }
    Ok(())
}

/// Publish current state for all sensors in a snapshot.
async fn publish_snapshot(
    cfg: &AppConfig,
    client: &AsyncClient,
    snapshot: &SystemSnapshot,
) -> Result<()> {
    // Per-inverter states
    for inv in &snapshot.inverters {
        let base = format!("{}/inv{:02}", cfg.mqtt.topic_prefix, inv.id);
        let availability = if inv.online { "online" } else { "offline" };
        client
            .publish(inverter_availability_topic(cfg, inv.id), QoS::AtLeastOnce, true, availability)
            .await?;
        publish_inverter(client, &base, inv).await?;

        // Per-inverter energy
        let energy_base = format!("{}/energy/inv{:02}", cfg.mqtt.topic_prefix, inv.id);
        if let Some(ref e) = snapshot
            .energy
            .inverter_energy
            .iter()
            .find(|ie| ie.inv_id == inv.id)
        {
            pub_f(client, &energy_base, "energy_pv1_today", e.energy_pv1_today).await?;
            pub_f(client, &energy_base, "energy_pv2_today", e.energy_pv2_today).await?;
            pub_f(client, &energy_base, "energy_pv_today", e.energy_pv_today).await?;
            pub_f(client, &energy_base, "energy_ac_output_today", e.energy_ac_output_today).await?;
            pub_f(client, &energy_base, "energy_battery_charged_today", e.energy_battery_charged_today).await?;
            pub_f(client, &energy_base, "energy_battery_discharged_today", e.energy_battery_discharged_today).await?;
            pub_f(client, &energy_base, "energy_grid_import_today", e.energy_grid_import_today).await?;
            pub_f(client, &energy_base, "energy_pv_to_battery_today", e.energy_pv_to_battery_today).await?;
            pub_f(client, &energy_base, "energy_ac_output_reactive_today", e.energy_ac_output_reactive_today).await?;
            pub_f(client, &energy_base, "energy_pv1_total", e.energy_pv1_total).await?;
            pub_f(client, &energy_base, "energy_pv2_total", e.energy_pv2_total).await?;
            pub_f(client, &energy_base, "energy_pv_total", e.energy_pv_total).await?;
            pub_f(client, &energy_base, "energy_ac_output_total", e.energy_ac_output_total).await?;
            pub_f(client, &energy_base, "energy_battery_charged_total", e.energy_battery_charged_total).await?;
            pub_f(client, &energy_base, "energy_battery_discharged_total", e.energy_battery_discharged_total).await?;
            pub_f(client, &energy_base, "energy_grid_import_total", e.energy_grid_import_total).await?;
            pub_f(client, &energy_base, "energy_pv_to_battery_total", e.energy_pv_to_battery_total).await?;
            pub_f(client, &energy_base, "energy_ac_output_reactive_total", e.energy_ac_output_reactive_total).await?;
        }
    }

    // System totals
    let t = &snapshot.totals;
    let tb = format!("{}/totals", cfg.mqtt.topic_prefix);
    pub_f(client, &tb, "battery_voltage_avg", t.battery_voltage_avg).await?;
    pub_f(client, &tb, "battery_soc_avg", t.battery_soc_avg).await?;
    pub_f(client, &tb, "battery_charging_current_total", t.battery_charging_current_total).await?;
    pub_f(client, &tb, "battery_discharge_current_total", t.battery_discharge_current_total).await?;
    pub_f(client, &tb, "battery_real_charge_total", t.battery_real_charge_total).await?;
    pub_f(client, &tb, "battery_real_power_total", t.battery_real_power_total).await?;
    pub_f(client, &tb, "pv1_power_total", t.pv1_power_total).await?;
    pub_f(client, &tb, "pv2_power_total", t.pv2_power_total).await?;
    pub_f(client, &tb, "pv_power_total", t.pv_power_total).await?;
    pub_f(client, &tb, "pv_input_current_total", t.pv_input_current_total).await?;
    pub_f(client, &tb, "ac_output_active_power_total", t.ac_output_active_power_total).await?;
    pub_f(client, &tb, "ac_output_apparent_power_total", t.ac_output_apparent_power_total).await?;
    pub_f(client, &tb, "ac_output_reactive_power_total", t.ac_output_reactive_power_total).await?;
    pub_f(client, &tb, "output_load_percent_avg", t.output_load_percent_avg).await?;
    pub_f(client, &tb, "grid_voltage_avg", t.grid_voltage_avg).await?;
    pub_f(client, &tb, "grid_frequency_avg", t.grid_frequency_avg).await?;
    pub_f(client, &tb, "grid_input_power_estimate_total", t.grid_input_power_estimate_total).await?;
    pub_f(client, &tb, "heatsink_temperature_max", t.heatsink_temperature_max).await?;
    pub_f(client, &tb, "heatsink_temperature_avg", t.heatsink_temperature_avg).await?;
    pub_u(client, &tb, "inverters_online", t.inverters_online as u64).await?;
    pub_f(client, &tb, "system_power_factor", t.system_power_factor).await?;
    pub_f(client, &tb, "solar_self_sufficiency_percent", t.solar_self_sufficiency_percent).await?;
    pub_u(client, &tb, "system_status", t.system_status as u64).await?;
    if let Some(v) = t.battery_time_remaining_min {
        pub_f(client, &tb, "battery_time_remaining_min", v).await?;
    } else {
        pub_str(client, &tb, "battery_time_remaining_min", NO_VALUE).await?;
    }
    if let Some(v) = t.battery_time_to_full_min {
        pub_f(client, &tb, "battery_time_to_full_min", v).await?;
    } else {
        pub_str(client, &tb, "battery_time_to_full_min", NO_VALUE).await?;
    }

    // System energy totals
    let eb = format!("{}/energy/system", cfg.mqtt.topic_prefix);
    let se = &snapshot.energy.system;
    pub_f(client, &eb, "total_energy_pv_today", se.total_energy_pv_today).await?;
    pub_f(client, &eb, "total_energy_pv1_today", se.total_energy_pv1_today).await?;
    pub_f(client, &eb, "total_energy_pv2_today", se.total_energy_pv2_today).await?;
    pub_f(client, &eb, "total_energy_ac_output_today", se.total_energy_ac_output_today).await?;
    pub_f(client, &eb, "total_energy_battery_charged_today", se.total_energy_battery_charged_today).await?;
    pub_f(client, &eb, "total_energy_battery_discharged_today", se.total_energy_battery_discharged_today).await?;
    pub_f(client, &eb, "total_energy_pv_total", se.total_energy_pv_total).await?;
    pub_f(client, &eb, "total_energy_ac_output_total", se.total_energy_ac_output_total).await?;
    pub_f(client, &eb, "total_energy_grid_import_today", se.total_energy_grid_import_today).await?;
    pub_f(client, &eb, "total_energy_pv_to_battery_today", se.total_energy_pv_to_battery_today).await?;
    pub_f(client, &eb, "total_energy_ac_output_reactive_today", se.total_energy_ac_output_reactive_today).await?;
    pub_f(client, &eb, "total_energy_pv1_total", se.total_energy_pv1_total).await?;
    pub_f(client, &eb, "total_energy_pv2_total", se.total_energy_pv2_total).await?;
    pub_f(client, &eb, "total_energy_battery_charged_total", se.total_energy_battery_charged_total).await?;
    pub_f(client, &eb, "total_energy_battery_discharged_total", se.total_energy_battery_discharged_total).await?;
    pub_f(client, &eb, "total_energy_grid_import_total", se.total_energy_grid_import_total).await?;
    pub_f(client, &eb, "total_energy_pv_to_battery_total", se.total_energy_pv_to_battery_total).await?;
    pub_f(client, &eb, "total_energy_ac_output_reactive_total", se.total_energy_ac_output_reactive_total).await?;
    pub_f(client, &eb, "system_battery_cycles", se.system_battery_cycles).await?;
    pub_f(client, &eb, "co2_saved_today_kg", se.co2_saved_today_kg).await?;
    pub_f(client, &eb, "co2_saved_total_kg", se.co2_saved_total_kg).await?;
    // kWh energy sensors
    pub_f(client, &eb, "total_pv_production_today_kwh", se.total_pv_production_today_kwh).await?;
    pub_f(client, &eb, "total_pv_production_total_kwh", se.total_pv_production_total_kwh).await?;
    pub_f(client, &eb, "total_consumption_today_kwh", se.total_consumption_today_kwh).await?;
    pub_f(client, &eb, "total_consumption_total_kwh", se.total_consumption_total_kwh).await?;
    pub_f(client, &eb, "total_battery_charged_today_kwh", se.total_battery_charged_today_kwh).await?;
    pub_f(client, &eb, "total_battery_charged_total_kwh", se.total_battery_charged_total_kwh).await?;
    pub_f(client, &eb, "total_battery_discharged_today_kwh", se.total_battery_discharged_today_kwh).await?;
    pub_f(client, &eb, "total_battery_discharged_total_kwh", se.total_battery_discharged_total_kwh).await?;
    pub_f(client, &eb, "total_grid_import_today_kwh", se.total_grid_import_today_kwh).await?;
    pub_f(client, &eb, "total_grid_import_total_kwh", se.total_grid_import_total_kwh).await?;
    pub_f(client, &eb, "total_pv_to_battery_today_kwh", se.total_pv_to_battery_today_kwh).await?;
    pub_f(client, &eb, "total_pv_to_battery_total_kwh", se.total_pv_to_battery_total_kwh).await?;

    debug!("MQTT snapshot published");
    Ok(())
}

async fn publish_inverter(
    client: &AsyncClient,
    base: &str,
    inv: &InverterData,
) -> Result<()> {
    if let Some(ref m) = inv.qmod {
        pub_str(client, base, "inverter_mode", &m.mode_name).await?;
    }
    if let Some(ref q) = inv.qpigs {
        pub_f(client, base, "grid_voltage", q.grid_voltage).await?;
        pub_f(client, base, "grid_frequency", q.grid_frequency).await?;
        pub_f(client, base, "ac_output_voltage", q.ac_output_voltage).await?;
        pub_f(client, base, "ac_output_frequency", q.ac_output_frequency).await?;
        pub_i(client, base, "ac_output_apparent_power", q.ac_output_apparent_power as i64).await?;
        pub_i(client, base, "ac_output_active_power", q.ac_output_active_power as i64).await?;
        pub_i(client, base, "output_load_percent", q.output_load_percent as i64).await?;
        pub_i(client, base, "bus_voltage", q.bus_voltage as i64).await?;
        pub_f(client, base, "battery_voltage", q.battery_voltage).await?;
        pub_f(client, base, "battery_voltage_scc", q.battery_voltage_scc).await?;
        pub_i(client, base, "battery_charging_current", q.battery_charging_current as i64).await?;
        pub_i(client, base, "battery_discharge_current", q.battery_discharge_current as i64).await?;
        pub_i(client, base, "battery_capacity", q.battery_capacity as i64).await?;
        pub_i(client, base, "heatsink_temperature", q.heatsink_temperature as i64).await?;
        pub_f(client, base, "pv1_input_voltage", q.pv1_input_voltage).await?;
        pub_f(client, base, "pv1_input_current", q.pv1_input_current).await?;
        pub_i(client, base, "pv1_charging_power", q.pv1_charging_power as i64).await?;
        pub_i(client, base, "battery_voltage_offset_fan", q.battery_voltage_offset_fan as i64).await?;
        pub_i(client, base, "eeprom_version", q.eeprom_version as i64).await?;
        // Status bits
        pub_u(client, base, "status_ac_charging", q.status_ac_charging as u64).await?;
        pub_u(client, base, "status_scc_charging", q.status_scc_charging as u64).await?;
        pub_u(client, base, "status_charging", q.status_charging as u64).await?;
        pub_u(client, base, "status_battery_steady", q.status_battery_steady as u64).await?;
        pub_u(client, base, "status_load_on", q.status_load_on as u64).await?;
        pub_u(client, base, "status_config_changed", q.status_config_changed as u64).await?;
        pub_u(client, base, "status_charging_to_float", q.status_charging_to_float as u64).await?;
    }

    if let Some(ref q2) = inv.qpigs2 {
        pub_f(client, base, "pv2_input_voltage", q2.pv2_input_voltage).await?;
        pub_f(client, base, "pv2_input_current", q2.pv2_input_current).await?;
        pub_i(client, base, "pv2_charging_power", q2.pv2_charging_power as i64).await?;
    }

    if let Some(ref c) = inv.calculated {
        pub_f(client, base, "pv1_input_power", c.pv1_input_power).await?;
        pub_f(client, base, "pv2_input_power", c.pv2_input_power).await?;
        pub_f(client, base, "battery_real_charge_current", c.battery_real_charge_current).await?;
        pub_f(client, base, "battery_real_power", c.battery_real_power).await?;
        pub_f(client, base, "ac_output_reactive_power", c.ac_output_reactive_power).await?;
        pub_f(client, base, "power_factor", c.power_factor).await?;
        pub_f(client, base, "pv_total_input_current", c.pv_total_input_current).await?;
        pub_f(client, base, "grid_input_power_estimate", c.grid_input_power_estimate).await?;
        pub_f(client, base, "pv_total_power", c.pv_total_power).await?;
    }

    if let Some(ref serial) = inv.serial {
        pub_str(client, base, "serial_number", serial).await?;
    }

    // Settings reported by the inverter (QPIRI)
    if let Some(ref r) = inv.qpiri {
        pub_i(client, base, "charger_source_priority", r.charger_source_priority as i64).await?;
        pub_i(client, base, "max_ac_charging_current", r.max_ac_charging_current as i64).await?;
        pub_i(client, base, "output_mode", r.output_mode as i64).await?;
    }

    if let Some(ref f) = inv.qflag {
        pub_u(client, base, "flag_buzzer", f.flag_buzzer as u64).await?;
        pub_u(client, base, "flag_overload_bypass", f.flag_overload_bypass as u64).await?;
        pub_u(client, base, "flag_power_saving", f.flag_power_saving as u64).await?;
        pub_u(client, base, "flag_lcd_timeout_escape", f.flag_lcd_timeout_escape as u64).await?;
        pub_u(client, base, "flag_overload_restart", f.flag_overload_restart as u64).await?;
        pub_u(client, base, "flag_overtemp_restart", f.flag_overtemp_restart as u64).await?;
        pub_u(client, base, "flag_backlight", f.flag_backlight as u64).await?;
        pub_u(client, base, "flag_alarm_primary_input", f.flag_alarm_primary_input as u64).await?;
        pub_u(client, base, "flag_fault_code_record", f.flag_fault_code_record as u64).await?;
    }

    if let Some(ref w) = inv.qpiws {
        pub_u(client, base, "warn_pv_loss", w.warn_pv_loss as u64).await?;
        pub_u(client, base, "warn_inverter_fault", w.warn_inverter_fault as u64).await?;
        pub_u(client, base, "warn_bus_over", w.warn_bus_over as u64).await?;
        pub_u(client, base, "warn_bus_under", w.warn_bus_under as u64).await?;
        pub_u(client, base, "warn_line_fail", w.warn_line_fail as u64).await?;
        pub_u(client, base, "warn_over_temperature", w.warn_over_temperature as u64).await?;
        pub_u(client, base, "warn_fan_locked", w.warn_fan_locked as u64).await?;
        pub_u(client, base, "warn_battery_low_alarm", w.warn_battery_low_alarm as u64).await?;
        pub_u(client, base, "warn_battery_under_shutdown", w.warn_battery_under_shutdown as u64).await?;
        pub_u(client, base, "warn_overload", w.warn_overload as u64).await?;
        pub_u(client, base, "warn_battery_voltage_high", w.warn_battery_voltage_high as u64).await?;
        pub_u(client, base, "warn_inverter_voltage_low", w.warn_inverter_voltage_low as u64).await?;
        pub_u(client, base, "warn_inverter_voltage_high", w.warn_inverter_voltage_high as u64).await?;
        pub_u(client, base, "warn_power_limit", w.warn_power_limit as u64).await?;
        pub_u(client, base, "warn_pv_voltage_high", w.warn_pv_voltage_high as u64).await?;
        pub_u(client, base, "warn_battery_too_low_charge", w.warn_battery_too_low_charge as u64).await?;
        pub_u(client, base, "warn_bus_soft_fail", w.warn_bus_soft_fail as u64).await?;
        pub_u(client, base, "warn_opv_short", w.warn_opv_short as u64).await?;
        pub_u(client, base, "warn_battery_derating", w.warn_battery_derating as u64).await?;
        pub_u(client, base, "warn_eeprom_fault", w.warn_eeprom_fault as u64).await?;
        pub_u(client, base, "warn_inverter_over_current", w.warn_inverter_over_current as u64).await?;
        pub_u(client, base, "warn_inverter_soft_fail", w.warn_inverter_soft_fail as u64).await?;
        pub_u(client, base, "warn_self_test_fail", w.warn_self_test_fail as u64).await?;
        pub_u(client, base, "warn_op_dc_voltage_over", w.warn_op_dc_voltage_over as u64).await?;
        pub_u(client, base, "warn_bat_open", w.warn_bat_open as u64).await?;
        pub_u(client, base, "warn_current_sensor_fail", w.warn_current_sensor_fail as u64).await?;
        pub_u(client, base, "warn_battery_short", w.warn_battery_short as u64).await?;
        pub_u(client, base, "warn_mppt_overload_fault", w.warn_mppt_overload_fault as u64).await?;
        pub_u(client, base, "warn_mppt_overload_warning", w.warn_mppt_overload_warning as u64).await?;
    }

    pub_u(client, base, "inverter_status", inv.inverter_status as u64).await?;

    if let Some(v) = inv.battery_time_remaining_min {
        pub_f(client, base, "battery_time_remaining_min", v).await?;
    } else {
        pub_str(client, base, "battery_time_remaining_min", NO_VALUE).await?;
    }
    if let Some(v) = inv.battery_time_to_full_min {
        pub_f(client, base, "battery_time_to_full_min", v).await?;
    } else {
        pub_str(client, base, "battery_time_to_full_min", NO_VALUE).await?;
    }

    Ok(())
}

// ─── Helper publish functions ─────────────────────────────────────────────────

async fn pub_f(client: &AsyncClient, base: &str, key: &str, val: f64) -> Result<()> {
    let topic = format!("{}/{}", base, key);
    // At most 2 decimals, as shown in the web UI
    let payload = format!("{:.2}", val);
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("MQTT publish {}: {}", topic, e))
}

async fn pub_i(client: &AsyncClient, base: &str, key: &str, val: i64) -> Result<()> {
    let topic = format!("{}/{}", base, key);
    client
        .publish(&topic, QoS::AtLeastOnce, true, val.to_string().as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("MQTT publish {}: {}", topic, e))
}

async fn pub_u(client: &AsyncClient, base: &str, key: &str, val: u64) -> Result<()> {
    let topic = format!("{}/{}", base, key);
    client
        .publish(&topic, QoS::AtLeastOnce, true, val.to_string().as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("MQTT publish {}: {}", topic, e))
}

async fn pub_str(client: &AsyncClient, base: &str, key: &str, val: &str) -> Result<()> {
    let topic = format!("{}/{}", base, key);
    client
        .publish(&topic, QoS::AtLeastOnce, true, val.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("MQTT publish {}: {}", topic, e))
}

/// The full system snapshot broadcast to MQTT publisher and WebSocket clients.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemSnapshot {
    pub inverters: Vec<InverterData>,
    pub totals: SystemTotals,
    pub energy: EnergySnapshot,
    /// True once per `delay_between_cycles_ms` (energy accumulated): only these
    /// snapshots go to MQTT. The others are live updates for the web UI.
    #[serde(skip)]
    pub end_of_cycle: bool,
}
