use crate::history::History;
use crate::model::BandwidthSeries;

pub(super) fn build_bandwidth(history: &History) -> BandwidthSeries {
    let samples = history.window(86_400, 120);
    let down: Vec<f64> = samples.iter().map(|s| s.wan_down_mbps).collect();
    let up: Vec<f64> = samples.iter().map(|s| s.wan_up_mbps).collect();
    let points = samples.len();

    let peak_down = down.iter().cloned().fold(0.0, f64::max);
    let peak_up = up.iter().cloned().fold(0.0, f64::max);
    let totals: Vec<f64> = samples
        .iter()
        .map(|s| s.wan_down_mbps + s.wan_up_mbps)
        .collect();
    let avg = if totals.is_empty() {
        0.0
    } else {
        totals.iter().sum::<f64>() / totals.len() as f64
    };

    // Approximate transferred volume by integrating throughput over time.
    let mut transferred_gb = 0.0;
    for w in samples.windows(2) {
        let dt = (w[1].t - w[0].t).max(0) as f64;
        let avg_mbps =
            (w[0].wan_down_mbps + w[0].wan_up_mbps + w[1].wan_down_mbps + w[1].wan_up_mbps) / 2.0;
        transferred_gb += avg_mbps * dt / 8.0 / 1000.0;
    }

    let window_label = if points < 2 {
        "collecting WAN history".to_string()
    } else {
        let span = samples.last().unwrap().t - samples.first().unwrap().t;
        if span >= 23 * 3600 {
            "last 24h".to_string()
        } else if span >= 3600 {
            format!("last {}h {}m", span / 3600, (span % 3600) / 60)
        } else {
            format!("last {}m", (span / 60).max(1))
        }
    };

    BandwidthSeries {
        down,
        up,
        points,
        window_label,
        peak_down: (peak_down * 10.0).round() / 10.0,
        peak_up: (peak_up * 10.0).round() / 10.0,
        avg: (avg * 10.0).round() / 10.0,
        transferred_gb: (transferred_gb * 100.0).round() / 100.0,
    }
}

#[cfg(test)]
mod tests {
    use crate::history::Sample;

    use super::*;

    #[test]
    fn bandwidth_integrates_transferred_volume() {
        let history = History::new(
            vec![
                Sample {
                    t: 0,
                    wan_down_mbps: 80.0,
                    wan_up_mbps: 0.0,
                    ..Sample::default()
                },
                Sample {
                    t: 100,
                    wan_down_mbps: 80.0,
                    wan_up_mbps: 0.0,
                    ..Sample::default()
                },
            ],
            10,
        );

        let series = build_bandwidth(&history);
        assert_eq!(series.points, 2);
        assert_eq!(series.transferred_gb, 1.0);
    }
}
