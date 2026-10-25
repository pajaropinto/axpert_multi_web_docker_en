# Axpert Multi-Inverter Monitor

Monitoring and control of **Voltronic Axpert / MPPSolar inverters** (1 to 9 units, in parallel or standalone), with a web interface, MQTT with Home Assistant discovery, and Docker deployment.

The application is written in **Rust** (tokio + axum). It talks to each inverter through a TCP/serial converter, computes system totals, accumulates energy in an embedded database and publishes everything to an MQTT broker.

## ✅ Features

- Reads **1 to 9 inverters** with the `QPIGS`, `QPIGS2`, `QFLAG`, `QPIRI`, `QPIWS`, `QMOD` and `QID` commands, with an independent polling task per inverter. An inverter only counts as "online" when it answers `QPIGS` (a TCP converter that accepts the connection while the inverter behind it does not answer is not enough).
- **System totals**: PV power, consumption, net battery current, battery power, estimated runtime, etc.
- **Energy counters**, daily and lifetime, per inverter and for the system: PV production (PV1, PV2 and total), consumption (AC output), battery charge and discharge, PV → battery energy, grid import (estimated) and reactive energy. Also CO₂ avoided and equivalent battery cycles. Everything is stored in a **redb** database (safe against power cuts) and published with `state_class: total_increasing`, ready for the Home Assistant Energy dashboard.
- **MQTT** with **Home Assistant discovery**: the sensors show up in HA by themselves.
- **Web interface** with real-time updates over **WebSocket**:
  - Login with users and hashed passwords (bcrypt).
  - Editing of the general configuration and of each inverter.
  - Sending configuration commands to the inverter (priorities, charging currents, alarms…).
  - Resetting energy counters and managing users.
- **Hot reload**: changes to `app_config.conf` apply without restarting, including the MQTT broker and adding, removing or renaming inverters.
- **Two rates**: each inverter is polled every `delay_between_inverters_ms` (the web UI updates at that rate). Energy, totals and MQTT are updated every `delay_between_cycles_ms`. Counters are written to disk once a minute and when the container stops.
- **Availability in Home Assistant**: if the app stops or loses its connection (last-will message on `axpert/status`), or an inverter stops answering (`axpert/invNN/availability`), its entities show as "Unavailable" instead of stale values.
- **Inverter mode** (line, battery, fault…) through QMOD. Fault mode raises the overall status to FAULT.

## 📁 Structure

```
.
├── src/
│   ├── main.rs          # Startup, data aggregator
│   ├── config/          # TOML configuration loading and change watching
│   ├── inverter/        # Voltronic protocol, polling and a parser per command
│   ├── totals/          # System totals
│   ├── database/        # Accumulated energy and users (redb)
│   ├── mqtt/            # MQTT publishing and HA discovery
│   └── web/             # REST API, authentication and WebSocket
├── www/                 # Web interface (HTML, JS, CSS)
├── config.defaults/     # Templates copied on first start
├── docker/              # Image build/test/push script and apt helper
├── Dockerfile           # Two-stage build (Rust → Alpine)
├── docker-compose.yml
├── .env.example         # Template for .env: storage directory, port and timezone
└── entrypoint.sh        # Creates the default configuration when missing
```

## ⚙️ Configuration

| File | Contents |
|---|---|
| `config/app_config.conf` | Number of inverters, polling delays, battery, MQTT, web port and TCP IP/port of each inverter |
| `config/invNN_config.conf` | Parameters of each inverter (`inv01_config.conf`, `inv02_config.conf`…), editable from the web UI |
| `data/axpert.db` | Accumulated energy and users |

Changes to `app_config.conf` apply without restarting, whether they are saved from the web UI or edited by hand on the host.

## 💾 Persistent storage

Everything the application stores (configuration and database) lives in **a single host directory**, outside the container. It is set with `AXPERT_STORAGE_DIR` in the `.env` file next to `docker-compose.yml`:

```
${AXPERT_STORAGE_DIR}/
├── config/   → /app/config in the container
└── data/     → /app/data in the container
```

- **Deleting, recreating or updating the container never touches that directory.**
- **Data on the volume always wins over the image.** On start, `entrypoint.sh` uses the files that already exist as they are and never overwrites them. It only copies a template from the image when a file is missing: on first start, or when a new inverter is added to `app_config.conf`.
- On every start, the image templates are written to `config/` as `app_config.conf.example` and `inv_config.conf.example`, so the options of a new version can be seen without touching your configuration.
- A newer image can read an older `app_config.conf`: new options take their default value and unknown ones are ignored.
- The configuration is saved atomically: a power cut in the middle of a write never leaves a corrupt file.
- If `/app/config` or `/app/data` are not mounted on a volume, the container warns in its log that that data will be lost.

## 🐳 Using it with Docker

### With docker compose (recommended)

1. Copy `.env.example` to `.env` and adjust it:

   ```
   AXPERT_STORAGE_DIR=/srv/docker/axpert_monitor
   AXPERT_WEB_PORT=60606
   TZ=Europe/Madrid
   ```

2. Check the network in `docker-compose.yml`: it uses an external network called `homeassistant` with a fixed IP. Create that network, or change or remove the `networks` section to suit your setup.

3. Pull the image from Docker Hub and start it:

   ```bash
   docker compose pull
   docker compose up -d
   ```

On first start, `config/` and `data/` are created inside `AXPERT_STORAGE_DIR`. Then edit `config/app_config.conf` with your IPs and MQTT credentials; it applies without restarting.

To update to a new image without losing anything:

```bash
docker compose pull
docker compose up -d
```

### With docker run

```bash
docker run -d \
  --name axpert_monitor_multi \
  --restart unless-stopped \
  -p 60606:60606 \
  -v /persistent/path/config:/app/config \
  -v /persistent/path/data:/app/data \
  -e TZ=Europe/Madrid \
  pajaropinto/axpert_monitor_multi_es:latest
```

### Web interface

Open `http://[your-server]:60606`.

> 🔒 On first start the user **`admin`** with password **`admin`** is created. Change it right after logging in.

User roles:

| Role | Can |
|---|---|
| `admin` | Everything: view data, change the configuration, send commands to the inverters, reset counters and manage users |
| `user` | Only view live data and change their own password |

> ⚠️ Inverter commands differ between Voltronic models and firmwares. The **charger source priority** and **maximum AC charging current** commands have not been verified on real hardware yet: check, in the "Inverter" column of the configuration table (values read back from the inverter with QPIRI), that a setting does what you expect before relying on it.

### The image

- Based on **Alpine 3.22**. The program is statically linked: it needs no system libraries and downloads nothing at start. It only connects to the MQTT broker and the inverters.
- Includes admin tools: `sudo`, `nano`, `bash`, `curl`, `jq`, `htop`, `ping`, `dig`, `ip`, `nc` (to test the inverters' TCP converters) and `mosquitto_sub`/`mosquitto_pub` (to test the broker).
- Alpine uses `apk` instead of `apt`. The bundled `apt` command translates the basic commands (`apt update`, `apt install PACKAGE`…) to `apk`.

To build, test and publish the image: `./docker/build_and_push.sh` (see `INSTRUCTIONS.txt`).

## 🏷️ Versions

The version is defined in `Cargo.toml` (`version = "X.Y.Z"`) and is used everywhere:

- Docker image tags `X.Y.Z`, `X.Y` and `latest` on [Docker Hub](https://hub.docker.com/r/pajaropinto/axpert_monitor_multi_es), created by `docker/build_and_push.sh`.
- Git tag `vX.Y.Z` in this repository.
- The startup log line: `Axpert Multi-Inverter Monitor vX.Y.Z`.

## 🛠️ Development

Requirements: [Rust](https://rustup.rs) (stable).

```bash
cargo build
cargo test
cargo run
```

The program uses paths relative to the working directory (`config/`, `data/`, `www/`), so it has to be started from the project root or from a folder with the same layout.

## 📋 Installation requirements

- Voltronic Axpert / MPPSolar inverters supporting the `QPIGS` protocol.
- One TCP/serial converter per inverter.
- An MQTT broker (e.g. Mosquitto) reachable from the container.
- Docker.

## 📄 License

MIT — see [LICENSE](LICENSE).
