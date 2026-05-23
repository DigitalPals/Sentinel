//! Rolling metric history.
//!
//! Each poll appends one [`Sample`] of cluster-wide scalars. The series powers
//! the KPI sparklines and the 24h dashboard bandwidth chart. Every sample is
//! persisted to the `metric_samples` TimescaleDB hypertable; this struct keeps
//! the recent working set in memory so the sparkline/trend/window helpers stay
//! cheap. The working set is seeded from the database on startup.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// One time-stamped row of monitored scalars.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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
    pub unraid_servers_online: f64,
    pub unraid_array_used_pct: f64,
    pub unraid_array_used_tb: f64,
    pub unraid_containers_running: f64,
    pub unraid_vms_running: f64,
    pub events_total: f64,
    pub error_events: f64,
}

/// In-memory working set of recent samples, capped at `max` entries.
#[derive(Debug)]
pub struct History {
    pub samples: VecDeque<Sample>,
    max: usize,
}

impl History {
    /// Build the working set from samples loaded out of the database
    /// (oldest-first), trimmed to the most recent `max`.
    pub fn new(samples: Vec<Sample>, max: usize) -> Self {
        let max = max.max(1);
        let mut samples: VecDeque<Sample> = samples.into();
        while samples.len() > max {
            samples.pop_front();
        }
        Self { samples, max }
    }

    /// Update the working-set cap (the `history_max_samples` setting may change
    /// at runtime), trimming immediately if it shrank.
    pub fn set_max(&mut self, max: usize) {
        self.max = max.max(1);
        while self.samples.len() > self.max {
            self.samples.pop_front();
        }
    }

    /// Append a freshly-built sample to the working set.
    pub fn push(&mut self, s: Sample) {
        self.samples.push_back(s);
        while self.samples.len() > self.max {
            self.samples.pop_front();
        }
    }

    /// Last `n` values of a field, oldest first — used for KPI sparklines.
    pub fn spark(&self, n: usize, field: impl Fn(&Sample) -> f64) -> Vec<f64> {
        let len = self.samples.len();
        let start = len.saturating_sub(n);
        self.samples.iter().skip(start).map(|s| field(s)).collect()
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
        let recent: Vec<&Sample> = self.samples.iter().filter(|s| s.t >= cutoff).collect();
        if recent.len() <= max_points {
            return recent.into_iter().cloned().collect();
        }
        let step = recent.len() as f64 / max_points as f64;
        (0..max_points)
            .map(|i| recent[(i as f64 * step) as usize].clone())
            .collect()
    }
}
