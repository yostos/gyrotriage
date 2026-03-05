use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use telemetry_parser::tags_impl::{GetWithType, GroupId, TagId, TimeQuaternion};
use telemetry_parser::Input;

use crate::error::GyroTriageError;

/// Data extracted from an MP4 file.
#[allow(dead_code)]
pub struct ExtractedData {
    pub quaternions: Vec<TimeQuaternion<f64>>,
    pub camera_model: Option<String>,
}

/// Extract quaternion data from a DJI MP4 file.
pub fn extract_quaternions(path: &Path) -> Result<ExtractedData, GyroTriageError> {
    // Check file exists
    if !path.exists() {
        return Err(GyroTriageError::FileNotFound(path.to_path_buf()));
    }

    // Check MP4 extension
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());
    match ext.as_deref() {
        Some("mp4") | Some("mov") => {}
        _ => return Err(GyroTriageError::NotMp4(path.to_path_buf())),
    }

    // Open and parse
    let mut file = std::fs::File::open(path).map_err(|e| GyroTriageError::ParseError {
        path: path.to_path_buf(),
        source: e.into(),
    })?;
    let filesize = file
        .metadata()
        .map_err(|e| GyroTriageError::ParseError {
            path: path.to_path_buf(),
            source: e.into(),
        })?
        .len() as usize;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let input = Input::from_stream(&mut file, filesize, path, |_progress| {}, cancel_flag)
        .map_err(|e| GyroTriageError::ParseError {
            path: path.to_path_buf(),
            source: e.into(),
        })?;

    // Check if it's DJI
    let camera_type = input.camera_type();
    if camera_type != "DJI" {
        return Err(GyroTriageError::NoDjiMetadata(path.to_path_buf()));
    }

    let camera_model = input.camera_model().cloned();

    // Extract quaternions from all samples
    let mut all_quaternions: Vec<TimeQuaternion<f64>> = Vec::new();

    let samples = match input.samples {
        Some(s) => s,
        None => {
            return Err(GyroTriageError::NoMotionData {
                path: path.to_path_buf(),
                hint: motion_data_hint(),
            });
        }
    };

    for sample in &samples {
        if let Some(ref tag_map) = sample.tag_map {
            if let Some(quat_group) = tag_map.get(&GroupId::Quaternion) {
                if let Some(quats) = GetWithType::<Vec<TimeQuaternion<f64>>>::get_t(quat_group, TagId::Data) {
                    all_quaternions.extend_from_slice(quats);
                }
            }
        }
    }

    if all_quaternions.is_empty() {
        return Err(GyroTriageError::NoMotionData {
            path: path.to_path_buf(),
            hint: motion_data_hint(),
        });
    }

    // Sort by timestamp
    all_quaternions.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));

    if all_quaternions.len() < 2 {
        return Err(GyroTriageError::InsufficientData {
            count: all_quaternions.len(),
        });
    }

    Ok(ExtractedData {
        quaternions: all_quaternions,
        camera_model,
    })
}

fn motion_data_hint() -> String {
    "Neo/Neo2は4:3で撮影が必要。Avata/Avata2はEISオフ・FOV Wideが必要。".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_not_found() {
        let result = extract_quaternions(Path::new("nonexistent.mp4"));
        assert!(matches!(result, Err(GyroTriageError::FileNotFound(_))));
    }

    #[test]
    fn test_not_mp4() {
        // Use Cargo.toml as a non-MP4 file
        let result = extract_quaternions(Path::new("Cargo.toml"));
        assert!(matches!(result, Err(GyroTriageError::NotMp4(_))));
    }

    /// Integration test with real MP4 data.
    /// Requires testdata/DJI_20260228080801_0003_D.MP4 to be present.
    #[test]
    fn test_extract_real_mp4() {
        let path = PathBuf::from("testdata/DJI_20260228080801_0003_D.MP4");
        if !path.exists() {
            eprintln!("Skipping: test data not found at {}", path.display());
            return;
        }

        let data = extract_quaternions(&path).expect("Should extract quaternions from DJI Neo MP4");
        assert!(
            data.quaternions.len() > 100,
            "Expected many quaternions, got {}",
            data.quaternions.len()
        );
        // Verify timestamps are sorted
        for w in data.quaternions.windows(2) {
            assert!(w[0].t <= w[1].t, "Timestamps should be sorted");
        }
    }
}
