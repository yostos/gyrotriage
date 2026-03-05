use crate::analyze::AnalysisResult;

/// DJI Neo FOV in degrees (fixed lens, no FOV options).
const FOV_DEG: f64 = 117.6;

/// Gyroflow recommended parameters.
#[derive(Debug, Clone)]
pub struct Recommendation {
    pub smoothness: f64,
    pub crop: f64,
}

/// Map analysis result to Gyroflow recommended parameters.
///
/// Smoothness mapping (piecewise linear):
/// - Score 0–25:   0.2
/// - Score 26–50:  0.3–0.5
/// - Score 51–75:  0.5–0.8
/// - Score 76–100: 0.8–1.5
///
/// Crop estimation: `crop ≈ 1 + smoothness × rms_velocity / FOV`.
/// RMS angular velocity × smoothness approximates the effective angular
/// displacement over the stabilization window.
pub fn recommend(result: &AnalysisResult) -> Recommendation {
    let score = result.score as f64;

    let smoothness = if score <= 25.0 {
        0.2
    } else if score <= 50.0 {
        lerp(0.3, 0.5, (score - 26.0) / 24.0)
    } else if score <= 75.0 {
        lerp(0.5, 0.8, (score - 51.0) / 24.0)
    } else {
        lerp(0.8, 1.5, (score - 76.0) / 24.0)
    };

    let crop = 1.0 + smoothness * result.rms_velocity / FOV_DEG;

    Recommendation { smoothness, crop }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{AnalysisResult, AxisStats, Level};

    fn make_result(score: u32) -> AnalysisResult {
        AnalysisResult {
            duration_secs: 60.0,
            sample_count: 12000,
            sample_rate_hz: 200.0,
            rms_velocity: score as f64 * 0.2,
            peak_velocity: score as f64 * 0.5,
            score,
            level: Level::from_score(score),
            pitch: AxisStats { avg: 0.0, std_dev: 0.0, max: 0.0 },
            roll: AxisStats { avg: 0.0, std_dev: 0.0, max: 0.0 },
            yaw: AxisStats { avg: 0.0, std_dev: 0.0, max: 0.0 },
        }
    }

    #[test]
    fn test_stable_score() {
        let rec = recommend(&make_result(10));
        assert!((rec.smoothness - 0.2).abs() < 0.01);
        assert!(rec.crop >= 1.0);
    }

    #[test]
    fn test_mild_score() {
        let rec = recommend(&make_result(38));
        assert!(rec.smoothness >= 0.3 && rec.smoothness <= 0.5);
    }

    #[test]
    fn test_moderate_score() {
        let rec = recommend(&make_result(63));
        assert!(rec.smoothness >= 0.5 && rec.smoothness <= 0.8);
    }

    #[test]
    fn test_severe_score() {
        let rec = recommend(&make_result(90));
        assert!(rec.smoothness >= 0.8 && rec.smoothness <= 1.5);
    }

    #[test]
    fn test_crop_increases_with_rms() {
        let rec_low = recommend(&make_result(30));  // rms = 6.0
        let rec_high = recommend(&make_result(80)); // rms = 16.0
        assert!(rec_high.crop > rec_low.crop);
    }

    #[test]
    fn test_crop_minimum_is_one() {
        let rec = recommend(&make_result(0));
        assert!((rec.crop - 1.0).abs() < 1e-10);
    }
}
