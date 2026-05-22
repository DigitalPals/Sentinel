//! Rolling metric history.
//!
//! Each poll appends one [`Sample`] of cluster-wide scalars. The series powers
//! the KPI sparklines and the 24h dashboard bandwidth chart. It is persisted
//! to `data/history.json` so the chart survives restarts and keeps filling in.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Roughly 24h of samples at a 15s poll interval, capped for memory/disk.
const MAX_SAMPLES: usize = 6000;
const HISTORY_PATH: &str = "data/history.json";

/// One time-stamped row of monitored scalars.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sample {
    /// Unix epoch seconds.
    pub t: i64,
    pub wan_down_mbps: f64,
    pub wan_up_mbps: f64,
    pub availability: f64,
    pub devices_online: f64,
    pub devices_total: f64,
    pub active_alerts: f64,
    pub alerts_crit: f64,
    pub alerts_warn: f64,
    pub vm_count: f64,
    pub lxc_count: f64,
    pub nodes_online: f64,
    pub storage_tb: f64,
    pub wireless_clients: f64,
    pub wired_clients: f64,
    pub poe_ports: f64,
    pub events_total: f64,
    pub error_events: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    pub samples: VecDeque<Sample>,
}

impl History {
    /// Load persisted history from disk, or start fresh.
    pub fn load() -> Self {
        match std::fs::read_to_string(HISTORY_PATH) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn push(&mut self, s: Sample) {
        self.samples.push_back(s);
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Persist the series; failure is logged but never fatal.
    pub fn save(&self) {
        let _ = std::fs::create_dir_all("data");
        if let Ok(json) = serde_json::to_string(self) {
            if let Err(e) = std::fs::write(HISTORY_PATH, json) {
                tracing::warn!("could not persist history: {e}");
            }
        }
    }

    /// Last `n` values of a field, oldest first — used for KPI sparklines.
    pub fn spark(&self, n: usize, field: impl Fn(&Sample) -> f64) -> Vec<f64> {
        let len = self.samples.len();
        let start = len.saturating_sub(n);
        self.samples.iter().skip(start).map(|s| field(&s)).collect()
    }

    /// Signed change of a field over roughly the last `n` samples.
    pub fn trend(&self, n: usize, field: impl Fn(&Sample) -> f64) -> f64 {
        let len = self.samples.len();
        if len < 2 {
            return 0.0;
        }
        let cur = field(self.samples.back().unwrap());
        let idx = len.saturating_sub(n + 1);
        let prev = field(&self.samples[idx]);
        cur - prev
    }

    /// Samples from the last `window_secs` seconds, downsampled to at most
    /// `max_points` buckets — feeds the dashboard bandwidth chart.
    pub fn window(&self, window_secs: i64, max_points: usize) -> Vec<Sample> {
        let now = self.samples.back().map(|s| s.t).unwrap_or(0);
        let cutoff = now - window_secs;
        let recent: Vec<&Sample> = self
            .samples
            .iter()
            .filter(|s| s.t >= cutoff)
            .collect();
        if recent.len() <= max_points {
            return recent.into_iter().cloned().collect();
        }
        let step = recent.len() as f64 / max_points as f64;
        (0..max_points)
            .map(|i| recent[(i as f64 * step) as usize].clone())
            .collect()
    }
}
