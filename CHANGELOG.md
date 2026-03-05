# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-06

### Added

- **Core analysis**: MP4からクォータニオン姿勢データを抽出し、RMS角速度ベースのブレスコア（0-100）を算出
- **4段階レベル判定**: STABLE / MILD / MODERATE / SEVERE
- **Gyroflow推奨パラメータ**: スコアに基づくsmoothness（区分線形マッピング）とcrop（`1 + smoothness × rms_velocity / FOV`）の自動算出
- **テキスト出力**: ファイル情報、スコア、レベル、RMS/Peak、3軸統計、Gyroflow推奨値
- **HUDスタイルチャート** (`--visual` / `--output-image`):
  - 1440x900px PNG、Tokyo Night寒色基調のダーク背景
  - スコアゲージ（緑→シアン→青→紫→赤グラデーション弧 + グローエフェクト）
  - 3軸レーダーチャート（Pitch/Roll/Yaw）
  - 4折れ線グラフ（Composite/Pitch/Roll/Yaw、グローエフェクト付き）
  - 条件付きフッター（MODERATE以上でGyroflow推奨パラメータ表示）
- **ターミナルインライン表示** (`--visual`):
  - Sixel / iTerm2プロトコル自動検出（TERM_PROGRAM / TERM環境変数）
  - `--sixel` / `--iterm2` による強制指定
  - iTerm2: `width=100%` で横幅フィット
  - Sixel: `ioctl(TIOCGWINSZ)` でピクセル幅取得、アスペクト比維持リサイズ
- **ANSIスパークライン** (`--sparkline`): SSH/パイプ環境向け簡易可視化
- **エラーハンドリング**: モーションデータなしの場合に撮影条件のヒント表示（Neo→4:3必須、Avata→EISオフ+FOV Wide）
- **ドキュメント**: 機能仕様書、構想書、ADR-001〜004、telemetry-parserリファレンス

### Supported devices

- DJI Avata / Avata 2（EIS off, FOV Wide）
- DJI Neo / Neo2（4:3 aspect ratio）
