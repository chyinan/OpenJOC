use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_TERMINAL_WIDTH: usize = 80;

pub(crate) struct ProgressReporter {
    enabled: bool,
    action: String,
    layout: String,
    total_frames: u64,
    total_samples: u64,
    sample_rate: u32,
    max_width: usize,
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
        terminal_width: Option<u16>,
    ) -> Self {
        Self::new_named(
            enabled,
            "Rendering",
            layout,
            total_frames,
            total_samples,
            sample_rate,
            terminal_width,
        )
    }

    pub(crate) fn new_named(
        enabled: bool,
        action: &str,
        layout: &str,
        total_frames: u64,
        total_samples: u64,
        sample_rate: u32,
        terminal_width: Option<u16>,
    ) -> Self {
        Self {
            enabled,
            action: action.to_owned(),
            layout: layout.to_owned(),
            total_frames,
            total_samples,
            sample_rate,
            max_width: max_progress_width(terminal_width),
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
            let detailed = format!(
                "{} {} [{:>5.1}%] {} / {} audio  {:.2}x realtime  elapsed {}  ETA {}",
                self.action, self.layout, percentage, audio, total_audio, speed, elapsed_text, eta
            );
            select_progress_line(
                detailed,
                self.max_width,
                || {
                    format!(
                        "{} {} [{:>5.1}%] {} / {}  {:.2}x ETA {}",
                        self.action, self.layout, percentage, audio, total_audio, speed, eta
                    )
                },
                || {
                    format!(
                        "{} {} [{:>5.1}%] {:.2}x",
                        self.action, self.layout, percentage, speed
                    )
                },
            )
        } else {
            let detailed = format!(
                "{} {}  frame {} / {}  {} audio  {:.2}x realtime  elapsed {}",
                self.action, self.layout, frame, self.total_frames, audio, speed, elapsed_text
            );
            select_progress_line(
                detailed,
                self.max_width,
                || {
                    format!(
                        "{} {} frame {}/{} {:.2}x",
                        self.action, self.layout, frame, self.total_frames, speed
                    )
                },
                || format!("{} {} {:.2}x", self.action, self.layout, speed),
            )
        };
        let mut stderr = io::stderr().lock();
        let _ = write_progress_update(&mut stderr, &line, self.last_len);
        let _ = stderr.flush();
        self.last_len = display_width(&line);
        self.last_update = Instant::now();
        self.updates = self.updates.saturating_add(1);
        self.overhead += start.elapsed();
    }

    pub(crate) fn finish(&mut self) {
        if !self.enabled {
            return;
        }
        if self.last_len == 0 {
            return;
        }
        let start = Instant::now();
        let mut stderr = io::stderr().lock();
        let _ = write_progress_finish(&mut stderr, self.last_len);
        let _ = stderr.flush();
        self.overhead += start.elapsed();
        self.last_len = 0;
    }
}

fn max_progress_width(terminal_width: Option<u16>) -> usize {
    terminal_width.map_or(DEFAULT_TERMINAL_WIDTH - 1, |width| {
        usize::from(width).saturating_sub(1).max(1)
    })
}

fn display_width(line: &str) -> usize {
    line.chars().count()
}

fn select_progress_line(
    detailed: String,
    max_width: usize,
    compact: impl FnOnce() -> String,
    minimal: impl FnOnce() -> String,
) -> String {
    if display_width(&detailed) <= max_width {
        detailed
    } else {
        let compact = compact();
        if display_width(&compact) <= max_width {
            compact
        } else {
            let minimal = minimal();
            if display_width(&minimal) <= max_width {
                minimal
            } else {
                truncate_progress_line(&minimal, max_width)
            }
        }
    }
}

fn truncate_progress_line(line: &str, max_width: usize) -> String {
    if display_width(line) <= max_width {
        return line.to_owned();
    }
    if max_width < 3 {
        return line.chars().take(max_width).collect();
    }
    let mut truncated: String = line.chars().take(max_width - 3).collect();
    truncated.push_str("...");
    truncated
}

fn write_progress_update<W: Write>(
    writer: &mut W,
    line: &str,
    previous_width: usize,
) -> io::Result<()> {
    let padding = previous_width.saturating_sub(display_width(line));
    write!(writer, "\r{}{}", line, " ".repeat(padding))
}

fn write_progress_finish<W: Write>(writer: &mut W, previous_width: usize) -> io::Result<()> {
    write!(writer, "\r{}\r\n", " ".repeat(previous_width))
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

#[cfg(test)]
mod tests {
    use super::{
        display_width, max_progress_width, select_progress_line, write_progress_finish,
        write_progress_update,
    };

    #[test]
    fn progress_line_stays_inside_terminal_width() {
        let detailed =
            "Rendering 7.1.4 [64.1%] 00:12 / 00:20 audio  1.50x realtime  elapsed 00:13  ETA 00:08";
        let compact = "Rendering 7.1.4 [64.1%] 00:12 / 00:20  1.50x ETA 00:08";
        let minimal = "Rendering 7.1.4 [64.1%] 1.50x";

        for terminal_width in [Some(80), Some(40), Some(20), None] {
            let max_width = max_progress_width(terminal_width);
            let line = select_progress_line(
                detailed.to_owned(),
                max_width,
                || compact.to_owned(),
                || minimal.to_owned(),
            );
            assert!(display_width(&line) <= max_width, "{line:?} > {max_width}");
            assert!(!line.contains('\r'));
            assert!(!line.contains('\n'));
        }
    }

    #[test]
    fn detailed_progress_is_preserved_when_terminal_is_wide() {
        let detailed = "detailed".to_owned();
        let compact = "compact".to_owned();
        let minimal = "minimal".to_owned();
        assert_eq!(
            select_progress_line(detailed.clone(), 80, || compact, || minimal),
            detailed
        );
    }

    #[test]
    fn refresh_has_no_newline_and_finish_moves_to_a_clean_line() {
        let mut output = Vec::new();
        write_progress_update(&mut output, "Rendering 7.1.4 [64.1%]", 30).unwrap();
        assert_eq!(output, b"\rRendering 7.1.4 [64.1%]       ");

        write_progress_finish(&mut output, 30).unwrap();
        assert!(output.ends_with(b"\r                              \r\n"));
    }
}
