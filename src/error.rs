use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GyroTriageError {
    #[error("ファイルが見つかりません: {0}")]
    FileNotFound(PathBuf),

    #[error("MP4ファイルではありません: {0}")]
    NotMp4(PathBuf),

    #[error("DJIメタデータが見つかりません: {0}")]
    NoDjiMetadata(PathBuf),

    #[error("モーションデータが見つかりません: {path}")]
    NoMotionData {
        path: PathBuf,
        hint: String,
    },

    #[error("パースに失敗しました {path}: {source}")]
    ParseError {
        path: PathBuf,
        source: anyhow::Error,
    },

    #[error("データ不足: クォータニオンサンプルが最低2つ必要ですが、{count}個しかありません")]
    InsufficientData { count: usize },

    #[error("チャートエラー: {0}")]
    ChartError(String),
}
