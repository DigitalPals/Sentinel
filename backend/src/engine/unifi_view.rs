use std::collections::HashMap;

use chrono::{DateTime, Local, Utc};

use crate::config::AlertThresholds;
use crate::model::{Event, PortOut, RadioOut, TopoNode, UniDeviceOut, UnifiView};
use crate::unifi::UnifiData;

use super::alerts::Candidate;
use super::format::fmt_uptime;
use super::topology::build_topology;

pub(super) struct UnifiProcessed {
    pub view: UnifiView,
    pub topology: TopoNode,
    pub events: Vec<Event>,
    pub candidates: Vec<Candidate>,
    pub wan_down_mbps: f64,
    pub wan_up_mbps: f64,
}

pub(super) fn process_unifi(data: &UnifiData, now: i64, th: &AlertThresholds) -> UnifiProcessed {
    // Clients per device, by the device they connect through.
    let mut clients_per_device: HashMap<String, u32> = HashMap::new();
    let mut wireless = 0u32;
    let mut wired = 0u32;
    for c in &data.clients {
        match c.kind.as_deref() {
            Some("WIRELESS") => wireless += 1,
            Some("WIRED") => wired += 1,
            _ => {}
        }
        if let Some(id) = &c.uplink_device_id {
            *clients_per_device.entry(id.clone()).or_insert(0) += 1;
        }
    }

    let mut devices: Vec<UniDeviceOut> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut uplink_of: HashMap<String, Option<String>> = HashMap::new();
    let mut poe_active = 0u32;
    let mut poe_capable = 0u32;
    let mut wan_down = 0.0;
    let mut wan_up = 0.0;

    for b in &data.devices {
        let model = b.list.model.clone().unwrap_or_default();
        let online = b.list.state.as_deref() == Some("ONLINE");
        let features = &b.list.features;
        let is_ap = features.iter().any(|f| f == "accessPoint");
        let is_gateway = model.contains("UCG")
            || model.contains("UDM")
            || model.contains("UXG")
            || model.contains("UDR");
        let kind = if is_gateway {
            "Gateway"
        } else if is_ap {
            "Access Point"
        } else if model.contains("UPS") {
            "UPS"
        } else {
            "Switch"
        };

        let cpu = b.stats.cpu_utilization_pct.unwrap_or(0.0).round() as u32;
        let mem = b.stats.memory_utilization_pct.unwrap_or(0.0).round() as u32;
        let status = if !online {
            "crit"
        } else if cpu >= th.unifi_cpu_warn || mem >= th.unifi_mem_warn {
            "warn"
        } else {
            "ok"
        };

        let tx_bps = b
            .stats
            .uplink
            .as_ref()
            .and_then(|u| u.tx_rate_bps)
            .unwrap_or(0);
        let rx_bps = b
            .stats
            .uplink
            .as_ref()
            .and_then(|u| u.rx_rate_bps)
            .unwrap_or(0);
        let tx_mbps = tx_bps as f64 * 8.0 / 1_000_000.0;
        let rx_mbps = rx_bps as f64 * 8.0 / 1_000_000.0;

        let ports: Vec<PortOut> = b
            .detail
            .interfaces
            .as_ref()
            .map(|i| {
                i.ports
                    .iter()
                    .map(|p| {
                        let up = p.state.as_deref() == Some("UP");
                        let poe_up = p
                            .poe
                            .as_ref()
                            .map(|x| x.state.as_deref() == Some("UP"))
                            .unwrap_or(false);
                        let poe_cap = p.poe.as_ref().map(|x| x.enabled).unwrap_or(false);
                        if poe_cap {
                            poe_capable += 1;
                        }
                        if poe_up {
                            poe_active += 1;
                        }
                        PortOut {
                            idx: p.idx,
                            up,
                            poe: poe_up,
                            speed_mbps: p.speed_mbps.unwrap_or(0),
                            connector: p.connector.clone().unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let radios: Vec<RadioOut> = b
            .detail
            .interfaces
            .as_ref()
            .map(|i| {
                i.radios
                    .iter()
                    .map(|r| {
                        let band = match r.frequency_ghz {
                            Some(f) if f < 3.0 => "2.4 GHz",
                            Some(f) if f < 5.9 => "5 GHz",
                            Some(_) => "6 GHz",
                            None => "—",
                        };
                        RadioOut {
                            band: band.to_string(),
                            channel: r.channel.unwrap_or(0),
                            width: r.channel_width_mhz.unwrap_or(0),
                            standard: r.wlan_standard.clone().unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let dev = UniDeviceOut {
            id: b.list.id.clone(),
            name: b.list.name.clone().unwrap_or_else(|| "unnamed".to_string()),
            kind: kind.to_string(),
            model,
            ip: b.list.ip_address.clone().unwrap_or_default(),
            mac: b.list.mac_address.clone().unwrap_or_default(),
            status: status.to_string(),
            uptime: fmt_uptime(b.stats.uptime_sec.unwrap_or(0)),
            clients: clients_per_device.get(&b.list.id).copied().unwrap_or(0),
            tx_mbps: (tx_mbps * 100.0).round() / 100.0,
            rx_mbps: (rx_mbps * 100.0).round() / 100.0,
            fw: b.list.firmware_version.clone().unwrap_or_default(),
            site: data.site.clone(),
            cpu,
            mem,
            firmware_updatable: b.list.firmware_updatable,
            ports,
            radios,
        };

        if is_gateway {
            wan_down = (rx_mbps * 100.0).round() / 100.0;
            wan_up = (tx_mbps * 100.0).round() / 100.0;
        }

        // Alerts from device state.
        if !online {
            cands.push(Candidate {
                key: format!("unifi:{}:offline", dev.id),
                sev: "crit".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("{} '{}' is offline", dev.kind, dev.name),
                desc: format!(
                    "UniFi {} '{}' ({}) is not responding to the controller.",
                    dev.kind, dev.name, dev.model
                ),
                rule: "device.state == OFFLINE".to_string(),
            });
            events.push(Event {
                ts: Utc::now().to_rfc3339(),
                time: Local::now().format("%H:%M:%S").to_string(),
                level: "error".to_string(),
                source: format!("unifi/{}", dev.name),
                source_kind: "unifi".to_string(),
                target: "heartbeat".to_string(),
                msg: format!("device unreachable — {} '{}' offline", dev.kind, dev.name),
            });
        } else if cpu >= th.unifi_cpu_warn || mem >= th.unifi_mem_warn {
            cands.push(Candidate {
                key: format!("unifi:{}:load", dev.id),
                sev: "warn".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("{} '{}' under high load", dev.kind, dev.name),
                desc: format!("'{}' is at {cpu}% CPU / {mem}% memory.", dev.name),
                rule: format!(
                    "device.cpu >= {}% or device.mem >= {}%",
                    th.unifi_cpu_warn, th.unifi_mem_warn
                ),
            });
        }
        if b.list.firmware_updatable {
            cands.push(Candidate {
                key: format!("unifi:{}:fw", dev.id),
                sev: "info".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("Firmware update available for {}", dev.name),
                desc: format!(
                    "A newer firmware is available for {} '{}' (current {}).",
                    dev.model, dev.name, dev.fw
                ),
                rule: "device.firmwareUpdatable == true".to_string(),
            });
        }

        // Provisioning event (real timestamp from the controller).
        if let Some(prov) = &b.detail.provisioned_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(prov) {
                let local = dt.with_timezone(&Local);
                events.push(Event {
                    ts: dt.to_rfc3339(),
                    time: local.format("%H:%M:%S").to_string(),
                    level: "info".to_string(),
                    source: format!("unifi/{}", dev.name),
                    source_kind: "unifi".to_string(),
                    target: "provision".to_string(),
                    msg: format!("{} '{}' provisioned · fw {}", dev.kind, dev.name, dev.fw),
                });
            }
        }

        uplink_of.insert(
            dev.id.clone(),
            b.detail.uplink.as_ref().and_then(|u| u.device_id.clone()),
        );
        devices.push(dev);
    }

    devices.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));

    let topology = build_topology(&devices, &uplink_of, wan_down, wan_up);
    let _ = now;

    let view = UnifiView {
        kpis: Vec::new(),
        devices,
        poe_active,
        poe_capable,
        wireless_clients: wireless,
        wired_clients: wired,
    };

    UnifiProcessed {
        view,
        topology,
        events,
        candidates: cands,
        wan_down_mbps: wan_down,
        wan_up_mbps: wan_up,
    }
}

#[cfg(test)]
mod tests {
    use crate::unifi::{DeviceBundle, DeviceDetail, DeviceListItem, DeviceStats, UnifiData};

    use super::*;

    #[test]
    fn offline_unifi_device_generates_critical_alert_and_event() {
        let data = UnifiData {
            site_id: "site1".to_string(),
            site_reference: "default".to_string(),
            site: "default".to_string(),
            app_version: "9.0".to_string(),
            clients: Vec::new(),
            devices: vec![DeviceBundle {
                list: DeviceListItem {
                    id: "dev1".to_string(),
                    mac_address: Some("aa:bb".to_string()),
                    ip_address: Some("10.0.0.2".to_string()),
                    name: Some("Switch".to_string()),
                    model: Some("USW".to_string()),
                    state: Some("OFFLINE".to_string()),
                    firmware_version: Some("1.0".to_string()),
                    firmware_updatable: false,
                    features: Vec::new(),
                },
                detail: DeviceDetail::default(),
                stats: DeviceStats::default(),
            }],
        };

        let processed = process_unifi(&data, 0, &AlertThresholds::default());
        assert_eq!(processed.view.devices[0].status, "crit");
        assert!(processed
            .candidates
            .iter()
            .any(|c| c.key == "unifi:dev1:offline" && c.sev == "crit"));
        assert_eq!(processed.events[0].level, "error");
    }
}
