use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_http::reqwest::{self, header::HeaderValue};
use tokio::sync::Notify;

use crate::commands::alert::show_alert_inner;

const DEFAULT_ENDPOINT: &str = "https://xint.top/index.php/action/personal-api-v2";
const CONNECTION_KEY: &str = "api-dashboard.connection";
const POLL_INTERVAL_SECS: u64 = 30;
const SUBSCRIPTION_TTL: &str = "120";
const GOLD_9999_KEY: &str = "黄金_9999";
const LONDON_GOLD_KEY: &str = "伦敦金(现货黄金)";
const GOLD_9999_ID: &str = "gold9999";
const LONDON_GOLD_ID: &str = "londonGold";

static ALERT_STATE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
static MONITOR: Mutex<MonitorCache> = Mutex::new(MonitorCache {
    revision: 0,
    snapshot: None,
});
static REFRESH: Notify = Notify::const_new();

struct MonitorCache {
    revision: u64,
    snapshot: Option<GoldSnapshot>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct GoldConnection {
    endpoint: String,
    api_key: String,
    client_id: String,
}

impl Default for GoldConnection {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.into(),
            api_key: String::new(),
            client_id: String::new(),
        }
    }
}

// StoreBuilder silently ignores initial JSON read failures. Check the file
// before saving so configuring this panel cannot replace corrupt settings.
fn connection_store(
    app: &AppHandle,
) -> Result<std::sync::Arc<tauri_plugin_store::Store<tauri::Wry>>, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes)
                .map_err(|_| "settings.json 格式错误，请修复后重试".to_string())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("无法读取 settings.json".into()),
    }
    tauri_plugin_store::StoreBuilder::new(app, "settings.json")
        .disable_auto_save()
        .build()
        .map_err(|e| e.to_string())
}

fn read_connection(
    store: &tauri_plugin_store::Store<tauri::Wry>,
) -> Result<GoldConnection, String> {
    match store.get(CONNECTION_KEY) {
        Some(value) => serde_json::from_value(value)
            .map_err(|_| "请修复 settings.json 中 PersonalApiV2 的连接配置后重试".into()),
        None => Ok(GoldConnection::default()),
    }
}

#[tauri::command]
pub fn get_gold_connection(app: AppHandle) -> Result<GoldConnection, String> {
    read_connection(connection_store(&app)?.as_ref())
}

fn validate_endpoint(endpoint: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| "请输入完整的 PersonalApiV2 接口地址".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("接口地址必须使用 http 或 https".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("接口地址请勿包含账号、查询参数或片段；Key 通过请求头发送".into());
    }
    Ok(url)
}

fn valid_client_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

#[tauri::command]
pub fn save_gold_connection(
    app: AppHandle,
    endpoint: String,
    api_key: String,
) -> Result<GoldConnection, String> {
    let endpoint = validate_endpoint(endpoint.trim())?.to_string();
    let api_key = api_key.trim().to_string();
    HeaderValue::from_str(&api_key).map_err(|_| "Key 包含无效的请求头字符".to_string())?;

    // Serialize saves with publication: a response for the previous Key/server
    // must never repopulate the panel after switching users or endpoints.
    let mut cache = MONITOR.lock().unwrap();
    let store = connection_store(&app)?;
    let previous = store.get(CONNECTION_KEY);
    let client_id = read_connection(&store)
        .ok()
        .map(|c| c.client_id)
        .filter(|id| valid_client_id(id))
        .unwrap_or_else(|| format!("assistant-panel-{}", nanoid::nanoid!(16)));
    let connection = GoldConnection {
        endpoint,
        api_key,
        client_id,
    };
    store.set(
        CONNECTION_KEY,
        serde_json::to_value(&connection).map_err(|e| e.to_string())?,
    );
    if let Err(error) = store.save() {
        match previous {
            Some(value) => store.set(CONNECTION_KEY, value),
            None => {
                store.delete(CONNECTION_KEY);
            }
        }
        return Err(format!("连接配置保存失败：{error}"));
    }
    cache.revision += 1;
    let message = if connection.api_key.is_empty() {
        "请在 API 面板中配置 PersonalApiV2 的 Key"
    } else {
        "正在连接 PersonalApiV2…"
    };
    let snapshot = error_snapshot(message.into());
    cache.snapshot = Some(snapshot.clone());
    *ALERT_STATE.lock().unwrap() = None;
    let _ = app.emit("gold:data", &snapshot);
    REFRESH.notify_one();
    Ok(connection)
}

#[tauri::command]
pub fn get_gold_snapshot() -> Option<GoldSnapshot> {
    MONITOR.lock().unwrap().snapshot.clone()
}

#[tauri::command]
pub fn refresh_gold_data() {
    REFRESH.notify_one();
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
    stale: bool,
    node_name: Option<String>,
    metals: Vec<MetalInfo>,
    collected_at: Option<i64>,
    expires_at: Option<i64>,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AlertPayload {
    metal_id: String,
    metal_name: String,
    current_value: f64,
    unit: String,
    rule: String,
    time: String,
}

#[derive(Deserialize, Default, Clone)]
struct Threshold {
    max: Option<f64>,
    min: Option<f64>,
}

fn parse_price(v: Option<&serde_json::Value>) -> Option<f64> {
    let price = match v {
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        _ => None,
    };
    price.filter(|p| p.is_finite())
}

fn load_thresholds(app: &AppHandle) -> HashMap<String, Threshold> {
    tauri_plugin_store::StoreBuilder::new(app, "settings.json")
        .build()
        .ok()
        .and_then(|store| store.get("api-dashboard.thresholds"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

fn extract_metal(metals: &serde_json::Value, key: &str, id: &str) -> Option<MetalInfo> {
    let m = metals.get(key)?;
    let sell_price = parse_price(m.get("sell_price"))?;
    Some(MetalInfo {
        id: id.into(),
        name: key.into(),
        sell_price: Some(sell_price),
        high_price: parse_price(m.get("high_price")),
        low_price: parse_price(m.get("low_price")),
        unit: m.get("unit").and_then(|u| u.as_str()).unwrap_or("").into(),
    })
}

fn evaluate_alerts(app: &AppHandle, metals: &[MetalInfo]) {
    let thresholds = load_thresholds(app);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut guard = ALERT_STATE.lock().unwrap();
    let state = guard.get_or_insert_with(HashMap::new);

    for metal in metals {
        let Some(th) = thresholds.get(&metal.id) else {
            continue;
        };
        let Some(value) = metal.sell_price else {
            continue;
        };
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
            Some(_) => {}
            None => {
                state.insert(metal.id.clone(), false);
            }
        }
    }
}

fn error_snapshot(message: String) -> GoldSnapshot {
    GoldSnapshot {
        ok: false,
        stale: true,
        node_name: None,
        metals: Vec::new(),
        collected_at: None,
        expires_at: None,
        error: Some(message),
    }
}

/// Preserve the timestamp with the values on a failed poll, including in newly
/// opened windows. Configuration changes explicitly replace the whole cache.
fn retain_last_values(snapshot: &mut GoldSnapshot, previous: Option<&GoldSnapshot>) {
    if snapshot.metals.is_empty() {
        if let Some(previous) = previous {
            snapshot.metals = previous.metals.clone();
            snapshot.node_name = previous.node_name.clone();
            snapshot.collected_at = previous.collected_at;
            snapshot.expires_at = previous.expires_at;
        }
    }
}

fn timestamp(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64()).filter(|t| *t > 0)
}

/// V2 wraps nodes in `data`; prefer a fresh sample, then the latest collected_at.
fn build_snapshot(json: &serde_json::Value, now: i64) -> GoldSnapshot {
    if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let message = json
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("PersonalApiV2 返回了无效响应");
        return error_snapshot(message.into());
    }
    let Some(nodes) = json.get("data").and_then(|v| v.as_object()) else {
        return error_snapshot("PersonalApiV2 节点数据格式错误".into());
    };
    let mut best: Option<GoldSnapshot> = None;
    let mut failure = "暂无金价数据，已订阅 gold，等待节点采集回报".to_string();
    for (node_id, node) in nodes {
        let Some(gold) = node.pointer("/collectors/gold") else {
            continue;
        };
        if gold.get("status").and_then(|v| v.as_str()) != Some("ok") {
            if let Some(error) = gold.get("error").and_then(|v| v.as_str()) {
                failure = format!("金价采集失败：{error}");
            }
            continue;
        }
        let Some(values) = gold.pointer("/data/metals") else {
            continue;
        };
        let metals: Vec<_> = [
            (GOLD_9999_KEY, GOLD_9999_ID),
            (LONDON_GOLD_KEY, LONDON_GOLD_ID),
        ]
        .iter()
        .filter_map(|(key, id)| extract_metal(values, key, id))
        .collect();
        if metals.is_empty() {
            continue;
        }
        let collected_at = timestamp(gold.get("collected_at"));
        let received_at = timestamp(gold.get("received_at"));
        let expires_at = collected_at
            .zip(received_at)
            .zip(
                gold.get("max_age")
                    .and_then(|v| v.as_i64())
                    .filter(|v| *v > 0),
            )
            .and_then(|((collected, received), age)| collected.min(received).checked_add(age));
        let stale = gold.get("stale").and_then(|v| v.as_bool()) != Some(false)
            || node.get("activity").and_then(|v| v.as_str()) != Some("active")
            || expires_at.map(|t| now > t).unwrap_or(true);
        let snapshot = GoldSnapshot {
            ok: true,
            stale,
            node_name: Some(
                node.get("node_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(node_id)
                    .into(),
            ),
            metals,
            collected_at,
            expires_at,
            error: stale.then(|| "金价数据已过期或节点离线，等待新的采集回报".into()),
        };
        let rank = |s: &GoldSnapshot| (!s.stale, s.collected_at.unwrap_or(0));
        if best
            .as_ref()
            .map(|s| rank(&snapshot) > rank(s))
            .unwrap_or(true)
        {
            best = Some(snapshot);
        }
    }
    best.unwrap_or_else(|| error_snapshot(failure))
}

fn build_request(
    client: &reqwest::Client,
    connection: &GoldConnection,
) -> Result<reqwest::Request, String> {
    let endpoint = validate_endpoint(&connection.endpoint)?;
    if connection.api_key.is_empty() {
        return Err("请在 API 面板中配置 PersonalApiV2 的 Key".into());
    }
    if !valid_client_id(&connection.client_id) {
        return Err("请重新保存 PersonalApiV2 连接设置以生成客户端 ID".into());
    }
    let mut key = HeaderValue::from_str(&connection.api_key)
        .map_err(|_| "Key 包含无效的请求头字符".to_string())?;
    key.set_sensitive(true);
    client
        .get(endpoint)
        .query(&[
            ("group", "node"),
            ("type", "list"),
            ("collectors", "gold"),
            ("client_id", connection.client_id.as_str()),
            ("ttl", SUBSCRIPTION_TTL),
        ])
        .header("X-API-Key", key)
        .header("Accept", "application/json")
        .header("Cache-Control", "no-cache")
        .build()
        .map_err(|_| "无法构造 PersonalApiV2 请求".into())
}

fn parse_response(status: u16, body: &str) -> GoldSnapshot {
    let json = serde_json::from_str::<serde_json::Value>(body);
    if !(200..300).contains(&status) {
        let message = json
            .as_ref()
            .ok()
            .and_then(|v| v.pointer("/error/message"))
            .and_then(|v| v.as_str())
            .unwrap_or("请求失败，请检查接口地址和 Key");
        return error_snapshot(format!("HTTP {status}：{message}"));
    }
    match json {
        Ok(json) => build_snapshot(&json, chrono::Utc::now().timestamp()),
        Err(_) => error_snapshot("接口未返回有效 JSON，请检查 PersonalApiV2 地址".into()),
    }
}

async fn fetch_snapshot(
    client: &reqwest::Client,
    connection: Result<GoldConnection, String>,
) -> GoldSnapshot {
    let request = connection.and_then(|c| build_request(client, &c));
    let request = match request {
        Ok(request) => request,
        Err(error) => return error_snapshot(error),
    };
    match client.execute(request).await {
        Ok(response) => {
            let status = response.status().as_u16();
            match response.text().await {
                Ok(body) => parse_response(status, &body),
                Err(_) => error_snapshot("无法读取 PersonalApiV2 响应".into()),
            }
        }
        Err(error) => error_snapshot(
            if error.is_timeout() {
                "PersonalApiV2 请求超时"
            } else {
                "无法连接 PersonalApiV2，请检查地址和网络"
            }
            .into(),
        ),
    }
}

fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        // X-API-Key must never be forwarded to a redirect target.
        .redirect(reqwest::redirect::Policy::none())
}

/// One monitor for all windows. Saving configuration wakes it immediately and
/// cancels any pending fetch; the revision also guards against publication races.
pub fn start_monitor(app: AppHandle) {
    thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async move {
            let Ok(client) = client_builder().build() else {
                return;
            };
            loop {
                let (revision, connection) = {
                    let cache = MONITOR.lock().unwrap();
                    (cache.revision, get_gold_connection(app.clone()))
                };
                let mut snapshot = tokio::select! {
                    biased;
                    _ = REFRESH.notified() => continue,
                    snapshot = fetch_snapshot(&client, connection) => snapshot,
                };
                {
                    let mut cache = MONITOR.lock().unwrap();
                    if cache.revision != revision {
                        continue;
                    }
                    retain_last_values(&mut snapshot, cache.snapshot.as_ref());
                    cache.snapshot = Some(snapshot.clone());
                    let _ = app.emit("gold:data", &snapshot);
                    if snapshot.ok && !snapshot.stale {
                        // Window queries must run on the UI thread, without a
                        // worker holding the cache lock while waiting for it.
                        let alert_app = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            let cache = MONITOR.lock().unwrap();
                            if cache.revision == revision {
                                evaluate_alerts(&alert_app, &snapshot.metals);
                            }
                        });
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)) => {},
                    _ = REFRESH.notified() => {},
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(name: &str, collected: i64, stale: bool) -> serde_json::Value {
        json!({ "node_name": name, "activity": "active", "collectors": { "gold": {
            "status": "ok", "stale": stale, "collected_at": collected,
            "received_at": collected + 1, "max_age": 180, "data": { "metals": {
                "黄金_9999": { "sell_price": "980.5", "high_price": 990, "low_price": "970", "unit": "元/克" },
                "伦敦金(现货黄金)": { "sell_price": 3300, "unit": "美元/盎司" }
            } }
        } } })
    }

    #[test]
    fn v2_wrapper_prices_node_name_and_collection_timestamp() {
        let snap = build_snapshot(
            &json!({"ok": true, "data": {"n": node("家庭节点", 1000, false)}}),
            1100,
        );
        assert!(snap.ok && !snap.stale);
        assert_eq!(snap.node_name.as_deref(), Some("家庭节点"));
        assert_eq!(snap.metals.len(), 2);
        assert_eq!(snap.metals[0].sell_price, Some(980.5));
        assert_eq!(snap.collected_at, Some(1000));
        assert_eq!(snap.expires_at, Some(1180));
        let serialized = serde_json::to_value(snap).unwrap();
        assert_eq!(serialized["collectedAt"], 1000);
        assert_eq!(serialized["nodeName"], "家庭节点");
    }

    #[test]
    fn prefers_fresh_nodes_then_newest_collection() {
        let response = json!({"ok":true, "data": {
            "a": node("离线", 1090, true), "b": node("旧", 1000, false), "c": node("新", 1050, false)
        }});
        assert_eq!(
            build_snapshot(&response, 1100).node_name.as_deref(),
            Some("新")
        );
    }

    #[test]
    fn expired_and_offline_samples_keep_prices_and_actual_timestamp() {
        let mut sample = node("n", 1000, false);
        for field in ["age", "activity", "timestamp", "received"] {
            let mut test = sample.clone();
            match field {
                "activity" => test["activity"] = json!("stale"),
                "timestamp" => test["collectors"]["gold"]["collected_at"] = json!(null),
                "received" => test["collectors"]["gold"]["received_at"] = json!(700),
                _ => {}
            }
            let now = if field == "age" { 1200 } else { 1100 };
            let snap = build_snapshot(&json!({"ok":true,"data":{"n":test}}), now);
            assert!(snap.ok && snap.stale, "{field}");
            assert_eq!(snap.metals.len(), 2);
        }
        sample["collectors"]["gold"]["stale"] = json!(true);
        assert!(build_snapshot(&json!({"ok":true,"data":{"n":sample}}), 1100).stale);
    }

    #[test]
    fn http_errors_and_invalid_or_pending_payloads_are_not_successes() {
        let denied = parse_response(401, r#"{"ok":false,"error":{"message":"Key 无效"}}"#);
        assert!(denied.error.unwrap().contains("HTTP 401：Key 无效"));
        assert!(!parse_response(500, "<html>error</html>").ok);
        assert!(!parse_response(200, "<html>login</html>").ok);
        assert!(!build_snapshot(&json!({"legacy-node":node("n",1000,false)}), 1100).ok);
        assert!(!build_snapshot(&json!({"ok":true,"data":[]}), 1100).ok);
        let pending = build_snapshot(&json!({"ok":true,"data":{}}), 1100);
        assert!(pending.error.unwrap().contains("等待节点"));
        let failed = build_snapshot(
            &json!({"ok":true,"data":{"n":{"collectors":{"gold":{
                "status":"error","error":"upstream unavailable"
            }}}}}),
            1100,
        );
        assert!(failed.error.unwrap().contains("upstream unavailable"));
    }

    #[test]
    fn missing_or_non_finite_prices_cannot_trigger_alerts() {
        let mut sample = node("n", 1000, false);
        sample["collectors"]["gold"]["data"]["metals"]["黄金_9999"]["sell_price"] = json!("NaN");
        sample["collectors"]["gold"]["data"]["metals"]["伦敦金(现货黄金)"]["sell_price"] =
            json!(null);
        assert!(!build_snapshot(&json!({"ok":true,"data":{"n":sample}}), 1100).ok);
        assert_eq!(parse_price(Some(&json!("inf"))), None);
    }

    #[test]
    fn failed_poll_preserves_values_and_collection_time_but_not_success_state() {
        let good = build_snapshot(&json!({"ok":true,"data":{"n":node("n",1000,false)}}), 1100);
        let mut failed = error_snapshot("network error".into());
        retain_last_values(&mut failed, Some(&good));
        assert!(!failed.ok && failed.stale);
        assert_eq!(failed.metals.len(), 2);
        assert_eq!(failed.collected_at, Some(1000));
        assert_eq!(failed.expires_at, good.expires_at);
        assert_eq!(failed.error.as_deref(), Some("network error"));
    }

    #[test]
    fn request_uses_header_key_and_subscribes_only_gold() {
        let connection = GoldConnection {
            api_key: "test-key".into(),
            client_id: "assistant-panel-test".into(),
            ..Default::default()
        };
        let request = build_request(&reqwest::Client::new(), &connection).unwrap();
        assert_eq!(request.headers()["X-API-Key"], "test-key");
        assert!(request.headers()["X-API-Key"].is_sensitive());
        assert!(request.headers().get("Authorization").is_none());
        assert!(!request.url().as_str().contains("test-key"));
        let query: HashMap<_, _> = request.url().query_pairs().collect();
        assert_eq!(query.get("group").unwrap(), "node");
        assert_eq!(query.get("type").unwrap(), "list");
        assert_eq!(query.get("collectors").unwrap(), "gold");
        assert_eq!(query.get("client_id").unwrap(), "assistant-panel-test");
        assert_eq!(query.get("ttl").unwrap(), "120");
    }

    #[test]
    fn configuration_rejects_url_keys_credentials_and_invalid_headers() {
        for endpoint in [
            "https://example.com/api?key=secret",
            "https://user:pass@example.com/api",
            "file:///api",
            "https://example.com/api#key",
        ] {
            assert!(validate_endpoint(endpoint).is_err());
        }
        let mut connection = GoldConnection {
            api_key: "bad\r\nheader".into(),
            client_id: "client".into(),
            ..Default::default()
        };
        assert!(build_request(&reqwest::Client::new(), &connection).is_err());
        connection.api_key.clear();
        assert!(build_request(&reqwest::Client::new(), &connection)
            .unwrap_err()
            .contains("配置"));
        assert!(!valid_client_id("a/b"));
        assert!(valid_client_id("assistant-panel_1"));
    }

    fn mock_server(
        status: &str,
        body: String,
        extra_headers: &str,
    ) -> (String, thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/index.php/action/personal-api-v2",
            listener.local_addr().unwrap()
        );
        let status = status.to_string();
        let extra_headers = extra_headers.to_string();
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("Mock API did not receive a connection: {e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 2048];
            while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
            }
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}", body.len()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (endpoint, handle)
    }

    #[tokio::test]
    async fn local_http_api_verifies_real_request_response_and_authentication_error() {
        let client = client_builder().no_proxy().build().unwrap();
        let now = chrono::Utc::now().timestamp();
        for (status, body, success) in [
            (
                "200 OK",
                json!({"ok":true,"data":{"n":node("本地测试节点",now,false)}}).to_string(),
                true,
            ),
            (
                "401 Unauthorized",
                json!({"ok":false,"error":{"message":"Key 无效"}}).to_string(),
                false,
            ),
        ] {
            let (endpoint, server) = mock_server(status, body, "");
            let connection = GoldConnection {
                endpoint,
                api_key: "local-test-key".into(),
                client_id: "assistant-panel-test".into(),
            };
            let snapshot = fetch_snapshot(&client, Ok(connection)).await;
            let request = server.join().unwrap();
            let request_line = request.lines().next().unwrap();
            assert!(request_line.contains(
                "group=node&type=list&collectors=gold&client_id=assistant-panel-test&ttl=120"
            ));
            assert!(!request_line.contains("local-test-key"));
            assert!(request
                .to_ascii_lowercase()
                .contains("x-api-key: local-test-key\r\n"));
            assert_eq!(snapshot.ok, success);
            if success {
                assert_eq!(snapshot.collected_at, Some(now));
                assert_eq!(snapshot.node_name.as_deref(), Some("本地测试节点"));
                assert!(!snapshot.stale);
            } else {
                assert!(snapshot.error.unwrap().contains("401"));
            }
        }
    }

    #[tokio::test]
    async fn redirect_is_reported_without_forwarding_the_key() {
        let (endpoint, server) = mock_server(
            "302 Found",
            String::new(),
            "Location: http://127.0.0.1:1/must-not-follow\r\n",
        );
        let connection = GoldConnection {
            endpoint,
            api_key: "local-test-key".into(),
            client_id: "client".into(),
        };
        let snapshot = fetch_snapshot(
            &client_builder().no_proxy().build().unwrap(),
            Ok(connection),
        )
        .await;
        server.join().unwrap();
        assert!(!snapshot.ok);
        assert!(snapshot.error.unwrap().contains("HTTP 302"));
    }
}
