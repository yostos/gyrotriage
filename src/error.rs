use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GyroTriageError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Not an MP4 file: {0}")]
    NotMp4(PathBuf),

    #[error("No DJI metadata found in {0}")]
    NoDjiMetadata(PathBuf),

    #[error("No motion data found in {path}")]
    NoMotionData {
        path: PathBuf,
        hint: String,
    },

    #[error("Failed to parse {path}: {source}")]
    ParseError {
        path: PathBuf,
        source: anyhow::Error,
    },

    #[error("Insufficient data: need at least 2 quaternion samples, found {count}")]
    InsufficientData { count: usize },

    #[error("Invalid timecode: {0} (expected HH:MM:SS:FF, HH:MM:SS, MM:SS, or seconds)")]
    InvalidTimecode(String),

    #[error("Frame rate could not be determined for timecode {0}; specify --fps")]
    FpsRequired(String),

    #[error("Invalid analysis range: --in ({start:.3}s) must be before --out ({end:.3}s)")]
    InvalidRange { start: f64, end: f64 },

    #[error("Chart error: {0}")]
    ChartError(String),
}
