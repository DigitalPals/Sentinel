use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Local};

use crate::db::AlertStateRow;
use crate::history::History;
use crate::model::{Alert, AlertsView, Event, Issue};

use super::format::{fmt_ago, kpi};
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
    loaded_from_db: bool,
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

pub(super) struct ReconciledAlerts {
    pub alerts: Vec<Alert>,
    pub newly_active: Vec<Alert>,
    pub events: Vec<Event>,
}

impl AlertStore {
    /// Reconcile freshly-derived candidates with tracked state, returning the
    /// alert list with stable ages, re-fire counts and workflow statuses.
    pub(super) fn reconcile(&mut self, cands: &[Candidate], now: i64) -> ReconciledAlerts {
        for m in self.map.values_mut() {
            m.present_before = m.present;
            m.present = false;
        }
        let mut out = Vec::new();
        let mut newly_active = Vec::new();
        let mut events = Vec::new();
        for c in cands {
            let meta = self.map.entry(c.key.clone()).or_insert(AlertMeta {
                first_seen: now,
                last_seen: now,
                occurrences: 0,
                status: "open".to_string(),
                assignee: None,
                present: false,
                present_before: false,
                loaded_from_db: false,
            });
            // On the first poll after startup, a persisted alert that is still
            // present is the same workflow item, regardless of how long the
            // backend was down.
            let persisted_continuation = meta.loaded_from_db && !meta.present_before;
            let became_present = !meta.present_before && !persisted_continuation;
            let suppressed_flap =
                became_present && meta.occurrences > 0 && now - meta.last_seen < 30;
            if became_present {
                meta.occurrences += 1;
                if meta.status != "open" {
                    meta.status = "open".to_string();
                    meta.assignee = None;
                }
            }
            meta.present = true;
            meta.loaded_from_db = false;
            meta.last_seen = now;
            let alert = Alert {
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
            };
            if became_present && !suppressed_flap {
                newly_active.push(alert.clone());
            }
            if alert.status != "resolved" {
                events.push(alert_event(&alert, meta.first_seen));
            }
            out.push(alert);
        }
        // Drop conditions that cleared more than 30 minutes ago.
        self.map
            .retain(|_, m| m.present || now - m.last_seen < 1800);
        ReconciledAlerts {
            alerts: out,
            newly_active,
            events,
        }
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
                    loaded_from_db: true,
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
    let open = alerts.iter().filter(|a| a.status == "open").count() as u32;
    let crit = alerts
        .iter()
        .filter(|a| a.sev == "crit" && a.status == "open")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status == "open")
        .count() as u32;
    let dashboard_alert_kpi = kpi(
        open.to_string(),
        "",
        format!("{crit} critical · {warn} warning"),
        history.trend(24, |s| s.active_alerts),
        history.spark(24, |s| s.active_alerts),
    );
    let dashboard_issues = build_dashboard_issues(&alerts);
    let alerts_view = build_alerts_view(alerts, &history);
    drop(history);
    let mut next = (*cur).clone();
    next.alerts = alerts_view;
    next.dashboard.issues = dashboard_issues;
    if let Some(k) = next.dashboard.kpis.get_mut(2) {
        *k = dashboard_alert_kpi;
    }
    let next = Arc::new(next);
    *state.snapshot.write().unwrap() = next.clone();
    let _ = state.snapshot_tx.send(next);
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
        .filter(|a| a.sev == "crit" && a.status == "open")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status == "open")
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

pub(super) fn build_dashboard_issues(alerts: &[Alert]) -> Vec<Issue> {
    alerts
        .iter()
        .filter(|a| a.status == "open")
        .take(6)
        .map(|a| Issue {
            sev: a.sev.clone(),
            title: a.title.clone(),
            source: format!("{} · {}", a.host, a.target),
            time: fmt_ago(a.age_min),
        })
        .collect()
}

fn alert_event(alert: &Alert, first_seen: i64) -> Event {
    let dt: DateTime<Local> = DateTime::from_timestamp(first_seen, 0)
        .unwrap_or_default()
        .with_timezone(&Local);
    let state = match alert.status.as_str() {
        "ack" => "acknowledged",
        _ => "active",
    };
    Event {
        ts: dt.to_rfc3339(),
        time: dt.format("%H:%M:%S").to_string(),
        level: match alert.sev.as_str() {
            "crit" => "error".to_string(),
            "warn" => "warn".to_string(),
            _ => "info".to_string(),
        },
        source: alert.host.clone(),
        source_kind: alert.source.clone(),
        target: alert.target.clone(),
        msg: format!("Alert {state}: {} ({})", alert.title, alert.rule),
        dedupe_key: Some(format!("alert:{}:{state}", alert.id)),
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
        assert_eq!(first.alerts[0].occurrences, 1);
        assert_eq!(first.newly_active.len(), 1);
        assert!(store.apply("a", "ack"));

        let second = store.reconcile(&[candidate("a")], 1_120);
        assert_eq!(second.alerts[0].status, "ack");
        assert_eq!(second.alerts[0].age_min, 2);
        assert_eq!(second.alerts[0].occurrences, 1);
        assert!(second.newly_active.is_empty());
    }

    #[test]
    fn resolved_alert_refires_when_condition_returns() {
        let mut store = AlertStore::default();
        store.reconcile(&[candidate("a")], 1_000);
        assert!(store.apply("a", "resolve"));
        store.reconcile(&[], 1_060);

        let fired = store.reconcile(&[candidate("a")], 1_120);
        assert_eq!(fired.alerts[0].status, "open");
        assert_eq!(fired.alerts[0].occurrences, 2);
        assert_eq!(fired.newly_active.len(), 1);
    }

    #[test]
    fn recent_persisted_alert_does_not_refire_on_startup() {
        let mut store = AlertStore::from_rows(vec![AlertStateRow {
            alert_key: "a".to_string(),
            first_seen: 1_000,
            last_seen: 1_120,
            occurrences: 1,
            status: "ack".to_string(),
            assignee: Some("ops".to_string()),
        }]);

        let restored = store.reconcile(&[candidate("a")], 1_140);

        assert!(restored.newly_active.is_empty());
        assert_eq!(restored.alerts[0].status, "ack");
        assert_eq!(restored.alerts[0].occurrences, 1);
    }

    #[test]
    fn old_persisted_ack_stays_ack_when_present_on_startup() {
        let mut store = AlertStore::from_rows(vec![AlertStateRow {
            alert_key: "a".to_string(),
            first_seen: 1_000,
            last_seen: 1_120,
            occurrences: 1,
            status: "ack".to_string(),
            assignee: Some("ops".to_string()),
        }]);

        let restored = store.reconcile(&[candidate("a")], 10_000);

        assert!(restored.newly_active.is_empty());
        assert_eq!(restored.alerts[0].status, "ack");
        assert_eq!(restored.alerts[0].occurrences, 1);
    }

    #[test]
    fn dashboard_issues_include_only_open_alerts() {
        let mut store = AlertStore::default();
        store.reconcile(&[candidate("a"), candidate("b")], 1_000);
        assert!(store.apply("a", "ack"));

        let reconciled = store.reconcile(&[candidate("a"), candidate("b")], 1_060);
        let issues = build_dashboard_issues(&reconciled.alerts);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].source, "host · target");
    }

    #[test]
    fn active_alerts_are_exposed_as_events() {
        let mut store = AlertStore::default();

        let reconciled = store.reconcile(&[candidate("a")], 1_000);

        assert_eq!(reconciled.events.len(), 1);
        assert_eq!(reconciled.events[0].level, "warn");
        assert_eq!(reconciled.events[0].source_kind, "test");
        assert!(reconciled.events[0].msg.contains("Alert active"));
    }

    #[test]
    fn severity_rank_sorts_critical_first() {
        assert!(sev_rank("crit") < sev_rank("warn"));
        assert!(sev_rank("warn") < sev_rank("info"));
    }
}
