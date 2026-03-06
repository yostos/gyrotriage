use crate::analyze::AnalysisResult;
use crate::spectrum;

/// Gyroflow recommended parameters.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Smoothness (%) — base stabilization strength.
    pub smoothness_pct: f64,
    /// Max smoothness (seconds) — upper limit for sudden shake.
    pub max_smoothness_s: f64,
    /// Max smoothness at high velocity (seconds) — limit during fast rotation.
    pub max_smoothness_at_high_velocity_s: f64,
    /// Zoom limit (%) — maximum allowed zoom.
    pub zoom_limit_pct: f64,
    /// Zooming speed (seconds) — dynamic zoom FOV transition window.
    pub zooming_speed_s: f64,
}

/// Map analysis result to Gyroflow recommended parameters using PSD-based
/// frequency analysis.
///
/// The approach:
/// 1. Compute PSD of angular velocity time series
/// 2. Find cutoff frequency between intentional motion and shake
/// 3. Derive time constants from cutoff frequencies
/// 4. Convert to Gyroflow parameter units
pub fn recommend(result: &AnalysisResult) -> Recommendation {
    let spectrum = spectrum::analyze_spectrum(
        &result.pitch_velocities,
        &result.roll_velocities,
        &result.yaw_velocities,
        result.sample_rate_hz,
    );

    // Smoothness (%): derived from shake power ratio and RMS velocity.
    // Higher shake ratio → more smoothing needed.
    // Range: 15% (very stable) to 50% (severe shake).
    // FPV sweet spot is typically 20-35%.
    let smoothness_pct = smoothness_from_spectrum(&spectrum, result.rms_velocity);

    // Max smoothness (s): time constant from the primary cutoff frequency.
    // This is the maximum stabilization applied during sudden shakes.
    // Clamped to [0.3, 2.0] seconds (Gyroflow practical range).
    let max_smoothness_s = spectrum.time_constant.clamp(0.3, 2.0);

    // Max smoothness at high velocity (s): shorter time constant for fast rotation.
    // Keeps responsiveness during aggressive maneuvers.
    // Clamped to [0.03, 0.3] seconds.
    let max_smoothness_at_high_velocity_s = spectrum.high_velocity_time_constant.clamp(0.03, 0.3);

    // Zoom limit (%): estimated from smoothness and RMS angular displacement.
    // Higher smoothness + more shake → more zoom needed.
    // Range: 105% (minimal) to 140% (heavy stabilization).
    let zoom_limit_pct = estimate_zoom_limit(smoothness_pct, result.rms_velocity);

    // Zooming speed (s): dynamic zoom FOV transition window.
    // Derived from temporal variability of angular velocity.
    // Intermittent shake → shorter window (FOV adapts quickly).
    // Constant shake → longer window (smooth FOV transitions).
    let zooming_speed_s = estimate_zooming_speed(&result.pitch_velocities, &result.roll_velocities, &result.yaw_velocities, result.sample_rate_hz);

    Recommendation {
        smoothness_pct,
        max_smoothness_s,
        max_smoothness_at_high_velocity_s,
        zoom_limit_pct,
        zooming_speed_s,
    }
}

/// Convert spectral analysis to smoothness percentage.
///
/// Based on shake power ratio (what fraction of signal is unwanted shake)
/// and RMS angular velocity (overall magnitude).
fn smoothness_from_spectrum(spectrum: &spectrum::SpectrumResult, rms_velocity: f64) -> f64 {
    // Base: shake power ratio scaled to percentage range
    // 0% shake → ~15%, 100% shake → ~50%
    let base = 15.0 + 35.0 * spectrum.shake_power_ratio;

    // Adjust by RMS velocity magnitude
    // Very low RMS (< 3°/s) → reduce slightly (less correction needed)
    // Very high RMS (> 15°/s) → increase slightly (more correction needed)
    let velocity_factor = if rms_velocity < 3.0 {
        0.85
    } else if rms_velocity > 15.0 {
        1.15
    } else {
        0.85 + 0.30 * (rms_velocity - 3.0) / 12.0
    };

    (base * velocity_factor).clamp(15.0, 50.0)
}

/// Estimate zooming speed from temporal variability of angular velocity.
///
/// Computes the coefficient of variation of rolling RMS over 1-second windows.
/// High variability (intermittent bursts) → short window (2-3s).
/// Low variability (constant shake) → long window (5-6s).
fn estimate_zooming_speed(pitch: &[f64], roll: &[f64], yaw: &[f64], sample_rate_hz: f64) -> f64 {
    if pitch.is_empty() || sample_rate_hz <= 0.0 {
        return 4.0; // Gyroflow default
    }

    // Compute composite angular velocity magnitude
    let composite: Vec<f64> = pitch.iter()
        .zip(roll.iter())
        .zip(yaw.iter())
        .map(|((&p, &r), &y)| (p * p + r * r + y * y).sqrt())
        .collect();

    // Compute rolling RMS over 1-second windows
    let window_size = (sample_rate_hz as usize).max(1);
    if composite.len() < window_size * 2 {
        return 4.0; // not enough data for meaningful variability analysis
    }

    let mut window_rms_values: Vec<f64> = Vec::new();
    let step = window_size / 2; // 50% overlap
    let mut start = 0;
    while start + window_size <= composite.len() {
        let window = &composite[start..start + window_size];
        let sum_sq: f64 = window.iter().map(|v| v * v).sum();
        let rms = (sum_sq / window.len() as f64).sqrt();
        window_rms_values.push(rms);
        start += step.max(1);
    }

    if window_rms_values.len() < 2 {
        return 4.0;
    }

    // Coefficient of variation = std_dev / mean
    let mean: f64 = window_rms_values.iter().sum::<f64>() / window_rms_values.len() as f64;
    if mean <= 0.0 {
        return 4.0;
    }
    let variance: f64 = window_rms_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
        / window_rms_values.len() as f64;
    let cv = variance.sqrt() / mean;

    // Map CV to zooming speed:
    // cv ≈ 0 (very uniform) → 6.0s (slow, smooth transitions)
    // cv ≈ 0.5 (moderate variation) → 4.0s (default)
    // cv ≈ 1.0+ (highly intermittent) → 2.0s (fast adaptation)
    let speed = 6.0 - 4.0 * cv.min(1.0);
    speed.clamp(2.0, 6.0)
}

/// Estimate zoom limit from smoothness and angular velocity.
///
/// More stabilization (higher smoothness) on shakier footage requires
/// more zoom headroom. FPV ideal range is 110-125%.
fn estimate_zoom_limit(smoothness_pct: f64, rms_velocity: f64) -> f64 {
    // Base zoom from smoothness: 105% at 15% smoothness, 130% at 50%
    let base = 105.0 + (smoothness_pct - 15.0) * 25.0 / 35.0;

    // Additional zoom for high RMS velocity
    let velocity_extra = (rms_velocity / 20.0 * 5.0).min(10.0);

    (base + velocity_extra).clamp(105.0, 140.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{AnalysisResult, AxisStats, Level};

    fn make_result_with_velocities(
        score: u32,
        rms: f64,
        velocities: Vec<f64>,
    ) -> AnalysisResult {
        let n = velocities.len();
        AnalysisResult {
            duration_secs: n as f64 / 200.0,
            sample_count: n + 1,
            sample_rate_hz: 200.0,
            rms_velocity: rms,
            peak_velocity: rms * 2.5,
            score,
            level: Level::from_score(score),
            pitch: AxisStats { avg: rms * 0.5, std_dev: rms * 0.3, max: rms * 2.0 },
            roll: AxisStats { avg: rms * 0.3, std_dev: rms * 0.2, max: rms * 1.5 },
            yaw: AxisStats { avg: rms * 0.4, std_dev: rms * 0.25, max: rms * 1.8 },
            pitch_velocities: velocities.clone(),
            roll_velocities: velocities.iter().map(|v| v * 0.5).collect(),
            yaw_velocities: velocities.iter().map(|v| v * 0.7).collect(),
        }
    }

    fn make_sinusoidal_velocities(freq_hz: f64, amplitude: f64, n: usize, rate: f64) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                amplitude * (2.0 * std::f64::consts::PI * freq_hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn test_stable_flight_low_smoothness() {
        // Low-frequency gentle motion → low smoothness
        let vels = make_sinusoidal_velocities(0.3, 2.0, 4000, 200.0);
        let result = make_result_with_velocities(10, 1.5, vels);
        let rec = recommend(&result);
        assert!(rec.smoothness_pct < 30.0, "smoothness={}%", rec.smoothness_pct);
    }

    #[test]
    fn test_shaky_flight_higher_smoothness() {
        // Mixed: intentional motion + significant shake
        let vels: Vec<f64> = (0..4000)
            .map(|i| {
                let t = i as f64 / 200.0;
                5.0 * (2.0 * std::f64::consts::PI * 0.5 * t).sin()
                    + 8.0 * (2.0 * std::f64::consts::PI * 8.0 * t).sin()
            })
            .collect();
        let result = make_result_with_velocities(60, 12.0, vels);
        let rec = recommend(&result);
        assert!(rec.smoothness_pct > 20.0, "smoothness={}%", rec.smoothness_pct);
    }

    #[test]
    fn test_max_smoothness_in_range() {
        let vels = make_sinusoidal_velocities(3.0, 10.0, 4000, 200.0);
        let result = make_result_with_velocities(50, 7.0, vels);
        let rec = recommend(&result);
        assert!(rec.max_smoothness_s >= 0.3 && rec.max_smoothness_s <= 2.0,
            "max_smoothness={}s", rec.max_smoothness_s);
    }

    #[test]
    fn test_high_velocity_tc_shorter_than_normal() {
        let vels: Vec<f64> = (0..4000)
            .map(|i| {
                let t = i as f64 / 200.0;
                10.0 * (2.0 * std::f64::consts::PI * 1.0 * t).sin()
                    + 3.0 * (2.0 * std::f64::consts::PI * 15.0 * t).sin()
            })
            .collect();
        let result = make_result_with_velocities(40, 8.0, vels);
        let rec = recommend(&result);
        assert!(
            rec.max_smoothness_at_high_velocity_s < rec.max_smoothness_s,
            "high_vel={}s should be < max={}s",
            rec.max_smoothness_at_high_velocity_s, rec.max_smoothness_s
        );
    }

    #[test]
    fn test_zoom_limit_in_range() {
        let vels = make_sinusoidal_velocities(5.0, 15.0, 4000, 200.0);
        let result = make_result_with_velocities(70, 14.0, vels);
        let rec = recommend(&result);
        assert!(rec.zoom_limit_pct >= 105.0 && rec.zoom_limit_pct <= 140.0,
            "zoom_limit={}%", rec.zoom_limit_pct);
    }

    #[test]
    fn test_zoom_limit_increases_with_shake() {
        let stable_vels = make_sinusoidal_velocities(0.5, 2.0, 4000, 200.0);
        let shaky_vels: Vec<f64> = (0..4000)
            .map(|i| {
                let t = i as f64 / 200.0;
                5.0 * (2.0 * std::f64::consts::PI * 0.5 * t).sin()
                    + 10.0 * (2.0 * std::f64::consts::PI * 10.0 * t).sin()
            })
            .collect();
        let rec_stable = recommend(&make_result_with_velocities(10, 2.0, stable_vels));
        let rec_shaky = recommend(&make_result_with_velocities(70, 14.0, shaky_vels));
        assert!(
            rec_shaky.zoom_limit_pct > rec_stable.zoom_limit_pct,
            "shaky zoom={}% should be > stable zoom={}%",
            rec_shaky.zoom_limit_pct, rec_stable.zoom_limit_pct
        );
    }

    #[test]
    fn test_smoothness_pct_range() {
        // Any input should produce smoothness in 15-50%
        let vels = make_sinusoidal_velocities(5.0, 30.0, 4000, 200.0);
        let result = make_result_with_velocities(100, 20.0, vels);
        let rec = recommend(&result);
        assert!(rec.smoothness_pct >= 15.0 && rec.smoothness_pct <= 50.0);
    }

    #[test]
    fn test_zooming_speed_in_range() {
        let vels = make_sinusoidal_velocities(5.0, 10.0, 4000, 200.0);
        let result = make_result_with_velocities(50, 7.0, vels);
        let rec = recommend(&result);
        assert!(rec.zooming_speed_s >= 2.0 && rec.zooming_speed_s <= 6.0,
            "zooming_speed={}s", rec.zooming_speed_s);
    }

    #[test]
    fn test_zooming_speed_shorter_for_intermittent_shake() {
        // Constant shake
        let constant_vels = make_sinusoidal_velocities(5.0, 10.0, 8000, 200.0);
        // Intermittent: alternating calm and shaky sections
        let intermittent_vels: Vec<f64> = (0..8000)
            .map(|i| {
                let t = i as f64 / 200.0;
                let section = (t / 2.0) as usize % 2; // alternates every 2 seconds
                if section == 0 {
                    1.0 * (2.0 * std::f64::consts::PI * 5.0 * t).sin() // calm
                } else {
                    20.0 * (2.0 * std::f64::consts::PI * 5.0 * t).sin() // shaky
                }
            })
            .collect();
        let rec_constant = recommend(&make_result_with_velocities(50, 7.0, constant_vels));
        let rec_intermittent = recommend(&make_result_with_velocities(50, 10.0, intermittent_vels));
        assert!(
            rec_intermittent.zooming_speed_s < rec_constant.zooming_speed_s,
            "intermittent={}s should be < constant={}s",
            rec_intermittent.zooming_speed_s, rec_constant.zooming_speed_s
        );
    }
}
