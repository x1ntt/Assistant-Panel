//! Native queries, pin mutations and publication share one lock. All discovered
//! system interfaces are monitored; pins only affect presentation order.
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

pub const REMOVED_KEY: &str = "network-traffic.removedInterfaces";
pub const PINNED_KEY: &str = "network-traffic.pinnedInterfaces";
pub const EVENT: &str = "network-traffic:stats";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub is_filter: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub is_filter: bool,
    pub speed_in: Option<f64>,
    pub speed_out: Option<f64>,
    pub total_bytes_in: String,
    pub total_bytes_out: String,
    pub highlighted: bool,
    pub pinned: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub updated_at: u64,
    pub enabled: bool,
    pub error: Option<String>,
    pub config_error: Option<String>,
    pub warning: Option<String>,
    pub interfaces: Vec<TrafficRow>,
}

#[derive(Clone)]
struct DiscoveredInterface {
    meta: InterfaceMeta,
    luid: u64,
}

struct CounterSample {
    bytes_in: u64,
    bytes_out: u64,
    at: Instant,
}

struct Tracked {
    row: TrafficRow,
    baseline: Option<CounterSample>,
    total_in: u64,
    total_out: u64,
    luid: u64,
}

impl Tracked {
    fn new(interface: &DiscoveredInterface) -> Self {
        Self {
            row: TrafficRow {
                id: interface.meta.id.clone(), name: interface.meta.name.clone(),
                description: interface.meta.description.clone(), status: interface.meta.status.clone(),
                is_filter: interface.meta.is_filter,
                speed_in: None, speed_out: None, total_bytes_in: "0".into(),
                total_bytes_out: "0".into(), highlighted: false, pinned: false, error: None,
            },
            baseline: None, total_in: 0, total_out: 0, luid: interface.luid,
        }
    }

    fn invalidate(&mut self, error: Option<String>) {
        self.baseline = None;
        self.row.speed_in = None;
        self.row.speed_out = None;
        self.row.error = error;
    }

    fn sample(&mut self, sample: CounterSample) {
        self.row.error = None;
        self.row.speed_in = Some(0.0);
        self.row.speed_out = Some(0.0);
        if let Some(old) = &self.baseline {
            let elapsed = sample.at.saturating_duration_since(old.at).as_secs_f64();
            // Either direction rolling back invalidates the entire baseline.
            if sample.bytes_in >= old.bytes_in && sample.bytes_out >= old.bytes_out && elapsed > 0.0 {
                let incoming = sample.bytes_in - old.bytes_in;
                let outgoing = sample.bytes_out - old.bytes_out;
                self.total_in = self.total_in.saturating_add(incoming);
                self.total_out = self.total_out.saturating_add(outgoing);
                self.row.speed_in = Some(incoming as f64 / elapsed);
                self.row.speed_out = Some(outgoing as f64 / elapsed);
            }
        }
        self.row.total_bytes_in = self.total_in.to_string();
        self.row.total_bytes_out = self.total_out.to_string();
        self.baseline = Some(sample);
    }
}

pub struct Monitor {
    enabled: bool,
    revision: u64,
    updated_at: u64,
    error: Option<String>,
    config_error: Option<String>,
    migration_warning: Option<String>,
    directory: BTreeMap<String, DiscoveredInterface>,
    tracked: BTreeMap<String, Tracked>,
    pinned_order: Vec<String>,
}

impl Monitor {
    fn new(enabled: bool, pinned_order: Vec<String>) -> Self {
        Self { enabled, pinned_order, revision: 0, updated_at: 0, error: None, config_error: None, migration_warning: None,
            directory: BTreeMap::new(), tracked: BTreeMap::new() }
    }

    pub fn snapshot(&self) -> Snapshot {
        let pinned: BTreeSet<&String> = self.pinned_order.iter().collect();
        let interfaces = self.pinned_order.iter()
            .filter_map(|id| self.tracked.get(id))
            .chain(self.tracked.iter().filter(|(id, _)| !pinned.contains(id)).map(|(_, v)| v))
            .map(|v| { let mut row = v.row.clone(); row.pinned = pinned.contains(&row.id); row })
            .collect();
        Snapshot { revision: self.revision, updated_at: self.updated_at, enabled: self.enabled,
            error: self.error.clone(), config_error: self.config_error.clone(),
            warning: self.migration_warning.clone(),
            interfaces }
    }

    pub fn publish(&mut self, app: &AppHandle) -> Snapshot {
        self.advance();
        let snapshot = self.snapshot();
        // Emit under the same lock as mutation/sample commit: ordered revisions.
        let _ = app.emit(EVENT, &snapshot);
        snapshot
    }

    fn advance(&mut self) {
        self.revision += 1;
    }

    fn discover(&mut self, result: Result<Vec<DiscoveredInterface>, String>) {
        let interfaces = match result {
            Ok(v) => v,
            Err(error) => { self.error = Some(error); return; }
        };
        self.error = None;
        self.directory = interfaces.into_iter().map(|v| (v.meta.id.clone(), v)).collect();
        for (id, interface) in &self.directory {
            let tracked = self.tracked.entry(id.clone()).or_insert_with(|| Tracked::new(interface));
            if tracked.luid != interface.luid || tracked.row.status != interface.meta.status {
                tracked.invalidate(None);
            }
            tracked.luid = interface.luid;
            tracked.row.name = interface.meta.name.clone();
            tracked.row.description = interface.meta.description.clone();
            tracked.row.status = interface.meta.status.clone();
            // Classification is metadata only: never reset counters for it.
            tracked.row.is_filter = interface.meta.is_filter;
        }
        for (id, tracked) in &mut self.tracked {
            if !self.directory.contains_key(id) {
                tracked.row.status = "unavailable".into();
                tracked.invalidate(Some("Interface is no longer available".into()));
            }
        }
    }

    fn tick<D, R>(&mut self, discover: D, mut read: R)
    where D: FnOnce() -> Result<Vec<DiscoveredInterface>, String>,
          R: FnMut(&DiscoveredInterface) -> Result<CounterSample, String> {
        if !self.enabled || self.config_error.is_some() { return; }
        self.discover(discover());
        for (id, tracked) in &mut self.tracked {
            if let Some(interface) = self.directory.get(id) {
                match read(interface) {
                    Ok(sample) => { tracked.row.status = interface.meta.status.clone(); tracked.sample(sample); }
                    Err(error) => { tracked.row.status = "unavailable".into(); tracked.invalidate(Some(error)); }
                }
            }
        }
        self.updated_at = epoch_ms();
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if let Some(error) = &self.config_error { return Err(error.clone()); }
        if self.enabled == enabled { return Ok(()); }
        self.enabled = enabled;
        for tracked in self.tracked.values_mut() { tracked.invalidate(None); }
        Ok(())
    }

    pub fn highlight(&mut self, id: &str, highlighted: bool) -> Result<(), String> {
        self.tracked.get_mut(id).ok_or("Interface is not monitored")?.row.highlighted = highlighted;
        Ok(())
    }

    // Save before committing order. Sampling state is never rebuilt by pinning.
    pub fn pin<F>(&mut self, id: &str, save: F) -> Result<(), String>
    where F: FnOnce(&[String]) -> Result<(), String> {
        if let Some(error) = &self.config_error { return Err(error.clone()); }
        if !self.tracked.contains_key(id) { return Err("Interface is not monitored".into()); }
        let mut order = self.pinned_order.clone();
        order.retain(|pinned| pinned != id);
        order.insert(0, id.to_owned());
        save(&order)?;
        self.pinned_order = order;
        self.migration_warning = None;
        Ok(())
    }

    pub fn unpin<F>(&mut self, id: &str, save: F) -> Result<(), String>
    where F: FnOnce(&[String]) -> Result<(), String> {
        if let Some(error) = &self.config_error { return Err(error.clone()); }
        if !self.tracked.contains_key(id) { return Err("Interface is not monitored".into()); }
        let order: Vec<_> = self.pinned_order.iter().filter(|pinned| pinned.as_str() != id).cloned().collect();
        save(&order)?;
        self.pinned_order = order;
        self.migration_warning = None;
        Ok(())
    }
}

pub struct NetworkTrafficState {
    pub inner: Mutex<Monitor>,
    started: AtomicBool,
    cancel: CancellationToken,
}

pub struct LoadedSettings {
    pub pinned_order: Vec<String>,
    pub migration_warning: Option<String>,
}

impl NetworkTrafficState {
    pub fn new(enabled: bool, pinned_order: Vec<String>) -> Self {
        Self { inner: Mutex::new(Monitor::new(enabled, pinned_order)), started: AtomicBool::new(false),
            cancel: CancellationToken::new() }
    }
    pub fn from_settings(enabled: bool, settings: Result<LoadedSettings, String>) -> Self {
        match settings {
            Ok(settings) => {
                let state = Self::new(enabled, settings.pinned_order);
                state.inner.lock().unwrap().migration_warning = settings.migration_warning;
                state
            }
            Err(error) => {
                let state = Self::new(enabled, Vec::new());
                let mut monitor = state.inner.lock().unwrap();
                let error = format!("Network traffic settings could not be loaded; fix settings and restart: {error}");
                monitor.error = Some(error.clone());
                monitor.config_error = Some(error);
                drop(monitor);
                state
            }
        }
    }
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

pub fn start_monitor(app: AppHandle) {
    let state = app.state::<NetworkTrafficState>();
    if state.started.swap(true, Ordering::SeqCst) { return; }
    let cancel = state.cancel.clone();
    tauri::async_runtime::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                _ = timer.tick() => {
                    let sample_app = app.clone();
                    let sample_cancel = cancel.clone();
                    // Await each blocking pass: at most one native sampler in
                    // flight, with no mutex/API/disk waits on an async worker.
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        let state = sample_app.state::<NetworkTrafficState>();
                        let Ok(mut monitor) = state.inner.lock() else { return false; };
                        if sample_cancel.is_cancelled() { return false; }
                        monitor.tick(discover_interfaces, read_counters);
                        if !sample_cancel.is_cancelled() { monitor.publish(&sample_app); }
                        true
                    }).await;
                    if !matches!(result, Ok(true)) { break; }
                }
            }
        }
    });
}

fn epoch_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(any(windows, test))]
fn is_filter_interface(flags: u8) -> bool {
    // MIB_IF_ROW2: HardwareInterface is bit 0, FilterInterface is bit 1
    // (a filter module), independent of adapter names and other flags.
    // https://learn.microsoft.com/en-us/windows/win32/api/netioapi/ns-netioapi-mib_if_row2
    flags & 0x02 != 0
}

#[cfg(windows)]
fn discover_interfaces() -> Result<Vec<DiscoveredInterface>, String> {
    use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2Ex,
        MibIfTableNormalWithoutStatistics, MIB_IF_TABLE2, MIB_IF_ROW2};
    struct Table(*mut MIB_IF_TABLE2);
    impl Drop for Table {
        fn drop(&mut self) {
            if !self.0.is_null() { unsafe { FreeMibTable(self.0.cast()); } }
        }
    }
    let mut table = Table(std::ptr::null_mut());
    // Windows 10 1703+; never fall back to all-interface statistics.
    let result = unsafe { GetIfTable2Ex(MibIfTableNormalWithoutStatistics, &mut table.0) };
    if result.0 != 0 { return Err(format!("GetIfTable2Ex (metadata only) failed: {}", result.0)); }
    if table.0.is_null() { return Err("GetIfTable2Ex returned a null table".into()); }
    // OS allocation has NumEntries aligned rows. Raw pointers avoid a reference
    // limited to the flexible array's declared [1] element.
    let rows: &[MIB_IF_ROW2] = unsafe { std::slice::from_raw_parts(std::ptr::addr_of!((*table.0).Table).cast(), (*table.0).NumEntries as usize) };
    Ok(rows.iter().map(|row| {
        DiscoveredInterface { meta: InterfaceMeta { id: guid_id(row.InterfaceGuid),
            name: wide_string(&row.Alias), description: wide_string(&row.Description),
            status: interface_status(row.OperStatus.0),
            is_filter: is_filter_interface(row.InterfaceAndOperStatusFlags._bitfield) },
            luid: unsafe { row.InterfaceLuid.Value } }
    }).collect())
}

#[cfg(windows)]
fn read_counters(interface: &DiscoveredInterface) -> Result<CounterSample, String> {
    use windows::Win32::NetworkManagement::IpHelper::{GetIfEntry2, MIB_IF_ROW2};
    let mut row = MIB_IF_ROW2::default();
    row.InterfaceLuid.Value = interface.luid;
    let result = unsafe { GetIfEntry2(&mut row) };
    if result.0 != 0 { return Err(format!("GetIfEntry2 failed: {}", result.0)); }
    if guid_id(row.InterfaceGuid) != interface.meta.id { return Err("Interface identity changed".into()); }
    Ok(CounterSample { bytes_in: row.InOctets, bytes_out: row.OutOctets, at: Instant::now() })
}

#[cfg(windows)]
fn guid_id(guid: windows::core::GUID) -> String {
    format!("{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1, guid.data2, guid.data3, guid.data4[0], guid.data4[1], guid.data4[2],
        guid.data4[3], guid.data4[4], guid.data4[5], guid.data4[6], guid.data4[7])
}

#[cfg(windows)]
fn wide_string(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|v| *v == 0).unwrap_or(value.len())])
}

#[cfg(windows)]
fn interface_status(status: i32) -> String {
    match status { 1 => "up", 2 => "down", 3 => "testing", 4 => "unknown",
        5 => "dormant", 6 => "not-present", 7 => "lower-layer-down", _ => "unknown" }.into()
}

#[cfg(not(windows))]
fn discover_interfaces() -> Result<Vec<DiscoveredInterface>, String> {
    Err("Network traffic monitoring is only supported on Windows".into())
}
#[cfg(not(windows))]
fn read_counters(_: &DiscoveredInterface) -> Result<CounterSample, String> {
    Err("Network traffic monitoring is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(id: &str) -> DiscoveredInterface {
        DiscoveredInterface { meta: InterfaceMeta { id: id.into(), name: id.into(),
            description: "virtual or physical".into(), status: "up".into(), is_filter: false }, luid: 42 }
    }
    fn sample(at: Instant, incoming: u64, outgoing: u64) -> CounterSample {
        CounterSample { at, bytes_in: incoming, bytes_out: outgoing }
    }
    fn tick(m: &mut Monitor, at: Instant, incoming: u64, outgoing: u64) {
        m.tick(|| Ok(vec![interface("a")]), |_| Ok(sample(at, incoming, outgoing)));
    }

    #[test]
    fn network_filter_flag_uses_only_second_bit() {
        for (flags, expected) in [(0x00, false), (0x01, false), (0x02, true),
            (0x03, true), (0x04, false), (0x80, false), (0xfd, false), (0xff, true)] {
            assert_eq!(is_filter_interface(flags), expected, "flags={flags:#04x}");
        }
        for flags in 0..=u8::MAX {
            assert_eq!(is_filter_interface(flags), (flags >> 1) & 1 == 1);
        }
    }

    #[test]
    fn network_filter_classification_is_independent_of_names() {
        for name in ["Ethernet", "Wi-Fi", "VPN", "Hyper-V", "Loopback",
            "QoS Packet Scheduler", "WFP filter", "过滤器", ""] {
            for flags in [0x00, 0x01, 0x02, 0x03, 0xfc, 0xfe] {
                let mut discovered = interface("a");
                discovered.meta.name = name.into();
                discovered.meta.description = name.into();
                discovered.meta.is_filter = is_filter_interface(flags);
                let mut m = Monitor::new(true, Vec::new());
                m.discover(Ok(vec![discovered.clone()]));
                assert_eq!(m.snapshot().interfaces[0].is_filter, flags & 0x02 != 0);
                let meta_json = serde_json::to_value(&discovered.meta).unwrap();
                let snapshot_json = serde_json::to_value(m.snapshot()).unwrap();
                assert_eq!(meta_json["isFilter"], flags & 0x02 != 0);
                assert_eq!(snapshot_json["interfaces"][0]["isFilter"], flags & 0x02 != 0);
                assert!(meta_json.get("is_filter").is_none());
                assert!(snapshot_json["interfaces"][0].get("is_filter").is_none());
            }
        }
    }

    #[test]
    fn network_filter_metadata_updates_preserve_state_and_sample_all_interfaces() {
        let mut m = Monitor::new(true, Vec::new());
        let at = Instant::now();
        tick(&mut m, at, 100, 100);
        tick(&mut m, at + Duration::from_secs(1), 200, 300);
        m.highlight("a", true).unwrap();
        m.pin("a", |_| Ok(())).unwrap();
        let mut updated = interface("a");
        updated.meta.is_filter = true;
        updated.meta.name = "renamed".into();
        updated.meta.description = "updated description".into();
        m.discover(Ok(vec![updated.clone(), interface("b")]));
        let tracked = &m.tracked["a"];
        assert!(tracked.row.is_filter);
        assert_eq!(tracked.row.name, "renamed");
        assert_eq!(tracked.row.description, "updated description");
        assert_eq!(tracked.baseline.as_ref().unwrap().at, at + Duration::from_secs(1));
        assert_eq!(tracked.baseline.as_ref().unwrap().bytes_in, 200);
        assert_eq!(tracked.baseline.as_ref().unwrap().bytes_out, 300);
        assert_eq!((tracked.total_in, tracked.total_out), (100, 200));
        assert_eq!(tracked.row.speed_in, Some(100.0));
        assert_eq!(tracked.row.speed_out, Some(200.0));
        assert!(tracked.row.highlighted);
        assert!(m.snapshot().interfaces[0].pinned);
        let mut read_ids = Vec::new();
        m.tick(|| Ok(vec![updated.clone(), interface("b")]), |v| {
            read_ids.push(v.meta.id.clone());
            Ok(sample(at + Duration::from_secs(2), 350, 550))
        });
        assert_eq!(read_ids, ["a", "b"]);
        let row = &m.snapshot().interfaces[0];
        assert!(row.is_filter && row.highlighted && row.pinned);
        assert_eq!(row.total_bytes_in, "250");
        assert_eq!(row.total_bytes_out, "450");
        assert_eq!(row.speed_in, Some(150.0));
        assert_eq!(row.speed_out, Some(250.0));
        // The reverse classification change also keeps the current baseline.
        updated.meta.is_filter = false;
        m.tick(|| Ok(vec![updated]), |_| Ok(sample(at + Duration::from_secs(3), 450, 650)));
        let row = &m.snapshot().interfaces[0];
        assert!(!row.is_filter);
        assert!(row.highlighted && row.pinned);
        assert_eq!(row.total_bytes_in, "350");
        assert_eq!(row.total_bytes_out, "550");
        assert_eq!(row.speed_in, Some(100.0));
    }

    #[test]
    fn network_missing_and_failed_discovery_keep_last_filter_classification() {
        let mut m = Monitor::new(true, Vec::new());
        let mut filter = interface("a");
        filter.meta.is_filter = true;
        let at = Instant::now();
        m.tick(|| Ok(vec![filter]), |_| Ok(sample(at, 100, 100)));
        m.tick(|| Err("discovery failed".into()), |_| Ok(sample(at + Duration::from_secs(1), 200, 200)));
        assert!(m.snapshot().interfaces[0].is_filter);
        m.tick(|| Ok(vec![]), |_| panic!("missing interface must not be queried"));
        let row = &m.snapshot().interfaces[0];
        assert!(row.is_filter);
        assert_eq!(row.status, "unavailable");
        assert_eq!(row.total_bytes_in, "100");
        tick(&mut m, at + Duration::from_secs(2), 300, 300);
        assert!(!m.snapshot().interfaces[0].is_filter);
        assert_eq!(m.snapshot().interfaces[0].total_bytes_in, "100");
    }

    #[test]
    fn network_baseline_elapsed_direction_and_exact_totals() {
        let mut m = Monitor::new(true, Vec::new());
        let at = Instant::now();
        tick(&mut m, at, 9_007_199_254_740_992, 100);
        assert_eq!(m.snapshot().interfaces[0].total_bytes_in, "0");
        assert_eq!(m.snapshot().interfaces[0].speed_in, Some(0.0));
        tick(&mut m, at + Duration::from_millis(2500), 9_007_199_254_741_242, 600);
        let row = &m.snapshot().interfaces[0];
        assert_eq!(row.speed_in, Some(100.0));
        assert_eq!(row.speed_out, Some(200.0));
        assert_eq!(row.total_bytes_in, "250");
        assert_eq!(row.total_bytes_out, "500");
        let json = serde_json::to_value(m.snapshot()).unwrap();
        assert!(json["interfaces"][0]["totalBytesIn"].is_string());
        assert!(json.get("updatedAt").is_some());
    }

    #[test]
    fn network_rollback_and_read_failure_rebaseline_preserve_totals() {
        let mut m = Monitor::new(true, Vec::new());
        let at = Instant::now();
        tick(&mut m, at, 100, 100);
        tick(&mut m, at + Duration::from_secs(1), 200, 300);
        tick(&mut m, at + Duration::from_secs(2), 1, 10_000);
        assert_eq!(m.snapshot().interfaces[0].total_bytes_out, "200");
        m.tick(|| Ok(vec![interface("a")]), |_| Err("read failed".into()));
        assert_eq!(m.snapshot().interfaces[0].speed_in, None);
        assert!(m.snapshot().interfaces[0].error.is_some());
        tick(&mut m, at + Duration::from_secs(10), 10_000, 20_000);
        assert_eq!(m.snapshot().interfaces[0].total_bytes_in, "100");
        assert_eq!(m.snapshot().interfaces[0].speed_in, Some(0.0));
    }

    #[test]
    fn network_disappearance_and_discovery_failure_are_distinct() {
        let mut m = Monitor::new(true, Vec::new());
        let at = Instant::now();
        tick(&mut m, at, 100, 100);
        m.tick(|| Err("discovery failed".into()), |_| Ok(sample(at + Duration::from_secs(1), 200, 300)));
        assert_eq!(m.snapshot().interfaces[0].status, "up");
        assert_eq!(m.snapshot().interfaces[0].total_bytes_in, "100");
        assert!(m.snapshot().error.is_some());
        m.tick(|| Ok(vec![]), |_| panic!("missing interface must not be queried"));
        assert_eq!(m.snapshot().interfaces[0].status, "unavailable");
        assert_eq!(m.snapshot().interfaces[0].speed_in, None);
        tick(&mut m, at + Duration::from_secs(10), 10_000, 20_000);
        assert_eq!(m.snapshot().interfaces[0].total_bytes_in, "100");
    }

    fn ids(m: &Monitor) -> Vec<String> {
        m.snapshot().interfaces.into_iter().map(|row| row.id).collect()
    }

    #[test]
    fn network_pin_a_b_a_and_restart_multiple_pins() {
        let mut m = Monitor::new(true, Vec::new());
        m.discover(Ok(vec![interface("c"), interface("b"), interface("a")]));
        assert_eq!(ids(&m), ["a", "b", "c"]);
        let mut disk = Vec::new();
        for (id, expected) in [("a", vec!["a", "b", "c"]), ("b", vec!["b", "a", "c"]),
                               ("a", vec!["a", "b", "c"]), ("a", vec!["a", "b", "c"])] {
            m.pin(id, |order| { disk = order.to_vec(); Ok(()) }).unwrap();
            m.advance();
            assert_eq!(ids(&m), expected);
        }
        assert_eq!(disk, ["a", "b"]);
        let mut restarted = Monitor::new(true, disk);
        restarted.discover(Ok(vec![interface("c"), interface("a"), interface("b")]));
        assert_eq!(ids(&restarted), ["a", "b", "c"]);
        assert!(restarted.snapshot().interfaces[0].pinned);
        assert!(restarted.snapshot().interfaces[1].pinned);
        assert!(!restarted.snapshot().interfaces[2].pinned);
    }

    #[test]
    fn network_unpin_persists_other_pins_and_preserves_sampling_and_highlight() {
        let mut m = Monitor::new(true, vec!["b".into(), "a".into()]);
        let at = Instant::now();
        tick(&mut m, at, 100, 200);
        tick(&mut m, at + Duration::from_secs(1), 300, 500);
        m.highlight("a", true).unwrap();
        let before = m.snapshot().interfaces.into_iter().find(|row| row.id == "a").unwrap();
        let mut disk = Vec::new();
        m.unpin("a", |order| { disk = order.to_vec(); Ok(()) }).unwrap();
        assert_eq!(disk, ["b"]);
        let after = m.snapshot().interfaces.into_iter().find(|row| row.id == "a").unwrap();
        assert!(!after.pinned);
        assert!(after.highlighted);
        assert_eq!(after.total_bytes_in, before.total_bytes_in);
        assert_eq!(after.total_bytes_out, before.total_bytes_out);
        assert_eq!(after.speed_in, before.speed_in);
        assert_eq!(after.speed_out, before.speed_out);
        let mut restarted = Monitor::new(true, disk);
        restarted.discover(Ok(vec![interface("a"), interface("b")]));
        assert!(restarted.snapshot().interfaces[0].pinned);
        assert!(!restarted.snapshot().interfaces[1].pinned);
    }

    #[test]
    fn network_unpin_failure_preserves_order_and_can_remove_the_last_pin() {
        let mut m = Monitor::new(true, vec!["a".into()]);
        m.discover(Ok(vec![interface("a"), interface("b")]));
        assert!(m.unpin("a", |_| Err("disk denied".into())).is_err());
        assert_eq!(m.pinned_order, ["a"]);
        assert!(m.snapshot().interfaces[0].pinned);
        assert!(m.unpin("unknown", |_| panic!("unknown interface must not save")).is_err());
        m.unpin("a", |order| { assert!(order.is_empty()); Ok(()) }).unwrap();
        assert!(m.snapshot().interfaces.iter().all(|row| !row.pinned));
        m.unpin("a", |_| Ok(())).unwrap();
        m.config_error = Some("bad settings".into());
        assert!(m.unpin("a", |_| panic!("invalid settings must not save")).is_err());
    }

    #[test]
    fn network_missing_pin_reappears_and_all_interfaces_are_sampled() {
        let mut m = Monitor::new(true, vec!["missing".into(), "b".into()]);
        let at = Instant::now();
        m.tick(|| Ok(vec![interface("a"), interface("b")]), |_| Ok(sample(at, 100, 100)));
        assert_eq!(ids(&m), ["b", "a"]);
        assert_eq!(m.pinned_order, ["missing", "b"]);
        let mut read_ids = Vec::new();
        m.tick(|| Ok(vec![interface("a"), interface("missing"), interface("b")]), |v| {
            read_ids.push(v.meta.id.clone()); Ok(sample(at + Duration::from_secs(1), 200, 300))
        });
        assert_eq!(read_ids, ["a", "b", "missing"]);
        assert_eq!(ids(&m), ["missing", "b", "a"]);
        assert_eq!(m.tracked["missing"].total_in, 0);
        m.tick(|| Ok(vec![interface("a"), interface("b")]), |_| Ok(sample(at + Duration::from_secs(2), 300, 400)));
        assert_eq!(m.tracked["missing"].row.status, "unavailable");
        m.tick(|| Ok(vec![interface("a"), interface("missing"), interface("b")]), |_| Ok(sample(at + Duration::from_secs(3), 400, 500)));
        assert_eq!(ids(&m), ["missing", "b", "a"]);
        assert_eq!(m.tracked["missing"].total_in, 0);
    }

    #[test]
    fn network_pin_failure_and_success_preserve_sampling_state() {
        let mut m = Monitor::new(true, vec!["absent".into()]);
        let at = Instant::now();
        tick(&mut m, at, 100, 100);
        tick(&mut m, at + Duration::from_secs(1), 200, 300);
        m.highlight("a", true).unwrap();
        m.updated_at = 1234;
        let revision = m.revision;
        let before = serde_json::to_value(m.snapshot()).unwrap();
        assert!(m.pin("unknown", |_| panic!("unknown ID must not save")).is_err());
        assert!(m.pin("a", |_| Err("disk denied".into())).is_err());
        assert_eq!(serde_json::to_value(m.snapshot()).unwrap(), before);
        assert_eq!(m.pinned_order, ["absent"]);
        assert_eq!(m.revision, revision);
        assert_eq!(m.updated_at, 1234);
        assert_eq!(m.tracked["a"].baseline.as_ref().unwrap().at, at + Duration::from_secs(1));
        m.pin("a", |_| Ok(())).unwrap();
        let mut after = serde_json::to_value(m.snapshot()).unwrap();
        after["interfaces"][0]["pinned"] = false.into();
        assert_eq!(after, before, "pin only changes presentation, not sampling/highlight/time");
        assert_eq!(m.updated_at, 1234);
        assert_eq!(m.tracked["a"].total_in, 100);
        assert_eq!(m.tracked["a"].total_out, 200);
        assert!(m.tracked["a"].row.highlighted);
        assert_eq!(m.tracked["a"].baseline.as_ref().unwrap().bytes_in, 200);
        tick(&mut m, at + Duration::from_secs(2), 300, 400);
        assert_eq!(m.tracked["a"].total_in, 200);
        assert_eq!(m.snapshot().interfaces[0].speed_in, Some(100.0));
        assert!(m.snapshot().interfaces[0].pinned);
    }

    #[test]
    fn network_migration_warning_does_not_block_sampling_and_retries_on_pin() {
        let state = NetworkTrafficState::from_settings(true, Ok(LoadedSettings {
            pinned_order: Vec::new(), migration_warning: Some("cleanup denied".into()),
        }));
        let mut m = state.inner.lock().unwrap();
        tick(&mut m, Instant::now(), 100, 100);
        assert_eq!(ids(&m), ["a"]);
        assert!(m.snapshot().config_error.is_none());
        assert!(m.snapshot().error.is_none());
        assert_eq!(m.snapshot().warning.as_deref(), Some("cleanup denied"));
        assert_eq!(m.snapshot().interfaces[0].speed_in, Some(0.0));
        assert!(m.pin("a", |_| Err("denied again".into())).is_err());
        assert!(m.snapshot().warning.is_some());
        m.pin("a", |_| Ok(())).unwrap();
        assert!(m.snapshot().error.is_none());
        assert!(m.snapshot().warning.is_none());
    }

    #[test]
    fn network_disable_pauses_all_queries_and_resume_keeps_session() {
        let mut m = Monitor::new(true, Vec::new());
        let at = Instant::now();
        tick(&mut m, at, 100, 100);
        tick(&mut m, at + Duration::from_secs(1), 200, 300);
        m.discover(Ok(vec![interface("a"), interface("b")]));
        m.highlight("a", true).unwrap();
        m.highlight("b", true).unwrap();
        m.set_enabled(true).unwrap(); // idempotent: must preserve baseline
        assert!(m.tracked["a"].baseline.is_some());
        m.set_enabled(false).unwrap();
        m.tick(|| panic!("disabled discovery"), |_| panic!("disabled counter read"));
        m.set_enabled(false).unwrap();
        m.set_enabled(true).unwrap();
        tick(&mut m, at + Duration::from_secs(10), 10_000, 20_000);
        assert_eq!(m.tracked["a"].row.total_bytes_in, "100");
        assert!(m.tracked["a"].row.highlighted);
        assert!(m.tracked["b"].row.highlighted);
        m.highlight("a", false).unwrap();
        assert!(!m.tracked["a"].row.highlighted);
        m.advance();
        let revision = m.snapshot().revision;
        m.advance();
        assert!(m.snapshot().revision > revision);
    }

    #[test]
    fn network_all_statuses_are_discovered_and_readable_down_can_be_zero() {
        let mut m = Monitor::new(true, Vec::new());
        let mut down = interface("loopback");
        down.meta.status = "down".into();
        m.tick(|| Ok(vec![down.clone()]), |_| Ok(sample(Instant::now(), 0, 0)));
        assert_eq!(m.snapshot().interfaces[0].status, "down");
        assert_eq!(m.snapshot().interfaces[0].speed_out, Some(0.0));
    }

    #[test]
    fn network_corrupt_settings_block_queries_and_writes_not_application() {
        let state = NetworkTrafficState::from_settings(true, Err("invalid JSON value".into()));
        let mut m = state.inner.lock().unwrap();
        assert!(m.snapshot().error.is_some());
        m.tick(|| panic!("corrupt settings must not discover"), |_| panic!("must not sample"));
        assert!(m.set_enabled(false).is_err());
        assert!(m.set_enabled(true).is_err());
        assert!(m.snapshot().config_error.is_some());
        m.tick(|| panic!("enable cannot bypass corrupt settings"), |_| panic!("must not sample"));
        assert!(m.pin("a", |_| panic!("must not overwrite settings")).is_err());
        assert!(m.snapshot().error.is_some());
    }

    #[test]
    fn network_publication_and_mutations_do_not_refresh_sample_timestamp() {
        let mut m = Monitor::new(true, Vec::new());
        assert_eq!(m.snapshot().updated_at, 0);
        m.advance(); // publication changes revision only
        m.advance();
        assert_eq!(m.snapshot().updated_at, 0);
        tick(&mut m, Instant::now(), 100, 100);
        assert!(m.snapshot().updated_at > 0);
        // Fixed sentinel makes the test independent of wall-clock granularity.
        m.updated_at = 1234;
        let revision = m.snapshot().revision;
        m.highlight("a", true).unwrap();
        m.advance();
        m.discover(Ok(vec![interface("a"), interface("b")]));
        m.advance();
        m.pin("a", |_| Ok(())).unwrap();
        m.advance();
        m.pin("b", |_| Ok(())).unwrap();
        m.advance();
        assert_eq!(m.snapshot().updated_at, 1234);
        assert!(m.snapshot().revision > revision);
        m.set_enabled(false).unwrap();
        m.advance();
        m.tick(|| panic!("disabled discovery"), |_| panic!("disabled counters"));
        m.advance();
        assert_eq!(m.snapshot().updated_at, 1234);
        m.set_enabled(true).unwrap();
        assert_eq!(m.snapshot().updated_at, 1234);
        tick(&mut m, Instant::now(), 200, 200);
        assert!(m.snapshot().updated_at > 1234);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Explicit read-only native API smoke test on Windows"]
    fn network_native_smoke() {
        let interfaces = discover_interfaces().expect("metadata-only discovery failed");
        assert!(!interfaces.is_empty());
        // Windows reserves loopback LUID IfType 24. Metadata-only discovery
        // must include it; no name/localization assumptions or type filtering.
        assert!(interfaces.iter().any(|v| ((v.luid >> 48) & 0xffff) == 24));
        println!("native interface counts: total={}, filter={}", interfaces.len(),
            interfaces.iter().filter(|v| v.meta.is_filter).count());
        let mut tracked = Vec::new();
        for interface in &interfaces {
            let first = read_counters(interface).expect("first native counter read or GUID check failed");
            let mut row = Tracked::new(interface);
            row.sample(first);
            assert_eq!(row.row.total_bytes_in, "0");
            assert_eq!(row.row.total_bytes_out, "0");
            assert_eq!(row.row.speed_in, Some(0.0));
            tracked.push(row);
        }
        std::thread::sleep(Duration::from_secs(1));
        for (interface, row) in interfaces.iter().zip(&mut tracked) {
            let second = read_counters(interface).expect("second native counter read or GUID check failed");
            let first = row.baseline.as_ref().unwrap();
            assert!(second.bytes_in >= first.bytes_in);
            assert!(second.bytes_out >= first.bytes_out);
            let incoming = second.bytes_in - first.bytes_in;
            let outgoing = second.bytes_out - first.bytes_out;
            let elapsed = second.at.duration_since(first.at).as_secs_f64();
            assert!(elapsed >= 1.0);
            row.sample(second);
            assert_eq!(row.row.total_bytes_in, incoming.to_string());
            assert_eq!(row.row.total_bytes_out, outgoing.to_string());
            assert_eq!(row.row.speed_in, Some(incoming as f64 / elapsed));
            assert_eq!(row.row.speed_out, Some(outgoing as f64 / elapsed));
            assert!(row.row.speed_in.unwrap().is_finite());
        }
    }
}
