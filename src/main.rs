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
    let result = analyze::analyze(&data.quaternions);
    let rec = recommend::recommend(&result);

    // Text output
    let mut text = output::format_result(&cli.file, &result, &rec);

    // Sparkline (only if --visual is not set)
    if cli.sparkline && !cli.visual {
        let (composite, pitch, roll, yaw) = build_velocity_series(&data.quaternions);
        let sparkline_output = sparkline::generate(&composite, &pitch, &roll, &yaw, 60);
        text.push_str(&sparkline::format_sparklines(&sparkline_output));
    }

    println!("{text}");

    // Chart generation (--output-image and/or --visual)
    if cli.output_image.is_some() || cli.visual {
        let (composite_samples, axis_samples) = build_chart_samples(&data.quaternions);
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
