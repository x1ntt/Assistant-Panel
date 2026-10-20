use crate::modules::port_forwarder;
use crate::state::{AppState, ForwardRuleSnapshot, InterfaceInfo, NewRule};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Deserialize, Serialize)]
pub struct SavedForwardRule {
    #[serde(flatten)]
    rule: NewRule,
    #[serde(default)]
    paused: bool,
}

fn parse_saved_rules(bytes: &[u8]) -> Result<Vec<SavedForwardRule>, String> {
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|error| format!("配置文件无效: {error}"))?;
    let Some(value) = settings.get("port-forward.rules") else {
        return Ok(Vec::new());
    };
    let rules: Vec<SavedForwardRule> = serde_json::from_value(value.clone())
        .map_err(|error| format!("转发规则配置无效: {error}"))?;
    for saved in &rules {
        if saved.rule.listen_addr.trim().is_empty()
            || saved.rule.target_addr.trim().is_empty()
            || saved.rule.listen_port == 0
            || saved.rule.target_port == 0
            || !matches!(saved.rule.protocol.as_str(), "tcp" | "udp" | "both")
        {
            return Err("转发规则配置无效: 地址、端口或协议无效".into());
        }
    }
    Ok(rules)
}

#[tauri::command]
pub fn get_saved_forward_rules(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<SavedForwardRule>, String> {
    let result = (|| {
        let path = app.path().app_data_dir().map_err(|error| error.to_string())?.join("settings.json");
        match std::fs::read(path) {
            Ok(bytes) => parse_saved_rules(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(format!("读取转发规则失败: {error}")),
        }
    })();
    if let Err(error) = &result {
        state.logger.plugin("port-forward", "ERROR", error);
    }
    result
}

#[tauri::command]
pub async fn get_interfaces() -> Result<Vec<InterfaceInfo>, String> {
    let mut interfaces = vec![InterfaceInfo {
        name: "全部".to_string(),
        ip: "0.0.0.0".to_string(),
    }];

    if let Ok(addrs) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in addrs {
            let ip_str = ip.to_string();
            if !ip_str.starts_with("127.") && !ip_str.starts_with("::") {
                interfaces.push(InterfaceInfo {
                    name: "接口".to_string(),
                    ip: ip_str,
                });
            }
        }
    }

    interfaces.push(InterfaceInfo {
        name: "本地".to_string(),
        ip: "127.0.0.1".to_string(),
    });

    Ok(interfaces)
}

#[tauri::command]
pub fn add_forward_rule(app: AppHandle, rule: NewRule, state: State<'_, AppState>) -> Result<ForwardRuleSnapshot, String> {
    let id = port_forwarder::start_tcp_sync(rule.clone(), app, &state).map_err(|error| {
        state.logger.plugin("port-forward", "ERROR", &format!("规则启动失败: {error}"));
        error
    })?;

    state.logger.plugin("port-forward", "INFO",
        &format!("规则添加: {}:{} -> {}:{} [{}]",
            rule.listen_addr, rule.listen_port, rule.target_addr, rule.target_port, rule.protocol));

    let rules = state.port_forwards.lock().unwrap();
    if let Some(r) = rules.get(&id) {
        Ok(ForwardRuleSnapshot {
            id: r.id.clone(),
            listen_addr: r.listen_addr.clone(),
            listen_port: r.listen_port,
            target_addr: r.target_addr.clone(),
            target_port: r.target_port,
            protocol: r.protocol.clone(),
            total_bytes_in: 0,
            total_bytes_out: 0,
            speed_in: 0,
            speed_out: 0,
            connection_count: 0,
            connections: vec![],
            running: true,
        })
    } else {
        Err("规则创建失败".to_string())
    }
}

#[tauri::command]
pub fn remove_forward_rule(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let rule = {
        let rules = state.port_forwards.lock().unwrap();
        rules.get(&id).cloned()
    };
    if let Some(rule) = rule {
        rule.cancel.cancel();
        let mut rules = state.port_forwards.lock().unwrap();
        rules.remove(&id);

        state.logger.plugin("port-forward", "INFO",
            &format!("规则删除: {}:{} -> {}:{}",
                rule.listen_addr, rule.listen_port, rule.target_addr, rule.target_port));
        Ok(())
    } else {
        Err("规则不存在".to_string())
    }
}

#[tauri::command]
pub fn pause_forward(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let rule = {
        let rules = state.port_forwards.lock().unwrap();
        rules.get(&id).cloned()
    };
    if let Some(rule) = rule {
        rule.cancel.cancel();
        let mut rules = state.port_forwards.lock().unwrap();
        rules.remove(&id);

        state.logger.plugin("port-forward", "INFO",
            &format!("规则暂停: {}:{} -> {}:{}",
                rule.listen_addr, rule.listen_port, rule.target_addr, rule.target_port));
        Ok(())
    } else {
        Err("规则不存在".to_string())
    }
}

#[tauri::command]
pub fn resume_forward(app: AppHandle, rule: NewRule, state: State<'_, AppState>) -> Result<ForwardRuleSnapshot, String> {
    state.logger.plugin("port-forward", "INFO",
        &format!("规则恢复: {}:{} -> {}:{}",
            rule.listen_addr, rule.listen_port, rule.target_addr, rule.target_port));
    add_forward_rule(app, rule, state)
}

#[tauri::command]
pub fn disconnect_rule_connection(
    rule_id: String,
    client_addr: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.logger.plugin("port-forward", "INFO",
        &format!("断开连接: {} (规则 {})", client_addr, rule_id));
    port_forwarder::disconnect_connection_sync(&rule_id, &client_addr, &state)
}

#[tauri::command]
pub fn get_forward_rules(state: State<'_, AppState>) -> Vec<ForwardRuleSnapshot> {
    port_forwarder::get_snapshots(&state)
}

#[cfg(test)]
mod tests {
    use super::parse_saved_rules;

    #[test]
    fn accepts_legacy_rules_and_preserves_paused_state() {
        let rules = parse_saved_rules(br#"{"port-forward.rules":[
            {"listenAddr":"0.0.0.0","listenPort":8080,"targetAddr":"localhost","targetPort":80,"protocol":"tcp"},
            {"listenAddr":"127.0.0.1","listenPort":8081,"targetAddr":"localhost","targetPort":80,"protocol":"tcp","paused":true}
        ]}"#).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(!rules[0].paused);
        assert!(rules[1].paused);
    }

    #[test]
    fn only_missing_rules_are_treated_as_empty() {
        assert!(parse_saved_rules(br#"{}"#).unwrap().is_empty());
        for bytes in [b"{".as_slice(), b"[]", br#"{"port-forward.rules":null}"#,
            br#"{"port-forward.rules":[{}]}"#] {
            assert!(parse_saved_rules(bytes).is_err());
        }
    }

    #[test]
    fn rejects_invalid_ports_and_paused_types() {
        for extra in [r#""listenPort":0"#, r#""listenPort":65536"#, r#""listenPort":8080,"paused":"true""#] {
            let json = format!(r#"{{"port-forward.rules":[{{"listenAddr":"0.0.0.0",{extra},"targetAddr":"localhost","targetPort":80,"protocol":"tcp"}}]}}"#);
            assert!(parse_saved_rules(json.as_bytes()).is_err());
        }
    }
}
