use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) struct ProgressReporter {
    enabled: bool,
    layout: String,
    total_frames: u64,
    total_samples: u64,
    sample_rate: u32,
    started_at: Instant,
    last_update: Instant,
    last_len: usize,
    updates: u64,
    overhead: Duration,
}

impl ProgressReporter {
    pub(crate) fn new(
        enabled: bool,
        layout: &str,
        total_frames: u64,
        total_samples: u64,
        sample_rate: u32,
    ) -> Self {
        Self {
            enabled,
            layout: layout.to_owned(),
            total_frames,
            total_samples,
            sample_rate,
            started_at: Instant::now(),
            last_update: Instant::now()
                .checked_sub(UPDATE_INTERVAL)
                .unwrap_or_else(Instant::now),
            last_len: 0,
            updates: 0,
            overhead: Duration::ZERO,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn updates(&self) -> u64 {
        self.updates
    }

    pub(crate) fn overhead(&self) -> Duration {
        self.overhead
    }

    pub(crate) fn update(&mut self, frame: usize, samples: u64) {
        if !self.enabled || self.last_update.elapsed() < UPDATE_INTERVAL {
            return;
        }
        let start = Instant::now();
        let elapsed = self.started_at.elapsed();
        let audio_seconds = if self.sample_rate == 0 {
            0.0
        } else {
            samples as f64 / f64::from(self.sample_rate)
        };
        let speed = if elapsed.as_secs_f64() > 0.0 {
            audio_seconds / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let percentage = if self.total_samples > 0 {
            (samples as f64 / self.total_samples as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        let audio = format_duration(audio_seconds);
        let total_audio = format_duration(if self.sample_rate == 0 {
            0.0
        } else {
            self.total_samples as f64 / f64::from(self.sample_rate)
        });
        let elapsed_text = format_duration(elapsed.as_secs_f64());
        let eta = if speed > 0.0 && self.sample_rate > 0 && self.total_samples >= samples {
            format_duration(
                (self.total_samples - samples) as f64 / f64::from(self.sample_rate) / speed,
            )
        } else {
            "--:--".to_owned()
        };
        let line = if self.total_samples > 0 {
            format!(
                "Rendering {} [{:>5.1}%] {} / {} audio  {:.2}x realtime  elapsed {}  ETA {}",
                self.layout, percentage, audio, total_audio, speed, elapsed_text, eta
            )
        } else {
            format!(
                "Rendering {}  frame {} / {}  {} audio  {:.2}x realtime  elapsed {}",
                self.layout, frame, self.total_frames, audio, speed, elapsed_text
            )
        };
        let mut stderr = io::stderr().lock();
        let padding = self.last_len.saturating_sub(line.len());
        let _ = write!(stderr, "\r{}{}", line, " ".repeat(padding));
        let _ = stderr.flush();
        self.last_len = line.len();
        self.last_update = Instant::now();
        self.updates = self.updates.saturating_add(1);
        self.overhead += start.elapsed();
    }

    pub(crate) fn finish(&mut self) {
        if !self.enabled {
            return;
        }
        let start = Instant::now();
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\r{}\r", " ".repeat(self.last_len));
        let _ = stderr.flush();
        self.overhead += start.elapsed();
        self.last_len = 0;
    }
}

fn format_duration(seconds: f64) -> String {
    let seconds = if seconds.is_finite() && seconds > 0.0 {
        Duration::from_secs_f64(seconds).as_secs()
    } else {
        0
    };
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}
