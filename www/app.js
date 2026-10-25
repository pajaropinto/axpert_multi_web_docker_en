/* ============================================================
   Solar Monitor - app.js
   Vanilla JS, WebSocket + REST, dynamic tab generation
   ============================================================ */

'use strict';

// ── State ────────────────────────────────────────────────────
const State = {
  ws: null,
  wsReconnectTimer: null,
  snapshot: null,
  activeTab: null,
  inverterCount: 0,
  tabsBuilt: false,
  username: null,
  role: null,
};

// Only admins can change configuration, send commands and manage users
const isAdmin = () => State.role === 'admin';

// ── DOM helpers ──────────────────────────────────────────────
const $ = (sel, ctx = document) => ctx.querySelector(sel);
const $$ = (sel, ctx = document) => Array.from(ctx.querySelectorAll(sel));

function el(tag, props = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') e.className = v;
    else if (k === 'html') e.innerHTML = v;
    else if (k === 'text') e.textContent = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  for (const c of children) {
    if (c == null) continue;
    e.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return e;
}

// Display format for live values: at most 2 decimals, no trailing zeros
function fmt(value) {
  if (value === null || value === undefined) return '—';
  if (typeof value === 'number' && !Number.isInteger(value)) return String(Number(value.toFixed(2)));
  return value;
}

function setVal(id, value) {
  const node = document.getElementById(id);
  if (node) node.textContent = fmt(value);
}

// ── Auth check ───────────────────────────────────────────────
async function checkAuth() {
  try {
    const r = await fetch('/api/status', { credentials: 'include' });
    if (r.status === 401) {
      window.location.href = '/login.html';
      return false;
    }
    const me = await fetch('/api/me', { credentials: 'include' });
    if (me.ok) {
      const d = await me.json();
      State.username = d.username;
      State.role = d.role;
    }
    return true;
  } catch (_) {
    // Server unreachable — still try to connect WS
    return true;
  }
}

// ── WebSocket ─────────────────────────────────────────────────
function connectWS() {
  clearTimeout(State.wsReconnectTimer);

  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const url = `${proto}://${location.host}/ws`;

  setConnStatus('reconnecting');

  try {
    State.ws = new WebSocket(url);
  } catch (e) {
    scheduleReconnect();
    return;
  }

  State.ws.onopen = () => {
    setConnStatus('connected');
  };

  State.ws.onmessage = (ev) => {
    try {
      const data = JSON.parse(ev.data);
      handleSnapshot(data);
    } catch (_) {}
  };

  State.ws.onerror = () => {};

  State.ws.onclose = async () => {
    setConnStatus('disconnected');
    // Sessions live in server memory: after a restart the session is gone and
    // the socket is refused. checkAuth() redirects to the login page then.
    if (await checkAuth()) scheduleReconnect();
  };
}

function scheduleReconnect() {
  clearTimeout(State.wsReconnectTimer);
  State.wsReconnectTimer = setTimeout(connectWS, 3000);
}

function setConnStatus(state) {
  const dot = document.getElementById('conn-dot');
  const label = document.getElementById('conn-label');
  if (!dot || !label) return;
  dot.className = 'conn-dot' + (state === 'connected' ? ' connected' : state === 'reconnecting' ? ' reconnecting' : '');
  label.textContent = state === 'connected' ? 'Connected' : state === 'reconnecting' ? 'Reconnecting…' : 'Disconnected';
}

// ── Snapshot handling ─────────────────────────────────────────
function handleSnapshot(snap) {
  State.snapshot = snap;

  const numInv = (snap.inverters || []).length;

  // Names can change at runtime (config hot-reload): keep tab labels in sync
  for (const inv of snap.inverters || []) {
    const btn = document.querySelector(`.tab-btn[data-tab="inv-${inv.id}"]`);
    const label = inv.name || `Inverter ${inv.id}`;
    if (btn && btn.textContent !== label) btn.textContent = label;
  }

  if (!State.tabsBuilt || numInv !== State.inverterCount) {
    State.inverterCount = numInv;
    buildTabs(snap);
    State.tabsBuilt = true;
    // Hide initial loading
    const loadEl = document.getElementById('initial-loading');
    if (loadEl) loadEl.remove();
  }

  updateAllTabs(snap);
}

// ── Tab architecture ──────────────────────────────────────────
function buildTabs(snap) {
  const tabBar = document.getElementById('tab-bar');
  const tabContent = document.getElementById('tab-content');

  tabBar.innerHTML = '';
  // Remove all existing panes except loading overlay
  $$('.tab-pane', tabContent).forEach(p => p.remove());

  const tabs = [];

  // Inverter tabs
  for (const inv of snap.inverters) {
    tabs.push({ id: `inv-${inv.id}`, label: inv.name || `Inverter ${inv.id}` });
  }
  // Fixed tabs
  tabs.push({ id: 'totals', label: 'Totals' });
  if (isAdmin()) tabs.push({ id: 'appconfig', label: 'App Config' });
  tabs.push({ id: 'admin', label: isAdmin() ? 'Admin' : 'Account' });

  for (const tab of tabs) {
    const btn = el('button', {
      class: 'tab-btn',
      'data-tab': tab.id,
      onclick: () => activateTab(tab.id),
      text: tab.label,
    });
    tabBar.appendChild(btn);

    const pane = el('div', { class: 'tab-pane', id: `pane-${tab.id}` });

    if (tab.id.startsWith('inv-')) {
      const invId = parseInt(tab.id.split('-')[1], 10);
      pane.appendChild(buildInverterPane(invId, snap));
    } else if (tab.id === 'totals') {
      pane.appendChild(buildTotalsPane(snap));
    } else if (tab.id === 'appconfig') {
      pane.appendChild(buildAppConfigPane());
    } else if (tab.id === 'admin') {
      pane.appendChild(buildAdminPane());
    }

    tabContent.appendChild(pane);
  }

  // Activate first tab
  const firstTab = State.activeTab && document.getElementById(`pane-${State.activeTab}`)
    ? State.activeTab
    : tabs[0].id;
  activateTab(firstTab);
}

function activateTab(id) {
  State.activeTab = id;
  $$('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === id));
  $$('.tab-pane').forEach(p => p.classList.toggle('active', p.id === `pane-${id}`));

  // Auto-load on tab open
  if (id === 'admin' && isAdmin()) loadUsers();
  if (id === 'appconfig') {
    const feedbackEl = document.getElementById('appconfig-feedback');
    const loadBtn    = document.getElementById('appconfig-loadbtn');
    loadAndRenderAppConfig(loadBtn, feedbackEl);
  }
}

// ── Update all live data ──────────────────────────────────────
function updateAllTabs(snap) {
  const energies = snap.energy?.inverter_energy || [];
  for (const inv of snap.inverters) {
    updateInverterLiveData({ ...inv, energy: energies.find(e => e.inv_id === inv.id) });
  }
  updateTotalsLiveData(snap);
}

// ============================================================
// INVERTER PANE
// ============================================================

function buildInverterPane(invId, snap) {
  const inv = snap.inverters.find(i => i.id === invId) || {};
  const isFirst = invId === snap.inverters[0]?.id;

  const wrapper = el('div', { class: 'two-col' });

  // LEFT: Live Data
  const livePanel = el('div', { class: 'panel' });
  livePanel.appendChild(el('div', { class: 'panel-header', text: 'Live Data' }));
  const liveBody = el('div', { class: 'panel-body', id: `live-body-${invId}` });
  liveBody.appendChild(buildLiveDataSections(invId));
  livePanel.appendChild(liveBody);

  // RIGHT: Configuration
  const cfgPanel = el('div', { class: 'panel' });
  cfgPanel.appendChild(el('div', { class: 'panel-header', text: 'Configuration' }));
  const cfgBody = el('div', { class: 'panel-body' });
  cfgBody.appendChild(buildConfigForm(invId, isFirst));
  cfgPanel.appendChild(cfgBody);

  wrapper.appendChild(livePanel);
  if (isAdmin()) wrapper.appendChild(cfgPanel);
  return wrapper;
}

// ── Live data section definitions ────────────────────────────
const LIVE_SECTIONS = [
  {
    title: 'Grid',
    fields: [
      ['mode_name', 'Inverter Mode'],
      ['grid_voltage', 'Grid Voltage'],
      ['grid_frequency', 'Grid Frequency'],
      ['grid_input_power_estimate', 'Grid Input Power (est.)'],
    ],
  },
  {
    title: 'AC Output',
    fields: [
      ['ac_output_voltage', 'AC Output Voltage'],
      ['ac_output_frequency', 'AC Output Frequency'],
      ['ac_output_apparent_power', 'Apparent Power'],
      ['ac_output_active_power', 'Active Power'],
      ['ac_output_reactive_power', 'Reactive Power'],
      ['output_load_percent', 'Load %'],
      ['power_factor', 'Power Factor'],
    ],
  },
  {
    title: 'Battery',
    fields: [
      ['battery_voltage', 'Voltage'],
      ['battery_voltage_scc', 'Voltage SCC'],
      ['battery_charging_current', 'Charge Current'],
      ['battery_discharge_current', 'Discharge Current'],
      ['battery_real_charge_current', 'Real Charge Current'],
      ['battery_real_power', 'Real Power'],
      ['battery_capacity', 'Capacity %'],
      ['battery_voltage_offset_fan', 'Voltage Offset Fan'],
    ],
  },
  {
    title: 'PV Solar',
    fields: [
      ['pv1_input_voltage', 'PV1 Voltage'],
      ['pv1_input_current', 'PV1 Current'],
      ['pv1_input_power', 'PV1 Power'],
      ['pv1_charging_power', 'PV1 Charging Power'],
      ['pv2_input_voltage', 'PV2 Voltage'],
      ['pv2_input_current', 'PV2 Current'],
      ['pv2_input_power', 'PV2 Power'],
      ['pv2_charging_power', 'PV2 Charging Power'],
      ['pv_total_input_current', 'PV Input Current'],
    ],
  },
  {
    title: 'Energy (kWh)',
    fields: [
      ['energy_pv_today', 'PV Today'],
      ['energy_pv_total', 'PV Lifetime'],
      ['energy_pv1_today', 'PV1 Today'],
      ['energy_pv2_today', 'PV2 Today'],
      ['energy_ac_output_today', 'AC Output Today'],
      ['energy_ac_output_total', 'AC Output Lifetime'],
      ['energy_grid_import_today', 'Grid Import Today (est.)'],
      ['energy_grid_import_total', 'Grid Import Lifetime (est.)'],
      ['energy_battery_charged_today', 'Battery Charged Today'],
      ['energy_battery_discharged_today', 'Battery Discharged Today'],
      ['energy_pv_to_battery_today', 'PV to Battery Today'],
      ['energy_ac_output_reactive_today', 'Reactive Today (kvarh)'],
    ],
  },
  {
    title: 'Temperature',
    fields: [
      ['heatsink_temperature', 'Heatsink Temp'],
    ],
  },
];

const STATUS_FIELDS = [
  ['status_ac_charging', 'AC Charging'],
  ['status_scc_charging', 'SCC Charging'],
  ['status_load_on', 'Load On'],
  ['status_charging_to_float', 'Charging to Float'],
  ['eeprom_version', 'EEPROM Version'],
];

const FLAG_FIELDS = [
  ['flag_buzzer', 'Buzzer'],
  ['flag_overload_bypass', 'Overload Bypass'],
  ['flag_power_saving', 'Power Saving'],
  ['flag_lcd_timeout_escape', 'LCD Timeout Escape'],
  ['flag_overload_restart', 'Overload Restart'],
  ['flag_overtemp_restart', 'Overtemp Restart'],
  ['flag_backlight', 'Backlight'],
  ['flag_alarm_primary_input', 'Alarm Primary Input'],
  ['flag_fault_code_record', 'Fault Code Record'],
];

function buildLiveDataSections(invId) {
  const frag = document.createDocumentFragment();

  // Standard sections from qpigs/qpigs2/calculated
  for (const sec of LIVE_SECTIONS) {
    const section = el('div', { class: 'data-section' });
    section.appendChild(el('div', { class: 'section-title', text: sec.title }));
    const grid = el('div', { class: 'data-grid' });
    for (const [key, label] of sec.fields) {
      grid.appendChild(el('div', { class: 'data-label', text: label }));
      grid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-${key}`, text: '—' }));
    }
    section.appendChild(grid);
    frag.appendChild(section);
  }

  // Status section
  const statusSec = el('div', { class: 'data-section' });
  statusSec.appendChild(el('div', { class: 'section-title', text: 'Status' }));
  const statusGrid = el('div', { class: 'data-grid' });
  for (const [key, label] of STATUS_FIELDS) {
    statusGrid.appendChild(el('div', { class: 'data-label', text: label }));
    statusGrid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-${key}`, text: '—' }));
  }
  statusSec.appendChild(statusGrid);
  frag.appendChild(statusSec);

  // Flags section
  const flagSec = el('div', { class: 'data-section' });
  flagSec.appendChild(el('div', { class: 'section-title', text: 'Flags' }));
  const flagGrid = el('div', { class: 'data-grid' });
  for (const [key, label] of FLAG_FIELDS) {
    flagGrid.appendChild(el('div', { class: 'data-label', text: label }));
    flagGrid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-${key}`, text: '—' }));
  }
  flagSec.appendChild(flagGrid);
  frag.appendChild(flagSec);

  // Warnings section
  const warnSec = el('div', { class: 'data-section' });
  warnSec.appendChild(el('div', { class: 'section-title', text: 'Warnings' }));
  const warnGrid = el('div', { class: 'data-grid', id: `warn-grid-${invId}` });
  warnSec.appendChild(warnGrid);
  frag.appendChild(warnSec);

  // Times section
  const timeSec = el('div', { class: 'data-section' });
  timeSec.appendChild(el('div', { class: 'section-title', text: 'Times' }));
  const timeGrid = el('div', { class: 'data-grid' });
  timeGrid.appendChild(el('div', { class: 'data-label', text: 'Battery Time Remaining' }));
  timeGrid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-battery_time_remaining_min`, text: '—' }));
  timeGrid.appendChild(el('div', { class: 'data-label', text: 'Battery Time to Full' }));
  timeGrid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-battery_time_to_full_min`, text: '—' }));
  timeSec.appendChild(timeGrid);
  frag.appendChild(timeSec);

  // Inverter Status badge
  const badgeSec = el('div', { class: 'data-section' });
  badgeSec.appendChild(el('div', { class: 'section-title', text: 'Inverter Status' }));
  badgeSec.appendChild(el('span', { class: 'status-badge ok', id: `inv-status-badge-${invId}`, text: 'OK' }));
  frag.appendChild(badgeSec);

  return frag;
}

// ── Live data update ──────────────────────────────────────────
function flattenInverter(inv) {
  // Merge all sub-objects into a flat map
  const flat = {};
  const merge = (obj) => { if (obj && typeof obj === 'object') Object.assign(flat, obj); };
  merge(inv.qpigs);
  merge(inv.qpigs2);
  merge(inv.qflag);
  merge(inv.qpiri);
  merge(inv.qpiws);
  merge(inv.qmod);
  merge(inv.calculated);
  // Energy counters come in Wh (varh for reactive): show them in kWh / kvarh
  if (inv.energy) {
    for (const [k, v] of Object.entries(inv.energy)) {
      if (k.startsWith('energy_') && typeof v === 'number') flat[k] = v / 1000;
    }
  }
  // Top-level fields
  flat.battery_time_remaining_min = inv.battery_time_remaining_min;
  flat.battery_time_to_full_min = inv.battery_time_to_full_min;
  flat.inverter_status = inv.inverter_status;
  flat.online = inv.online;
  return flat;
}

function updateInverterLiveData(inv) {
  const invId = inv.id;
  const flat = flattenInverter(inv);

  // Update standard fields
  const allFields = [
    ...LIVE_SECTIONS.flatMap(s => s.fields),
    ...STATUS_FIELDS,
    ...FLAG_FIELDS,
    ['battery_time_remaining_min', ''],
    ['battery_time_to_full_min', ''],
  ];

  for (const [key] of allFields) {
    const node = document.getElementById(`val-${invId}-${key}`);
    if (!node) continue;
    const val = flat[key];
    if (val === null || val === undefined) {
      node.textContent = '—';
      node.className = 'data-value';
    } else if (typeof val === 'boolean') {
      node.textContent = val ? 'Yes' : 'No';
      node.className = `data-value ${val ? 'bool-true' : 'bool-false'}`;
    } else if (key === 'battery_time_remaining_min' || key === 'battery_time_to_full_min') {
      node.textContent = `${fmt(val)} min`;
    } else {
      node.textContent = fmt(val);
      node.className = 'data-value';
    }
  }

  // Warnings — dynamic
  const warnGrid = document.getElementById(`warn-grid-${invId}`);
  if (warnGrid) {
    // Collect warn_* keys from flat
    const warnKeys = Object.keys(flat).filter(k => k.startsWith('warn_'));
    if (warnKeys.length && !warnGrid.dataset.built) {
      warnGrid.innerHTML = '';
      for (const k of warnKeys) {
        warnGrid.appendChild(el('div', { class: 'data-label', text: k.replace(/^warn_/, '').replace(/_/g, ' ') }));
        warnGrid.appendChild(el('div', { class: 'data-value', id: `val-${invId}-${k}`, text: '—' }));
      }
      warnGrid.dataset.built = '1';
    }
    for (const k of warnKeys) {
      const node = document.getElementById(`val-${invId}-${k}`);
      if (!node) continue;
      const v = flat[k];
      node.textContent = v === null || v === undefined ? '—' : String(v);
      node.className = `data-value warn-${v === 0 ? '0' : v === 1 ? '1' : v === 2 ? '2' : '0'}`;
    }
  }

  // Configuration table: values the inverter reports (QPIRI / QFLAG)
  for (const param of CONFIG_PARAMS) {
    const node = document.getElementById(`cfg-curr-${invId}-${param.key}`);
    if (node) node.textContent = param.live ? (param.live(flat) ?? '—') : '—';
  }

  // Status badge
  const badge = document.getElementById(`inv-status-badge-${invId}`);
  if (badge) {
    const s = inv.inverter_status;
    if (!inv.online) { badge.className = 'status-badge fault'; badge.textContent = 'OFFLINE'; }
    else if (s === 0) { badge.className = 'status-badge ok'; badge.textContent = 'OK'; }
    else if (s === 1) { badge.className = 'status-badge warning'; badge.textContent = 'WARNING'; }
    else if (s === 2) { badge.className = 'status-badge fault'; badge.textContent = 'FAULT'; }
    else { badge.className = 'status-badge ok'; badge.textContent = inv.online ? 'Online' : 'Offline'; }
  }
}

// ── Configuration form ────────────────────────────────────────
// live: current value as reported by the inverter (QPIRI or QFLAG), null if unknown.
// readCmd: query shown by the Read button.
const pad2 = v => (v === undefined || v === null) ? null : String(v).padStart(2, '0');
// Flag state from QFLAG (1 = enabled) as the command code that sets it
const flagCode = (v, on, off) => (v === undefined || v === null) ? null : (v === 1 ? on : off);

const CONFIG_PARAMS = [
  {
    key: 'source_priority',
    readCmd: 'QPIRI',
    live: f => pad2(f.output_source_priority),
    label: 'Source Priority',
    type: 'select',
    options: [['00', 'Utility First'], ['01', 'Solar First'], ['02', 'SBU']],
    inv1Only: true,
  },
  {
    key: 'ac_input_range',
    readCmd: 'QPIRI',
    live: f => pad2(f.input_voltage_range),
    label: 'AC Input Range',
    type: 'select',
    options: [['00', 'Appliance'], ['01', 'UPS']],
  },
  {
    key: 'output_frequency',
    readCmd: 'QPIRI',
    live: f => f.ac_output_rating_frequency == null ? null : String(Math.round(f.ac_output_rating_frequency)),
    label: 'Output Frequency',
    type: 'select',
    options: [['50', '50 Hz'], ['60', '60 Hz']],
    inv1Only: true,
  },
  {
    key: 'max_charger_current',
    readCmd: 'QPIRI',
    live: f => f.max_ac_charging_current ?? null,
    label: 'Max Charger Current',
    type: 'number',
  },
  {
    key: 'charger_priority',
    readCmd: 'QPIRI',
    live: f => pad2(f.charger_source_priority),
    label: 'Charger Priority',
    type: 'select',
    options: [['00', 'Solar First'], ['01', 'Solar + Utility'], ['02', 'Only Solar']],
  },
  {
    key: 'max_discharge_current',
    readCmd: 'QPIRI',
    label: 'Max Discharge Current',
    type: 'number',
  },
  {
    key: 'overload_restart',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_overload_restart, 'PEU', 'PDU'),
    label: 'Overload Restart',
    type: 'select',
    options: [['PEU', 'Enable'], ['PDU', 'Disable']],
  },
  {
    key: 'overtemp_restart',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_overtemp_restart, 'PEV', 'PDV'),
    label: 'Overtemp Restart',
    type: 'select',
    options: [['PEV', 'Enable'], ['PDV', 'Disable']],
  },
  {
    key: 'buzzer',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_buzzer, 'PEA', 'PDA'),
    label: 'Buzzer',
    type: 'select',
    options: [['PEA', 'Enable'], ['PDA', 'Disable']],
  },
  {
    key: 'lcd_escape',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_lcd_timeout_escape, 'PEK', 'PDK'),
    label: 'LCD Escape',
    type: 'select',
    options: [['PEK', 'Enable'], ['PDK', 'Disable']],
  },
  {
    key: 'backlight',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_backlight, 'PEX', 'PDX'),
    label: 'Backlight',
    type: 'select',
    options: [['PEX', 'Enable'], ['PDX', 'Disable']],
  },
  {
    key: 'grid_loss_alarm',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_alarm_primary_input, 'PEY', 'PDY'),
    label: 'Grid Loss Alarm',
    type: 'select',
    options: [['PEY', 'Enable'], ['PDY', 'Disable']],
  },
  {
    key: 'overload_bypass',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_overload_bypass, 'PEB', 'PDB'),
    label: 'Overload Bypass',
    type: 'select',
    options: [['PEB', 'Enable'], ['PDB', 'Disable']],
  },
  {
    key: 'fault_logging',
    readCmd: 'QFLAG',
    live: f => flagCode(f.flag_fault_code_record, 'PEZ', 'PDZ'),
    label: 'Fault Logging',
    type: 'select',
    options: [['PEZ', 'Enable'], ['PDZ', 'Disable']],
  },
  {
    key: 'equalization',
    readCmd: 'QPIRI',
    label: 'Equalization',
    type: 'select',
    options: [['0', 'Disable'], ['1', 'Enable']],
  },
];

function buildConfigForm(invId, isFirst) {
  const frag = document.createDocumentFragment();

  const table = el('table', { class: 'config-table' });
  const thead = el('thead');
  thead.appendChild(el('tr', {}, ...['Parameter', 'Inverter', 'New value', 'Actions'].map(t =>
    el('th', { text: t })
  )));
  table.appendChild(thead);

  const tbody = el('tbody');

  for (const param of CONFIG_PARAMS) {
    if (param.inv1Only && !isFirst) continue;

    const tr = el('tr');

    // Name
    const tdName = el('td', { class: 'config-param-name', text: param.label });
    tr.appendChild(tdName);

    // Current value display
    const tdCurr = el('td', { class: 'config-current' });
    const currSpan = el('span', { id: `cfg-curr-${invId}-${param.key}`, text: '—' });
    tdCurr.appendChild(currSpan);
    tr.appendChild(tdCurr);

    // Input
    const tdInput = el('td', { class: 'config-input-cell' });
    let inputEl;
    if (param.type === 'select') {
      inputEl = el('select', { id: `cfg-input-${invId}-${param.key}` });
      for (const [val, lbl] of param.options) {
        inputEl.appendChild(el('option', { value: val, text: `${val} = ${lbl}` }));
      }
    } else {
      inputEl = el('input', { type: 'number', id: `cfg-input-${invId}-${param.key}` });
    }
    tdInput.appendChild(inputEl);
    tr.appendChild(tdInput);

    // Actions
    const tdAct = el('td');
    const actDiv = el('div', { class: 'btn-actions' });

    const readBtn = el('button', {
      class: 'btn btn-warning btn-sm',
      text: 'Read',
      onclick: async function () {
        await readParam(invId, param, readBtn);
      },
    });

    const sendBtn = el('button', {
      class: 'btn btn-success btn-sm',
      text: 'Send',
      onclick: async function () {
        await sendParam(invId, param.key, inputEl, sendBtn);
      },
    });

    actDiv.appendChild(readBtn);
    actDiv.appendChild(sendBtn);
    tdAct.appendChild(actDiv);
    tr.appendChild(tdAct);

    tbody.appendChild(tr);
  }

  table.appendChild(tbody);
  frag.appendChild(table);

  // Bottom actions
  const actions = el('div', { class: 'config-actions' });
  const feedbackEl = el('span', { class: 'feedback', id: `cfg-feedback-${invId}` });

  const loadBtn = el('button', { class: 'btn btn-primary', text: 'Load Config',
    onclick: () => loadInverterConfig(invId, loadBtn, feedbackEl)
  });
  const saveBtn = el('button', { class: 'btn btn-success', text: 'Save Config',
    onclick: () => saveInverterConfig(invId, saveBtn, feedbackEl)
  });

  actions.appendChild(loadBtn);
  actions.appendChild(saveBtn);
  actions.appendChild(feedbackEl);
  frag.appendChild(actions);

  return frag;
}

async function readParam(invId, param, btn) {
  const command = param.readCmd || 'QPIRI';
  btn.disabled = true;
  const orig = btn.textContent;
  btn.innerHTML = '<span class="spinner"></span>';
  try {
    const r = await apiFetch('/api/read', {
      method: 'POST',
      body: JSON.stringify({ inv_id: invId, command }),
    });
    const data = await r.json();
    showModal(`${command} — ${param.label} (Inverter ${invId})`, JSON.stringify(data, null, 2));
  } catch (e) {
    showModal('Read Error', String(e));
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

async function sendParam(invId, param, inputEl, btn) {
  const value = inputEl.value;
  if (value === '' || value === null) { showFeedback(`cfg-feedback-${invId}`, 'error', 'No value entered.'); return; }

  btn.disabled = true;
  const orig = btn.textContent;
  btn.innerHTML = '<span class="spinner"></span>';
  try {
    const r = await apiFetch('/api/command', {
      method: 'POST',
      body: JSON.stringify({ inv_id: invId, param, value }),
    });
    const data = await r.json().catch(() => ({}));
    if (r.ok && data.success) {
      showFeedback(`cfg-feedback-${invId}`, 'success', data.message || 'Command sent.');
    } else {
      showFeedback(`cfg-feedback-${invId}`, 'error', data.message || `HTTP ${r.status}`);
    }
  } catch (e) {
    showFeedback(`cfg-feedback-${invId}`, 'error', 'Network error.');
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

async function loadInverterConfig(invId, btn, feedbackEl) {
  if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner"></span> Loading…'; }
  try {
    const r = await apiFetch(`/api/config/inverter/${invId}`);
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    const cfg = await r.json();
    // API returns { params: { source_priority: "02", ... } }
    const params = cfg.params || cfg;
    for (const param of CONFIG_PARAMS) {
      const val = params[param.key];
      if (val === undefined || val === null) continue;
      const inputEl = document.getElementById(`cfg-input-${invId}-${param.key}`);
      if (inputEl) inputEl.value = val;
    }
    if (feedbackEl) showFeedbackEl(feedbackEl, 'success', 'Config loaded.');
  } catch (e) {
    if (feedbackEl) showFeedbackEl(feedbackEl, 'error', `Load failed: ${e.message}`);
  } finally {
    if (btn) { btn.disabled = false; btn.textContent = 'Load Config'; }
  }
}

async function saveInverterConfig(invId, btn, feedbackEl) {
  const params = {};
  for (const param of CONFIG_PARAMS) {
    const inputEl = document.getElementById(`cfg-input-${invId}-${param.key}`);
    if (inputEl && inputEl.value !== '') params[param.key] = inputEl.value;
  }

  if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner"></span> Saving…'; }
  try {
    // API expects InverterParamConfig: { params: { ... } }
    const r = await apiFetch(`/api/config/inverter/${invId}`, {
      method: 'PUT',
      body: JSON.stringify({ params }),
    });
    if (r.ok) {
      if (feedbackEl) showFeedbackEl(feedbackEl, 'success', 'Config saved.');
    } else {
      const data = await r.json().catch(() => ({}));
      if (feedbackEl) showFeedbackEl(feedbackEl, 'error', data.message || `HTTP ${r.status}`);
    }
  } catch (e) {
    if (feedbackEl) showFeedbackEl(feedbackEl, 'error', 'Save failed.');
  } finally {
    if (btn) { btn.disabled = false; btn.textContent = 'Save Config'; }
  }
}

// ============================================================
// TOTALS PANE
// ============================================================

function buildTotalsPane(snap) {
  const wrapper = el('div', {});

  // KPI row
  const kpiRow = el('div', { class: 'totals-kpi-row', id: 'totals-kpi-row' });
  wrapper.appendChild(kpiRow);

  // Two columns
  const twoCols = el('div', { class: 'two-col' });

  // LEFT: live totals
  const leftPanel = el('div', { class: 'panel' });
  leftPanel.appendChild(el('div', { class: 'panel-header', text: 'System Totals' }));
  const leftBody = el('div', { class: 'panel-body', id: 'totals-live-body' });
  leftBody.appendChild(buildTotalsLiveBody());
  leftPanel.appendChild(leftBody);

  // RIGHT: common config
  const rightPanel = el('div', { class: 'panel' });
  rightPanel.appendChild(el('div', { class: 'panel-header', text: 'System Configuration' }));
  const rightBody = el('div', { class: 'panel-body' });
  rightBody.appendChild(buildTotalsConfig());
  rightPanel.appendChild(rightBody);

  twoCols.appendChild(leftPanel);
  if (isAdmin()) twoCols.appendChild(rightPanel);
  wrapper.appendChild(twoCols);

  return wrapper;
}

function buildTotalsLiveBody() {
  const frag = document.createDocumentFragment();

  const totalFields = [
    // ── Power / Grid ──────────────────────────────────
    ['pv_power_total',                     'PV Power Total'],
    ['pv1_power_total',                    'PV1 Power Total'],
    ['pv2_power_total',                    'PV2 Power Total'],
    ['pv_input_current_total',             'PV Input Current Total'],
    ['ac_output_active_power_total',       'AC Output Power'],
    ['ac_output_apparent_power_total',     'AC Apparent Power'],
    ['ac_output_reactive_power_total',     'AC Reactive Power'],
    ['output_load_percent_avg',            'Load %  (avg)'],
    ['system_power_factor',                'Power Factor'],
    ['solar_self_sufficiency_percent',     'Solar Self-Sufficiency Today %'],
    // ── Battery ───────────────────────────────────────
    ['battery_voltage_avg',                'Battery Voltage (avg)'],
    ['battery_soc_avg',                    'Battery SOC % (avg)'],
    ['battery_charging_current_total',     'Charging Current Total'],
    ['battery_discharge_current_total',    'Discharge Current Total'],
    ['battery_real_charge_total',          'Net Charge Current'],
    ['battery_real_power_total',           'Net Battery Power'],
    ['battery_time_remaining_min',         'Battery Time Remaining (min)'],
    ['battery_time_to_full_min',           'Battery Time to Full (min)'],
    // ── Grid ─────────────────────────────────────────
    ['grid_voltage_avg',                   'Grid Voltage (avg)'],
    ['grid_frequency_avg',                 'Grid Frequency (avg)'],
    ['grid_input_power_estimate_total',    'Grid Input Power (est.)'],
    // ── System ────────────────────────────────────────
    ['inverters_online',                   'Inverters Online'],
    ['heatsink_temperature_max',           'Heatsink Temp Max (°C)'],
    ['heatsink_temperature_avg',           'Heatsink Temp Avg (°C)'],
    ['system_status',                      'System Status'],
  ];

  const secTotals = el('div', { class: 'data-section' });
  secTotals.appendChild(el('div', { class: 'section-title', text: 'Totals' }));
  const gridTotals = el('div', { class: 'data-grid' });
  for (const [key, label] of totalFields) {
    gridTotals.appendChild(el('div', { class: 'data-label', text: label }));
    gridTotals.appendChild(el('div', { class: 'data-value', id: `totals-val-${key}`, text: '—' }));
  }
  secTotals.appendChild(gridTotals);
  frag.appendChild(secTotals);

  // Energy section (kWh)
  const energyFields = [
    ['total_pv_production_today_kwh',            'PV Production Today (kWh)'],
    ['total_pv_production_total_kwh',            'PV Production Lifetime (kWh)'],
    ['total_consumption_today_kwh',              'Consumption Today (kWh)'],
    ['total_consumption_total_kwh',              'Consumption Lifetime (kWh)'],
    ['total_battery_charged_today_kwh',          'Battery Charged Today (kWh)'],
    ['total_battery_charged_total_kwh',          'Battery Charged Lifetime (kWh)'],
    ['total_battery_discharged_today_kwh',       'Battery Discharged Today (kWh)'],
    ['total_battery_discharged_total_kwh',       'Battery Discharged Lifetime (kWh)'],
    ['total_grid_import_today_kwh',              'Grid Import Today (kWh, est.)'],
    ['total_grid_import_total_kwh',              'Grid Import Lifetime (kWh, est.)'],
    ['total_pv_to_battery_today_kwh',            'PV to Battery Today (kWh)'],
    ['total_pv_to_battery_total_kwh',            'PV to Battery Lifetime (kWh)'],
    ['co2_saved_today_kg',                       'CO₂ Saved Today (kg)'],
    ['co2_saved_total_kg',                       'CO₂ Saved Lifetime (kg)'],
    ['system_battery_cycles',                    'Battery Cycles (equivalent)'],
  ];
  const secEnergy = el('div', { class: 'data-section' });
  secEnergy.appendChild(el('div', { class: 'section-title', text: 'Energy' }));
  const gridEnergy = el('div', { class: 'data-grid' });
  for (const [key, label] of energyFields) {
    gridEnergy.appendChild(el('div', { class: 'data-label', text: label }));
    gridEnergy.appendChild(el('div', { class: 'data-value', id: `totals-val-energy-${key}`, text: '—' }));
  }
  secEnergy.appendChild(gridEnergy);
  frag.appendChild(secEnergy);

  return frag;
}

function buildTotalsConfig() {
  const frag = document.createDocumentFragment();

  // Source Priority (system-wide, inv 1)
  const table = el('table', { class: 'config-table' });
  const tbody = el('tbody');

  const systemParams = CONFIG_PARAMS.filter(p => p.key === 'source_priority' || p.key === 'output_frequency');
  for (const param of systemParams) {
    const tr = el('tr');
    tr.appendChild(el('td', { class: 'config-param-name', text: param.label }));
    const tdCurr = el('td', { class: 'config-current' });
    tdCurr.appendChild(el('span', { id: `totals-cfg-curr-${param.key}`, text: '—' }));
    tr.appendChild(tdCurr);

    const tdIn = el('td', { class: 'config-input-cell' });
    const sel = el('select', { id: `totals-cfg-input-${param.key}` });
    for (const [val, lbl] of param.options) {
      sel.appendChild(el('option', { value: val, text: `${val} = ${lbl}` }));
    }
    tdIn.appendChild(sel);
    tr.appendChild(tdIn);

    const tdAct = el('td');
    const inv1Id = State.snapshot?.inverters?.[0]?.id ?? 1;
    const sendBtn = el('button', { class: 'btn btn-success btn-sm', text: 'Send',
      onclick: async () => {
        await sendParam(inv1Id, param.key, sel, sendBtn);
      }
    });
    tdAct.appendChild(sendBtn);
    tr.appendChild(tdAct);
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  frag.appendChild(table);

  // Energy reset controls
  const divider = el('hr', { class: 'divider' });
  frag.appendChild(divider);

  const resetSec = el('div', { class: 'data-section' });
  resetSec.appendChild(el('div', { class: 'section-title', text: 'Energy Counters' }));
  const feedbackEl = el('span', { class: 'feedback', id: 'totals-cfg-feedback' });
  const resetDayBtn = el('button', { class: 'btn btn-warning btn-sm mt-1', text: 'Reset Daily',
    onclick: async () => {
      await apiAction('/api/energy/reset/daily', 'POST', resetDayBtn, feedbackEl, 'Daily counters reset.');
    }
  });
  const resetAllBtn = el('button', { class: 'btn btn-danger btn-sm mt-1', text: 'Reset All',
    onclick: async () => {
      if (!confirm('Reset ALL energy counters? This cannot be undone.')) return;
      await apiAction('/api/energy/reset/all', 'POST', resetAllBtn, feedbackEl, 'All counters reset.');
    }
  });
  const btnRow = el('div', { class: 'btn-actions mt-1' });
  btnRow.appendChild(resetDayBtn);
  btnRow.appendChild(resetAllBtn);
  resetSec.appendChild(btnRow);
  resetSec.appendChild(feedbackEl);
  frag.appendChild(resetSec);

  return frag;
}

function updateTotalsLiveData(snap) {
  const totals = snap.totals || {};
  const energy = snap.energy?.system || {};

  // Totals fields — all come from snap.totals
  const totalKeys = [
    'pv_power_total', 'pv1_power_total', 'pv2_power_total', 'pv_input_current_total',
    'ac_output_active_power_total', 'ac_output_apparent_power_total', 'ac_output_reactive_power_total',
    'output_load_percent_avg', 'system_power_factor', 'solar_self_sufficiency_percent',
    'battery_voltage_avg', 'battery_soc_avg',
    'battery_charging_current_total', 'battery_discharge_current_total',
    'battery_real_charge_total', 'battery_real_power_total',
    'battery_time_remaining_min', 'battery_time_to_full_min',
    'grid_voltage_avg', 'grid_frequency_avg', 'grid_input_power_estimate_total',
    'inverters_online', 'heatsink_temperature_max', 'heatsink_temperature_avg', 'system_status',
  ];
  for (const key of totalKeys) {
    const node = document.getElementById(`totals-val-${key}`);
    if (node) {
      const v = totals[key];
      node.textContent = fmt(v);
    }
  }

  // Energy fields — come from snap.energy.system
  const energyKeys = [
    'total_pv_production_today_kwh', 'total_pv_production_total_kwh',
    'total_consumption_today_kwh', 'total_consumption_total_kwh',
    'total_battery_charged_today_kwh', 'total_battery_charged_total_kwh',
    'total_battery_discharged_today_kwh', 'total_battery_discharged_total_kwh',
    'total_grid_import_today_kwh', 'total_grid_import_total_kwh',
    'total_pv_to_battery_today_kwh', 'total_pv_to_battery_total_kwh',
    'co2_saved_today_kg', 'co2_saved_total_kg', 'system_battery_cycles',
  ];
  for (const key of energyKeys) {
    const node = document.getElementById(`totals-val-energy-${key}`);
    if (node) {
      const v = energy[key];
      node.textContent = fmt(v);
    }
  }

  updateTotalsKPIs(snap);
}

function updateTotalsKPIs(snap) {
  const kpiRow = document.getElementById('totals-kpi-row');
  if (!kpiRow) return;
  const t = snap.totals || {};
  const e = snap.energy?.system || {};

  const kpis = [
    { label: 'Battery',       value: t.battery_voltage_avg != null ? t.battery_voltage_avg.toFixed(1) : '—',        unit: 'V'   },
    { label: 'SOC',           value: t.battery_soc_avg != null ? Math.round(t.battery_soc_avg) + '%' : '—',         unit: ''    },
    { label: 'PV Power',      value: t.pv_power_total ?? '—',                                                       unit: 'W'   },
    { label: 'Load',          value: t.ac_output_active_power_total ?? '—',                                         unit: 'W'   },
    { label: 'PV Today',      value: e.total_pv_production_today_kwh ?? '—',                                        unit: 'kWh' },
    { label: 'CO₂ Today',     value: e.co2_saved_today_kg ?? '—',                                                   unit: 'kg'  },
    { label: 'Status',        value: ['OK', 'WARNING', 'FAULT'][t.system_status] ?? '—',                            unit: ''    },
  ];

  if (!kpiRow.dataset.built) {
    kpiRow.innerHTML = '';
    for (const k of kpis) {
      const card = el('div', { class: 'kpi-card' });
      card.appendChild(el('div', { class: 'kpi-label', text: k.label }));
      card.appendChild(el('div', { class: 'kpi-value', id: `kpi-${k.label.replace(/\s+/g, '')}`, text: fmt(k.value) }));
      card.appendChild(el('div', { class: 'kpi-unit', text: k.unit }));
      kpiRow.appendChild(card);
    }
    kpiRow.dataset.built = '1';
  } else {
    for (const k of kpis) {
      const node = document.getElementById(`kpi-${k.label.replace(/\s+/g, '')}`);
      if (node) node.textContent = fmt(k.value);
    }
  }
}

// ============================================================
// APP CONFIG PANE
// ============================================================

function buildAppConfigPane() {
  const pane = el('div', { class: 'panel' });
  pane.appendChild(el('div', { class: 'panel-header', text: 'Application Configuration' }));

  const body = el('div', { class: 'panel-body' });

  // Form root — built entirely from the API response on tab open
  body.appendChild(el('div', { id: 'appconfig-form-root', class: 'text-dim', text: 'Loading configuration…' }));

  const actions = el('div', { class: 'config-actions mt-2' });
  const feedbackEl = el('span', { class: 'feedback', id: 'appconfig-feedback' });
  const reloadBtn = el('button', { class: 'btn btn-primary', id: 'appconfig-loadbtn', text: 'Reload',
    onclick: () => loadAndRenderAppConfig(reloadBtn, feedbackEl)
  });
  const saveBtn = el('button', { class: 'btn btn-success', id: 'appconfig-savebtn', text: 'Save Config',
    onclick: () => saveAppConfig(saveBtn, feedbackEl)
  });
  actions.appendChild(reloadBtn);
  actions.appendChild(saveBtn);
  actions.appendChild(feedbackEl);
  body.appendChild(actions);

  pane.appendChild(body);
  return pane;
}

async function loadAndRenderAppConfig(btn, feedbackEl) {
  if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner"></span> Loading…'; }
  const root = document.getElementById('appconfig-form-root');
  if (!root) return;

  try {
    const r = await apiFetch('/api/config/app');
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    const cfg = await r.json();
    State.appConfig = cfg; // saved config keeps the fields the form does not show

    root.innerHTML = '';

    // Section definitions: which fields to show from each config section
    const sections = [
      { key: 'system', title: 'General', fields: [
        { field: 'num_inverters',              label: 'Number of Inverters',          type: 'number'   },
        { field: 'delay_between_inverters_ms', label: 'Delay Between Inverters (ms)', type: 'number'   },
        { field: 'delay_between_cycles_ms',    label: 'Delay Between Cycles (ms)',    type: 'number'   },
        { field: 'log_level',                  label: 'Log Level',                    type: 'text'     },
        { field: 'inverter_efficiency_percent', label: 'Inverter Efficiency (%)',     type: 'number'   },
      ]},
      { key: 'mqtt', title: 'MQTT / Broker', fields: [
        { field: 'broker_ip',    label: 'Broker IP',       type: 'text'     },
        { field: 'broker_port',  label: 'Broker Port',     type: 'number'   },
        { field: 'user',         label: 'MQTT Username',   type: 'text'     },
        { field: 'password',     label: 'MQTT Password',   type: 'password' },
        { field: 'topic_prefix', label: 'Topic Prefix',    type: 'text'     },
        { field: 'ha_discovery_prefix', label: 'HA Prefix', type: 'text'     },
      ]},
      { key: 'battery', title: 'Battery', fields: [
        { field: 'capacity_ah',           label: 'Battery Capacity (Ah)', type: 'number' },
        { field: 'safety_margin_percent', label: 'Safety Margin (%)',     type: 'number' },
        { field: 'co2_factor_kg_per_kwh', label: 'CO₂ Factor (kg/kWh)',  type: 'number' },
      ]},
      { key: 'web', title: 'Web Server', fields: [
        { field: 'port',                    label: 'Web Server Port',        type: 'number' },
        { field: 'session_timeout_seconds', label: 'Session Timeout (secs)', type: 'number' },
      ]},
    ];

    for (const sec of sections) {
      if (!cfg[sec.key]) continue;
      root.appendChild(el('div', { class: 'appconfig-section-title', text: sec.title }));
      const grid = el('div', { class: 'appconfig-grid' });
      for (const f of sec.fields) {
        const val = cfg[sec.key][f.field];
        if (val === undefined) continue;
        const group = el('div', { class: 'field-group' });
        const inputId = `appcfg-${sec.key}-${f.field}`;
        group.appendChild(el('label', { class: 'field-label', text: f.label, for: inputId }));
        const input = el('input', { type: f.type, id: inputId,
          'data-section': sec.key, 'data-field': f.field });
        input.value = val;
        group.appendChild(input);
        grid.appendChild(group);
      }
      root.appendChild(grid);
    }

    // Per-inverter section
    if (Array.isArray(cfg.inverters) && cfg.inverters.length > 0) {
      root.appendChild(el('div', { class: 'appconfig-section-title', text: 'Inverters' }));
      const invFields = [
        { field: 'name',           label: 'Name',           type: 'text'   },
        { field: 'tcp_ip',         label: 'TCP IP',         type: 'text'   },
        { field: 'tcp_port',       label: 'TCP Port',       type: 'number' },
        { field: 'ha_device_name', label: 'HA Device Name', type: 'text'   },
      ];
      for (const inv of cfg.inverters) {
        const card = el('div', { class: 'appconfig-inv-card' });
        card.appendChild(el('div', { class: 'appconfig-inv-title',
          text: `Inverter ${inv.id}${inv.name ? ' — ' + inv.name : ''}` }));
        const grid = el('div', { class: 'appconfig-grid' });
        for (const f of invFields) {
          const group = el('div', { class: 'field-group' });
          const inputId = `appcfg-inv-${inv.id}-${f.field}`;
          group.appendChild(el('label', { class: 'field-label', text: f.label, for: inputId }));
          const input = el('input', { type: f.type, id: inputId,
            'data-inv-id': String(inv.id), 'data-inv-field': f.field });
          input.value = inv[f.field] !== undefined ? inv[f.field] : '';
          group.appendChild(input);
          grid.appendChild(group);
        }
        // Enabled checkbox
        const chkGroup = el('div', { class: 'field-group' });
        const chkId = `appcfg-inv-${inv.id}-enabled`;
        chkGroup.appendChild(el('label', { class: 'field-label', text: 'Enabled', for: chkId }));
        const chk = el('input', { type: 'checkbox', id: chkId,
          'data-inv-id': String(inv.id), 'data-inv-field': 'enabled' });
        chk.checked = inv.enabled !== false;
        chkGroup.appendChild(chk);
        grid.appendChild(chkGroup);
        card.appendChild(grid);
        root.appendChild(card);
      }
    }

    if (feedbackEl) showFeedbackEl(feedbackEl, 'success', 'Config loaded.');
  } catch (e) {
    if (root) root.innerHTML = `<div class="text-dim">Failed to load config: ${e.message}</div>`;
    if (feedbackEl) showFeedbackEl(feedbackEl, 'error', `Load failed: ${e.message}`);
  } finally {
    if (btn) { btn.disabled = false; btn.textContent = 'Reload'; }
  }
}

async function saveAppConfig(btn, feedbackEl) {
  const root = document.getElementById('appconfig-form-root');
  if (!root) return;

  // Start from the loaded config so fields not shown in the form are kept
  const cfg = structuredClone(State.appConfig || {});
  const invMap = {};
  for (const inv of cfg.inverters || []) invMap[inv.id] = { ...inv };

  // Collect section fields via data attributes
  root.querySelectorAll('[data-section][data-field]').forEach(inp => {
    const sec   = inp.getAttribute('data-section');
    const field = inp.getAttribute('data-field');
    if (!cfg[sec]) cfg[sec] = {};
    cfg[sec][field] = inp.type === 'number' ? Number(inp.value) : inp.value;
  });

  // Collect inverter fields via data attributes
  root.querySelectorAll('[data-inv-id][data-inv-field]').forEach(inp => {
    const id    = Number(inp.getAttribute('data-inv-id'));
    const field = inp.getAttribute('data-inv-field');
    if (!invMap[id]) invMap[id] = { id };
    if (inp.type === 'checkbox')    invMap[id][field] = inp.checked;
    else if (inp.type === 'number') invMap[id][field] = Number(inp.value);
    else                            invMap[id][field] = inp.value;
  });

  cfg.inverters = Object.values(invMap).sort((a, b) => a.id - b.id);

  btn.disabled = true;
  btn.innerHTML = '<span class="spinner"></span> Saving…';
  try {
    const r = await apiFetch('/api/config/app', { method: 'PUT', body: JSON.stringify(cfg) });
    if (r.ok) {
      showFeedbackEl(feedbackEl, 'success', 'Config saved.');
    } else {
      const d = await r.json().catch(() => ({}));
      showFeedbackEl(feedbackEl, 'error', d.message || `HTTP ${r.status}`);
    }
  } catch (e) {
    showFeedbackEl(feedbackEl, 'error', 'Save failed.');
  } finally {
    btn.disabled = false;
    btn.textContent = 'Save Config';
  }
}

// ============================================================
// ADMIN PANE
// ============================================================

function buildAdminPane() {
  if (!isAdmin()) return buildAccountPane();

  const pane = el('div', { class: 'panel' });
  pane.appendChild(el('div', { class: 'panel-header', text: 'User Management' }));

  const body = el('div', { class: 'panel-body' });

  // Users table
  const tableWrap = el('div', { id: 'users-table-wrap' });
  tableWrap.appendChild(el('div', { class: 'text-dim', text: 'Loading users…' }));
  body.appendChild(tableWrap);

  body.appendChild(el('hr', { class: 'divider' }));

  // Create user form
  const createTitle = el('div', { class: 'section-title mt-2', text: 'Create User' });
  body.appendChild(createTitle);

  const createFb = el('span', { class: 'feedback', id: 'create-user-feedback' });
  const form = el('div', { class: 'create-user-form mt-1' });

  const newUsername = el('div', { class: 'field-group' });
  newUsername.appendChild(el('label', { class: 'field-label', text: 'Username' }));
  newUsername.appendChild(el('input', { type: 'text', id: 'new-username' }));

  const newPassword = el('div', { class: 'field-group' });
  newPassword.appendChild(el('label', { class: 'field-label', text: 'Password' }));
  newPassword.appendChild(el('input', { type: 'password', id: 'new-password' }));

  const newRole = el('div', { class: 'field-group' });
  newRole.appendChild(el('label', { class: 'field-label', text: 'Role' }));
  const roleSelect = el('select', { id: 'new-role' });
  roleSelect.appendChild(el('option', { value: 'user', text: 'User' }));
  roleSelect.appendChild(el('option', { value: 'admin', text: 'Admin' }));
  newRole.appendChild(roleSelect);

  const btnWrap = el('div', { class: 'field-group' });
  btnWrap.appendChild(el('label', { class: 'field-label', html: '&nbsp;' }));
  const createBtn = el('button', { class: 'btn btn-primary', text: 'Create User',
    onclick: () => createUser(createBtn, createFb)
  });
  btnWrap.appendChild(createBtn);

  form.appendChild(newUsername);
  form.appendChild(newPassword);
  form.appendChild(newRole);
  form.appendChild(btnWrap);
  body.appendChild(form);
  body.appendChild(createFb);

  body.appendChild(buildLogoutSection());

  pane.appendChild(body);
  return pane;
}

// Non-admin users: change own password and log out
function buildAccountPane() {
  const pane = el('div', { class: 'panel' });
  pane.appendChild(el('div', { class: 'panel-header', text: 'Account' }));

  const body = el('div', { class: 'panel-body' });
  body.appendChild(el('div', { class: 'text-dim', text: `Logged in as "${State.username}" (read-only access).` }));
  body.appendChild(el('button', { class: 'btn btn-warning mt-2', text: 'Change My Password',
    onclick: () => changePasswordPrompt(State.username)
  }));
  body.appendChild(buildLogoutSection());

  pane.appendChild(body);
  return pane;
}

function buildLogoutSection() {
  const logoutSec = el('div', { class: 'logout-section' });
  const logoutFb = el('span', { class: 'feedback', id: 'logout-feedback' });
  const logoutBtn = el('button', { class: 'btn btn-danger', text: 'Logout',
    onclick: () => doLogout(logoutBtn, logoutFb)
  });
  logoutSec.appendChild(logoutBtn);
  logoutSec.appendChild(el('span', { class: 'text-dim', text: 'End your current session.' }));
  logoutSec.appendChild(logoutFb);
  return logoutSec;
}

async function loadUsers() {
  const wrap = document.getElementById('users-table-wrap');
  if (!wrap) return;
  try {
    const r = await apiFetch('/api/users');
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    const users = await r.json();
    renderUsersTable(users, wrap);
  } catch (e) {
    wrap.innerHTML = `<span class="text-red">Failed to load users: ${e.message}</span>`;
  }
}

function renderUsersTable(users, wrap) {
  if (!Array.isArray(users) || users.length === 0) {
    wrap.innerHTML = '<span class="text-dim">No users found.</span>';
    return;
  }

  const table = el('table', { class: 'user-table' });
  const thead = el('thead');
  thead.appendChild(el('tr', {},
    el('th', { text: 'Username' }),
    el('th', { text: 'Role' }),
    el('th', { text: 'Actions' }),
  ));
  table.appendChild(thead);

  const tbody = el('tbody');
  for (const user of users) {
    const tr = el('tr');
    tr.appendChild(el('td', { text: user.username }));

    const roleTd = el('td');
    roleTd.appendChild(el('span', {
      class: `role-badge ${user.role === 'admin' ? 'role-admin' : 'role-user'}`,
      text: user.role,
    }));
    tr.appendChild(roleTd);

    const actTd = el('td');
    const actDiv = el('div', { class: 'btn-actions' });

    const chgPwBtn = el('button', { class: 'btn btn-warning btn-sm', text: 'Change Password',
      onclick: () => changePasswordPrompt(user.username)
    });
    const delBtn = el('button', { class: 'btn btn-danger btn-sm', text: 'Delete',
      onclick: async () => {
        if (!confirm(`Delete user "${user.username}"?`)) return;
        await deleteUser(user.username, delBtn, wrap);
      }
    });

    actDiv.appendChild(chgPwBtn);
    actDiv.appendChild(delBtn);
    actTd.appendChild(actDiv);
    tr.appendChild(actTd);
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  wrap.innerHTML = '';
  wrap.appendChild(table);
}

async function createUser(btn, feedbackEl) {
  const username = document.getElementById('new-username')?.value?.trim();
  const password = document.getElementById('new-password')?.value;
  const role = document.getElementById('new-role')?.value;

  if (!username || !password) { showFeedbackEl(feedbackEl, 'error', 'Username and password required.'); return; }

  btn.disabled = true;
  btn.innerHTML = '<span class="spinner"></span>';
  try {
    const r = await apiFetch('/api/users', {
      method: 'POST',
      body: JSON.stringify({ username, password, role }),
    });
    if (r.ok) {
      showFeedbackEl(feedbackEl, 'success', `User "${username}" created.`);
      document.getElementById('new-username').value = '';
      document.getElementById('new-password').value = '';
      await loadUsers();
    } else {
      const d = await r.json().catch(() => ({}));
      showFeedbackEl(feedbackEl, 'error', d.message || d.error || `HTTP ${r.status}`);
    }
  } catch (e) {
    showFeedbackEl(feedbackEl, 'error', 'Create failed.');
  } finally {
    btn.disabled = false;
    btn.textContent = 'Create User';
  }
}

function changePasswordPrompt(username) {
  const newPw = prompt(`New password for "${username}":`);
  if (!newPw) return;
  apiFetch(`/api/users/${encodeURIComponent(username)}/password`, {
    method: 'PUT',
    body: JSON.stringify({ password: newPw }),
  }).then(r => {
    if (r.ok) showModal('Success', `Password for "${username}" changed.`);
    else r.json().then(d => showModal('Error', d.message || d.error || `HTTP ${r.status}`)).catch(() => showModal('Error', `HTTP ${r.status}`));
  }).catch(() => showModal('Error', 'Network error.'));
}

async function deleteUser(username, btn, wrap) {
  btn.disabled = true;
  try {
    const r = await apiFetch(`/api/users/${encodeURIComponent(username)}`, { method: 'DELETE' });
    if (r.ok) {
      await loadUsers();
    } else {
      const d = await r.json().catch(() => ({}));
      showModal('Delete Error', d.message || d.error || `HTTP ${r.status}`);
      btn.disabled = false;
    }
  } catch (e) {
    showModal('Delete Error', 'Network error.');
    btn.disabled = false;
  }
}

async function doLogout(btn, feedbackEl) {
  btn.disabled = true;
  btn.innerHTML = '<span class="spinner"></span> Logging out…';
  try {
    await apiFetch('/api/logout', { method: 'POST' });
  } catch (_) {}
  window.location.href = '/login.html';
}

// ============================================================
// UTILITIES
// ============================================================

async function apiFetch(url, opts = {}) {
  const defaults = {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', ...(opts.headers || {}) },
  };
  const res = await fetch(url, { ...defaults, ...opts, headers: { ...defaults.headers, ...(opts.headers || {}) } });
  if (res.status === 401) {
    window.location.href = '/login.html';
    throw new Error('Unauthorized');
  }
  return res;
}

async function apiAction(url, method, btn, feedbackEl, successMsg) {
  btn.disabled = true;
  const orig = btn.textContent;
  btn.innerHTML = '<span class="spinner"></span>';
  try {
    const r = await apiFetch(url, { method });
    if (r.ok) {
      showFeedbackEl(feedbackEl, 'success', successMsg);
    } else {
      const d = await r.json().catch(() => ({}));
      showFeedbackEl(feedbackEl, 'error', d.message || d.error || `HTTP ${r.status}`);
    }
  } catch (e) {
    if (e.message !== 'Unauthorized') showFeedbackEl(feedbackEl, 'error', 'Request failed.');
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

function showFeedback(id, type, msg) {
  const el_ = document.getElementById(id);
  if (!el_) return;
  showFeedbackEl(el_, type, msg);
}

function showFeedbackEl(el_, type, msg) {
  el_.className = `feedback ${type} visible`;
  el_.textContent = msg;
  clearTimeout(el_._timer);
  el_._timer = setTimeout(() => el_.classList.remove('visible'), 5000);
}

// ── Modal ─────────────────────────────────────────────────────
function showModal(title, text) {
  document.getElementById('modal-title').textContent = title;
  document.getElementById('modal-body-text').textContent = text;
  document.getElementById('modal').style.display = 'flex';
}

function closeModal() {
  document.getElementById('modal').style.display = 'none';
}

// ── Init ──────────────────────────────────────────────────────
async function init() {
  // Modal close handlers
  document.getElementById('modal-close').addEventListener('click', closeModal);
  document.getElementById('modal').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closeModal();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeModal();
  });

  const authed = await checkAuth();
  if (!authed) return;

  connectWS();
}

document.addEventListener('DOMContentLoaded', init);
