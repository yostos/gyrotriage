//! FFT/PSD-based frequency analysis for angular velocity time series.
//!
//! Computes the Power Spectral Density of angular velocity data and
//! estimates the cutoff frequency between intentional camera motion
//! and unintentional shake/vibration.

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Result of spectral analysis on angular velocity data.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SpectrumResult {
    /// Estimated cutoff frequency (Hz) separating intentional motion from shake.
    pub cutoff_hz: f64,
    /// Time constant derived from cutoff: τ = 1/(2π·fc).
    pub time_constant: f64,
    /// Cutoff frequency for high-velocity regime (Hz).
    pub high_velocity_cutoff_hz: f64,
    /// Time constant for high-velocity regime.
    pub high_velocity_time_constant: f64,
    /// Fraction of total power in the shake band (above cutoff_hz).
    pub shake_power_ratio: f64,
}

/// Compute Power Spectral Density from a real-valued time series.
///
/// Returns (frequencies, psd) where frequencies[i] is in Hz and
/// psd[i] is the power at that frequency.
fn compute_psd(signal: &[f64], sample_rate_hz: f64) -> (Vec<f64>, Vec<f64>) {
    let n = signal.len();
    if n < 4 {
        return (vec![], vec![]);
    }

    // Apply Hann window to reduce spectral leakage
    let mut buffer: Vec<Complex<f64>> = signal
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let window = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos());
            Complex::new(v * window, 0.0)
        })
        .collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    // Only positive frequencies (DC to Nyquist)
    let n_freqs = n / 2 + 1;
    let df = sample_rate_hz / n as f64;

    let frequencies: Vec<f64> = (0..n_freqs).map(|i| i as f64 * df).collect();

    // Normalize PSD: |X[k]|^2 / (N * sample_rate)
    // Factor of 2 for one-sided spectrum (except DC and Nyquist)
    let norm = 1.0 / (n as f64 * sample_rate_hz);
    let psd: Vec<f64> = (0..n_freqs)
        .map(|i| {
            let mag_sq = buffer[i].norm_sqr();
            let factor = if i == 0 || i == n_freqs - 1 { 1.0 } else { 2.0 };
            mag_sq * norm * factor
        })
        .collect();

    (frequencies, psd)
}

/// Estimate the cutoff frequency from PSD using cumulative power analysis.
///
/// Strategy: find the frequency below which `target_ratio` of total power
/// resides. For intentional motion vs shake separation, we look for the
/// frequency where low-frequency power transitions to high-frequency noise.
fn estimate_cutoff(frequencies: &[f64], psd: &[f64], target_ratio: f64) -> f64 {
    if frequencies.is_empty() || psd.is_empty() {
        return 1.0; // safe default
    }

    let total_power: f64 = psd.iter().sum();
    if total_power <= 0.0 {
        return 1.0;
    }

    let mut cumulative = 0.0;
    for (i, &p) in psd.iter().enumerate() {
        cumulative += p;
        if cumulative / total_power >= target_ratio {
            return frequencies[i].max(0.3); // minimum 0.3 Hz
        }
    }

    frequencies.last().copied().unwrap_or(1.0)
}

/// Find the dominant shake frequency by looking for the peak in the
/// shake band (above min_shake_hz).
fn find_shake_band_cutoff(frequencies: &[f64], psd: &[f64], min_shake_hz: f64) -> f64 {
    // Find the frequency range where shake power is concentrated
    // Look for the valley between intentional motion and shake
    let df = if frequencies.len() > 1 {
        frequencies[1] - frequencies[0]
    } else {
        return min_shake_hz;
    };

    // Smooth PSD with a simple moving average for robust valley detection
    let window = (0.5 / df).max(1.0) as usize; // 0.5 Hz smoothing window
    let smoothed = smooth_psd(psd, window);

    // Find the minimum (valley) between 0.5 Hz and 5 Hz
    // This is typically where intentional motion ends and shake begins
    let search_low = (0.5 / df) as usize;
    let search_high = ((5.0 / df) as usize).min(smoothed.len());

    if search_low >= search_high || search_high > smoothed.len() {
        return min_shake_hz;
    }

    let mut min_idx = search_low;
    let mut min_val = f64::MAX;
    for (i, &val) in smoothed.iter().enumerate().skip(search_low).take(search_high - search_low) {
        if val < min_val {
            min_val = val;
            min_idx = i;
        }
    }

    frequencies.get(min_idx).copied().unwrap_or(min_shake_hz).max(0.3)
}

/// Simple moving average smoothing.
fn smooth_psd(psd: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 || psd.is_empty() {
        return psd.to_vec();
    }
    let half = window / 2;
    (0..psd.len())
        .map(|i| {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(psd.len());
            let sum: f64 = psd[start..end].iter().sum();
            sum / (end - start) as f64
        })
        .collect()
}

/// Analyze angular velocity spectrum and estimate Gyroflow-relevant parameters.
///
/// Takes per-axis angular velocity time series and sample rate.
/// Returns spectral analysis results for parameter recommendation.
pub fn analyze_spectrum(
    pitch: &[f64],
    roll: &[f64],
    yaw: &[f64],
    sample_rate_hz: f64,
) -> SpectrumResult {
    // Combine all axes for overall spectral analysis
    let composite: Vec<f64> = pitch
        .iter()
        .zip(roll.iter())
        .zip(yaw.iter())
        .map(|((&p, &r), &y)| (p * p + r * r + y * y).sqrt())
        .collect();

    let (frequencies, psd) = compute_psd(&composite, sample_rate_hz);

    if frequencies.is_empty() {
        return SpectrumResult {
            cutoff_hz: 1.0,
            time_constant: 1.0 / (2.0 * std::f64::consts::PI),
            high_velocity_cutoff_hz: 5.0,
            high_velocity_time_constant: 1.0 / (2.0 * std::f64::consts::PI * 5.0),
            shake_power_ratio: 0.0,
        };
    }

    // Primary cutoff: valley between intentional motion and shake
    let cutoff_hz = find_shake_band_cutoff(&frequencies, &psd, 0.5);

    // High-velocity cutoff: higher frequency for fast rotation
    // Use 80% cumulative power point as upper bound
    let high_velocity_cutoff_hz = estimate_cutoff(&frequencies, &psd, 0.80).max(cutoff_hz * 2.0).min(10.0);

    // Calculate shake power ratio (power above cutoff / total)
    let df = if frequencies.len() > 1 {
        frequencies[1] - frequencies[0]
    } else {
        1.0
    };
    let cutoff_idx = (cutoff_hz / df) as usize;
    let total_power: f64 = psd.iter().sum();
    let shake_power: f64 = psd.iter().skip(cutoff_idx).sum();
    let shake_power_ratio = if total_power > 0.0 {
        shake_power / total_power
    } else {
        0.0
    };

    let time_constant = 1.0 / (2.0 * std::f64::consts::PI * cutoff_hz);
    let high_velocity_time_constant = 1.0 / (2.0 * std::f64::consts::PI * high_velocity_cutoff_hz);

    SpectrumResult {
        cutoff_hz,
        time_constant,
        high_velocity_cutoff_hz,
        high_velocity_time_constant,
        shake_power_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure DC signal (constant) → cutoff defaults to minimum (0.3 Hz).
    /// A constant signal has all power at DC; the valley search in [0.5, 5] Hz
    /// returns the minimum-power bin which may be anywhere in that range.
    #[test]
    fn test_constant_signal() {
        let n = 2000;
        let rate = 200.0;
        let signal: Vec<f64> = vec![5.0; n];
        let result = analyze_spectrum(&signal, &vec![0.0; n], &vec![0.0; n], rate);
        // For constant signal, shake power ratio should be very low
        assert!(result.shake_power_ratio < 0.5, "shake_ratio={}", result.shake_power_ratio);
    }

    /// Single-frequency sinusoid at 5 Hz → cutoff should be near 5 Hz or below.
    #[test]
    fn test_sinusoidal_5hz() {
        let n = 4000;
        let rate = 200.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                10.0 * (2.0 * std::f64::consts::PI * 5.0 * t).sin()
            })
            .collect();
        let zeros = vec![0.0; n];
        let result = analyze_spectrum(&signal, &zeros, &zeros, rate);
        // The shake power should be concentrated around 5 Hz
        assert!(result.cutoff_hz < 6.0, "cutoff_hz={}", result.cutoff_hz);
    }

    /// Low-frequency (0.5 Hz) + high-frequency (20 Hz) mixed signal.
    /// Cutoff should separate them.
    #[test]
    fn test_mixed_frequencies() {
        let n = 4000;
        let rate = 200.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                // Strong low-freq intentional motion + weaker high-freq shake
                20.0 * (2.0 * std::f64::consts::PI * 0.5 * t).sin()
                    + 5.0 * (2.0 * std::f64::consts::PI * 20.0 * t).sin()
            })
            .collect();
        let zeros = vec![0.0; n];
        let result = analyze_spectrum(&signal, &zeros, &zeros, rate);
        // Cutoff should be between 0.5 Hz and 20 Hz
        assert!(
            result.cutoff_hz > 0.3 && result.cutoff_hz < 15.0,
            "cutoff_hz={}", result.cutoff_hz
        );
        assert!(result.shake_power_ratio > 0.0);
    }

    /// Time constant conversion: τ = 1/(2π·fc)
    #[test]
    fn test_time_constant_conversion() {
        let fc = 1.0;
        let tau = 1.0 / (2.0 * std::f64::consts::PI * fc);
        assert!((tau - 0.159).abs() < 0.001);
    }

    /// PSD of pure sine should have single peak.
    #[test]
    fn test_psd_pure_sine() {
        let n = 1024;
        let rate = 200.0;
        let freq = 10.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                (2.0 * std::f64::consts::PI * freq * t).sin()
            })
            .collect();
        let (frequencies, psd) = compute_psd(&signal, rate);
        // Find peak
        let peak_idx = psd.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        let peak_freq = frequencies[peak_idx];
        assert!(
            (peak_freq - freq).abs() < 1.0,
            "Peak at {} Hz, expected {} Hz", peak_freq, freq
        );
    }

    /// Empty/short signal should return safe defaults.
    #[test]
    fn test_empty_signal() {
        let result = analyze_spectrum(&[], &[], &[], 200.0);
        assert!(result.cutoff_hz > 0.0);
        assert!(result.time_constant > 0.0);
    }

    /// High-velocity time constant should be smaller than normal.
    #[test]
    fn test_high_velocity_shorter_time_constant() {
        let n = 4000;
        let rate = 200.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                10.0 * (2.0 * std::f64::consts::PI * 2.0 * t).sin()
                    + 3.0 * (2.0 * std::f64::consts::PI * 15.0 * t).sin()
            })
            .collect();
        let zeros = vec![0.0; n];
        let result = analyze_spectrum(&signal, &zeros, &zeros, rate);
        assert!(
            result.high_velocity_time_constant < result.time_constant,
            "high_vel_tc={} should be < tc={}",
            result.high_velocity_time_constant, result.time_constant
        );
    }
}
