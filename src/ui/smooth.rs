use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::ui::theme::SPARKLINE_SAMPLES;

pub const DEFAULT_TAU_SECS: f64 = 3.0;
pub const DEFAULT_EASE_FACTOR: f64 = 0.15;

/// Exponential Moving Average (EMA) speed smoother with time-delta invariance and sparkline history.
///
/// Traditional discrete EMA (`alpha = const`) assumes uniform time intervals. In CLI downloaders,
/// progress stdout lines arrive irregularly (bursts on small chunks vs multi-second pauses on handshakes).
/// This implementation computes `alpha = 1 - exp(-dt / tau)`, making the smoothing mathematically
/// invariant to update arrival frequency.
///
/// # Examples
///
/// ```
/// use std::time::{Duration, Instant};
/// use dwdr::ui::smooth::SpeedSmoother;
///
/// let mut s = SpeedSmoother::new();
/// let t0 = Instant::now();
/// s.feed(1_000_000.0, t0);
/// s.feed(2_000_000.0, t0 + Duration::from_millis(500));
/// assert!(s.eta_seconds(10_000_000).is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SpeedSmoother {
    pub ema: f64,
    pub peak: f64,
    pub samples: VecDeque<f64>,
    pub last_update: Option<Instant>,
    pub tau_secs: f64,
}

impl Default for SpeedSmoother {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeedSmoother {
    pub fn new() -> Self {
        Self {
            ema: 0.0,
            peak: 0.0,
            samples: VecDeque::with_capacity(SPARKLINE_SAMPLES),
            last_update: None,
            tau_secs: DEFAULT_TAU_SECS,
        }
    }

    /// Feeds a new raw speed sample with explicit timestamp.
    /// Uses exponential moving average with alpha = 1 - exp(-dt / tau).
    pub fn feed(&mut self, raw_speed: f64, now: Instant) -> f64 {
        let alpha = match self.last_update {
            Some(last) => {
                let dt = now.saturating_duration_since(last).as_secs_f64();
                if self.tau_secs > 0.0 {
                    (1.0 - (-dt / self.tau_secs).exp()).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }
            None => 1.0,
        };
        self.last_update = Some(now);

        let speed = raw_speed.max(0.0);
        if self.ema == 0.0 {
            self.ema = speed;
        } else {
            self.ema = alpha * speed + (1.0 - alpha) * self.ema;
        }

        if speed > self.peak {
            self.peak = speed;
        }

        self.samples.push_back(speed);
        if self.samples.len() > SPARKLINE_SAMPLES {
            self.samples.pop_front();
        }

        self.ema
    }

    /// Feeds a new raw speed sample using the current time.
    pub fn feed_now(&mut self, raw_speed: f64) -> f64 {
        self.feed(raw_speed, Instant::now())
    }

    /// Computes smoothed ETA in seconds given remaining bytes.
    pub fn eta_seconds(&self, remaining_bytes: u64) -> Option<u64> {
        if self.ema > 0.0 && remaining_bytes > 0 {
            Some((remaining_bytes as f64 / self.ema) as u64)
        } else if remaining_bytes == 0 {
            Some(0)
        } else {
            None
        }
    }

    pub fn sparkline_samples(&self) -> Vec<f64> {
        self.samples.iter().copied().collect()
    }
}

/// Asymptotic progress interpolator that eases visual movement to prevent jarring jumps on chunk arrivals.
#[derive(Debug, Clone)]
pub struct EasedProgress {
    pub displayed: f64,
    pub target: f64,
}

impl Default for EasedProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl EasedProgress {
    pub fn new() -> Self {
        Self {
            displayed: 0.0,
            target: 0.0,
        }
    }

    pub fn set_target(&mut self, target: f64) {
        self.target = target.clamp(0.0, 100.0);
    }

    /// Advances the displayed percentage smoothly towards target using exponential decay.
    pub fn tick(&mut self) -> f64 {
        let diff = self.target - self.displayed;
        if diff.abs() < 0.05 {
            self.displayed = self.target;
        } else {
            self.displayed += diff * DEFAULT_EASE_FACTOR;
        }
        self.displayed = self.displayed.clamp(0.0, 100.0);
        self.displayed
    }
}

/// Rate-limiting throttle for plain (non-TTY / CI) progress output.
///
/// Prevents stdout log flooding by requiring either a significant progress advance
/// (`step_percent`, default 5.0%) or an elapsed time interval (`step_duration`, default 10s)
/// before emitting the next linear log line.
#[derive(Debug, Clone)]
pub struct PlainProgressThrottle {
    pub last_percent: f64,
    pub last_time: Option<Instant>,
    pub step_percent: f64,
    pub step_duration: Duration,
}

impl Default for PlainProgressThrottle {
    fn default() -> Self {
        Self::new(5.0, Duration::from_secs(10))
    }
}

impl PlainProgressThrottle {
    pub fn new(step_percent: f64, step_duration: Duration) -> Self {
        Self {
            last_percent: -step_percent,
            last_time: None,
            step_percent,
            step_duration,
        }
    }

    /// Determines if a new progress line should be printed.
    pub fn should_print(&mut self, current_percent: f64, now: Instant) -> bool {
        if current_percent >= 100.0 {
            self.last_percent = 100.0;
            self.last_time = Some(now);
            return true;
        }

        let percent_due = (current_percent - self.last_percent) >= self.step_percent;
        let time_due = match self.last_time {
            Some(last) => now.saturating_duration_since(last) >= self.step_duration,
            None => true,
        };

        if percent_due || time_due {
            self.last_percent = current_percent;
            self.last_time = Some(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_smoother_time_based_ema_fake_clock() {
        let t0 = Instant::now();
        let mut s = SpeedSmoother::new();

        // First sample at t0: takes initial speed
        s.feed(1_000_000.0, t0);
        assert_eq!(s.ema, 1_000_000.0);
        assert_eq!(s.peak, 1_000_000.0);

        // After dt = 3 seconds (equal to TAU = 3s):
        // alpha = 1 - exp(-1) ≈ 0.63212
        let t1 = t0 + Duration::from_secs(3);
        s.feed(2_000_000.0, t1);
        let expected_alpha = 1.0 - (-1.0_f64).exp();
        let expected_ema = expected_alpha * 2_000_000.0 + (1.0 - expected_alpha) * 1_000_000.0;
        assert!((s.ema - expected_ema).abs() < 10.0);
        assert_eq!(s.peak, 2_000_000.0);

        // Stall: feed 0 speed after 3 seconds: EMA dips towards 0
        let t2 = t1 + Duration::from_secs(3);
        s.feed(0.0, t2);
        assert!(s.ema < expected_ema);
        assert_eq!(s.samples.back().copied(), Some(0.0));
    }

    #[test]
    fn test_speed_smoother_edge_cases_and_cap() {
        let t0 = Instant::now();
        let mut s = SpeedSmoother::new();

        // Negative / 0 should not panic, treated as 0
        s.feed(0.0, t0);
        assert_eq!(s.ema, 0.0);
        assert_eq!(s.samples.len(), 1);

        // Feed 25 samples: must cap at SPARKLINE_SAMPLES (20)
        for i in 1..=25 {
            s.feed(i as f64 * 100_000.0, t0 + Duration::from_millis(i * 100));
        }
        assert_eq!(s.samples.len(), 20);
        assert_eq!(s.sparkline_samples().len(), 20);

        // ETA calculation
        assert_eq!(s.eta_seconds(0), Some(0));
        assert!(s.eta_seconds(10_000_000).is_some());
    }

    #[test]
    fn test_eased_progress_convergence() {
        let mut p = EasedProgress::new();
        p.set_target(100.0);
        assert_eq!(p.displayed, 0.0);

        for _ in 0..50 {
            p.tick();
        }
        assert!((p.displayed - 100.0).abs() < 0.1);
        p.tick();
        assert_eq!(p.displayed, 100.0);
    }

    #[test]
    fn test_plain_progress_throttle() {
        let t0 = Instant::now();
        let mut throttle = PlainProgressThrottle::default();

        // Initial 0% prints
        assert!(throttle.should_print(0.0, t0));

        // 2% after 1s: does not print (< 5% and < 10s)
        assert!(!throttle.should_print(2.0, t0 + Duration::from_secs(1)));

        // 5% after 2s: prints (>= 5%)
        assert!(throttle.should_print(5.0, t0 + Duration::from_secs(2)));

        // 6% after 3s: does not print (< 5% delta)
        assert!(!throttle.should_print(6.0, t0 + Duration::from_secs(3)));

        // 6% after 13s (11s elapsed): prints due to time trigger (>= 10s)
        assert!(throttle.should_print(6.0, t0 + Duration::from_secs(13)));

        // 100% prints unconditionally
        assert!(throttle.should_print(100.0, t0 + Duration::from_secs(14)));
    }

    #[test]
    fn test_speed_smoother_fixed_interval_sampling_simulation() {
        let t0 = Instant::now();
        let mut smoother = SpeedSmoother::new();
        let mut last_sample_time = t0;
        let mut last_speed_update = t0;
        let mut raw_speed = 5.0 * 1024.0 * 1024.0;

        let total_ms = 5000;
        let tick_ms = 80;
        let sample_interval_ms = 250;
        let stall_threshold = Duration::from_secs(2);

        let mut sim_ms = 0;
        while sim_ms <= total_ms {
            let now = t0 + Duration::from_millis(sim_ms);
            if sim_ms > 0 && sim_ms % 500 == 0 {
                raw_speed = 5.0 * 1024.0 * 1024.0;
                last_speed_update = now;
            }

            if now.duration_since(last_sample_time) >= Duration::from_millis(sample_interval_ms) {
                let is_stalled = now.duration_since(last_speed_update) > stall_threshold;
                let sample_speed = if is_stalled { 0.0 } else { raw_speed };
                smoother.feed(sample_speed, now);
                last_sample_time = now;
            }

            sim_ms += tick_ms;
        }

        let target = 5.0 * 1024.0 * 1024.0;
        assert!(
            (smoother.ema - target).abs() / target < 0.05,
            "EMA {} is not within 5% of target {}",
            smoother.ema,
            target
        );
        assert!(
            smoother.samples.len() >= 15 && smoother.samples.len() <= 20,
            "expected ~20 samples, got {}",
            smoother.samples.len()
        );

        let prev_ema = smoother.ema;
        let stall_start = t0 + Duration::from_millis(sim_ms);
        for step in 1..=(3000 / tick_ms) {
            let now = stall_start + Duration::from_millis(step * tick_ms);
            if now.duration_since(last_sample_time) >= Duration::from_millis(sample_interval_ms) {
                let is_stalled = now.duration_since(last_speed_update) > stall_threshold;
                let sample_speed = if is_stalled { 0.0 } else { raw_speed };
                smoother.feed(sample_speed, now);
                last_sample_time = now;
            }
        }

        assert!(smoother.ema < prev_ema);
        assert_eq!(smoother.samples.back().copied(), Some(0.0));
    }
}
