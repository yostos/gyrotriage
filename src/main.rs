mod analyze;
mod chart;
mod downsample;
mod error;
mod extract;
mod output;
mod recommend;
mod sparkline;
mod spectrum;
mod terminal;

use std::path::PathBuf;
use std::process;

use clap::Parser;
use telemetry_parser::tags_impl::TimeQuaternion;

use crate::downsample::{AxisSample, Sample};
use crate::error::GyroTriageError;

#[derive(Parser)]
#[command(name = "gyrotriage")]
#[command(version)]
#[command(about = "Score shake severity from DJI FPV drone MP4 and suggest Gyroflow parameters")]
struct Cli {
    /// MP4 file to analyze
    file: PathBuf,

    /// Display graph in terminal via Sixel/iTerm2
    #[arg(short = 'v', long)]
    visual: bool,

    /// Save chart as PNG image file
    #[arg(short = 'o', long, value_name = "PATH")]
    output_image: Option<PathBuf>,

    /// Append ANSI sparklines to text output
    #[arg(short = 's', long)]
    sparkline: bool,

    /// Force Sixel protocol
    #[arg(short = 'x', long)]
    sixel: bool,

    /// Force iTerm2 protocol
    #[arg(short = 'i', long)]
    iterm2: bool,

    /// Analyze only from this source timecode (HH:MM:SS:FF) or seconds
    #[arg(long = "in", value_name = "TC")]
    in_tc: Option<String>,

    /// Analyze only up to this source timecode (HH:MM:SS:FF) or seconds
    #[arg(long = "out", value_name = "TC")]
    out_tc: Option<String>,

    /// Frame rate for the :FF field of --in/--out (default: read from the MP4)
    #[arg(long, value_name = "FPS")]
    fps: Option<f64>,
}

fn main() {
    let cli = Cli::parse();

    match run(&cli) {
        Ok(()) => {}
        Err(GyroTriageError::NoMotionData { ref path, ref hint }) => {
            println!("{}", crate::output::format_no_motion_data(path, hint));
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn run(cli: &Cli) -> Result<(), GyroTriageError> {
    let data = extract::extract_quaternions(&cli.file)?;

    // Total file span (from the full, unfiltered series) for the Range readout.
    let total_secs = match (data.quaternions.first(), data.quaternions.last()) {
        (Some(f), Some(l)) => (l.t - f.t) / 1000.0,
        _ => 0.0,
    };

    // Resolve fps: explicit --fps wins, otherwise use the container-detected value.
    let fps = cli.fps.or(data.fps);

    // Parse the optional analysis range (timecode or seconds).
    let start_s = match &cli.in_tc {
        Some(s) => Some(parse_timecode(s, fps)?),
        None => None,
    };
    let end_s = match &cli.out_tc {
        Some(s) => Some(parse_timecode(s, fps)?),
        None => None,
    };
    let range_active = start_s.is_some() || end_s.is_some();

    // Restrict the analysis to the requested range, if any.
    let quaternions = filter_quaternions_by_range(data.quaternions, start_s, end_s)?;

    let result = analyze::analyze(&quaternions);
    let rec = recommend::recommend(&result);

    // Range readout line (only when a range is in effect).
    let range_line = if range_active {
        Some(format!(
            "{} – {} (of {:.1}s)",
            format_secs(start_s.unwrap_or(0.0)),
            format_secs(end_s.unwrap_or(total_secs)),
            total_secs,
        ))
    } else {
        None
    };

    // Text output
    let mut text = output::format_result(&cli.file, &result, &rec, range_line.as_deref());

    // Sparkline (only if --visual is not set)
    if cli.sparkline && !cli.visual {
        let (composite, pitch, roll, yaw) = build_velocity_series(&quaternions);
        let sparkline_output = sparkline::generate(&composite, &pitch, &roll, &yaw, 60);
        text.push_str(&sparkline::format_sparklines(&sparkline_output));
    }

    println!("{text}");

    // Chart generation (--output-image and/or --visual)
    if cli.output_image.is_some() || cli.visual {
        let (composite_samples, axis_samples) = build_chart_samples(&quaternions);
        let filename = cli
            .file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cli.file.to_string_lossy().to_string());
        let chart_data = chart::prepare_data(
            &filename,
            &result,
            &rec,
            &composite_samples,
            &axis_samples,
        );

        if let Some(ref path) = cli.output_image {
            chart::render_to_file(&chart_data, path)
                .map_err(|e| GyroTriageError::ChartError(e.to_string()))?;
            eprintln!("Chart saved to {}", path.display());
        }

        if cli.visual {
            let png = chart::render_to_png(&chart_data)
                .map_err(|e| GyroTriageError::ChartError(e.to_string()))?;
            let protocol = terminal::detect_protocol(cli.sixel, cli.iterm2)
                .map_err(|e| GyroTriageError::ChartError(e.to_string()))?;
            terminal::display_image(&png, protocol)
                .map_err(|e| GyroTriageError::ChartError(e.to_string()))?;
        }
    }

    Ok(())
}

/// Format seconds as `HH:MM:SS.ss` for the Range readout.
fn format_secs(t: f64) -> String {
    let total = t.max(0.0);
    let h = (total / 3600.0).floor() as u64;
    let m = ((total % 3600.0) / 60.0).floor() as u64;
    let s = total % 60.0;
    format!("{h:02}:{m:02}:{s:05.2}")
}

/// Parse a timecode / seconds string into seconds.
///
/// Accepts `HH:MM:SS:FF`, `HH:MM:SS`, `MM:SS`, or a plain (fractional) seconds value.
/// `fps` is required only when a frame field (`:FF`) is present; if it is `None` and a
/// frame field is given, an [`GyroTriageError::FpsRequired`] is returned.
fn parse_timecode(s: &str, fps: Option<f64>) -> Result<f64, GyroTriageError> {
    let s = s.trim();

    // Plain (fractional) seconds — no colon.
    if !s.contains(':') {
        return s
            .parse::<f64>()
            .map_err(|_| GyroTriageError::InvalidTimecode(s.to_string()));
    }

    // Colon-separated timecode.
    let mut nums = Vec::with_capacity(4);
    for part in s.split(':') {
        let v = part
            .parse::<f64>()
            .map_err(|_| GyroTriageError::InvalidTimecode(s.to_string()))?;
        nums.push(v);
    }

    let secs = match nums.as_slice() {
        [mm, ss] => mm * 60.0 + ss,
        [hh, mm, ss] => hh * 3600.0 + mm * 60.0 + ss,
        [hh, mm, ss, ff] => {
            let base = hh * 3600.0 + mm * 60.0 + ss;
            if *ff == 0.0 {
                base
            } else {
                let fps = fps.ok_or_else(|| GyroTriageError::FpsRequired(s.to_string()))?;
                base + ff / fps
            }
        }
        _ => return Err(GyroTriageError::InvalidTimecode(s.to_string())),
    };

    Ok(secs)
}

/// Filter quaternions to the `[start_s, end_s]` window, where bounds are seconds
/// relative to the first sample. `None` bounds are open-ended. Returns the filtered
/// samples, or an error if the range is inverted or yields fewer than 2 samples.
fn filter_quaternions_by_range(
    quaternions: Vec<TimeQuaternion<f64>>,
    start_s: Option<f64>,
    end_s: Option<f64>,
) -> Result<Vec<TimeQuaternion<f64>>, GyroTriageError> {
    if start_s.is_none() && end_s.is_none() {
        return Ok(quaternions);
    }

    if let (Some(a), Some(b)) = (start_s, end_s) {
        if a >= b {
            return Err(GyroTriageError::InvalidRange { start: a, end: b });
        }
    }

    // Bounds are seconds relative to the first sample; timestamps are milliseconds.
    // A small tolerance avoids dropping boundary samples to float rounding.
    const TOL_MS: f64 = 1e-3;
    let base = quaternions.first().map(|q| q.t).unwrap_or(0.0);
    let lo = start_s.map(|s| base + s * 1000.0).unwrap_or(f64::NEG_INFINITY) - TOL_MS;
    let hi = end_s.map(|s| base + s * 1000.0).unwrap_or(f64::INFINITY) + TOL_MS;

    let filtered: Vec<TimeQuaternion<f64>> = quaternions
        .into_iter()
        .filter(|q| q.t >= lo && q.t <= hi)
        .collect();

    if filtered.len() < 2 {
        return Err(GyroTriageError::InsufficientData {
            count: filtered.len(),
        });
    }

    Ok(filtered)
}

/// Build per-axis velocity series from quaternion data for sparklines.
fn build_velocity_series(quaternions: &[telemetry_parser::tags_impl::TimeQuaternion<f64>]) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut composite = Vec::with_capacity(quaternions.len());
    let mut pitch = Vec::with_capacity(quaternions.len());
    let mut roll = Vec::with_capacity(quaternions.len());
    let mut yaw = Vec::with_capacity(quaternions.len());

    for i in 0..quaternions.len().saturating_sub(1) {
        let q1 = &quaternions[i].v;
        let q2 = &quaternions[i + 1].v;
        let dt_ms = quaternions[i + 1].t - quaternions[i].t;
        let dt = dt_ms / 1000.0;
        if dt <= 0.0 {
            continue;
        }

        let theta = angular_displacement(q1, q2);
        composite.push(theta.to_degrees() / dt);

        let (p, r, y) = decompose_to_euler(q1, q2);
        pitch.push(p.to_degrees().abs() / dt);
        roll.push(r.to_degrees().abs() / dt);
        yaw.push(y.to_degrees().abs() / dt);
    }

    (composite, pitch, roll, yaw)
}

/// Build timestamped samples for chart rendering.
fn build_chart_samples(
    quaternions: &[telemetry_parser::tags_impl::TimeQuaternion<f64>],
) -> (Vec<Sample>, Vec<AxisSample>) {
    let t_start = if quaternions.is_empty() {
        0.0
    } else {
        quaternions[0].t
    };

    let mut composite = Vec::with_capacity(quaternions.len());
    let mut axes = Vec::with_capacity(quaternions.len());

    for i in 0..quaternions.len().saturating_sub(1) {
        let q1 = &quaternions[i].v;
        let q2 = &quaternions[i + 1].v;
        let dt_ms = quaternions[i + 1].t - quaternions[i].t;
        let dt = dt_ms / 1000.0;
        if dt <= 0.0 {
            continue;
        }

        let t_secs = (quaternions[i].t - t_start) / 1000.0;
        let theta = angular_displacement(q1, q2);
        let omega = theta.to_degrees() / dt;

        composite.push(Sample {
            time_secs: t_secs,
            velocity: omega,
        });

        let (p, r, y) = decompose_to_euler(q1, q2);
        axes.push(AxisSample {
            time_secs: t_secs,
            pitch: p.to_degrees().abs() / dt,
            roll: r.to_degrees().abs() / dt,
            yaw: y.to_degrees().abs() / dt,
        });
    }

    (composite, axes)
}

// Re-use quaternion math from analyze.rs (extracted here to avoid circular deps)
fn conjugate(q: &telemetry_parser::tags_impl::Quaternion<f64>) -> telemetry_parser::tags_impl::Quaternion<f64> {
    telemetry_parser::tags_impl::Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    }
}

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

fn angular_displacement(
    q1: &telemetry_parser::tags_impl::Quaternion<f64>,
    q2: &telemetry_parser::tags_impl::Quaternion<f64>,
) -> f64 {
    let q_diff = quat_mul(&conjugate(q1), q2);
    let w_clamped = q_diff.w.abs().min(1.0);
    2.0 * w_clamped.acos()
}

fn decompose_to_euler(
    q1: &telemetry_parser::tags_impl::Quaternion<f64>,
    q2: &telemetry_parser::tags_impl::Quaternion<f64>,
) -> (f64, f64, f64) {
    let q_diff = quat_mul(&conjugate(q1), q2);
    let w = q_diff.w;
    let x = q_diff.x;
    let y = q_diff.y;
    let z = q_diff.z;

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

#[cfg(test)]
mod tests {
    use super::*;
    use telemetry_parser::tags_impl::Quaternion;

    const EPS: f64 = 1e-9;

    fn identity_quat() -> Quaternion<f64> {
        Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    /// Build N samples spaced `step_ms` apart starting at `start_ms`.
    fn samples(start_ms: f64, step_ms: f64, n: usize) -> Vec<TimeQuaternion<f64>> {
        (0..n)
            .map(|i| TimeQuaternion {
                t: start_ms + step_ms * i as f64,
                v: identity_quat(),
            })
            .collect()
    }

    // ----- parse_timecode -----

    #[test]
    fn parse_plain_seconds() {
        assert!((parse_timecode("18.5", None).unwrap() - 18.5).abs() < EPS);
        assert!((parse_timecode("0", None).unwrap() - 0.0).abs() < EPS);
    }

    #[test]
    fn parse_mm_ss() {
        assert!((parse_timecode("01:02", None).unwrap() - 62.0).abs() < EPS);
    }

    #[test]
    fn parse_hh_mm_ss() {
        assert!((parse_timecode("01:02:03", None).unwrap() - 3723.0).abs() < EPS);
    }

    #[test]
    fn parse_timecode_with_frames() {
        // 18s + 7 frames @ 60fps = 18 + 7/60
        let got = parse_timecode("00:00:18:07", Some(60.0)).unwrap();
        assert!((got - (18.0 + 7.0 / 60.0)).abs() < EPS);
        // same frames @ 30fps differ
        let got30 = parse_timecode("00:00:18:07", Some(30.0)).unwrap();
        assert!((got30 - (18.0 + 7.0 / 30.0)).abs() < EPS);
    }

    #[test]
    fn parse_frames_without_fps_errors() {
        let err = parse_timecode("00:00:18:07", None).unwrap_err();
        assert!(matches!(err, GyroTriageError::FpsRequired(_)));
    }

    #[test]
    fn parse_frames_zero_field_needs_no_fps() {
        // :00 frames contributes nothing, so fps is not required
        let got = parse_timecode("00:00:18:00", None).unwrap();
        assert!((got - 18.0).abs() < EPS);
    }

    #[test]
    fn parse_invalid_text_errors() {
        assert!(matches!(
            parse_timecode("abc", None).unwrap_err(),
            GyroTriageError::InvalidTimecode(_)
        ));
    }

    #[test]
    fn parse_too_many_fields_errors() {
        assert!(matches!(
            parse_timecode("00:00:18:07:99", Some(60.0)).unwrap_err(),
            GyroTriageError::InvalidTimecode(_)
        ));
    }

    // ----- filter_quaternions_by_range -----

    #[test]
    fn filter_none_returns_all() {
        let s = samples(0.0, 100.0, 11); // t = 0..1000ms
        let out = filter_quaternions_by_range(s.clone(), None, None).unwrap();
        assert_eq!(out.len(), s.len());
    }

    #[test]
    fn filter_inclusive_window() {
        let s = samples(0.0, 100.0, 11); // t = 0,100,...,1000
        // [0.2s, 0.5s] -> 200,300,400,500 = 4 samples
        let out = filter_quaternions_by_range(s, Some(0.2), Some(0.5)).unwrap();
        assert_eq!(out.len(), 4);
        assert!((out.first().unwrap().t - 200.0).abs() < EPS);
        assert!((out.last().unwrap().t - 500.0).abs() < EPS);
    }

    #[test]
    fn filter_relative_to_first_sample() {
        // first sample at 1000ms; range is relative to it
        let s = samples(1000.0, 100.0, 11); // t = 1000..2000
        // [0.0s, 0.2s] relative -> absolute 1000..1200 -> 1000,1100,1200
        let out = filter_quaternions_by_range(s, Some(0.0), Some(0.2)).unwrap();
        assert_eq!(out.len(), 3);
        assert!((out.first().unwrap().t - 1000.0).abs() < EPS);
    }

    #[test]
    fn filter_open_start() {
        let s = samples(0.0, 100.0, 11);
        let out = filter_quaternions_by_range(s, None, Some(0.2)).unwrap();
        // 0,100,200 = 3 samples
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn filter_inverted_range_errors() {
        let s = samples(0.0, 100.0, 11);
        assert!(matches!(
            filter_quaternions_by_range(s, Some(0.5), Some(0.2)).unwrap_err(),
            GyroTriageError::InvalidRange { .. }
        ));
    }

    #[test]
    fn filter_too_few_samples_errors() {
        let s = samples(0.0, 100.0, 11);
        // window catching a single sample (only t=500)
        assert!(matches!(
            filter_quaternions_by_range(s, Some(0.46), Some(0.54)).unwrap_err(),
            GyroTriageError::InsufficientData { .. }
        ));
    }
}
