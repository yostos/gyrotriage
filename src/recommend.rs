use crate::analyze::AnalysisResult;
use crate::spectrum;

/// Gyroflow OpenFX plugin ("Adjust parameters") recommended values.
///
/// Only motion-derived values are stored here. Fixed DJI recommendations
/// (Integration method, Lens correction) are constants below.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Smoothness — base stabilization strength.
    /// Plugin value range 1–300 (equals Gyroflow core `smoothness` ×100).
    /// gyrotriage's historical `smoothness_pct` (15–50) maps here unchanged.
    pub smoothness: f64,
    /// Zoom limit (%) — maximum allowed zoom (plugin range 51–300).
    pub zoom_limit_pct: f64,
    /// FOV (factor) — baseline only; not computed (see ADR-005). Plugin range 0.1–3.0.
    pub fov: f64,
}

/// Integration method recommended for DJI footage: quaternions are pre-recorded,
/// so no re-integration is needed.
pub const INTEGRATION_METHOD: &str = "None";
/// Lens correction (%) recommended for DJI footage: full correction, assuming the
/// correct DJI lens profile/preset is loaded.
pub const LENS_CORRECTION: f64 = 100.0;
/// FOV baseline (Gyroflow plugin default); FOV is not auto-computed yet.
pub const FOV_BASELINE: f64 = 1.0;

/// Map analysis result to Gyroflow plugin recommended values using PSD-based
/// frequency analysis.
///
/// The approach:
/// 1. Compute PSD of angular velocity time series
/// 2. Derive shake power ratio (power above the intentional-motion cutoff)
/// 3. Convert to plugin Smoothness and Zoom limit
pub fn recommend(result: &AnalysisResult) -> Recommendation {
    let spectrum = spectrum::analyze_spectrum(
        &result.pitch_velocities,
        &result.roll_velocities,
        &result.yaw_velocities,
        result.sample_rate_hz,
    );

    // Smoothness: derived from shake power ratio and RMS velocity.
    // Higher shake ratio → more smoothing needed.
    // Range: 15 (very stable) to 50 (severe shake). FPV sweet spot 20-35.
    // The value is used directly as the plugin Smoothness (1–300 scale).
    let smoothness = smoothness_from_spectrum(&spectrum, result.rms_velocity);

    // Zoom limit (%): estimated from smoothness and RMS angular displacement.
    // Higher smoothness + more shake → more zoom needed.
    // Range: 105% (minimal) to 140% (heavy stabilization).
    let zoom_limit_pct = estimate_zoom_limit(smoothness, result.rms_velocity);

    Recommendation {
        smoothness,
        zoom_limit_pct,
        fov: FOV_BASELINE,
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

/// Estimate zoom limit from smoothness and angular velocity.
///
/// More stabilization (higher smoothness) on shakier footage requires
/// more zoom headroom. FPV ideal range is 110-125%.
fn estimate_zoom_limit(smoothness: f64, rms_velocity: f64) -> f64 {
    // Base zoom from smoothness: 105% at smoothness 15, 130% at 50
    let base = 105.0 + (smoothness - 15.0) * 25.0 / 35.0;

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
        assert!(rec.smoothness < 30.0, "smoothness={}", rec.smoothness);
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
        assert!(rec.smoothness > 20.0, "smoothness={}", rec.smoothness);
    }

    #[test]
    fn test_smoothness_within_plugin_range() {
        // Computed Smoothness must be a valid plugin value (1–300).
        let vels = make_sinusoidal_velocities(3.0, 10.0, 4000, 200.0);
        let result = make_result_with_velocities(50, 7.0, vels);
        let rec = recommend(&result);
        assert!(rec.smoothness >= 1.0 && rec.smoothness <= 300.0,
            "smoothness={} must be in plugin range 1–300", rec.smoothness);
    }

    #[test]
    fn test_fov_baseline() {
        // FOV is not computed; it must be the plugin baseline 1.0.
        let vels = make_sinusoidal_velocities(3.0, 10.0, 4000, 200.0);
        let result = make_result_with_velocities(50, 7.0, vels);
        let rec = recommend(&result);
        assert_eq!(rec.fov, FOV_BASELINE);
        assert_eq!(rec.fov, 1.0);
    }

    #[test]
    fn test_fixed_recommendations() {
        // DJI fixed recommendations are stable constants.
        assert_eq!(INTEGRATION_METHOD, "None");
        assert_eq!(LENS_CORRECTION, 100.0);
    }

    #[test]
    fn test_zoom_limit_within_plugin_range() {
        // Computed Zoom limit must be a valid plugin value (51–300%).
        let vels = make_sinusoidal_velocities(5.0, 30.0, 4000, 200.0);
        let result = make_result_with_velocities(100, 20.0, vels);
        let rec = recommend(&result);
        assert!(rec.zoom_limit_pct >= 51.0 && rec.zoom_limit_pct <= 300.0,
            "zoom_limit={}% must be in plugin range 51–300", rec.zoom_limit_pct);
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
    fn test_smoothness_clamp_range() {
        // The smoothness mapping is clamped to 15-50 regardless of input severity.
        let vels = make_sinusoidal_velocities(5.0, 30.0, 4000, 200.0);
        let result = make_result_with_velocities(100, 20.0, vels);
        let rec = recommend(&result);
        assert!(rec.smoothness >= 15.0 && rec.smoothness <= 50.0);
    }
}
