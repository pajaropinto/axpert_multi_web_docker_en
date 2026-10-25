/// InverterManager — async task that polls one inverter.
///
/// Sequence per poll (every `delay_between_inverters_ms`):
///   1. QPIGS  (main status; if it fails the inverter is offline and
///      the rest of the poll is skipped)
///   2. QID    (first poll, then every 60 polls to refresh the serial)
///   3. QPIGS2 (PV2; optional — backs off if the model does not answer)
///   4. QFLAG
///   5. QPIRI  (every 12 polls; the last answer is kept in between)
///   6. QPIWS
///   7. QMOD   (optional — backs off if the model does not answer)
///
/// Sends InverterData via mpsc channel after each poll.
use crate::config::{AppConfig, InverterConfig};
use crate::inverter::{
    config_cmds::{CMD_QFLAG, CMD_QID, CMD_QMOD, CMD_QPIGS, CMD_QPIGS2, CMD_QPIRI, CMD_QPIWS},
    parsers::{qflag, qid, qmod, qpigs, qpigs2, qpiri, qpiws},
    protocol::{self, AckResult},
    InverterData, QpiriData,
};
use anyhow::Result;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, info, warn};

const QPIRI_EVERY_N_POLLS: u64 = 12;
const QID_EVERY_N_POLLS: u64 = 60;
/// Consecutive failures after which an optional command is considered
/// unsupported by the model and only retried every `OPTIONAL_RETRY_AFTER`.
const OPTIONAL_MAX_FAILURES: u32 = 3;
const OPTIONAL_RETRY_AFTER: Duration = Duration::from_secs(300);

pub async fn run(
    inv_cfg: InverterConfig,
    config_rx: broadcast::Receiver<AppConfig>,
    data_tx: mpsc::Sender<InverterData>,
    initial_config: AppConfig,
) {
    let mut config_rx = config_rx;
    let mut inv_cfg = inv_cfg;
    let mut cfg = initial_config;
    let mut state = PollState::default();

    info!(
        "[Inv {}] Manager started → {}:{}",
        inv_cfg.id, inv_cfg.tcp_ip, inv_cfg.tcp_port
    );

    loop {
        // Check for config update (non-blocking)
        while let Ok(new_cfg) = config_rx.try_recv() {
            // Pick up name / address changes for this inverter id
            if let Some(updated) = new_cfg.inverters.iter().find(|i| i.id == inv_cfg.id) {
                if updated.tcp_ip != inv_cfg.tcp_ip || updated.tcp_port != inv_cfg.tcp_port {
                    info!(
                        "[Inv {}] Address changed → {}:{}",
                        inv_cfg.id, updated.tcp_ip, updated.tcp_port
                    );
                }
                inv_cfg = updated.clone();
            }
            cfg = new_cfg;
            debug!("[Inv {}] Config updated", inv_cfg.id);
        }

        let data = poll_inverter(&inv_cfg, &cfg, &mut state).await;
        let _ = data_tx.send(data).await;

        // Each inverter has its own TCP converter: polls run in parallel,
        // one every delay_between_inverters_ms per inverter.
        sleep(Duration::from_millis(cfg.system.delay_between_inverters_ms.max(100))).await;
        state.poll = state.poll.wrapping_add(1);
    }
}

/// What a polling task remembers between polls.
#[derive(Default)]
struct PollState {
    poll: u64,
    serial: Option<String>,
    /// QPIRI is queried every few polls; the last answer is kept in between
    qpiri: Option<QpiriData>,
    qpigs2: OptionalQuery,
    qmod: OptionalQuery,
}

/// Failure tracking for commands that some models do not support, so an
/// unsupported command does not log a warning on every poll.
#[derive(Default)]
struct OptionalQuery {
    failures: u32,
    retry_at: Option<Instant>,
}

impl OptionalQuery {
    fn due(&self) -> bool {
        self.retry_at.is_none_or(|t| Instant::now() >= t)
    }

    fn succeeded(&mut self, inv_id: u8, cmd: &str) {
        if self.failures >= OPTIONAL_MAX_FAILURES {
            info!("[Inv {}] {} answering again", inv_id, cmd);
        }
        self.failures = 0;
        self.retry_at = None;
    }

    fn failed(&mut self, inv_id: u8, cmd: &str, err: &str) {
        self.failures += 1;
        if self.failures < OPTIONAL_MAX_FAILURES {
            warn!("[Inv {}] {}: {}", inv_id, cmd, err);
            return;
        }
        if self.failures == OPTIONAL_MAX_FAILURES {
            warn!(
                "[Inv {}] {} failed {} times in a row ({}) — maybe not supported by this model; \
                 retrying every {} s",
                inv_id,
                cmd,
                self.failures,
                err,
                OPTIONAL_RETRY_AFTER.as_secs()
            );
        } else {
            debug!("[Inv {}] {}: {}", inv_id, cmd, err);
        }
        self.retry_at = Some(Instant::now() + OPTIONAL_RETRY_AFTER);
    }
}

/// Send `cmd` and parse the answer; errors are returned as text for logging.
async fn query<T>(
    stream: &mut TcpStream,
    cmd: &str,
    parse: impl Fn(&str) -> Result<T>,
) -> std::result::Result<T, String> {
    let resp = protocol::send_command(stream, cmd)
        .await
        .map_err(|e| format!("command failed: {}", e))?;
    parse(&resp).map_err(|e| format!("parse failed: {}", e))
}

async fn poll_inverter(inv: &InverterConfig, cfg: &AppConfig, state: &mut PollState) -> InverterData {
    let mut data = InverterData {
        id: inv.id,
        name: inv.name.clone(),
        timestamp: Some(chrono::Utc::now()),
        online: false,
        serial: state.serial.clone(),
        qpiri: state.qpiri.clone(),
        ..Default::default()
    };

    // Open TCP connection
    let mut stream = match protocol::connect(&inv.tcp_ip, inv.tcp_port).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[Inv {}] Cannot connect: {}", inv.id, e);
            return data;
        }
    };

    // QPIGS — main status. A TCP/serial converter accepts connections even
    // when the inverter behind it does not answer, so the inverter only
    // counts as online when QPIGS returns valid data.
    match query(&mut stream, CMD_QPIGS, qpigs::parse).await {
        Ok(d) => {
            data.qpigs = Some(d);
            data.online = true;
        }
        Err(e) => {
            warn!("[Inv {}] QPIGS {} — inverter not answering", inv.id, e);
            return data;
        }
    }

    // QID — serial number (first poll, then periodically)
    if state.serial.is_none() || state.poll % QID_EVERY_N_POLLS == 0 {
        match query(&mut stream, CMD_QID, qid::parse).await {
            Ok(d) => {
                debug!("[Inv {}] QID serial: {}", inv.id, d.serial_number);
                state.serial = Some(d.serial_number.clone());
                data.serial = Some(d.serial_number);
            }
            Err(e) => warn!("[Inv {}] QID {}", inv.id, e),
        }
    }

    // QPIGS2 — PV2 (optional: single-MPPT models do not answer)
    if state.qpigs2.due() {
        match query(&mut stream, CMD_QPIGS2, qpigs2::parse).await {
            Ok(d) => {
                state.qpigs2.succeeded(inv.id, CMD_QPIGS2);
                data.qpigs2 = Some(d);
            }
            Err(e) => state.qpigs2.failed(inv.id, CMD_QPIGS2, &e),
        }
    }

    // QFLAG — configuration flags
    match query(&mut stream, CMD_QFLAG, qflag::parse).await {
        Ok(d) => data.qflag = Some(d),
        Err(e) => warn!("[Inv {}] QFLAG {}", inv.id, e),
    }

    // QPIRI — rated info and settings (less frequent)
    if state.poll % QPIRI_EVERY_N_POLLS == 0 || state.qpiri.is_none() {
        match query(&mut stream, CMD_QPIRI, qpiri::parse).await {
            Ok(d) => {
                state.qpiri = Some(d.clone());
                data.qpiri = Some(d);
            }
            Err(e) => warn!("[Inv {}] QPIRI {}", inv.id, e),
        }
    }

    // QPIWS — warnings
    match query(&mut stream, CMD_QPIWS, qpiws::parse).await {
        Ok(d) => data.qpiws = Some(d),
        Err(e) => warn!("[Inv {}] QPIWS {}", inv.id, e),
    }

    // QMOD — operating mode (optional: line / battery / fault ...)
    if state.qmod.due() {
        match query(&mut stream, CMD_QMOD, qmod::parse).await {
            Ok(d) => {
                state.qmod.succeeded(inv.id, CMD_QMOD);
                data.qmod = Some(d);
            }
            Err(e) => state.qmod.failed(inv.id, CMD_QMOD, &e),
        }
    }

    // Compute derived values
    data.compute_calculated(cfg.system.inverter_efficiency_percent);

    data
}

/// Send a configuration command to a specific inverter and return ACK/NAK.
pub async fn send_config_command(
    tcp_ip: &str,
    tcp_port: u16,
    cmd: &str,
) -> Result<bool> {
    let mut stream = protocol::connect(tcp_ip, tcp_port).await?;
    let response = protocol::send_command(&mut stream, cmd).await?;
    match protocol::parse_ack(&response) {
        AckResult::Ack => Ok(true),
        AckResult::Nak => Ok(false),
        AckResult::Other(s) => {
            warn!("Unexpected response to '{}': {}", cmd, s);
            Ok(false)
        }
    }
}
