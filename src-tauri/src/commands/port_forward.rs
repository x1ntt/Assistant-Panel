use crate::modules::port_forwarder;
use crate::state::{AppState, ForwardRuleSnapshot, InterfaceInfo, NewRule};
use tauri::{AppHandle, State};

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
    let id = port_forwarder::start_tcp_sync(rule.clone(), app, &state)?;

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
