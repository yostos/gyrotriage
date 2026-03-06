use telemetry_parser::tags_impl::TimeQuaternion;

/// Reference maximum angular velocity (°/s) for score normalization.
/// Score 100 = this value or above.
const REFERENCE_MAX_DEG_PER_SEC: f64 = 20.0;

/// Stability level based on shake score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Stable,
    Mild,
    Moderate,
    Severe,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Stable => write!(f, "STABLE"),
            Level::Mild => write!(f, "MILD"),
            Level::Moderate => write!(f, "MODERATE"),
            Level::Severe => write!(f, "SEVERE"),
        }
    }
}

impl Level {
    pub fn from_score(score: u32) -> Self {
        match score {
            0..=25 => Level::Stable,
            26..=50 => Level::Mild,
            51..=75 => Level::Moderate,
            _ => Level::Severe,
        }
    }
}

/// Per-axis angular velocity statistics (°/s).
#[derive(Debug, Clone)]
pub struct AxisStats {
    pub avg: f64,
    pub std_dev: f64,
    pub max: f64,
}

/// Complete analysis result.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub duration_secs: f64,
    pub sample_count: usize,
    pub sample_rate_hz: f64,
    pub rms_velocity: f64,
    pub peak_velocity: f64,
    pub score: u32,
    pub level: Level,
    pub pitch: AxisStats,
    pub roll: AxisStats,
    pub yaw: AxisStats,
    /// Per-axis angular velocity time series (°/s) for spectral analysis.
    pub pitch_velocities: Vec<f64>,
    pub roll_velocities: Vec<f64>,
    pub yaw_velocities: Vec<f64>,
}

/// Quaternion conjugate: q* = (w, -x, -y, -z)
fn conjugate(q: &telemetry_parser::tags_impl::Quaternion<f64>) -> telemetry_parser::tags_impl::Quaternion<f64> {
    telemetry_parser::tags_impl::Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    }
}

/// Quaternion multiplication: p × q
fn quat_mul(
    p: &telemetry_parser::tags_impl::Quaternion<f64>,
    q: &telemetry_parser::tags_impl::Quaternion<f64>,
) -> telemetry_parser::tags_impl::Quaternion<f64> {
    telemetry_parser::tags_impl::Quaternion {
        w: p.w * q.w - p.x * q.x - p.y * q.y - p.z * q.z,
        x: p.w * q.x + p.x * q.w + p.y * q.z - p.z * q.y,
        y: p.w * q.y - p.x * q.z + p.y * q.w + p.z * q.x,
        z: p.w * q.z + p.x * q.y - p.y * q.x + p.z * q.w,
    }
}

/// Angular displacement between two quaternions (radians).
/// θ = 2 × arccos(min(|q_diff.w|, 1.0))
fn angular_displacement(
    q1: &telemetry_parser::tags_impl::Quaternion<f64>,
    q2: &telemetry_parser::tags_impl::Quaternion<f64>,
) -> f64 {
    let q_diff = quat_mul(&conjugate(q1), q2);
    let w_clamped = q_diff.w.abs().min(1.0);
    2.0 * w_clamped.acos()
}

/// Decompose the relative rotation between q1 and q2 into ZYX intrinsic Euler angles (radians).
/// Returns (pitch, roll, yaw).
fn decompose_to_euler(
    q1: &telemetry_parser::tags_impl::Quaternion<f64>,
    q2: &telemetry_parser::tags_impl::Quaternion<f64>,
) -> (f64, f64, f64) {
    let q_diff = quat_mul(&conjugate(q1), q2);
    let w = q_diff.w;
    let x = q_diff.x;
    let y = q_diff.y;
    let z = q_diff.z;

    // ZYX intrinsic Euler angles
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let pitch = sinr_cosp.atan2(cosr_cosp);

    let sinp = 2.0 * (w * y - z * x);
    let roll = if sinp.abs() >= 1.0 {
        std::f64::consts::FRAC_PI_2.copysign(sinp)
    } else {
        sinp.asin()
    };

    let siny_cosp = 2.0 * (w * z + x * y);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    let yaw = siny_cosp.atan2(cosy_cosp);

    (pitch, roll, yaw)
}

/// Analyze quaternion time series and produce scoring result.
pub fn analyze(quaternions: &[TimeQuaternion<f64>]) -> AnalysisResult {
    let n = quaternions.len();
    assert!(n >= 2, "Need at least 2 quaternion samples");

    // Timestamps from telemetry-parser are in milliseconds
    let duration_ms = quaternions.last().unwrap().t - quaternions.first().unwrap().t;
    let duration_secs = duration_ms / 1000.0;
    let sample_rate_hz = if duration_secs > 0.0 {
        (n - 1) as f64 / duration_secs
    } else {
        0.0
    };

    let mut angular_velocities: Vec<f64> = Vec::with_capacity(n - 1);
    let mut pitch_velocities: Vec<f64> = Vec::with_capacity(n - 1);
    let mut roll_velocities: Vec<f64> = Vec::with_capacity(n - 1);
    let mut yaw_velocities: Vec<f64> = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        let q1 = &quaternions[i].v;
        let q2 = &quaternions[i + 1].v;
        let dt_ms = quaternions[i + 1].t - quaternions[i].t;
        let dt = dt_ms / 1000.0; // Convert ms to seconds

        if dt <= 0.0 {
            continue;
        }

        let theta = angular_displacement(q1, q2);
        let omega = theta.to_degrees() / dt; // °/s
        angular_velocities.push(omega);

        let (pitch, roll, yaw) = decompose_to_euler(q1, q2);
        pitch_velocities.push(pitch.to_degrees().abs() / dt);
        roll_velocities.push(roll.to_degrees().abs() / dt);
        yaw_velocities.push(yaw.to_degrees().abs() / dt);
    }

    let rms_velocity = rms(&angular_velocities);
    let peak_velocity = angular_velocities.iter().cloned().fold(0.0_f64, f64::max);

    let score_raw = (rms_velocity / REFERENCE_MAX_DEG_PER_SEC * 100.0).min(100.0);
    let score = score_raw.round() as u32;
    let level = Level::from_score(score);

    let pitch_stats = compute_axis_stats(&pitch_velocities);
    let roll_stats = compute_axis_stats(&roll_velocities);
    let yaw_stats = compute_axis_stats(&yaw_velocities);

    AnalysisResult {
        duration_secs,
        sample_count: n,
        sample_rate_hz,
        rms_velocity,
        peak_velocity,
        score,
        level,
        pitch: pitch_stats,
        roll: roll_stats,
        yaw: yaw_stats,
        pitch_velocities,
        roll_velocities,
        yaw_velocities,
    }
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = values.iter().map(|v| v * v).sum();
    (sum_sq / values.len() as f64).sqrt()
}

fn compute_axis_stats(values: &[f64]) -> AxisStats {
    if values.is_empty() {
        return AxisStats {
            avg: 0.0,
            std_dev: 0.0,
            max: 0.0,
        };
    }
    let n = values.len() as f64;
    let avg = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    AxisStats { avg, std_dev, max }
}

#[cfg(test)]
mod tests {
    use super::*;
    use telemetry_parser::tags_impl::{Quaternion, TimeQuaternion};

    fn make_tq(t: f64, w: f64, x: f64, y: f64, z: f64) -> TimeQuaternion<f64> {
        TimeQuaternion {
            t,
            v: Quaternion { w, x, y, z },
        }
    }

    /// Identity quaternion sequence → score 0, STABLE
    #[test]
    fn test_identity_sequence() {
        // Timestamps in milliseconds (200Hz = 5ms interval)
        let quats: Vec<TimeQuaternion<f64>> = (0..100)
            .map(|i| make_tq(i as f64 * 5.0, 1.0, 0.0, 0.0, 0.0))
            .collect();
        let result = analyze(&quats);
        assert_eq!(result.score, 0);
        assert_eq!(result.level, Level::Stable);
        assert!(result.rms_velocity < 0.001);
    }

    /// Constant-rate rotation around Z axis → low but nonzero score
    #[test]
    fn test_constant_rotation() {
        // 1°/s rotation around Z axis at 200Hz (5ms interval)
        let rate_rad_per_sec = 1.0_f64.to_radians();
        let dt_ms = 5.0; // 200 Hz
        let n = 200;
        let quats: Vec<TimeQuaternion<f64>> = (0..n)
            .map(|i| {
                let t_ms = i as f64 * dt_ms;
                let t_s = t_ms / 1000.0;
                let angle = rate_rad_per_sec * t_s / 2.0;
                make_tq(t_ms, angle.cos(), 0.0, 0.0, angle.sin())
            })
            .collect();
        let result = analyze(&quats);
        assert!(result.score <= 10, "Score {} should be low for 1°/s rotation", result.score);
        assert_eq!(result.level, Level::Stable);
    }

    /// Sinusoidal shake → score proportional to amplitude
    #[test]
    fn test_sinusoidal_shake() {
        let dt_ms = 5.0; // 200 Hz
        let n = 1000;
        let freq = 5.0; // 5 Hz vibration
        let amplitude_deg: f64 = 5.0; // 5° amplitude
        let amplitude_rad = amplitude_deg.to_radians();

        let quats: Vec<TimeQuaternion<f64>> = (0..n)
            .map(|i| {
                let t_ms = i as f64 * dt_ms;
                let t_s = t_ms / 1000.0;
                let angle = amplitude_rad * (2.0 * std::f64::consts::PI * freq * t_s).sin() / 2.0;
                make_tq(t_ms, angle.cos(), angle.sin(), 0.0, 0.0)
            })
            .collect();
        let result = analyze(&quats);
        assert!(result.score > 0, "Score should be nonzero for shake");
        assert!(result.score <= 100);
    }

    /// Large shake → score should clamp at 100
    #[test]
    fn test_score_clamp_at_100() {
        let dt_ms = 5.0; // 200 Hz
        let n = 1000;
        let freq = 10.0;
        let amplitude_deg: f64 = 30.0; // Very large shake
        let amplitude_rad = amplitude_deg.to_radians();

        let quats: Vec<TimeQuaternion<f64>> = (0..n)
            .map(|i| {
                let t_ms = i as f64 * dt_ms;
                let t_s = t_ms / 1000.0;
                let angle = amplitude_rad * (2.0 * std::f64::consts::PI * freq * t_s).sin() / 2.0;
                make_tq(t_ms, angle.cos(), angle.sin(), 0.0, 0.0)
            })
            .collect();
        let result = analyze(&quats);
        assert_eq!(result.score, 100, "Score should clamp at 100");
    }

    /// Conjugate correctness: q × conj(q) ≈ identity
    #[test]
    fn test_conjugate() {
        let q = Quaternion {
            w: 0.5,
            x: 0.5,
            y: 0.5,
            z: 0.5,
        };
        let result = quat_mul(&q, &conjugate(&q));
        assert!((result.w - 1.0).abs() < 1e-10);
        assert!(result.x.abs() < 1e-10);
        assert!(result.y.abs() < 1e-10);
        assert!(result.z.abs() < 1e-10);
    }

    /// Angular displacement between identical quaternions = 0
    #[test]
    fn test_angular_displacement_zero() {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let q = Quaternion {
            w: s,
            x: s,
            y: 0.0,
            z: 0.0,
        };
        let disp = angular_displacement(&q, &q);
        assert!(disp.abs() < 1e-10);
    }

    /// Angular displacement for known rotation
    #[test]
    fn test_angular_displacement_90deg() {
        let q1 = Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // 90° rotation around Z axis
        let half_angle = std::f64::consts::FRAC_PI_4; // 45° = π/4
        let q2 = Quaternion {
            w: half_angle.cos(),
            x: 0.0,
            y: 0.0,
            z: half_angle.sin(),
        };
        let disp = angular_displacement(&q1, &q2);
        let expected = std::f64::consts::FRAC_PI_2; // 90° = π/2
        assert!(
            (disp - expected).abs() < 1e-10,
            "Expected {expected}, got {disp}"
        );
    }
}
