use crate::state::{AppState, ConnectionSnapshot, ConnectionStats, ForwardRule, ForwardRuleSnapshot, NewRule};
use nanoid::nanoid;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Synchronous: bind with std, spawn background thread with own Tokio runtime
pub fn start_tcp_sync(
    rule: NewRule,
    app: AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let id = nanoid!();
    let bind_addr = format!("{}:{}", rule.listen_addr, rule.listen_port);

    let std_listener = std::net::TcpListener::bind(&bind_addr)
        .map_err(|e| format!("绑定失败 {}: {}", bind_addr, e))?;
    std_listener.set_nonblocking(true)
        .map_err(|e| format!("设置非阻塞失败: {}", e))?;

    let forward_rule = Arc::new(ForwardRule {
        id: id.clone(),
        listen_addr: rule.listen_addr.clone(),
        listen_port: rule.listen_port,
        target_addr: rule.target_addr.clone(),
        target_port: rule.target_port,
        protocol: rule.protocol.clone(),
        total_bytes_in: AtomicU64::new(0),
        total_bytes_out: AtomicU64::new(0),
        connections: std::sync::Mutex::new(HashMap::new()),
        cancel: tokio_util::sync::CancellationToken::new(),
    });

    {
        let mut rules = state.port_forwards.lock().unwrap();
        rules.insert(id.clone(), forward_rule.clone());
    }

    let rule_clone = forward_rule.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            let listener = TcpListener::from_std(std_listener)
                .expect("failed to convert listener");
            let r = rule_clone.clone();
            tokio::spawn(async move { accept_loop(listener, r).await; });
            let r2 = rule_clone.clone();
            tokio::spawn(async move { stats_loop(r2, app).await; });
            rule_clone.cancel.cancelled().await;
        });
    });

    Ok(id)
}

async fn accept_loop(listener: TcpListener, rule: Arc<ForwardRule>) {
    loop {
        tokio::select! {
            _ = rule.cancel.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        let client = addr.to_string();
                        let target = format!("{}:{}", rule.target_addr, rule.target_port);
                        {
                            let mut conns = rule.connections.lock().unwrap();
                            conns.insert(client.clone(), Arc::new(ConnectionStats::new(client.clone())));
                        }
                        let r = rule.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_tcp(stream, &target, &client, &r).await {
                                eprintln!("连接错误 {}: {}", client, e);
                            }
                            r.connections.lock().unwrap().remove(&client);
                        });
                    }
                    Err(e) => eprintln!("接受连接错误: {}", e),
                }
            }
        }
    }
}

async fn handle_tcp(
    client: TcpStream,
    target: &str,
    client_addr: &str,
    rule: &Arc<ForwardRule>,
) -> Result<(), String> {
    let target_stream = TcpStream::connect(target)
        .await
        .map_err(|e| format!("连接目标失败 {}: {}", target, e))?;

    // Get reference to ConnectionStats for live updates
    let conn_stats = {
        let conns = rule.connections.lock().unwrap();
        conns.get(client_addr).cloned()
    };
    let conn_stats = match conn_stats {
        Some(c) => c,
        None => return Err("连接未注册".to_string()),
    };

    let (mut c_read, mut c_write) = client.into_split();
    let (mut t_read, mut t_write) = target_stream.into_split();

    // client → target (upload): update conn_stats.bytes_out + rule.total_bytes_out
    let cs_out = conn_stats.clone();
    let r1 = rule.clone();
    let upload = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match c_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if t_write.write_all(&buf[..n]).await.is_err() { break; }
                    cs_out.bytes_out.fetch_add(n as u64, Ordering::Relaxed);
                    r1.total_bytes_out.fetch_add(n as u64, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
    });

    // target → client (download): update conn_stats.bytes_in + rule.total_bytes_in
    let cs_in = conn_stats.clone();
    let r2 = rule.clone();
    let download = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match t_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if c_write.write_all(&buf[..n]).await.is_err() { break; }
                    cs_in.bytes_in.fetch_add(n as u64, Ordering::Relaxed);
                    r2.total_bytes_in.fetch_add(n as u64, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(upload, download);
    Ok(())
}

async fn stats_loop(rule: Arc<ForwardRule>, app: AppHandle) {
    let mut prev_in: u64 = 0;
    let mut prev_out: u64 = 0;
    let mut prev_conn: HashMap<String, (u64, u64)> = HashMap::new();

    loop {
        tokio::select! {
            _ = rule.cancel.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                let total_in = rule.total_bytes_in.load(Ordering::Relaxed);
                let total_out = rule.total_bytes_out.load(Ordering::Relaxed);
                let speed_in = total_in.saturating_sub(prev_in);
                let speed_out = total_out.saturating_sub(prev_out);
                prev_in = total_in;
                prev_out = total_out;

                let conns = rule.connections.lock().unwrap();
                let mut snapshots = Vec::new();
                let mut cur = HashMap::new();

                for (addr, s) in conns.iter() {
                    let ci = s.bytes_in.load(Ordering::Relaxed);
                    let co = s.bytes_out.load(Ordering::Relaxed);
                    let (pi, po) = prev_conn.get(addr).copied().unwrap_or((0, 0));
                    cur.insert(addr.clone(), (ci, co));
                    snapshots.push(ConnectionSnapshot {
                        client_addr: s.client_addr.clone(),
                        bytes_in: ci, bytes_out: co,
                        speed_in: ci.saturating_sub(pi),
                        speed_out: co.saturating_sub(po),
                    });
                }
                prev_conn = cur;

                let _ = app.emit("port-forward:stats", &ForwardRuleSnapshot {
                    id: rule.id.clone(),
                    listen_addr: rule.listen_addr.clone(),
                    listen_port: rule.listen_port,
                    target_addr: rule.target_addr.clone(),
                    target_port: rule.target_port,
                    protocol: rule.protocol.clone(),
                    total_bytes_in: total_in, total_bytes_out: total_out,
                    speed_in, speed_out,
                    connection_count: snapshots.len(),
                    connections: snapshots,
                    running: true,
                });
            }
        }
    }
}

pub fn stop_sync(rule_id: &str, state: &AppState) -> Result<(), String> {
    let rule = {
        let rules = state.port_forwards.lock().unwrap();
        rules.get(rule_id).cloned()
    };
    if let Some(rule) = rule {
        rule.cancel.cancel();
        let mut rules = state.port_forwards.lock().unwrap();
        rules.remove(rule_id);
        Ok(())
    } else {
        Err("规则不存在".to_string())
    }
}

pub fn disconnect_connection_sync(rule_id: &str, client_addr: &str, state: &AppState) -> Result<(), String> {
    let rules = state.port_forwards.lock().unwrap();
    if let Some(rule) = rules.get(rule_id) {
        rule.connections.lock().unwrap().remove(client_addr);
        Ok(())
    } else {
        Err("规则不存在".to_string())
    }
}

pub fn get_snapshots(state: &AppState) -> Vec<ForwardRuleSnapshot> {
    let rules = state.port_forwards.lock().unwrap();
    rules.values().map(|rule| {
        let conns = rule.connections.lock().unwrap();
        let cs: Vec<ConnectionSnapshot> = conns.values().map(|c| ConnectionSnapshot {
            client_addr: c.client_addr.clone(),
            bytes_in: c.bytes_in.load(Ordering::Relaxed),
            bytes_out: c.bytes_out.load(Ordering::Relaxed),
            speed_in: 0, speed_out: 0,
        }).collect();
        ForwardRuleSnapshot {
            id: rule.id.clone(), listen_addr: rule.listen_addr.clone(), listen_port: rule.listen_port,
            target_addr: rule.target_addr.clone(), target_port: rule.target_port, protocol: rule.protocol.clone(),
            total_bytes_in: rule.total_bytes_in.load(Ordering::Relaxed),
            total_bytes_out: rule.total_bytes_out.load(Ordering::Relaxed),
            speed_in: 0, speed_out: 0,
            connection_count: cs.len(), connections: cs,
            running: !rule.cancel.is_cancelled(),
        }
    }).collect()
}
