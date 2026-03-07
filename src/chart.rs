//! HUD-style chart generation for gyrotriage visual output.
//!
//! Renders a 1440×900 PNG with a 2-column HUD layout:
//! - Left: Score gauge (gradient arc) + Radar chart (3-axis)
//! - Right: 4 line graphs (Composite, Pitch, Roll, Yaw)
//! - Header: tool name + file info
//! - Footer: Gyroflow recommended parameters (MODERATE+ only)

use std::path::Path;

use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};

use crate::analyze::{AnalysisResult, Level};
use crate::downsample::{self, AxisSample, Sample};
use crate::recommend::Recommendation;

const WIDTH: u32 = 1440;
const HEIGHT: u32 = 900;

// Layout
const HEADER_H: i32 = 72;
const FOOTER_H: i32 = 60;
const LEFT_X: i32 = 15;
const LEFT_W: i32 = 450;
const RIGHT_X: i32 = 510;
const RIGHT_W: i32 = 915;
const CONTENT_TOP: i32 = 78;

// Colors (Tokyo Night, cool-tone emphasis)
const BG: RGBColor = RGBColor(26, 27, 38);
const GRID: RGBColor = RGBColor(45, 50, 75);
const GRID_DIM: RGBColor = RGBColor(35, 40, 60);
const TEXT: RGBColor = RGBColor(169, 177, 214);
const TEXT_BRIGHT: RGBColor = RGBColor(192, 202, 245);
const TEXT_DIM: RGBColor = RGBColor(86, 95, 137);
const CYAN: RGBColor = RGBColor(125, 207, 255);
const CYAN_DIM: RGBColor = RGBColor(80, 140, 180);
const BLUE: RGBColor = RGBColor(90, 130, 240);
const TEAL: RGBColor = RGBColor(60, 200, 180);
const PURPLE: RGBColor = RGBColor(180, 120, 255);
const GREEN: RGBColor = RGBColor(158, 206, 106);
const RED: RGBColor = RGBColor(247, 118, 142);
const RADAR_FILL: RGBColor = RGBColor(40, 50, 75);

const CHART_BINS: usize = 600;
const REFERENCE_MAX: f64 = 20.0; // for radar normalization

/// All data needed for chart rendering.
pub struct ChartData {
    pub filename: String,
    pub duration_secs: f64,
    pub sample_count: usize,
    pub sample_rate_hz: f64,
    pub score: u32,
    pub level: Level,
    pub rms_velocity: f64,
    pub peak_velocity: f64,
    pub pitch_rms: f64,
    pub roll_rms: f64,
    pub yaw_rms: f64,
    pub smoothness_pct: f64,
    pub max_smoothness_s: f64,
    pub max_smoothness_at_high_velocity_s: f64,
    pub zoom_limit_pct: f64,
    pub zooming_speed_s: f64,
    pub composite_bins: Vec<f64>,
    pub pitch_bins: Vec<f64>,
    pub roll_bins: Vec<f64>,
    pub yaw_bins: Vec<f64>,
}

/// Build ChartData from analysis results and raw samples.
pub fn prepare_data(
    filename: &str,
    result: &AnalysisResult,
    rec: &Recommendation,
    composite_samples: &[Sample],
    axis_samples: &[AxisSample],
) -> ChartData {
    let comp_bins = downsample::downsample(composite_samples, CHART_BINS);
    let axis_bins = downsample::downsample_axes(axis_samples, CHART_BINS);

    ChartData {
        filename: filename.to_string(),
        duration_secs: result.duration_secs,
        sample_count: result.sample_count,
        sample_rate_hz: result.sample_rate_hz,
        score: result.score,
        level: result.level,
        rms_velocity: result.rms_velocity,
        peak_velocity: result.peak_velocity,
        pitch_rms: result.pitch.avg,
        roll_rms: result.roll.avg,
        yaw_rms: result.yaw.avg,
        smoothness_pct: rec.smoothness_pct,
        max_smoothness_s: rec.max_smoothness_s,
        max_smoothness_at_high_velocity_s: rec.max_smoothness_at_high_velocity_s,
        zoom_limit_pct: rec.zoom_limit_pct,
        zooming_speed_s: rec.zooming_speed_s,
        composite_bins: comp_bins.iter().map(|b| b.rms).collect(),
        pitch_bins: axis_bins.iter().map(|b| b.pitch_rms).collect(),
        roll_bins: axis_bins.iter().map(|b| b.roll_rms).collect(),
        yaw_bins: axis_bins.iter().map(|b| b.yaw_rms).collect(),
    }
}

/// Render chart to a PNG file.
pub fn render_to_file(data: &ChartData, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let buf = render_to_png(data)?;
    std::fs::write(path, &buf)?;
    Ok(())
}

/// Render chart to PNG bytes in memory.
pub fn render_to_png(data: &ChartData) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pixel_buf = vec![0u8; (WIDTH * HEIGHT * 3) as usize];

    {
        let root =
            BitMapBackend::with_buffer(&mut pixel_buf, (WIDTH, HEIGHT)).into_drawing_area();
        root.fill(&BG)?;

        draw_header(&root, data)?;

        // Left column
        let score_bottom = CONTENT_TOP + 420;
        let radar_top = score_bottom + 12;
        let content_bottom = HEIGHT as i32 - FOOTER_H - 6;

        draw_panel_border(&root, LEFT_X, CONTENT_TOP, LEFT_X + LEFT_W, score_bottom)?;
        draw_panel_label(&root, LEFT_X + 16, CONTENT_TOP + 8, "SCORE")?;
        draw_score_gauge(&root, LEFT_X + LEFT_W / 2, CONTENT_TOP + 175, 110, data)?;

        draw_panel_border(&root, LEFT_X, radar_top, LEFT_X + LEFT_W, content_bottom)?;
        draw_panel_label(&root, LEFT_X + 16, radar_top + 8, "AXIS ANALYSIS")?;
        let radar_cy = radar_top + (content_bottom - radar_top) / 2 + 12;
        let radar_r = ((content_bottom - radar_top) / 2 - 45).min(115);
        draw_radar(&root, LEFT_X + LEFT_W / 2, radar_cy, radar_r, data)?;

        // Right column: 4 line graphs
        let graph_gap = 8;
        let total_h = content_bottom - CONTENT_TOP;
        let graph_h = (total_h - 3 * graph_gap) / 4;

        let graphs: [(&[f64], RGBColor, &str); 4] = [
            (&data.composite_bins, CYAN, "COMPOSITE"),
            (&data.pitch_bins, BLUE, "PITCH"),
            (&data.roll_bins, TEAL, "ROLL"),
            (&data.yaw_bins, PURPLE, "YAW"),
        ];

        for (i, (bins, color, label)) in graphs.iter().enumerate() {
            let gy = CONTENT_TOP + i as i32 * (graph_h + graph_gap);
            draw_panel_border(&root, RIGHT_X, gy, RIGHT_X + RIGHT_W, gy + graph_h)?;
            draw_line_graph(
                &root,
                &GraphParams { x: RIGHT_X, y: gy, w: RIGHT_W, h: graph_h, bins, color: *color, label },
            )?;
        }

        draw_footer(&root, data)?;

        root.present()?;
    }

    // Encode as PNG
    let img = image::RgbImage::from_raw(WIDTH, HEIGHT, pixel_buf)
        .ok_or("Failed to create image from pixel buffer")?;
    let mut png_buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_buf));
    image::ImageEncoder::write_image(
        encoder,
        &img,
        WIDTH,
        HEIGHT,
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(png_buf)
}

// ─── Drawing functions ──────────────────────────────────────────

type Area<'a> = DrawingArea<BitMapBackend<'a>, plotters::coord::Shift>;

fn draw_header(area: &Area, data: &ChartData) -> Result<(), Box<dyn std::error::Error>> {
    let font_title = ("monospace", 28).into_font().color(&CYAN);
    let font_ver = ("monospace", 18).into_font().color(&TEXT_DIM);
    let font_info = ("monospace", 20).into_font().color(&TEXT);
    let font_info_dim = ("monospace", 17).into_font().color(&TEXT_DIM);

    area.draw(&Text::new("GYROTRIAGE", (30, 16), font_title))?;
    let version_label = format!("v{}", env!("CARGO_PKG_VERSION"));
    area.draw(&Text::new(version_label.as_str(), (240, 22), font_ver))?;

    let right_style = font_info.pos(Pos::new(HPos::Right, VPos::Top));
    area.draw(&Text::new(
        data.filename.as_str(),
        (WIDTH as i32 - 30, 18),
        right_style,
    ))?;

    let info = format!(
        "{:.1}s  |  {} samples  |  {:.0}Hz",
        data.duration_secs, data.sample_count, data.sample_rate_hz
    );
    let right_dim = font_info_dim.pos(Pos::new(HPos::Right, VPos::Top));
    area.draw(&Text::new(info.as_str(), (WIDTH as i32 - 30, 42), right_dim))?;

    // Separator line
    area.draw(&PathElement::new(
        vec![(30, HEADER_H - 6), (WIDTH as i32 - 30, HEADER_H - 6)],
        GRID.stroke_width(1),
    ))?;

    Ok(())
}

fn draw_panel_border(
    area: &Area,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Rectangle outline
    area.draw(&Rectangle::new([(x0, y0), (x1, y1)], GRID.stroke_width(1)))?;

    // Corner accents
    let len = 15;
    for &(cx, cy, dx, dy) in &[
        (x0, y0, 1, 1),
        (x1, y0, -1, 1),
        (x0, y1, 1, -1),
        (x1, y1, -1, -1),
    ] {
        area.draw(&PathElement::new(
            vec![(cx, cy), (cx + len * dx, cy)],
            CYAN_DIM.stroke_width(2),
        ))?;
        area.draw(&PathElement::new(
            vec![(cx, cy), (cx, cy + len * dy)],
            CYAN_DIM.stroke_width(2),
        ))?;
    }

    Ok(())
}

fn draw_panel_label(
    area: &Area,
    x: i32,
    y: i32,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let font = ("monospace", 18).into_font().color(&TEXT_DIM);
    area.draw(&Text::new(label, (x, y), font))?;
    Ok(())
}

fn draw_score_gauge(
    area: &Area,
    cx: i32,
    cy: i32,
    r: i32,
    data: &ChartData,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_angle: f64 = 135.0;
    let end_angle: f64 = 405.0;
    let total_span = end_angle - start_angle;

    // Background arc
    draw_arc(area, &ArcParams { cx, cy, r, start_deg: start_angle, end_deg: end_angle, color: GRID_DIM, width: 4 })?;

    // Gradient arc (score portion)
    let score_end = start_angle + total_span * data.score as f64 / 100.0;
    let score_span = score_end - start_angle;

    if score_span > 0.0 {
        // Glow pass: single smooth arc (avoids segment joint artifacts)
        let mid_color = score_color_at(0.5_f64.min(data.score as f64 / 100.0));
        let glow = dim_color(mid_color, 3);
        draw_arc(area, &ArcParams { cx, cy, r, start_deg: start_angle, end_deg: score_end, color: glow, width: 10 })?;

        // Gradient segments on top
        let step = 1.0_f64;
        let mut a = start_angle;
        while a < score_end {
            let seg_end = (a + step + 0.5).min(score_end); // slight overlap to avoid gaps
            let t = (a - start_angle) / score_span;
            let color = score_color_at(t);
            draw_arc(area, &ArcParams { cx, cy, r, start_deg: a, end_deg: seg_end, color, width: 5 })?;
            a += step;
        }
    }

    // Tick marks
    for i in [0, 25, 50, 75, 100] {
        let angle = start_angle + total_span * i as f64 / 100.0;
        let p1 = arc_point(cx as f64, cy as f64, (r - 10) as f64, angle);
        let p2 = arc_point(cx as f64, cy as f64, (r + 5) as f64, angle);
        area.draw(&PathElement::new(vec![p1, p2], TEXT_DIM.stroke_width(1)))?;
    }

    // Score number
    let end_color = score_color_at(data.score as f64 / 100.0);
    let font_score = ("monospace", 72).into_font().color(&TEXT_BRIGHT);
    let font_level = ("monospace", 28).into_font().color(&end_color);
    let font_detail = ("monospace", 18).into_font().color(&TEXT_DIM);
    let center = Pos::new(HPos::Center, VPos::Center);

    area.draw(&Text::new(
        format!("{}", data.score).as_str(),
        (cx, cy - 20),
        font_score.pos(center),
    ))?;
    area.draw(&Text::new(
        format!("{}", data.level).as_str(),
        (cx, cy + 30),
        font_level.pos(center),
    ))?;
    area.draw(&Text::new(
        format!("{:.1} °/s RMS", data.rms_velocity).as_str(),
        (cx, cy + 58),
        font_detail.pos(center),
    ))?;
    area.draw(&Text::new(
        format!("PEAK {:.1} °/s", data.peak_velocity).as_str(),
        (cx, cy + 80),
        font_detail.pos(center),
    ))?;

    // Gyroflow recommended badge (MODERATE+ only)
    if data.level == Level::Moderate || data.level == Level::Severe {
        let bw = 280;
        let bh = 34;
        let bx = cx - bw / 2;
        let by = cy + 100;
        area.draw(&Rectangle::new(
            [(bx, by), (bx + bw, by + bh)],
            CYAN_DIM.stroke_width(1),
        ))?;
        let badge_font = ("monospace", 18).into_font().color(&CYAN);
        area.draw(&Text::new(
            "Gyroflow recommended",
            (cx, by + bh / 2),
            badge_font.pos(center),
        ))?;
    }

    Ok(())
}

fn draw_radar(
    area: &Area,
    cx: i32,
    cy: i32,
    r: i32,
    data: &ChartData,
) -> Result<(), Box<dyn std::error::Error>> {
    let font = ("monospace", 20).into_font();
    let font_val = ("monospace", 16).into_font().color(&TEXT_DIM);

    let axes_data = [
        ("PITCH", data.pitch_rms, BLUE),
        ("ROLL", data.roll_rms, TEAL),
        ("YAW", data.yaw_rms, PURPLE),
    ];
    // Angles: top, bottom-right, bottom-left
    let angles: [f64; 3] = [-90.0, 30.0, 150.0];

    // Grid rings
    for ring_frac in [0.25, 0.5, 0.75, 1.0] {
        let rr = (r as f64 * ring_frac) as i32;
        let mut pts: Vec<(i32, i32)> = angles
            .iter()
            .map(|&a| arc_point(cx as f64, cy as f64, rr as f64, a))
            .collect();
        pts.push(pts[0]); // close
        area.draw(&PathElement::new(pts, GRID_DIM.stroke_width(1)))?;
    }

    // Grid spokes
    for &a in &angles {
        let p = arc_point(cx as f64, cy as f64, r as f64, a);
        area.draw(&PathElement::new(vec![(cx, cy), p], GRID_DIM.stroke_width(1)))?;
    }

    // Data polygon
    let values: Vec<f64> = axes_data
        .iter()
        .map(|(_, rms, _)| (*rms / REFERENCE_MAX).min(1.0))
        .collect();

    let data_pts: Vec<(i32, i32)> = values
        .iter()
        .zip(angles.iter())
        .map(|(&v, &a)| arc_point(cx as f64, cy as f64, r as f64 * v, a))
        .collect();

    // Filled polygon (pre-blended against BG)
    let mut fill_pts = data_pts.clone();
    fill_pts.push(fill_pts[0]);
    area.draw(&Polygon::new(fill_pts.clone(), RADAR_FILL.filled()))?;

    // Outline with glow
    area.draw(&PathElement::new(fill_pts.clone(), GRID.stroke_width(4)))?;
    area.draw(&PathElement::new(fill_pts, CYAN.stroke_width(2)))?;

    // Vertex dots and labels
    for (i, (name, rms, color)) in axes_data.iter().enumerate() {
        let pt = data_pts[i];
        area.draw(&Circle::new(pt, 3, color.filled()))?;

        let a = angles[i];
        // Top axis (PITCH) keeps close labels; bottom axes (YAW/ROLL) push further down
        let extra = if a > 0.0 { 16.0 } else { 0.0 };
        let label_pt = arc_point(cx as f64, cy as f64, r as f64 + 26.0 + extra, a);
        let val_pt = arc_point(cx as f64, cy as f64, r as f64 + 46.0 + extra, a);
        let center = Pos::new(HPos::Center, VPos::Center);
        area.draw(&Text::new(*name, label_pt, font.clone().color(color).pos(center)))?;
        area.draw(&Text::new(
            format!("{:.1}°/s", rms).as_str(),
            val_pt,
            font_val.clone().pos(center),
        ))?;
    }

    Ok(())
}

struct GraphParams<'a> {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    bins: &'a [f64],
    color: RGBColor,
    label: &'a str,
}

fn draw_line_graph(area: &Area, p: &GraphParams) -> Result<(), Box<dyn std::error::Error>> {
    let GraphParams { x, y, w, h, bins, color, label } = *p;
    let font = ("monospace", 18).into_font().color(&color);

    // Label
    area.draw(&Text::new(label, (x + 10, y + 5), font))?;

    // Graph area
    let gx = x + 10;
    let gy = y + 24;
    let gw = w - 20;
    let gh = h - 30;

    // Horizontal grid lines
    for j in 0..=3 {
        let ly = gy + gh * j / 3;
        area.draw(&PathElement::new(
            vec![(gx, ly), (gx + gw, ly)],
            GRID_DIM.stroke_width(1),
        ))?;
    }

    // Vertical grid lines
    for j in 0..=6 {
        let lx = gx + gw * j / 6;
        area.draw(&PathElement::new(
            vec![(lx, gy), (lx, gy + gh)],
            GRID_DIM.stroke_width(1),
        ))?;
    }

    if bins.is_empty() {
        return Ok(());
    }

    let max_val = bins.iter().cloned().fold(0.0_f64, f64::max);
    if max_val <= 0.0 {
        return Ok(());
    }

    // Build polyline points
    let points: Vec<(i32, i32)> = bins
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let px = gx + (i as i64 * gw as i64 / bins.len() as i64) as i32;
            let normalized = (v / max_val).min(1.0);
            let py = gy + gh - (normalized * gh as f64) as i32;
            (px, py)
        })
        .collect();

    // Glow pass (wider, dimmer)
    let glow1 = dim_color(color, 4);
    let glow2 = dim_color(color, 2);
    area.draw(&PathElement::new(points.clone(), glow1.stroke_width(5)))?;
    area.draw(&PathElement::new(points.clone(), glow2.stroke_width(3)))?;
    // Main line
    area.draw(&PathElement::new(points, color.stroke_width(1)))?;

    Ok(())
}

fn draw_footer(area: &Area, data: &ChartData) -> Result<(), Box<dyn std::error::Error>> {
    let y = HEIGHT as i32 - FOOTER_H + 8;

    // Separator line
    area.draw(&PathElement::new(
        vec![(30, y), (WIDTH as i32 - 30, y)],
        GRID.stroke_width(1),
    ))?;

    // Show Gyroflow parameters only for MODERATE+
    if data.level == Level::Moderate || data.level == Level::Severe {
        let font_label = ("monospace", 18).into_font().color(&TEXT_DIM);
        let font_val = ("monospace", 20).into_font().color(&CYAN);

        area.draw(&Text::new(
            "Recommended Gyroflow parameters:",
            (30, y + 18),
            font_label,
        ))?;
        area.draw(&Text::new(
            format!("smoothness={:.0}%", data.smoothness_pct).as_str(),
            (420, y + 16),
            font_val.clone(),
        ))?;
        area.draw(&Text::new(
            format!("max={:.3}s", data.max_smoothness_s).as_str(),
            (600, y + 16),
            font_val.clone(),
        ))?;
        area.draw(&Text::new(
            format!("max@hv={:.3}s", data.max_smoothness_at_high_velocity_s).as_str(),
            (770, y + 16),
            font_val.clone(),
        ))?;
        area.draw(&Text::new(
            format!("zoom_limit={:.0}%", data.zoom_limit_pct).as_str(),
            (990, y + 16),
            font_val.clone(),
        ))?;
        area.draw(&Text::new(
            format!("zooming_speed={:.1}s", data.zooming_speed_s).as_str(),
            (1170, y + 16),
            font_val,
        ))?;
    }

    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────

fn arc_point(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (i32, i32) {
    let rad = angle_deg.to_radians();
    ((cx + r * rad.cos()) as i32, (cy + r * rad.sin()) as i32)
}

struct ArcParams {
    cx: i32,
    cy: i32,
    r: i32,
    start_deg: f64,
    end_deg: f64,
    color: RGBColor,
    width: u32,
}

fn draw_arc(area: &Area, p: &ArcParams) -> Result<(), Box<dyn std::error::Error>> {
    let ArcParams { cx, cy, r, start_deg, end_deg, color, width } = *p;
    let steps = ((end_deg - start_deg).abs() * 2.0).ceil() as usize;
    if steps < 2 {
        return Ok(());
    }
    let step = (end_deg - start_deg) / steps as f64;
    let points: Vec<(i32, i32)> = (0..=steps)
        .map(|i| {
            let angle = start_deg + i as f64 * step;
            arc_point(cx as f64, cy as f64, r as f64, angle)
        })
        .collect();
    area.draw(&PathElement::new(points, color.stroke_width(width)))?;
    Ok(())
}

fn score_color_at(t: f64) -> RGBColor {
    let stops: [(f64, RGBColor); 7] = [
        (0.0, GREEN),
        (0.20, GREEN),
        (0.35, CYAN),
        (0.50, BLUE),
        (0.70, PURPLE),
        (0.85, RED),
        (1.0, RED),
    ];
    for i in 0..stops.len() - 1 {
        let (t0, c0) = stops[i];
        let (t1, c1) = stops[i + 1];
        if t >= t0 && t <= t1 {
            let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return lerp_color(c0, c1, f);
        }
    }
    stops.last().unwrap().1
}

fn lerp_color(a: RGBColor, b: RGBColor, t: f64) -> RGBColor {
    RGBColor(
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
}

fn dim_color(c: RGBColor, factor: u8) -> RGBColor {
    RGBColor(c.0 / factor, c.1 / factor, c.2 / factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::AxisStats;
    use crate::recommend::Recommendation;

    fn make_test_result(score: u32, level: Level) -> (AnalysisResult, Recommendation) {
        let result = AnalysisResult {
            duration_secs: 30.0,
            sample_count: 1000,
            sample_rate_hz: 200.0,
            rms_velocity: 14.4,
            peak_velocity: 45.2,
            score,
            level,
            pitch: AxisStats { avg: 8.2, std_dev: 4.5, max: 32.1 },
            roll: AxisStats { avg: 3.4, std_dev: 2.2, max: 18.7 },
            yaw: AxisStats { avg: 5.6, std_dev: 3.1, max: 24.4 },
            pitch_velocities: vec![],
            roll_velocities: vec![],
            yaw_velocities: vec![],
        };
        let rec = Recommendation {
            smoothness_pct: 28.0,
            max_smoothness_s: 0.700,
            max_smoothness_at_high_velocity_s: 0.100,
            zoom_limit_pct: 115.0,
            zooming_speed_s: 4.0,
        };
        (result, rec)
    }

    fn make_test_data() -> ChartData {
        let n = 1000;
        let composite_samples: Vec<Sample> = (0..n)
            .map(|i| Sample {
                time_secs: i as f64 * 0.03,
                velocity: 5.0 + 10.0 * (i as f64 * 0.1).sin().abs(),
            })
            .collect();
        let axis_samples: Vec<AxisSample> = (0..n)
            .map(|i| {
                let t = i as f64 * 0.03;
                AxisSample {
                    time_secs: t,
                    pitch: 2.0 + 3.0 * (t * 0.5).sin().abs(),
                    roll: 8.0 + 12.0 * (t * 0.3).sin().abs(),
                    yaw: 1.0 + 4.0 * (t * 0.7).sin().abs(),
                }
            })
            .collect();

        let (result, rec) = make_test_result(72, Level::Moderate);
        prepare_data("DJI_TEST.MP4", &result, &rec, &composite_samples, &axis_samples)
    }

    #[test]
    fn test_render_to_png_produces_valid_png() {
        let data = make_test_data();
        let png = render_to_png(&data).expect("Should render PNG");
        assert!(png.len() > 8);
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_render_to_png_correct_dimensions() {
        let data = make_test_data();
        let png = render_to_png(&data).expect("Should render PNG");
        let img = image::load_from_memory(&png).expect("Should load PNG");
        assert_eq!(img.width(), 1440);
        assert_eq!(img.height(), 900);
    }

    #[test]
    fn test_render_to_png_has_dark_background() {
        let data = make_test_data();
        let png = render_to_png(&data).expect("Should render PNG");
        let img = image::load_from_memory(&png).expect("Should load PNG").to_rgb8();
        let pixel = img.get_pixel(0, 0);
        assert!(pixel[0] < 40 && pixel[1] < 40 && pixel[2] < 50);
    }

    #[test]
    fn test_render_to_file() {
        let data = make_test_data();
        let path = std::path::PathBuf::from("target/test_hud_chart.png");
        render_to_file(&data, &path).expect("Should write PNG file");
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 100);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_render_empty_data() {
        let (result, rec) = make_test_result(0, Level::Stable);
        let data = prepare_data("empty.MP4", &result, &rec, &[], &[]);
        let png = render_to_png(&data);
        assert!(png.is_ok());
    }

    #[test]
    fn test_score_color_at_boundaries() {
        assert_eq!(score_color_at(0.0), GREEN);
        assert_eq!(score_color_at(1.0), RED);
    }

    #[test]
    fn test_score_color_at_mid() {
        assert_eq!(score_color_at(0.5), BLUE);
    }

    #[test]
    fn test_dim_color() {
        assert_eq!(dim_color(RGBColor(200, 100, 50), 2), RGBColor(100, 50, 25));
    }

    #[test]
    fn test_stable_level_no_gyroflow_badge() {
        let (result, rec) = make_test_result(10, Level::Stable);
        let data = prepare_data("stable.MP4", &result, &rec, &[], &[]);
        let png = render_to_png(&data);
        assert!(png.is_ok());
    }
}
