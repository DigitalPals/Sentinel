use std::collections::HashMap;

use chrono::DateTime;

use crate::history::History;
use crate::model::{Event, EventsView};

use super::format::{kpi, pct};

pub(super) fn build_events_view(events: Vec<Event>, history: &History) -> EventsView {
    let errors = events.iter().filter(|e| e.level == "error").count() as u32;
    let warns = events.iter().filter(|e| e.level == "warn").count() as u32;

    // Most active source.
    let mut by_source: HashMap<&str, u32> = HashMap::new();
    for e in &events {
        *by_source.entry(e.source_kind.as_str()).or_insert(0) += 1;
    }
    let top_source = by_source
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(k, _)| *k)
        .unwrap_or("—");
    let top_label = match top_source {
        "proxmox" => "Proxmox",
        "unifi" => "UniFi",
        other => other,
    };

    // Event-rate chart: 60 buckets across the observed event time span.
    let mut times: Vec<i64> = events
        .iter()
        .filter_map(|e| DateTime::parse_from_rfc3339(&e.ts).ok())
        .map(|d| d.timestamp())
        .collect();
    times.sort_unstable();
    let mut rate = vec![0u32; 60];
    if let (Some(&min), Some(&max)) = (times.first(), times.last()) {
        let span = (max - min).max(1);
        for t in &times {
            let idx = (((t - min) as f64 / span as f64) * 59.0).round() as usize;
            rate[idx.min(59)] += 1;
        }
    }

    let kpis = vec![
        kpi(
            (events.len() as u32).to_string(),
            "",
            format!("{} sources", by_source.len()),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
        kpi(
            errors.to_string(),
            "",
            format!("{warns} warnings"),
            history.trend(24, |s| s.error_events),
            history.spark(24, |s| s.error_events),
        ),
        kpi(
            events
                .iter()
                .filter(|e| e.target == "vzdump" || e.msg.starts_with("Backup"))
                .count()
                .to_string(),
            "",
            "backup tasks".to_string(),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
        kpi(
            top_label.to_string(),
            "",
            format!(
                "{}% of events",
                pct(
                    by_source.get(top_source).copied().unwrap_or(0) as u64,
                    events.len().max(1) as u64
                )
            ),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
    ];

    EventsView { kpis, events, rate }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_rate_buckets_cover_observed_span() {
        let events = vec![
            Event {
                ts: "2024-01-01T00:00:00Z".to_string(),
                time: "00:00:00".to_string(),
                level: "info".to_string(),
                source: "a".to_string(),
                source_kind: "proxmox".to_string(),
                target: "vzdump".to_string(),
                msg: "Backup completed successfully".to_string(),
            },
            Event {
                ts: "2024-01-01T01:00:00Z".to_string(),
                time: "01:00:00".to_string(),
                level: "error".to_string(),
                source: "b".to_string(),
                source_kind: "unifi".to_string(),
                target: "heartbeat".to_string(),
                msg: "device offline".to_string(),
            },
        ];
        let history = History::new(Vec::new(), 10);

        let view = build_events_view(events, &history);
        assert_eq!(view.rate.iter().sum::<u32>(), 2);
        assert_eq!(view.kpis[1].display, "1");
    }
}
