use chrono::{DateTime, Local, Utc};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::AlertThresholds;
use crate::model::{
    Event, UnraidContainerOut, UnraidDiskOut, UnraidNotificationOut, UnraidServerOut,
    UnraidStorageOut, UnraidVmOut,
};
use crate::unraid::{UnraidData, UnraidDisk};

use super::alerts::Candidate;
use super::format::{fmt_mem, pct};

pub(super) struct UnraidProcessed {
    pub server: UnraidServerOut,
    pub events: Vec<Event>,
    pub candidates: Vec<Candidate>,
    pub storage_used_pct: u32,
    pub storage_used_tb: f64,
    pub containers_running: u32,
    pub containers_total: u32,
    pub vms_running: u32,
    pub vms_total: u32,
    pub software_update_count: u32,
}

pub(super) fn process_unraid(
    data: &UnraidData,
    _now: i64,
    th: &AlertThresholds,
) -> UnraidProcessed {
    let array_used_pct = pct(data.array.used_kb, data.array.total_kb);
    let array_total = fmt_kb(data.array.total_kb);
    let array_used = fmt_kb(data.array.used_kb);
    let storage_totals = storage_totals_kb(data);
    let storage_used_pct = pct(storage_totals.used_kb, storage_totals.total_kb);
    let storage_used_tb = storage_totals.used_kb as f64 * 1024.0 / 1_099_511_627_776.0;
    let storage_total = fmt_kb(storage_totals.total_kb);
    let storage_used = fmt_kb(storage_totals.used_kb);
    let containers_running = data
        .containers
        .iter()
        .filter(|c| c.state.eq_ignore_ascii_case("RUNNING"))
        .count() as u32;
    let containers_total = data.containers.len() as u32;
    let vms_running = data
        .vms
        .iter()
        .filter(|v| v.state.eq_ignore_ascii_case("RUNNING"))
        .count() as u32;
    let vms_total = data.vms.len() as u32;
    let software_update_count = data
        .containers
        .iter()
        .filter(|c| c.update_available)
        .count() as u32;
    let storage = build_storage_rows(data);

    let disks: Vec<UnraidDiskOut> = data
        .array
        .disks
        .iter()
        .map(|d| UnraidDiskOut {
            id: d.id.clone(),
            name: d.name.clone(),
            kind: disk_kind_label(&d.kind).to_string(),
            device: d.device.clone(),
            status: disk_status_label(&d.status).to_string(),
            temp: d
                .temp
                .map(|t| format!("{t} C"))
                .unwrap_or_else(|| "—".to_string()),
            size: fmt_kb(d.size_kb),
            used: fmt_kb(d.used_kb),
            used_pct: pct(d.used_kb, d.size_kb),
            spinning: d.is_spinning,
        })
        .collect();

    let containers: Vec<UnraidContainerOut> = data
        .containers
        .iter()
        .map(|c| UnraidContainerOut {
            id: c.id.clone(),
            name: c.name.clone(),
            image: c.image.clone(),
            state: c.state.clone(),
            status: c.status.clone(),
            cpu: c.cpu_pct,
            mem: c.mem_pct,
            memory: c.mem_usage.clone(),
            net_io: c.net_io.clone(),
            block_io: c.block_io.clone(),
            auto_start: c.auto_start,
            update_available: c.update_available,
            update_status: update_status_label(&c.update_status).to_string(),
            root_fs: fmt_optional_bytes(c.size_root_fs),
            writable: fmt_optional_bytes_precise(c.size_rw),
            log_size: fmt_optional_bytes_precise(c.size_log),
            network: if c.network_mode.is_empty() {
                "—".to_string()
            } else {
                c.network_mode.clone()
            },
            ports: if c.lan_ports.is_empty() {
                "—".to_string()
            } else {
                c.lan_ports.join(", ")
            },
            orphaned: c.is_orphaned,
        })
        .collect();

    let vms: Vec<UnraidVmOut> = data
        .vms
        .iter()
        .map(|v| UnraidVmOut {
            id: v.id.clone(),
            name: v.name.clone(),
            state: v.state.clone(),
        })
        .collect();

    let notifications: Vec<UnraidNotificationOut> = data
        .notifications
        .iter()
        .map(|n| UnraidNotificationOut {
            title: n.title.clone(),
            importance: n.importance.clone(),
            time: notification_time(&n.timestamp),
        })
        .collect();

    let mut events = Vec::new();
    let mut cands = Vec::new();
    let host = data.server_name.clone();

    if !data.status.eq_ignore_ascii_case("ONLINE") {
        cands.push(Candidate {
            key: format!("unraid:{}:server-offline", data.source_name),
            sev: "crit".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: data.host.clone(),
            title: format!("Unraid server {} is offline", data.server_name),
            desc: format!(
                "{} reports status {} through the Unraid API.",
                data.server_name, data.status
            ),
            rule: "server.status != ONLINE".to_string(),
        });
    }

    if data.array.state != "STARTED" {
        cands.push(Candidate {
            key: format!("unraid:{}:array-state", data.source_name),
            sev: "crit".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "array".to_string(),
            title: format!("Unraid array is {}", data.array.state),
            desc: format!("Array on {} is not STARTED.", data.server_name),
            rule: "array.state != STARTED".to_string(),
        });
    }
    if array_used_pct >= th.unraid_array_warn {
        cands.push(Candidate {
            key: format!("unraid:{}:array-capacity", data.source_name),
            sev: "warn".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "array".to_string(),
            title: format!("Unraid array {array_used_pct}% full"),
            desc: format!(
                "{} is using {array_used} of {array_total} array capacity.",
                data.server_name
            ),
            rule: format!("array.used >= {}%", th.unraid_array_warn),
        });
    }
    if data.cpu_pct >= th.unraid_cpu_warn {
        cands.push(Candidate {
            key: format!("unraid:{}:cpu", data.source_name),
            sev: "warn".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "cpu".to_string(),
            title: format!("Unraid CPU at {}%", data.cpu_pct),
            desc: format!(
                "{} is at {}% CPU utilization.",
                data.server_name, data.cpu_pct
            ),
            rule: format!("metrics.cpu.percentTotal >= {}%", th.unraid_cpu_warn),
        });
    }
    if data.mem_pct >= th.unraid_mem_warn {
        cands.push(Candidate {
            key: format!("unraid:{}:mem", data.source_name),
            sev: "warn".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "memory".to_string(),
            title: format!("Unraid memory at {}%", data.mem_pct),
            desc: format!(
                "{} is using {} of {} memory.",
                data.server_name,
                fmt_mem(data.mem_used),
                fmt_mem(data.mem_total)
            ),
            rule: format!("metrics.memory.percentTotal >= {}%", th.unraid_mem_warn),
        });
    }
    if let Some(temp) = data.temperature_c {
        if temp >= th.unraid_temp_crit as f64 {
            cands.push(temp_candidate(data, "crit", temp, th.unraid_temp_crit));
        } else if temp >= th.unraid_temp_warn as f64 {
            cands.push(temp_candidate(data, "warn", temp, th.unraid_temp_warn));
        }
    }
    if data.array.parity.errors > 0 {
        cands.push(Candidate {
            key: format!("unraid:{}:parity-errors", data.source_name),
            sev: "crit".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "parity".to_string(),
            title: format!("Parity reported {} error(s)", data.array.parity.errors),
            desc: format!(
                "{} parity check status is {} with {} error(s).",
                data.server_name, data.array.parity.status, data.array.parity.errors
            ),
            rule: "array.parityCheckStatus.errors > 0".to_string(),
        });
    }

    if software_update_count > 0 {
        let mut names = data
            .containers
            .iter()
            .filter(|c| c.update_available)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        let shown = names.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        let suffix = if names.len() > 5 {
            format!(" and {} more", names.len() - 5)
        } else {
            String::new()
        };
        cands.push(Candidate {
            key: format!("unraid:{}:software-updates", data.source_name),
            sev: "info".to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "software updates".to_string(),
            title: format!("{software_update_count} Unraid software update(s) available"),
            desc: format!(
                "{} reports updates for {}{}.",
                data.server_name, shown, suffix
            ),
            rule: "docker.containerUpdateStatuses contains UPDATE_AVAILABLE or REBUILD_READY"
                .to_string(),
        });
    }

    for d in &data.array.disks {
        let disk_pct = pct(d.used_kb, d.size_kb);
        if d.kind != "PARITY" && disk_pct >= th.unraid_disk_warn {
            cands.push(Candidate {
                key: format!("unraid:{}:{}:disk-full", data.source_name, d.id),
                sev: "warn".to_string(),
                source: "unraid".to_string(),
                host: host.clone(),
                target: d.name.clone(),
                title: format!("{} is {disk_pct}% full", d.name),
                desc: format!(
                    "Unraid {} {} has {} used.",
                    d.kind,
                    d.name,
                    fmt_kb(d.used_kb)
                ),
                rule: format!("disk.used >= {}%", th.unraid_disk_warn),
            });
        }
        if !matches!(d.status.as_str(), "" | "DISK_OK") {
            cands.push(Candidate {
                key: format!("unraid:{}:{}:disk-status", data.source_name, d.id),
                sev: "crit".to_string(),
                source: "unraid".to_string(),
                host: host.clone(),
                target: d.name.clone(),
                title: format!("{} status is {}", d.name, d.status),
                desc: format!(
                    "Unraid disk {} ({}) reports {}.",
                    d.name, d.device, d.status
                ),
                rule: "disk.status != DISK_OK".to_string(),
            });
        }
    }

    if data.array.parity.running {
        events.push(Event {
            ts: Utc::now().to_rfc3339(),
            time: Local::now().format("%H:%M:%S").to_string(),
            level: if data.array.parity.paused {
                "warn"
            } else {
                "info"
            }
            .to_string(),
            source: format!("unraid/{}", data.server_name),
            source_kind: "unraid".to_string(),
            target: "parity".to_string(),
            msg: format!(
                "parity check {}% · {}",
                data.array.parity.progress, data.array.parity.speed
            ),
            dedupe_key: Some(format!("unraid:{}:parity", data.source_name)),
        });
    }

    for n in &data.notifications {
        let level = if n.importance == "ALERT" {
            "error"
        } else {
            "warn"
        };
        events.push(Event {
            ts: if n.timestamp.is_empty() {
                Utc::now().to_rfc3339()
            } else {
                n.timestamp.clone()
            },
            time: notification_time(&n.timestamp),
            level: level.to_string(),
            source: format!("unraid/{}", data.server_name),
            source_kind: "unraid".to_string(),
            target: "notification".to_string(),
            msg: n.title.clone(),
            dedupe_key: Some(format!(
                "unraid:{}:notification:{}:{}",
                data.source_name,
                if n.timestamp.is_empty() {
                    "unknown-time"
                } else {
                    n.timestamp.as_str()
                },
                slug_key(&n.title)
            )),
        });
        cands.push(Candidate {
            key: format!(
                "unraid:{}:notification:{}",
                data.source_name,
                slug_key(&n.title)
            ),
            sev: if n.importance == "ALERT" {
                "crit"
            } else {
                "warn"
            }
            .to_string(),
            source: "unraid".to_string(),
            host: host.clone(),
            target: "notification".to_string(),
            title: n.title.clone(),
            desc: format!("Unraid notification severity: {}", n.importance),
            rule: "notifications.warningsAndAlerts not empty".to_string(),
        });
    }

    let server = UnraidServerOut {
        name: data.server_name.clone(),
        source: data.source_name.clone(),
        host: data.host.clone(),
        status: if data.status.eq_ignore_ascii_case("ONLINE") {
            "ok"
        } else {
            "crit"
        }
        .to_string(),
        lan_ip: data.lan_ip.clone(),
        local_url: data.local_url.clone(),
        version: data.version.clone(),
        api_version: data.api_version.clone(),
        kernel: data.kernel.clone(),
        uptime: data.uptime.clone(),
        cpu_brand: data.cpu_brand.clone(),
        cpu_cores: data.cpu_cores,
        cpu_threads: data.cpu_threads,
        cpu: data.cpu_pct,
        mem: data.mem_pct,
        memory: format!("{} / {}", fmt_mem(data.mem_used), fmt_mem(data.mem_total)),
        temp: data
            .temperature_c
            .map(|t| format!("{t:.0} C · {}", data.temperature_status))
            .unwrap_or_else(|| "—".to_string()),
        temp_sensor: data.temperature_name.clone(),
        array_state: data.array.state.clone(),
        array_used,
        array_total,
        array_used_pct,
        storage_used,
        storage_total,
        storage_used_pct,
        disk_count: disks.len() as u32,
        parity_status: data.array.parity.status.clone(),
        parity_progress: data.array.parity.progress,
        parity_errors: data.array.parity.errors,
        containers_running,
        containers_total,
        vms_running,
        vms_total,
        notification_count: notifications.len() as u32,
        software_update_count,
        storage,
        disks,
        containers,
        vms,
        notifications,
    };

    UnraidProcessed {
        server,
        events,
        candidates: cands,
        storage_used_pct,
        storage_used_tb,
        containers_running,
        containers_total,
        vms_running,
        vms_total,
        software_update_count,
    }
}

#[derive(Default)]
struct PoolAgg {
    name: String,
    members: u32,
    used_kb: u64,
    total_kb: u64,
    status: String,
    temp: Option<i32>,
}

#[derive(Default)]
struct StorageTotals {
    used_kb: u64,
    total_kb: u64,
}

fn build_storage_rows(data: &UnraidData) -> Vec<UnraidStorageOut> {
    let mut rows = vec![UnraidStorageOut {
        id: "array".to_string(),
        name: "Array".to_string(),
        kind: "Array".to_string(),
        status: data.array.state.clone(),
        used: fmt_kb(data.array.used_kb),
        total: fmt_kb(data.array.total_kb),
        used_pct: pct(data.array.used_kb, data.array.total_kb),
        members: data.array.disks.iter().filter(|d| d.kind == "DATA").count() as u32,
        temp: hottest_temp(data.array.disks.iter().filter(|d| d.kind == "DATA")),
    }];

    rows.extend(cache_pool_aggs(data).into_iter().map(|(id, p)| {
        UnraidStorageOut {
            id,
            name: p.name,
            kind: "Pool".to_string(),
            status: disk_status_label(&p.status).to_string(),
            used: fmt_kb(p.used_kb),
            total: fmt_kb(p.total_kb),
            used_pct: pct(p.used_kb, p.total_kb),
            members: p.members,
            temp: p
                .temp
                .map(|t| format!("{t} C"))
                .unwrap_or_else(|| "—".to_string()),
        }
    }));

    rows
}

fn storage_totals_kb(data: &UnraidData) -> StorageTotals {
    cache_pool_aggs(data).values().fold(
        StorageTotals {
            used_kb: data.array.used_kb,
            total_kb: data.array.total_kb,
        },
        |totals, pool| StorageTotals {
            used_kb: totals.used_kb.saturating_add(pool.used_kb),
            total_kb: totals.total_kb.saturating_add(pool.total_kb),
        },
    )
}

fn cache_pool_aggs(data: &UnraidData) -> BTreeMap<String, PoolAgg> {
    let cache_names = data
        .array
        .disks
        .iter()
        .filter(|d| d.kind == "CACHE")
        .map(|d| d.name.clone())
        .collect::<BTreeSet<_>>();
    let mut suffix_counts = BTreeMap::<String, u32>::new();
    for name in &cache_names {
        if let Some(base) = stripped_member_suffix(name) {
            *suffix_counts.entry(base).or_default() += 1;
        }
    }

    let mut pools = BTreeMap::<String, PoolAgg>::new();
    for disk in data.array.disks.iter().filter(|d| d.kind == "CACHE") {
        let key = cache_pool_key(&disk.name, &cache_names, &suffix_counts);
        let entry = pools.entry(key.clone()).or_insert_with(|| PoolAgg {
            name: key,
            status: "DISK_OK".to_string(),
            ..PoolAgg::default()
        });
        entry.members += 1;
        entry.total_kb = entry.total_kb.max(disk.size_kb);
        entry.used_kb = entry.used_kb.max(disk.used_kb);
        if !matches!(disk.status.as_str(), "" | "DISK_OK") {
            entry.status = disk.status.clone();
        }
        if let Some(temp) = disk.temp {
            entry.temp = Some(entry.temp.map(|t| t.max(temp)).unwrap_or(temp));
        }
    }

    pools
}

fn hottest_temp<'a>(disks: impl Iterator<Item = &'a UnraidDisk>) -> String {
    disks
        .filter_map(|d| d.temp)
        .max()
        .map(|t| format!("{t} C"))
        .unwrap_or_else(|| "—".to_string())
}

fn cache_pool_key(
    name: &str,
    cache_names: &BTreeSet<String>,
    suffix_counts: &BTreeMap<String, u32>,
) -> String {
    if let Some(base) = stripped_member_suffix(name) {
        if cache_names.contains(&base) || suffix_counts.get(&base).copied().unwrap_or(0) > 1 {
            return base;
        }
    }
    name.to_string()
}

fn stripped_member_suffix(name: &str) -> Option<String> {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if base.len() < name.len() && !base.is_empty() {
        Some(base.to_string())
    } else {
        None
    }
}

fn temp_candidate(data: &UnraidData, sev: &str, temp: f64, threshold: u32) -> Candidate {
    let sensor = if data.temperature_name.is_empty() {
        "hottest reported temperature".to_string()
    } else {
        format!("{} sensor", data.temperature_name)
    };
    Candidate {
        key: format!("unraid:{}:temperature", data.source_name),
        sev: sev.to_string(),
        source: "unraid".to_string(),
        host: data.server_name.clone(),
        target: "temperature".to_string(),
        title: format!("Unraid temperature {temp:.0} C"),
        desc: format!("{} {} is {temp:.0} C.", data.server_name, sensor),
        rule: format!("metrics.temperature.sensors >= {threshold} C"),
    }
}

fn notification_time(ts: &str) -> String {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| Local::now().format("%H:%M:%S").to_string())
}

fn slug_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(80)
        .collect()
}

fn disk_kind_label(kind: &str) -> &str {
    match kind {
        "DATA" => "Data",
        "PARITY" => "Parity",
        "CACHE" => "Cache",
        "BOOT" => "Boot",
        "FLASH" => "Flash",
        _ => kind,
    }
}

fn disk_status_label(status: &str) -> &str {
    match status {
        "DISK_OK" => "OK",
        "DISK_NP" => "Not present",
        "DISK_DSBL" | "DISK_NP_DSBL" | "DISK_DSBL_NEW" => "Disabled",
        "DISK_WRONG" => "Wrong disk",
        "DISK_INVALID" => "Invalid",
        "DISK_NEW" => "New",
        _ if status.is_empty() => "—",
        _ => status,
    }
}

fn update_status_label(status: &str) -> &str {
    match status {
        "UP_TO_DATE" => "Current",
        "UPDATE_AVAILABLE" => "Update available",
        "REBUILD_READY" => "Rebuild ready",
        "UNKNOWN" => "Unknown",
        _ => status,
    }
}

fn fmt_kb(kb: u64) -> String {
    let bytes = kb.saturating_mul(1024);
    fmt_mem(bytes)
}

fn fmt_optional_bytes(bytes: Option<u64>) -> String {
    bytes.map(fmt_mem).unwrap_or_else(|| "—".to_string())
}

fn fmt_optional_bytes_precise(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "—".to_string();
    };
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{} KB", bytes / 1024)
    } else {
        fmt_mem(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unraid::{UnraidArrayData, UnraidData, UnraidDisk};

    fn disk(name: &str, kind: &str, used_kb: u64, size_kb: u64) -> UnraidDisk {
        UnraidDisk {
            id: name.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            used_kb,
            size_kb,
            ..UnraidDisk::default()
        }
    }

    #[test]
    fn storage_totals_include_array_and_cache_pools() {
        let data = UnraidData {
            array: UnraidArrayData {
                used_kb: 400,
                total_kb: 1_000,
                disks: vec![
                    disk("disk1", "DATA", 400, 1_000),
                    disk("cache", "CACHE", 200, 500),
                    disk("cache2", "CACHE", 180, 500),
                    disk("fast", "CACHE", 50, 200),
                ],
                ..UnraidArrayData::default()
            },
            ..UnraidData::default()
        };

        let totals = storage_totals_kb(&data);

        assert_eq!(totals.used_kb, 650);
        assert_eq!(totals.total_kb, 1_700);
    }
}
