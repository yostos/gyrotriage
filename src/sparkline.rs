//! ANSI sparkline generation for `--sparkline` output.
//!
//! Renders angular velocity time series as Unicode block characters (▁▂▃▄▅▆▇█)
//! with ANSI color codes.

/// Block characters from lowest to highest.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// ANSI color codes (Tokyo Night cool-tone palette).
const COLOR_COMPOSITE: &str = "\x1b[38;2;125;207;255m"; // #7dcfff cyan
const COLOR_PITCH: &str = "\x1b[38;2;90;130;240m"; // #5a82f0 blue
const COLOR_ROLL: &str = "\x1b[38;2;60;200;180m"; // #3cc8b4 teal
const COLOR_YAW: &str = "\x1b[38;2;180;120;255m"; // #b478ff purple
const COLOR_RESET: &str = "\x1b[0m";

/// Label width including trailing spaces (e.g., "Shake:  ").
const LABEL_WIDTH: usize = 8;

/// A set of sparkline rows for all axes.
pub struct SparklineOutput {
    pub composite: String,
    pub pitch: String,
    pub roll: String,
    pub yaw: String,
}

/// Quantize a value into one of 8 block characters.
///
/// `value` is clamped to [0, max_value]. Returns index 0..7.
fn quantize(value: f64, max_value: f64) -> usize {
    if max_value <= 0.0 || value <= 0.0 {
        return 0;
    }
    let normalized = (value / max_value).min(1.0);
    let index = (normalized * 7.0).round() as usize;
    index.min(7)
}

/// Render a single sparkline row with a fixed max value (for cross-row comparison).
fn render_row_with_max(values: &[f64], width: usize, max_value: f64) -> String {
    if values.is_empty() || width == 0 {
        return String::new();
    }

    let binned = bin_values(values, width);

    binned
        .iter()
        .map(|&v| BLOCKS[quantize(v, max_value)])
        .collect()
}

/// Bin values into `num_bins` using RMS aggregation.
fn bin_values(values: &[f64], num_bins: usize) -> Vec<f64> {
    if values.is_empty() || num_bins == 0 {
        return Vec::new();
    }

    if values.len() <= num_bins {
        // Fewer values than bins: spread them out
        let mut result = vec![0.0; num_bins];
        for (i, &v) in values.iter().enumerate() {
            let bin_idx = (i * num_bins / values.len()).min(num_bins - 1);
            result[bin_idx] = v;
        }
        return result;
    }

    let bin_size_f = values.len() as f64 / num_bins as f64;
    let mut bins = Vec::with_capacity(num_bins);

    for i in 0..num_bins {
        let start = (i as f64 * bin_size_f) as usize;
        let end = ((i + 1) as f64 * bin_size_f) as usize;
        let end = end.min(values.len());

        if start >= end {
            bins.push(0.0);
        } else {
            let sum_sq: f64 = values[start..end].iter().map(|v| v * v).sum();
            let rms = (sum_sq / (end - start) as f64).sqrt();
            bins.push(rms);
        }
    }

    bins
}

/// Generate sparkline output for composite and per-axis data.
///
/// `terminal_width` is the total terminal character width.
/// The effective sparkline width = terminal_width - LABEL_WIDTH.
pub fn generate(
    composite: &[f64],
    pitch: &[f64],
    roll: &[f64],
    yaw: &[f64],
    terminal_width: usize,
) -> SparklineOutput {
    let width = terminal_width.saturating_sub(LABEL_WIDTH);

    // Use composite max as the shared max for all rows so they're comparable
    let binned_composite = bin_values(composite, width);
    let global_max = binned_composite.iter().cloned().fold(0.0_f64, f64::max);

    SparklineOutput {
        composite: format!(
            "{COLOR_COMPOSITE}Shake:  {}{COLOR_RESET}",
            render_row_with_max(composite, width, global_max)
        ),
        pitch: format!(
            "{COLOR_PITCH}Pitch:  {}{COLOR_RESET}",
            render_row_with_max(pitch, width, global_max)
        ),
        roll: format!(
            "{COLOR_ROLL}Roll:   {}{COLOR_RESET}",
            render_row_with_max(roll, width, global_max)
        ),
        yaw: format!(
            "{COLOR_YAW}Yaw:    {}{COLOR_RESET}",
            render_row_with_max(yaw, width, global_max)
        ),
    }
}

/// Format sparkline output as a multi-line string for appending to text output.
pub fn format_sparklines(output: &SparklineOutput) -> String {
    format!(
        "\n{}\n{}\n{}\n{}",
        output.composite, output.pitch, output.roll, output.yaw
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- quantize() ---

    #[test]
    fn test_quantize_zero() {
        assert_eq!(quantize(0.0, 10.0), 0);
    }

    #[test]
    fn test_quantize_max() {
        assert_eq!(quantize(10.0, 10.0), 7);
    }

    #[test]
    fn test_quantize_half() {
        let idx = quantize(5.0, 10.0);
        assert!(idx >= 3 && idx <= 4, "Half should be near middle, got {idx}");
    }

    #[test]
    fn test_quantize_over_max_clamps() {
        assert_eq!(quantize(20.0, 10.0), 7);
    }

    #[test]
    fn test_quantize_negative_value() {
        assert_eq!(quantize(-5.0, 10.0), 0);
    }

    #[test]
    fn test_quantize_zero_max() {
        assert_eq!(quantize(5.0, 0.0), 0);
    }

    // --- render_row_with_max() ---

    #[test]
    fn test_render_row_with_max_scales_correctly() {
        // Values are 5.0, but max is 10.0 → should be mid-level
        let values = vec![5.0; 50];
        let row = render_row_with_max(&values, 10, 10.0);
        let chars: Vec<char> = row.chars().collect();
        let idx = BLOCKS.iter().position(|&c| c == chars[0]).unwrap();
        assert!(idx >= 3 && idx <= 4, "Half of max should be mid-level, got {idx}");
    }

    #[test]
    fn test_render_row_with_max_zero_max() {
        let values = vec![5.0; 10];
        let row = render_row_with_max(&values, 10, 0.0);
        let chars: Vec<char> = row.chars().collect();
        assert!(chars.iter().all(|&c| c == '▁'));
    }

    // --- bin_values() ---

    #[test]
    fn test_bin_values_empty() {
        assert!(bin_values(&[], 10).is_empty());
    }

    #[test]
    fn test_bin_values_zero_bins() {
        assert!(bin_values(&[1.0, 2.0], 0).is_empty());
    }

    #[test]
    fn test_bin_values_correct_count() {
        let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let bins = bin_values(&values, 20);
        assert_eq!(bins.len(), 20);
    }

    #[test]
    fn test_bin_values_fewer_values_than_bins() {
        let values = vec![1.0, 2.0, 3.0];
        let bins = bin_values(&values, 10);
        assert_eq!(bins.len(), 10);
    }

    #[test]
    fn test_bin_values_uniform_rms() {
        let values = vec![4.0; 100];
        let bins = bin_values(&values, 10);
        for b in &bins {
            assert!((b - 4.0).abs() < 0.01, "Expected 4.0, got {b}");
        }
    }

    // --- generate() ---

    #[test]
    fn test_generate_contains_labels() {
        let composite = vec![10.0; 100];
        let pitch = vec![3.0; 100];
        let roll = vec![7.0; 100];
        let yaw = vec![2.0; 100];
        let output = generate(&composite, &pitch, &roll, &yaw, 60);
        assert!(output.composite.contains("Shake:"));
        assert!(output.pitch.contains("Pitch:"));
        assert!(output.roll.contains("Roll:"));
        assert!(output.yaw.contains("Yaw:"));
    }

    #[test]
    fn test_generate_contains_ansi_colors() {
        let composite = vec![10.0; 100];
        let pitch = vec![3.0; 100];
        let roll = vec![7.0; 100];
        let yaw = vec![2.0; 100];
        let output = generate(&composite, &pitch, &roll, &yaw, 60);
        assert!(output.composite.contains("\x1b[38;2;"));
        assert!(output.composite.contains("\x1b[0m"));
    }

    #[test]
    fn test_generate_empty_data() {
        let output = generate(&[], &[], &[], &[], 60);
        // Should not panic, labels still present
        assert!(output.composite.contains("Shake:"));
    }

    // --- format_sparklines() ---

    #[test]
    fn test_format_sparklines_four_lines() {
        let composite = vec![10.0; 50];
        let pitch = vec![3.0; 50];
        let roll = vec![7.0; 50];
        let yaw = vec![2.0; 50];
        let output = generate(&composite, &pitch, &roll, &yaw, 60);
        let formatted = format_sparklines(&output);
        // Leading newline + 3 internal newlines = 4 lines of content
        let lines: Vec<&str> = formatted.trim_start_matches('\n').lines().collect();
        assert_eq!(lines.len(), 4);
    }
}
