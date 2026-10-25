# Axpert Multi-Inverter Monitor (ES)

Monitorización y control de **inversores Voltronic Axpert / MPPSolar** (de 1 a 9 equipos, en paralelo o independientes), con interfaz web, MQTT con autodescubrimiento de Home Assistant y despliegue en Docker.

La aplicación está escrita en **Rust** (tokio + axum). Se comunica con cada inversor a través de un conversor TCP/serie, calcula totales del sistema, acumula la energía en una base de datos embebida y publica todo en un broker MQTT.

## ✅ Características

- Lectura de **1 a 9 inversores** mediante los comandos `QPIGS`, `QPIGS2`, `QFLAG`, `QPIRI`, `QPIWS`, `QMOD` y `QID`, con una tarea de sondeo independiente por inversor. Un inversor solo cuenta como «en línea» si responde a `QPIGS` (un conversor TCP que acepta la conexión con el inversor sin responder no basta).
- **Totales del sistema**: potencia FV, consumo, corriente neta de batería, potencia de batería, autonomía estimada, etc.
- **Contadores de energía** diarios y totales, por inversor y del sistema: producción FV (PV1, PV2 y total), consumo (salida AC), carga y descarga de batería, energía FV → batería, importación de red (estimada) y energía reactiva. Además, CO₂ evitado y ciclos de batería. Todo se guarda en una base de datos **redb** (resistente a cortes de luz) y se publica con `state_class: total_increasing`, listo para el panel de Energía de Home Assistant.
- **MQTT** con **autodescubrimiento de Home Assistant**: los sensores aparecen solos en HA.
- **Interfaz web** con actualización en tiempo real por **WebSocket**:
  - Inicio de sesión con usuarios y contraseñas cifradas (bcrypt).
  - Edición de la configuración general y de cada inversor.
  - Envío de comandos de configuración al inversor (prioridades, corrientes de carga, alarmas…).
  - Puesta a cero de contadores de energía y gestión de usuarios.
- **Configuración en caliente**: los cambios en `app_config.conf` se aplican sin reiniciar, incluidos el broker MQTT y añadir, quitar o renombrar inversores.
- **Dos ritmos**: cada inversor se consulta cada `delay_between_inverters_ms` (la web se actualiza a ese ritmo). La energía, los totales y MQTT se actualizan cada `delay_between_cycles_ms`. Los contadores se escriben a disco una vez por minuto y al parar el contenedor.
- **Disponibilidad en Home Assistant**: si la app se para o pierde la conexión (mensaje de última voluntad en `axpert/status`), o si un inversor deja de responder (`axpert/invNN/availability`), sus entidades aparecen como "No disponible" en lugar de mostrar valores antiguos.
- **Modo del inversor** (red, batería, fallo…) mediante QMOD. El modo fallo eleva el estado general a FAULT.

## 📁 Estructura

```
.
├── src/
│   ├── main.rs          # Arranque, agregador de datos
│   ├── config/          # Carga de configuración TOML y vigilancia de cambios
│   ├── inverter/        # Protocolo Voltronic, sondeo y parsers de cada comando
│   ├── totals/          # Cálculo de totales del sistema
│   ├── database/        # Energía acumulada y usuarios (redb)
│   ├── mqtt/            # Publicación MQTT y autodescubrimiento de HA
│   └── web/             # API REST, autenticación y WebSocket
├── www/                 # Interfaz web (HTML, JS, CSS)
├── config/              # Configuración de esta instalación (no entra en la imagen)
├── config.defaults/     # Plantillas que se copian en el primer arranque
├── Dockerfile           # Compilación en dos etapas (Rust → Alpine)
├── docker-compose.yml
├── .env.example       # Plantilla de .env: directorio persistente, puerto y zona horaria
└── entrypoint.sh        # Crea la configuración por defecto si falta
```

## ⚙️ Configuración

| Archivo | Contenido |
|---|---|
| `config/app_config.conf` | Número de inversores, tiempos de sondeo, batería, MQTT, puerto web e IP/puerto TCP de cada inversor |
| `config/invNN_config.conf` | Parámetros de cada inversor (`inv01_config.conf`, `inv02_config.conf`…), editables desde la web |
| `data/axpert.db` | Energía acumulada y usuarios |

Los cambios en `app_config.conf` se aplican sin reiniciar, tanto si se guardan desde la web como si se edita el archivo a mano en el host.

## 💾 Almacenamiento persistente

Todo lo que la aplicación guarda (configuración y base de datos) vive en **un único directorio del host**, fuera del contenedor. Se define con `AXPERT_STORAGE_DIR` en el archivo `.env`, junto a `docker-compose.yml`:

```
${AXPERT_STORAGE_DIR}/
├── config/   → /app/config en el contenedor
└── data/     → /app/data en el contenedor
```

- **Borrar, recrear o actualizar el contenedor no toca ese directorio.**
- **Los datos del volumen siempre tienen prioridad sobre los de la imagen.** Al arrancar, `entrypoint.sh` usa los archivos que ya existan tal cual y nunca los sobrescribe. Solo copia una plantilla de la imagen cuando falta un archivo: en el primer arranque, o al añadir un inversor nuevo en `app_config.conf`.
- En cada arranque se dejan en `config/` las plantillas de la imagen como `app_config.conf.example` e `inv_config.conf.example`. Así se ven las opciones nuevas de una versión sin tocar tu configuración.
- Una versión nueva de la imagen puede leer un `app_config.conf` antiguo: las opciones nuevas toman su valor por defecto y las desconocidas se ignoran.
- La configuración se guarda de forma atómica: un corte de luz a mitad de escritura no deja el archivo corrupto.
- Si `/app/config` o `/app/data` no están montados en un volumen, el contenedor avisa en el log de que esos datos se perderán.

## 🐳 Uso con Docker

### Con docker compose (recomendado)

1. Copia `.env.example` a `.env` y ajústalo:

   ```
   AXPERT_STORAGE_DIR=/main/storage/docker/axpert_monitor_multi_es
   AXPERT_WEB_PORT=60606
   TZ=Europe/Madrid
   ```

2. Revisa la red en `docker-compose.yml` (por defecto usa la red externa `homeassistant` con IP fija).

3. Descarga la imagen de Docker Hub y arranca:

   ```bash
   docker compose pull
   docker compose up -d
   ```

En el primer arranque se crean `config/` y `data/` dentro de `AXPERT_STORAGE_DIR`. Edita después `config/app_config.conf` con tus IPs y credenciales MQTT; se aplica sin reiniciar.

Para actualizar a una imagen nueva sin perder nada:

```bash
docker compose pull
docker compose up -d
```

### Con docker run

```bash
docker run -d \
  --name axpert_monitor_multi \
  --restart unless-stopped \
  -p 60606:60606 \
  -v /ruta/persistente/config:/app/config \
  -v /ruta/persistente/data:/app/data \
  -e TZ=Europe/Madrid \
  pajaropinto/axpert_monitor_multi_es:latest
```

### Acceso a la interfaz web

Abre `http://[tu-servidor]:60606`.

> 🔒 En el primer arranque se crea el usuario **`admin`** con contraseña **`admin`**. Cámbiala nada más entrar.

Roles de usuario:

| Rol | Puede |
|---|---|
| `admin` | Todo: ver datos, cambiar la configuración, enviar comandos a los inversores, poner a cero contadores y gestionar usuarios |
| `user` | Solo ver los datos en tiempo real y cambiar su propia contraseña |

### La imagen

- Basada en **Alpine 3.22**. El programa está compilado de forma estática: no necesita librerías del sistema ni descargar nada al arrancar. Solo se conecta al broker MQTT y a los inversores.
- Incluye herramientas de administración: `sudo`, `nano`, `bash`, `curl`, `jq`, `htop`, `ping`, `dig`, `ip`, `nc` (para probar los conversores TCP de los inversores) y `mosquitto_sub`/`mosquitto_pub` (para probar el broker).
- Alpine usa `apk` en lugar de `apt`. El comando `apt` incluido traduce las órdenes básicas (`apt update`, `apt install PAQUETE`…) a `apk`.

Para construir, probar y publicar la imagen: `./docker/build_and_push.sh` (ver `INSTRUCIONES.txt`).

## 🛠️ Desarrollo

Requisitos: [Rust](https://rustup.rs) (stable).

```bash
cargo build
cargo run
```

El programa usa rutas relativas al directorio de trabajo (`config/`, `data/`, `www/`), así que hay que lanzarlo desde la raíz del proyecto o desde una carpeta con esa misma estructura.

Instrucciones para compilar y publicar la imagen: ver `INSTRUCIONES.txt`.

## 📋 Requisitos de la instalación

- Inversores Voltronic Axpert / MPPSolar compatibles con el protocolo `QPIGS`.
- Un conversor TCP/serie por inversor (o uno que dé acceso a todos).
- Broker MQTT (p. ej. Mosquitto) accesible desde el contenedor.
- Docker.
