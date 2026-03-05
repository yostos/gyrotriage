//! Downsample a time series of angular velocities into fixed-width bins using RMS aggregation.
//!
//! Each bin covers an equal time span. The RMS of all samples within each bin
//! is computed as the representative value.

/// A single downsampled bin.
#[derive(Debug, Clone)]
pub struct Bin {
    /// RMS angular velocity (°/s) of samples within this bin.
    pub rms: f64,
}

/// Per-axis downsampled bin.
#[derive(Debug, Clone)]
pub struct AxisBin {
    pub pitch_rms: f64,
    pub roll_rms: f64,
    pub yaw_rms: f64,
}

/// A timestamped angular velocity sample.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Timestamp in seconds.
    pub time_secs: f64,
    /// Angular velocity (°/s).
    pub velocity: f64,
}

/// A timestamped per-axis angular velocity sample.
#[derive(Debug, Clone)]
pub struct AxisSample {
    pub time_secs: f64,
    pub pitch: f64,
    pub roll: f64,
    pub yaw: f64,
}

/// Downsample composite angular velocity into `num_bins` equal-time bins.
///
/// Returns empty Vec if `samples` is empty or `num_bins` is 0.
pub fn downsample(samples: &[Sample], num_bins: usize) -> Vec<Bin> {
    if samples.is_empty() || num_bins == 0 {
        return Vec::new();
    }

    let t_start = samples.first().unwrap().time_secs;
    let t_end = samples.last().unwrap().time_secs;
    let duration = t_end - t_start;

    if duration <= 0.0 {
        return vec![Bin {
            rms: rms_of(samples.iter().map(|s| s.velocity)),
        }];
    }

    let bin_width = duration / num_bins as f64;
    let mut bins = Vec::with_capacity(num_bins);
    let mut sample_idx = 0;

    for i in 0..num_bins {
        let bin_end = if i == num_bins - 1 {
            t_end + f64::EPSILON // Include last sample
        } else {
            t_start + (i + 1) as f64 * bin_width
        };

        let mut values = Vec::new();
        while sample_idx < samples.len() && samples[sample_idx].time_secs < bin_end {
            values.push(samples[sample_idx].velocity);
            sample_idx += 1;
        }

        bins.push(Bin {
            rms: if values.is_empty() {
                0.0
            } else {
                rms_of(values.into_iter())
            },
        });
    }

    bins
}

/// Downsample per-axis angular velocities into `num_bins` equal-time bins.
pub fn downsample_axes(samples: &[AxisSample], num_bins: usize) -> Vec<AxisBin> {
    if samples.is_empty() || num_bins == 0 {
        return Vec::new();
    }

    let t_start = samples.first().unwrap().time_secs;
    let t_end = samples.last().unwrap().time_secs;
    let duration = t_end - t_start;

    if duration <= 0.0 {
        return vec![AxisBin {
            pitch_rms: rms_of(samples.iter().map(|s| s.pitch)),
            roll_rms: rms_of(samples.iter().map(|s| s.roll)),
            yaw_rms: rms_of(samples.iter().map(|s| s.yaw)),
        }];
    }

    let bin_width = duration / num_bins as f64;
    let mut bins = Vec::with_capacity(num_bins);
    let mut sample_idx = 0;

    for i in 0..num_bins {
        let bin_end = if i == num_bins - 1 {
            t_end + f64::EPSILON
        } else {
            t_start + (i + 1) as f64 * bin_width
        };

        let mut pitches = Vec::new();
        let mut rolls = Vec::new();
        let mut yaws = Vec::new();

        while sample_idx < samples.len() && samples[sample_idx].time_secs < bin_end {
            pitches.push(samples[sample_idx].pitch);
            rolls.push(samples[sample_idx].roll);
            yaws.push(samples[sample_idx].yaw);
            sample_idx += 1;
        }

        bins.push(AxisBin {
            pitch_rms: if pitches.is_empty() { 0.0 } else { rms_of(pitches.into_iter()) },
            roll_rms: if rolls.is_empty() { 0.0 } else { rms_of(rolls.into_iter()) },
            yaw_rms: if yaws.is_empty() { 0.0 } else { rms_of(yaws.into_iter()) },
        });
    }

    bins
}

fn rms_of(iter: impl Iterator<Item = f64>) -> f64 {
    let mut sum_sq = 0.0;
    let mut count = 0u64;
    for v in iter {
        sum_sq += v * v;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        (sum_sq / count as f64).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_samples(velocities: &[(f64, f64)]) -> Vec<Sample> {
        velocities
            .iter()
            .map(|&(t, v)| Sample {
                time_secs: t,
                velocity: v,
            })
            .collect()
    }

    // --- downsample() ---

    #[test]
    fn test_empty_input() {
        let result = downsample(&[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_zero_bins() {
        let samples = make_samples(&[(0.0, 5.0), (1.0, 10.0)]);
        let result = downsample(&samples, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_correct_bin_count() {
        let samples: Vec<Sample> = (0..1000)
            .map(|i| Sample {
                time_secs: i as f64 * 0.001,
                velocity: 10.0,
            })
            .collect();
        let bins = downsample(&samples, 50);
        assert_eq!(bins.len(), 50);
    }

    #[test]
    fn test_uniform_values_rms_equals_value() {
        let samples: Vec<Sample> = (0..1000)
            .map(|i| Sample {
                time_secs: i as f64 * 0.001,
                velocity: 7.0,
            })
            .collect();
        let bins = downsample(&samples, 10);
        for bin in &bins {
            assert!(
                (bin.rms - 7.0).abs() < 0.1,
                "Expected RMS ~7.0, got {}",
                bin.rms
            );
        }
    }

    #[test]
    fn test_bins_cover_full_range() {
        let samples = make_samples(&[(0.0, 1.0), (5.0, 2.0), (10.0, 3.0)]);
        let bins = downsample(&samples, 5);
        assert_eq!(bins.len(), 5);
        // All bins should have some structure
        assert!(bins.iter().any(|b| b.rms > 0.0));
    }

    #[test]
    fn test_single_sample_returns_single_bin() {
        let samples = make_samples(&[(1.0, 5.0)]);
        let bins = downsample(&samples, 10);
        // Duration is 0, should return 1 bin
        assert_eq!(bins.len(), 1);
        assert!((bins[0].rms - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_more_bins_than_samples() {
        let samples = make_samples(&[(0.0, 3.0), (1.0, 6.0), (2.0, 9.0)]);
        let bins = downsample(&samples, 100);
        assert_eq!(bins.len(), 100);
        // Most bins will be empty (rms=0), but total non-zero bins should contain all samples
        let non_zero: Vec<_> = bins.iter().filter(|b| b.rms > 0.0).collect();
        assert!(!non_zero.is_empty());
    }

    #[test]
    fn test_rms_correctness() {
        // Two samples: 3.0 and 4.0. RMS = sqrt((9+16)/2) = sqrt(12.5) ≈ 3.536
        let samples = make_samples(&[(0.0, 3.0), (1.0, 4.0)]);
        let bins = downsample(&samples, 1);
        let expected = (12.5_f64).sqrt();
        assert!(
            (bins[0].rms - expected).abs() < 0.001,
            "Expected RMS {}, got {}",
            expected,
            bins[0].rms
        );
    }

    // --- downsample_axes() ---

    #[test]
    fn test_axes_empty_input() {
        let result = downsample_axes(&[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_axes_correct_bin_count() {
        let samples: Vec<AxisSample> = (0..100)
            .map(|i| AxisSample {
                time_secs: i as f64 * 0.01,
                pitch: 1.0,
                roll: 2.0,
                yaw: 3.0,
            })
            .collect();
        let bins = downsample_axes(&samples, 10);
        assert_eq!(bins.len(), 10);
    }

    #[test]
    fn test_axes_uniform_values() {
        let samples: Vec<AxisSample> = (0..100)
            .map(|i| AxisSample {
                time_secs: i as f64 * 0.01,
                pitch: 2.0,
                roll: 5.0,
                yaw: 8.0,
            })
            .collect();
        let bins = downsample_axes(&samples, 5);
        for bin in &bins {
            assert!((bin.pitch_rms - 2.0).abs() < 0.1);
            assert!((bin.roll_rms - 5.0).abs() < 0.1);
            assert!((bin.yaw_rms - 8.0).abs() < 0.1);
        }
    }

}
