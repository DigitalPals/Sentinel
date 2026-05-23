use crate::model::Kpi;

pub(super) fn pct(used: u64, total: u64) -> u32 {
    if total == 0 {
        0
    } else {
        ((used as f64 / total as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u32
    }
}

pub(super) fn fmt_uptime(secs: u64) -> String {
    if secs == 0 {
        return "—".to_string();
    }
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h:02}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

pub(super) fn fmt_mem(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb >= 1.0 {
        if (gb.fract()).abs() < 0.05 {
            format!("{} GB", gb.round() as u64)
        } else {
            format!("{gb:.1} GB")
        }
    } else {
        format!("{} MB", bytes / 1_048_576)
    }
}

pub(super) fn fmt_mbps(mbps: f64) -> String {
    if mbps >= 1000.0 {
        format!("{:.2} Gbps", mbps / 1000.0)
    } else {
        format!("{mbps:.0} Mbps")
    }
}

pub(super) fn fmt_ago(min: i64) -> String {
    if min < 1 {
        "just now".to_string()
    } else if min < 60 {
        format!("{min}m ago")
    } else if min < 1440 {
        format!("{}h {}m ago", min / 60, min % 60)
    } else {
        format!("{}d ago", min / 1440)
    }
}

pub(super) fn friendly_task(kind: &str) -> &'static str {
    match kind {
        "vzdump" => "Backup",
        "aptupdate" => "APT update",
        "vncproxy" | "spiceproxy" | "termproxy" => "Console session",
        "vzstart" => "Container start",
        "vzstop" | "vzshutdown" => "Container stop",
        "vzrestart" => "Container restart",
        "qmstart" => "VM start",
        "qmstop" | "qmshutdown" => "VM stop",
        "qmreboot" => "VM reboot",
        "qmigrate" => "VM migration",
        "qmsnapshot" => "VM snapshot",
        "imgcopy" | "imgdel" => "Disk image task",
        "download" => "Download",
        "startall" => "Bulk start",
        "stopall" => "Bulk stop",
        "srvreload" => "Service reload",
        _ => "Task",
    }
}

pub(super) fn kpi(
    display: impl Into<String>,
    unit: impl Into<String>,
    sub: impl Into<String>,
    trend: f64,
    spark: Vec<f64>,
) -> Kpi {
    Kpi {
        display: display.into(),
        unit: unit.into(),
        sub: sub.into(),
        trend: (trend * 100.0).round() / 100.0,
        spark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_clamps_and_handles_zero_total() {
        assert_eq!(pct(0, 0), 0);
        assert_eq!(pct(50, 100), 50);
        assert_eq!(pct(150, 100), 100);
    }

    #[test]
    fn bandwidth_formatter_switches_to_gbps() {
        assert_eq!(fmt_mbps(999.0), "999 Mbps");
        assert_eq!(fmt_mbps(1250.0), "1.25 Gbps");
    }
}
