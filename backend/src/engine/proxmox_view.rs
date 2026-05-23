use std::collections::HashMap;

use chrono::{DateTime, Local};

use crate::config::AlertThresholds;
use crate::model::{Event, Guest, GuestCount, NodeGuests, NodeTile};
use crate::proxmox::ProxmoxData;

use super::alerts::Candidate;
use super::format::{fmt_mem, fmt_uptime, friendly_task, pct};

pub(super) struct ProxmoxProcessed {
    pub nodes: Vec<NodeTile>,
    pub guest_groups: Vec<NodeGuests>,
    pub events: Vec<Event>,
    pub candidates: Vec<Candidate>,
    pub storage_used: u64,
    pub storage_total: u64,
}

pub(super) fn process_proxmox(
    data: &ProxmoxData,
    now: i64,
    th: &AlertThresholds,
) -> ProxmoxProcessed {
    let mut tiles: Vec<NodeTile> = Vec::new();
    let mut guests_by_node: HashMap<String, Vec<Guest>> = HashMap::new();
    let mut events: Vec<Event> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut storage_used = 0u64;
    let mut storage_total = 0u64;

    // Guests first, so node tiles can count them.
    for r in &data.resources {
        if r.kind != "qemu" && r.kind != "lxc" {
            continue;
        }
        let Some(node) = r.node.clone() else { continue };
        let running = r.status.as_deref() == Some("running");
        let cpu = (r.cpu * 100.0).round() as u32;
        let mem = pct(r.mem, r.maxmem);
        let disk = pct(r.disk, r.maxdisk);
        let status = if !running {
            "stop".to_string()
        } else if mem >= th.guest_mem_crit {
            "crit".to_string()
        } else if mem >= th.guest_mem_warn || cpu >= th.guest_cpu_warn {
            "warn".to_string()
        } else {
            "ok".to_string()
        };
        let tags = r
            .tags
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| t.replace(';', ", "))
            .unwrap_or_default();
        let kind = if r.kind == "qemu" { "vm" } else { "lxc" };
        let guest = Guest {
            id: r.vmid.unwrap_or(0),
            kind: kind.to_string(),
            name: r
                .name
                .clone()
                .unwrap_or_else(|| format!("{kind}-{}", r.vmid.unwrap_or(0))),
            status,
            cpu,
            mem,
            disk,
            net: 0,
            uptime: fmt_uptime(r.uptime),
            tags,
            cores: r.maxcpu.round() as u32,
            ram: fmt_mem(r.maxmem),
            node: node.clone(),
            server: data.server.clone(),
        };

        // Derive alerts from guest health.
        if running {
            let label = format!(
                "{} {} · {}",
                if kind == "vm" { "VM" } else { "CT" },
                guest.id,
                guest.name
            );
            if mem >= th.guest_mem_crit {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:mem", data.server, node, guest.id),
                    sev: "crit".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Memory pressure {mem}% on {}", guest.name),
                    desc: format!(
                        "{label} is at {mem}% memory utilization on node {node}. Sustained pressure can trigger swapping and OOM kills."
                    ),
                    rule: format!("guest.mem.utilization >= {}%", th.guest_mem_crit),
                });
            } else if mem >= th.guest_mem_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:mem", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Elevated memory {mem}% on {}", guest.name),
                    desc: format!(
                        "{label} is using {mem}% of its allocated memory on node {node}."
                    ),
                    rule: format!("guest.mem.utilization >= {}%", th.guest_mem_warn),
                });
            }
            if cpu >= th.guest_cpu_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:cpu", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Sustained CPU {cpu}% on {}", guest.name),
                    desc: format!("{label} is consuming {cpu}% CPU on node {node}."),
                    rule: format!("guest.cpu.utilization >= {}%", th.guest_cpu_warn),
                });
            }
            if disk >= th.guest_disk_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:disk", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label,
                    title: format!("Disk {disk}% full on {}", guest.name),
                    desc: format!("{} root volume is {disk}% full on node {node}.", guest.name),
                    rule: format!("guest.disk.utilization >= {}%", th.guest_disk_warn),
                });
            }
        }
        guests_by_node.entry(node).or_default().push(guest);
    }

    // Node tiles.
    for r in &data.resources {
        if r.kind != "node" {
            continue;
        }
        let Some(node) = r.node.clone() else { continue };
        let online = r.status.as_deref() == Some("online");
        let cpu = (r.cpu * 100.0).round() as u32;
        let mem = pct(r.mem, r.maxmem);
        let disk = pct(r.disk, r.maxdisk);
        let rrd = data.node_rrd.get(&node);
        let net_mbps = rrd
            .map(|p| (p.netin.unwrap_or(0.0) + p.netout.unwrap_or(0.0)) * 8.0 / 1_000_000.0)
            .unwrap_or(0.0);
        let net = (net_mbps / 10.0).round().clamp(0.0, 100.0) as u32;
        let guests = guests_by_node.get(&node).cloned().unwrap_or_default();
        let gc = GuestCount {
            vm: guests.iter().filter(|g| g.kind == "vm").count() as u32,
            lxc: guests.iter().filter(|g| g.kind == "lxc").count() as u32,
        };
        let status = if !online {
            "crit"
        } else if mem >= th.node_mem_crit || cpu >= th.node_cpu_crit {
            "crit"
        } else if mem >= th.node_mem_warn || cpu >= th.node_cpu_warn || disk >= th.node_disk_warn {
            "warn"
        } else {
            "ok"
        };

        if online {
            if mem >= th.node_mem_crit {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:node-mem", data.server, node),
                    sev: "crit".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: format!("node {node}"),
                    title: format!("Node {node} memory at {mem}%"),
                    desc: format!(
                        "Proxmox node {node} on {} is at {mem}% memory utilization.",
                        data.server
                    ),
                    rule: format!("node.mem.utilization >= {}%", th.node_mem_crit),
                });
            }
            if disk >= th.node_disk_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:node-disk", data.server, node),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: format!("node {node}"),
                    title: format!("Node {node} root filesystem {disk}% full"),
                    desc: format!("Proxmox node {node} root filesystem is {disk}% full."),
                    rule: format!("node.rootfs.utilization >= {}%", th.node_disk_warn),
                });
            }
        } else {
            cands.push(Candidate {
                key: format!("pmx:{}:{}:node-offline", data.server, node),
                sev: "crit".to_string(),
                source: "proxmox".to_string(),
                host: format!("{} / {}", data.server, node),
                target: format!("node {node}"),
                title: format!("Proxmox node {node} is offline"),
                desc: format!(
                    "Node {node} on {} is not responding to the cluster.",
                    data.server
                ),
                rule: "node.status == offline".to_string(),
            });
        }

        tiles.push(NodeTile {
            name: node.clone(),
            server: data.server.clone(),
            host: data.server.clone(),
            status: status.to_string(),
            cpu,
            mem,
            disk,
            net,
            net_mbps: (net_mbps * 10.0).round() / 10.0,
            guests: gc,
            model: format!("PVE {} · {} cores", data.release, r.maxcpu.round() as u32),
            uptime: fmt_uptime(r.uptime),
        });
    }

    // Guest storage (volumes that back VM/CT disks).
    for r in &data.resources {
        if r.kind != "storage" {
            continue;
        }
        let content = r.content.as_deref().unwrap_or("");
        if content.contains("rootdir") || content.contains("images") {
            storage_used += r.disk;
            storage_total += r.maxdisk;
        }
    }

    // Task log → events.
    for t in &data.tasks {
        let ts_epoch = t.endtime.unwrap_or(t.starttime);
        let dt: DateTime<Local> = DateTime::from_timestamp(ts_epoch, 0)
            .unwrap_or_default()
            .with_timezone(&Local);
        let status = t.status.clone().unwrap_or_else(|| "running".to_string());
        let level = if t.status.is_none() {
            "warn"
        } else if status == "OK" {
            "info"
        } else {
            "error"
        };
        let friendly = friendly_task(&t.kind);
        let id_part =
            t.id.as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
        let msg = if status == "OK" {
            format!("{friendly}{id_part} completed successfully")
        } else {
            format!("{friendly}{id_part} ended: {status}")
        };
        events.push(Event {
            ts: dt.to_rfc3339(),
            time: dt.format("%H:%M:%S").to_string(),
            level: level.to_string(),
            source: format!("proxmox/{}", t.node),
            source_kind: "proxmox".to_string(),
            target: t
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| t.kind.clone()),
            msg,
        });
    }
    let _ = now;

    let guest_groups: Vec<NodeGuests> = tiles
        .iter()
        .map(|tile| {
            let mut guests = guests_by_node.get(&tile.name).cloned().unwrap_or_default();
            guests.sort_by_key(|g| g.id);
            NodeGuests {
                node: tile.name.clone(),
                server: tile.server.clone(),
                guests,
            }
        })
        .collect();

    ProxmoxProcessed {
        nodes: tiles,
        guest_groups,
        events,
        candidates: cands,
        storage_used,
        storage_total,
    }
}

#[cfg(test)]
mod tests {
    use crate::proxmox::{ProxmoxData, PveResource};

    use super::*;

    fn resource(kind: &str, node: &str) -> PveResource {
        PveResource {
            kind: kind.to_string(),
            node: Some(node.to_string()),
            status: None,
            name: None,
            storage: None,
            content: None,
            tags: None,
            vmid: None,
            cpu: 0.0,
            maxcpu: 0.0,
            mem: 0,
            maxmem: 0,
            disk: 0,
            maxdisk: 0,
            uptime: 0,
        }
    }

    #[test]
    fn high_guest_memory_generates_critical_candidate() {
        let mut node = resource("node", "pve1");
        node.status = Some("online".to_string());
        node.maxcpu = 8.0;
        node.maxmem = 100;

        let mut guest = resource("qemu", "pve1");
        guest.status = Some("running".to_string());
        guest.name = Some("db".to_string());
        guest.vmid = Some(100);
        guest.maxcpu = 2.0;
        guest.mem = 93;
        guest.maxmem = 100;

        let data = ProxmoxData {
            server: "cluster".to_string(),
            release: "8.2".to_string(),
            resources: vec![node, guest],
            node_rrd: HashMap::new(),
            tasks: Vec::new(),
        };

        let processed = process_proxmox(&data, 0, &AlertThresholds::default());
        assert_eq!(processed.guest_groups[0].guests[0].status, "crit");
        assert!(processed
            .candidates
            .iter()
            .any(|c| c.key == "pmx:cluster:pve1:100:mem" && c.sev == "crit"));
    }
}
