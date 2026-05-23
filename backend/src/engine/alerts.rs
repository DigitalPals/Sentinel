use std::collections::HashMap;
use std::sync::Arc;

use crate::db::AlertStateRow;
use crate::history::History;
use crate::model::{Alert, AlertsView};

use super::format::kpi;
use super::AppState;

/// Per-alert bookkeeping: first-seen time, re-fire count and workflow status.
struct AlertMeta {
    first_seen: i64,
    last_seen: i64,
    occurrences: u32,
    status: String,
    assignee: Option<String>,
    present: bool,
    present_before: bool,
}

#[derive(Default)]
pub struct AlertStore {
    map: HashMap<String, AlertMeta>,
}

/// A condition that currently holds and should surface as an alert.
pub(super) struct Candidate {
    pub key: String,
    pub sev: String,
    pub source: String,
    pub host: String,
    pub target: String,
    pub title: String,
    pub desc: String,
    pub rule: String,
}

impl AlertStore {
    /// Reconcile freshly-derived candidates with tracked state, returning the
    /// alert list with stable ages, re-fire counts and workflow statuses.
    pub(super) fn reconcile(&mut self, cands: &[Candidate], now: i64) -> Vec<Alert> {
        for m in self.map.values_mut() {
            m.present_before = m.present;
            m.present = false;
        }
        let mut out = Vec::new();
        for c in cands {
            let meta = self.map.entry(c.key.clone()).or_insert(AlertMeta {
                first_seen: now,
                last_seen: now,
                occurrences: 0,
                status: "open".to_string(),
                assignee: None,
                present: false,
                present_before: false,
            });
            if !meta.present_before {
                meta.occurrences += 1;
                if meta.status == "resolved" {
                    meta.status = "open".to_string();
                    meta.assignee = None;
                }
            }
            meta.present = true;
            meta.last_seen = now;
            out.push(Alert {
                id: c.key.clone(),
                sev: c.sev.clone(),
                status: meta.status.clone(),
                title: c.title.clone(),
                desc: c.desc.clone(),
                source: c.source.clone(),
                host: c.host.clone(),
                target: c.target.clone(),
                age_min: (now - meta.first_seen) / 60,
                occurrences: meta.occurrences,
                assignee: meta.assignee.clone(),
                rule: c.rule.clone(),
            });
        }
        // Drop conditions that cleared more than 30 minutes ago.
        self.map
            .retain(|_, m| m.present || now - m.last_seen < 1800);
        out
    }

    /// Current workflow status and assignee for an alert, if tracked.
    pub fn status_of(&self, id: &str) -> Option<(String, Option<String>)> {
        self.map
            .get(id)
            .map(|m| (m.status.clone(), m.assignee.clone()))
    }

    /// Apply a workflow action to one alert; returns `true` if it existed.
    pub fn apply(&mut self, id: &str, action: &str) -> bool {
        let Some(meta) = self.map.get_mut(id) else {
            return false;
        };
        match action {
            "ack" => {
                meta.status = "ack".to_string();
                meta.assignee = Some("J. Pals".to_string());
            }
            "resolve" => meta.status = "resolved".to_string(),
            "reopen" => {
                meta.status = "open".to_string();
                meta.assignee = None;
            }
            _ => return false,
        }
        true
    }

    /// Rebuild the store from persisted rows — restores ack/resolve state and
    /// alert ages across a backend restart.
    pub fn from_rows(rows: Vec<AlertStateRow>) -> Self {
        let mut map = HashMap::new();
        for r in rows {
            map.insert(
                r.alert_key,
                AlertMeta {
                    first_seen: r.first_seen,
                    last_seen: r.last_seen,
                    occurrences: r.occurrences.max(0) as u32,
                    status: r.status,
                    assignee: r.assignee,
                    present: false,
                    present_before: false,
                },
            );
        }
        Self { map }
    }

    /// The tracked state as rows ready to persist to `alert_state`.
    pub fn rows(&self) -> Vec<AlertStateRow> {
        self.map
            .iter()
            .map(|(k, m)| AlertStateRow {
                alert_key: k.clone(),
                first_seen: m.first_seen,
                last_seen: m.last_seen,
                occurrences: m.occurrences as i32,
                status: m.status.clone(),
                assignee: m.assignee.clone(),
            })
            .collect()
    }
}

/// Re-apply alert workflow statuses to the live snapshot after an action,
/// so the UI reflects an acknowledge/resolve without waiting for the next poll.
pub fn patch_alerts(state: &AppState) {
    let cur = state.current();
    let mut alerts = cur.alerts.alerts.clone();
    {
        let store = state.alerts.read().unwrap();
        for a in &mut alerts {
            if let Some((status, assignee)) = store.status_of(&a.id) {
                a.status = status;
                a.assignee = assignee;
            }
        }
    }
    let history = state.history.read().unwrap();
    let alerts_view = build_alerts_view(alerts, &history);
    drop(history);
    let mut next = (*cur).clone();
    next.alerts = alerts_view;
    *state.snapshot.write().unwrap() = Arc::new(next);
}

pub(super) fn sev_rank(sev: &str) -> u8 {
    match sev {
        "crit" => 0,
        "warn" => 1,
        _ => 2,
    }
}

pub(super) fn build_alerts_view(alerts: Vec<Alert>, history: &History) -> AlertsView {
    let open = alerts.iter().filter(|a| a.status == "open").count() as u32;
    let ack = alerts.iter().filter(|a| a.status == "ack").count() as u32;
    let resolved = alerts.iter().filter(|a| a.status == "resolved").count() as u32;
    let crit = alerts
        .iter()
        .filter(|a| a.sev == "crit" && a.status != "resolved")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status != "resolved")
        .count() as u32;

    // 24h histogram: bucket each alert by the hour it was first raised.
    let mut hist = vec![0u32; 24];
    for a in &alerts {
        let hours_ago = (a.age_min / 60).clamp(0, 23) as usize;
        hist[23 - hours_ago] += 1;
    }

    let kpis = vec![
        kpi(
            open.to_string(),
            "",
            format!("{crit} critical · {warn} warning"),
            history.trend(24, |s| s.active_alerts),
            history.spark(24, |s| s.active_alerts),
        ),
        kpi(
            ack.to_string(),
            "",
            "awaiting resolution".to_string(),
            0.0,
            history.spark(24, |s| s.alerts_warn),
        ),
        kpi(
            resolved.to_string(),
            "",
            "this session".to_string(),
            0.0,
            history.spark(24, |s| (s.active_alerts - s.alerts_crit).max(0.0)),
        ),
        kpi(
            (alerts.len() as u32).to_string(),
            "",
            "tracked conditions".to_string(),
            history.trend(24, |s| s.active_alerts),
            history.spark(24, |s| s.alerts_crit),
        ),
    ];

    AlertsView {
        kpis,
        alerts,
        histogram: hist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(key: &str) -> Candidate {
        Candidate {
            key: key.to_string(),
            sev: "warn".to_string(),
            source: "test".to_string(),
            host: "host".to_string(),
            target: "target".to_string(),
            title: "title".to_string(),
            desc: "desc".to_string(),
            rule: "rule".to_string(),
        }
    }

    #[test]
    fn reconcile_preserves_ack_and_age_for_persistent_alerts() {
        let mut store = AlertStore::default();
        let first = store.reconcile(&[candidate("a")], 1_000);
        assert_eq!(first[0].occurrences, 1);
        assert!(store.apply("a", "ack"));

        let second = store.reconcile(&[candidate("a")], 1_120);
        assert_eq!(second[0].status, "ack");
        assert_eq!(second[0].age_min, 2);
        assert_eq!(second[0].occurrences, 1);
    }

    #[test]
    fn resolved_alert_refires_when_condition_returns() {
        let mut store = AlertStore::default();
        store.reconcile(&[candidate("a")], 1_000);
        assert!(store.apply("a", "resolve"));
        store.reconcile(&[], 1_060);

        let fired = store.reconcile(&[candidate("a")], 1_120);
        assert_eq!(fired[0].status, "open");
        assert_eq!(fired[0].occurrences, 2);
    }

    #[test]
    fn severity_rank_sorts_critical_first() {
        assert!(sev_rank("crit") < sev_rank("warn"));
        assert!(sev_rank("warn") < sev_rank("info"));
    }
}
