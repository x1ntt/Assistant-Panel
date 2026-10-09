use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::commands::alert::show_alert_inner;

const SERVER_URL: &str = "http://xint.top:8900/api/nodes";
const AUTH_TOKEN: &str = "KxagtD_JrF9Qtzzl8Gd9SRFvwWj8s-qxpumkXtqECC0";
const POLL_INTERVAL_SECS: u64 = 30;

/// Metal keys as they appear in the upstream `metals` map.
const GOLD_9999_KEY: &str = "黄金_9999";
const LONDON_GOLD_KEY: &str = "伦敦金(现货黄金)";

/// Logical ids used in thresholds store + alert payloads.
const GOLD_9999_ID: &str = "gold9999";
const LONDON_GOLD_ID: &str = "londonGold";

/// Tracks whether each rule id is currently in a breached (already-alerted) state,
/// so we only pop the alert once per breach episode (de-dup / anti-spam).
static ALERT_STATE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// Last successful/attempted snapshot, so newly-opened windows (floating/alert)
/// can fetch the current data immediately instead of waiting for the next poll.
static LAST_SNAPSHOT: Mutex<Option<GoldSnapshot>> = Mutex::new(None);

/// Command: return the most recent gold snapshot (may be None before first poll).
#[tauri::command]
pub fn get_gold_snapshot() -> Option<GoldSnapshot> {
    LAST_SNAPSHOT.lock().unwrap().clone()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetalInfo {
    id: String,
    name: String,
    sell_price: Option<f64>,
    high_price: Option<f64>,
    low_price: Option<f64>,
    unit: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GoldSnapshot {
    ok: bool,
    node_name: Option<String>,
    metals: Vec<MetalInfo>,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AlertPayload {
    metal_id: String,
    metal_name: String,
    current_value: f64,
    unit: String,
    rule: String,   // e.g. "> 950" or "< 900"
    time: String,   // formatted local time
}

/// Threshold rule for one metal. `max`/`min` may be null (no bound).
#[derive(serde::Deserialize, Default, Clone)]
struct Threshold {
    max: Option<f64>,
    min: Option<f64>,
}

fn parse_price(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        _ => None,
    }
}

/// Read thresholds from the shared settings store, keyed by `api-dashboard.thresholds`.
fn load_thresholds(app: &AppHandle) -> HashMap<String, Threshold> {
    let mut out = HashMap::new();
    if let Ok(store) = tauri_plugin_store::StoreBuilder::new(app, "settings.json").build() {
        if let Some(v) = store.get("api-dashboard.thresholds") {
            if let Ok(map) = serde_json::from_value::<HashMap<String, Threshold>>(v) {
                out = map;
            }
        }
    }
    out
}

/// Extract a metal's info from the metals JSON object.
fn extract_metal(
    metals: &serde_json::Value,
    key: &str,
    id: &str,
) -> Option<MetalInfo> {
    let m = metals.get(key)?;
    Some(MetalInfo {
        id: id.to_string(),
        name: key.to_string(),
        sell_price: parse_price(m.get("sell_price")),
        high_price: parse_price(m.get("high_price")),
        low_price: parse_price(m.get("low_price")),
        unit: m
            .get("unit")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Evaluate thresholds against the current metals and fire de-duplicated alerts.
fn evaluate_alerts(app: &AppHandle, metals: &[MetalInfo]) {
    let thresholds = load_thresholds(app);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut guard = ALERT_STATE.lock().unwrap();
    let state = guard.get_or_insert_with(HashMap::new);

    for metal in metals {
        let Some(th) = thresholds.get(&metal.id) else { continue };
        let Some(value) = metal.sell_price else { continue };

        let mut breached: Option<String> = None;
        if let Some(max) = th.max {
            if value > max {
                breached = Some(format!("> {}", max));
            }
        }
        if breached.is_none() {
            if let Some(min) = th.min {
                if value < min {
                    breached = Some(format!("< {}", min));
                }
            }
        }

        let was_alerted = state.get(&metal.id).copied().unwrap_or(false);
        match breached {
            Some(rule) if !was_alerted => {
                // New breach episode -> fire alert once.
                state.insert(metal.id.clone(), true);
                let payload = AlertPayload {
                    metal_id: metal.id.clone(),
                    metal_name: metal.name.clone(),
                    current_value: value,
                    unit: metal.unit.clone(),
                    rule,
                    time: now.clone(),
                };
                let _ = app.emit("gold:alert", &payload);
                show_alert_inner(app);
            }
            Some(_) => { /* still breached, already alerted -> do nothing */ }
            None => {
                // Value back to normal -> clear so a future breach can re-alert.
                state.insert(metal.id.clone(), false);
            }
        }
    }
}

/// One polling tick: fetch, parse, emit data + evaluate alerts.
async fn tick(app: &AppHandle, client: &tauri_plugin_http::reqwest::Client) {
    let resp = client
        .get(SERVER_URL)
        .header("Authorization", format!("Bearer {}", AUTH_TOKEN))
        .send()
        .await;

    let snapshot = match resp {
        Ok(r) => match r.text().await {
            Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json) => build_snapshot(&json),
                Err(e) => error_snapshot(format!("parse error: {}", e)),
            },
            Err(e) => error_snapshot(format!("read error: {}", e)),
        },
        Err(e) => error_snapshot(format!("request error: {}", e)),
    };

    let _ = app.emit("gold:data", &snapshot);

    if snapshot.ok {
        // Cache only good snapshots so freshly-opened windows get real data.
        *LAST_SNAPSHOT.lock().unwrap() = Some(snapshot.clone());
        evaluate_alerts(app, &snapshot.metals);
    }
}

fn error_snapshot(msg: String) -> GoldSnapshot {
    GoldSnapshot {
        ok: false,
        node_name: None,
        metals: Vec::new(),
        error: Some(msg),
    }
}

/// Find the first node whose `gold` collector is ok, and build the snapshot.
fn build_snapshot(json: &serde_json::Value) -> GoldSnapshot {
    let Some(nodes) = json.as_object() else {
        return error_snapshot("unexpected response shape".into());
    };

    for (_node_id, node) in nodes {
        let gold = node.get("collectors").and_then(|c| c.get("gold"));
        let Some(gold) = gold else { continue };
        let status_ok = gold
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| s == "ok")
            .unwrap_or(false);
        if !status_ok {
            continue;
        }
        let Some(metals_val) = gold.get("data").and_then(|d| d.get("metals")) else {
            continue;
        };

        let mut metals = Vec::new();
        if let Some(m) = extract_metal(metals_val, GOLD_9999_KEY, GOLD_9999_ID) {
            metals.push(m);
        }
        if let Some(m) = extract_metal(metals_val, LONDON_GOLD_KEY, LONDON_GOLD_ID) {
            metals.push(m);
        }
        if metals.is_empty() {
            continue;
        }

        let node_name = node
            .get("meta")
            .and_then(|meta| meta.get("node_name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        return GoldSnapshot {
            ok: true,
            node_name,
            metals,
            error: None,
        };
    }

    error_snapshot("no node with gold data".into())
}

/// Start the background gold-price monitor on a dedicated thread with its own
/// Tokio runtime (mirrors the port_forwarder pattern; cannot block_on the main thread).
pub fn start_monitor(app: AppHandle) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => return,
        };

        rt.block_on(async move {
            let client = match tauri_plugin_http::reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };

            loop {
                tick(&app, &client).await;
                tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            }
        });
    });
}
