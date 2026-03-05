use std::path::Path;

use crate::analyze::AnalysisResult;
use crate::recommend::Recommendation;

/// Format the full analysis result as text output per spec.md.
pub fn format_result(path: &Path, result: &AnalysisResult, rec: &Recommendation) -> String {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy());

    format!(
        "\
File:        {filename}
Duration:    {duration:.1}s ({samples} samples @ {rate:.0}Hz)
Score:       {score} / 100
Level:       {level}
RMS:         {rms:.1} °/s
Peak:        {peak:.1} °/s
Pitch:       avg={pavg:.1}°/s  std={pstd:.1}°/s  max={pmax:.1}°/s
Roll:        avg={ravg:.1}°/s  std={rstd:.1}°/s  max={rmax:.1}°/s
Yaw:         avg={yavg:.1}°/s  std={ystd:.1}°/s  max={ymax:.1}°/s
---
Gyroflow:    smoothness={smoothness:.1}  crop≈{crop:.1}x",
        duration = result.duration_secs,
        samples = result.sample_count,
        rate = result.sample_rate_hz,
        score = result.score,
        level = result.level,
        rms = result.rms_velocity,
        peak = result.peak_velocity,
        pavg = result.pitch.avg,
        pstd = result.pitch.std_dev,
        pmax = result.pitch.max,
        ravg = result.roll.avg,
        rstd = result.roll.std_dev,
        rmax = result.roll.max,
        yavg = result.yaw.avg,
        ystd = result.yaw.std_dev,
        ymax = result.yaw.max,
        smoothness = rec.smoothness,
        crop = rec.crop,
    )
}

/// Format the "no motion data" special output.
pub fn format_no_motion_data(path: &Path, hint: &str) -> String {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy());

    format!(
        "\
File:        {filename}
Status:      NO MOTION DATA
Hint:        {hint}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{AnalysisResult, AxisStats, Level};
    use crate::recommend::Recommendation;
    use std::path::PathBuf;

    #[test]
    fn test_format_result_contains_key_fields() {
        let result = AnalysisResult {
            duration_secs: 120.5,
            sample_count: 24100,
            sample_rate_hz: 200.0,
            rms_velocity: 14.4,
            peak_velocity: 45.2,
            score: 72,
            level: Level::Moderate,
            pitch: AxisStats { avg: 8.2, std_dev: 4.5, max: 32.1 },
            roll: AxisStats { avg: 3.4, std_dev: 2.2, max: 18.7 },
            yaw: AxisStats { avg: 5.6, std_dev: 3.1, max: 24.4 },
        };
        let rec = Recommendation { smoothness: 0.5, crop: 1.2 };
        let path = PathBuf::from("DJI_20260227_0001.MP4");
        let output = format_result(&path, &result, &rec);

        assert!(output.contains("DJI_20260227_0001.MP4"));
        assert!(output.contains("72 / 100"));
        assert!(output.contains("MODERATE"));
        assert!(output.contains("14.4 °/s"));
        assert!(output.contains("smoothness=0.5"));
        assert!(output.contains("crop≈1.2x"));
    }

    #[test]
    fn test_format_no_motion_data() {
        let path = PathBuf::from("DJI_20260227_0005.MP4");
        let hint = "Neo/Neo2は4:3で撮影が必要。Avata/Avata2はEISオフ・FOV Wideが必要。";
        let output = format_no_motion_data(&path, hint);

        assert!(output.contains("DJI_20260227_0005.MP4"));
        assert!(output.contains("NO MOTION DATA"));
        assert!(output.contains("Neo/Neo2"));
    }
}
