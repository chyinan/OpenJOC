// pattern: Functional Core

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimingSample {
    pub decode: f64,
    pub render: f64,
    pub total: f64,
    pub audio: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerformanceSummary {
    pub sample_count: usize,
    pub decode_mean_ms: f64,
    pub decode_p95_ms: f64,
    pub decode_max_ms: f64,
    pub render_mean_ms: f64,
    pub render_p95_ms: f64,
    pub render_max_ms: f64,
    pub total_mean_ms: f64,
    pub total_p95_ms: f64,
    pub total_max_ms: f64,
    pub realtime_factor: Option<f64>,
}

pub fn summarize(samples: &[TimingSample]) -> PerformanceSummary {
    if samples.is_empty() {
        return PerformanceSummary::default();
    }
    let decode = samples
        .iter()
        .map(|sample| sample.decode)
        .collect::<Vec<_>>();
    let render = samples
        .iter()
        .map(|sample| sample.render)
        .collect::<Vec<_>>();
    let total = samples
        .iter()
        .map(|sample| sample.total)
        .collect::<Vec<_>>();
    let total_ms = samples.iter().map(|sample| sample.total).sum::<f64>();
    let audio_ms = samples.iter().map(|sample| sample.audio).sum::<f64>();
    PerformanceSummary {
        sample_count: samples.len(),
        decode_mean_ms: mean(&decode),
        decode_p95_ms: percentile_95(&decode),
        decode_max_ms: maximum(&decode),
        render_mean_ms: mean(&render),
        render_p95_ms: percentile_95(&render),
        render_max_ms: maximum(&render),
        total_mean_ms: mean(&total),
        total_p95_ms: percentile_95(&total),
        total_max_ms: maximum(&total),
        realtime_factor: (total_ms > 0.0).then_some(audio_ms / total_ms),
    }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn maximum(values: &[f64]) -> f64 {
    values.iter().copied().fold(0.0, f64::max)
}

fn percentile_95(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let index = values
        .len()
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1);
    values[index]
}

#[cfg(test)]
mod tests {
    use super::{TimingSample, summarize};

    #[test]
    fn summary_reports_mean_p95_max_and_realtime_factor() {
        let summary = summarize(&[
            TimingSample {
                decode: 1.0,
                render: 2.0,
                total: 4.0,
                audio: 100.0,
            },
            TimingSample {
                decode: 3.0,
                render: 4.0,
                total: 8.0,
                audio: 100.0,
            },
            TimingSample {
                decode: 2.0,
                render: 3.0,
                total: 6.0,
                audio: 100.0,
            },
            TimingSample {
                decode: 4.0,
                render: 5.0,
                total: 10.0,
                audio: 100.0,
            },
        ]);

        assert_eq!(summary.sample_count, 4);
        assert_eq!(summary.decode_mean_ms, 2.5);
        assert_eq!(summary.decode_p95_ms, 4.0);
        assert_eq!(summary.decode_max_ms, 4.0);
        assert_eq!(summary.render_mean_ms, 3.5);
        assert_eq!(summary.render_p95_ms, 5.0);
        assert_eq!(summary.total_mean_ms, 7.0);
        assert_eq!(summary.total_p95_ms, 10.0);
        assert_eq!(summary.realtime_factor, Some(400.0 / 28.0));
    }
}
