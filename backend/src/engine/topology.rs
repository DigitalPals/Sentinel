use std::collections::{HashMap, HashSet};

use crate::model::{TopoCounts, TopoNode, UniDeviceOut};

use super::format::fmt_mbps;

fn topo_kind(d: &UniDeviceOut) -> &'static str {
    match d.kind.as_str() {
        "Gateway" => "router",
        "Access Point" => "ap",
        _ => {
            if d.model.contains("Aggregation") || d.model.contains("EnterpriseXG") {
                "agg"
            } else {
                "sw"
            }
        }
    }
}

pub(super) fn build_topology(
    devices: &[UniDeviceOut],
    uplink_of: &HashMap<String, Option<String>>,
    wan_down: f64,
    wan_up: f64,
) -> TopoNode {
    let by_id: HashMap<&str, &UniDeviceOut> = devices.iter().map(|d| (d.id.as_str(), d)).collect();

    // children[parent_id] = [child_id, ...]
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for d in devices {
        if let Some(Some(parent)) = uplink_of.get(&d.id) {
            if by_id.contains_key(parent.as_str()) {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(d.id.clone());
            }
        }
    }

    // Primary root: the gateway. Fallback: first device without a valid uplink.
    let gateway = devices.iter().find(|d| d.kind == "Gateway");
    let root_dev = gateway.or_else(|| {
        devices.iter().find(
            |d| !matches!(uplink_of.get(&d.id), Some(Some(p)) if by_id.contains_key(p.as_str())),
        )
    });
    let Some(root_dev) = root_dev else {
        return TopoNode::default();
    };

    // Devices that have no valid parent and are not the root → attach to root.
    let orphans: Vec<String> = devices
        .iter()
        .filter(|d| d.id != root_dev.id)
        .filter(
            |d| !matches!(uplink_of.get(&d.id), Some(Some(p)) if by_id.contains_key(p.as_str())),
        )
        .map(|d| d.id.clone())
        .collect();
    if !orphans.is_empty() {
        children
            .entry(root_dev.id.clone())
            .or_default()
            .extend(orphans);
    }

    let mut visited = HashSet::new();
    build_topo_node(root_dev, &by_id, &children, &mut visited, wan_down, wan_up)
}

fn build_topo_node(
    dev: &UniDeviceOut,
    by_id: &HashMap<&str, &UniDeviceOut>,
    children: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    wan_down: f64,
    wan_up: f64,
) -> TopoNode {
    visited.insert(dev.id.clone());
    let kind = topo_kind(dev);
    let up_ports = dev.ports.iter().filter(|p| p.up).count();
    let total_ports = dev.ports.len();

    let mut kids: Vec<TopoNode> = Vec::new();
    if let Some(ids) = children.get(&dev.id) {
        for id in ids.clone() {
            if visited.contains(&id) {
                continue;
            }
            if let Some(c) = by_id.get(id.as_str()).copied() {
                kids.push(build_topo_node(
                    c, by_id, children, visited, wan_down, wan_up,
                ));
            }
        }
    }
    kids.sort_by(|a, b| {
        topo_rank(&a.kind)
            .cmp(&topo_rank(&b.kind))
            .then(a.name.cmp(&b.name))
    });

    TopoNode {
        kind: kind.to_string(),
        id: dev.id.clone(),
        name: dev.name.clone(),
        model: dev.model.clone(),
        ip: dev.ip.clone(),
        status: dev.status.clone(),
        clients: dev.clients,
        ports: if total_ports > 0 {
            format!("{up_ports}/{total_ports} ports up")
        } else {
            String::new()
        },
        wan: if kind == "router" {
            format!("↓ {} · ↑ {}", fmt_mbps(wan_down), fmt_mbps(wan_up))
        } else {
            String::new()
        },
        children: kids,
    }
}

fn topo_rank(kind: &str) -> u8 {
    match kind {
        "router" => 0,
        "agg" => 1,
        "sw" => 2,
        _ => 3,
    }
}

pub(super) fn count_topology(root: &TopoNode) -> TopoCounts {
    let mut c = TopoCounts::default();
    fn walk(n: &TopoNode, c: &mut TopoCounts) {
        match n.kind.as_str() {
            "router" => c.router += 1,
            "ap" => c.ap += 1,
            _ => c.sw += 1,
        }
        match n.status.as_str() {
            "ok" => c.ok += 1,
            "warn" => c.warn += 1,
            _ => c.crit += 1,
        }
        c.total += 1;
        for k in &n.children {
            walk(k, c);
        }
    }
    if !root.id.is_empty() {
        walk(root, &mut c);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str, kind: &str, model: &str) -> UniDeviceOut {
        UniDeviceOut {
            id: id.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            model: model.to_string(),
            ip: String::new(),
            mac: String::new(),
            status: "ok".to_string(),
            uptime: String::new(),
            clients: 0,
            tx_mbps: 0.0,
            rx_mbps: 0.0,
            fw: String::new(),
            site: String::new(),
            cpu: 0,
            mem: 0,
            firmware_updatable: false,
            ports: Vec::new(),
            radios: Vec::new(),
        }
    }

    #[test]
    fn topology_uses_gateway_as_root_and_attaches_orphans() {
        let devices = vec![
            device("gw", "Gateway", "Gateway", "UDM"),
            device("sw1", "Switch 1", "Switch", "USW"),
            device("ap1", "AP 1", "Access Point", "UAP"),
            device("orphan", "Orphan", "Switch", "USW"),
        ];
        let uplinks = HashMap::from([
            ("sw1".to_string(), Some("gw".to_string())),
            ("ap1".to_string(), Some("sw1".to_string())),
            ("orphan".to_string(), Some("missing".to_string())),
        ]);

        let topo = build_topology(&devices, &uplinks, 100.0, 20.0);
        assert_eq!(topo.id, "gw");
        assert_eq!(topo.children.len(), 2);
        assert!(topo.children.iter().any(|n| n.id == "orphan"));
        assert_eq!(count_topology(&topo).total, 4);
    }
}
