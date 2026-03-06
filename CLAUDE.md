# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**jrnl Project Tag:** gyrotriage
gyrotriage: DJI FPVドローン（Avata/Neo系）のMP4からクォータニオンを抽出し、ブレ度合いのスコアリング・判定・Gyroflow推奨パラメータ提示を行うRust製CLIツール。

## Key Design Decisions

- **言語: Rust** — Gyroflowのtelemetry-parserクレートをそのまま利用するため（ADR-001）
- **単一ファイル入力** — ディレクトリ再帰探索は持たない。複数ファイルはシェル側で処理（ADR-002）
- **CSV出力なし** — 確立されたユースケースがないため不採用（ADR-003）
- **かっこよさ最優先** — 未来的なHUDグラフィックがターミナルに表示される体験がこのツールの存在意義
- **v2完了** — MVP（テキスト出力）+ v2（--visual/--output-image/--sparkline）実装済み
- **推奨パラメータv2完了** — FFT/PSDベースで5パラメータ（smoothness/max_smoothness/max@hv/zoom_limit/zooming_speed）算出

## Source Structure

- `src/main.rs` — CLIエントリポイント、クォータニオン→角速度変換
- `src/analyze.rs` — 解析ロジック（RMS/Peak/軸分解/スコアリング）
- `src/spectrum.rs` — FFT/PSD周波数解析（rustfft使用）
- `src/recommend.rs` — PSDベースGyroflow推奨パラメータ算出（5パラメータ）
- `src/output.rs` — テキスト出力フォーマット
- `src/chart.rs` — HUDスタイルチャート描画（1440×900 PNG、plotters BitMapBackend）
- `src/sparkline.rs` — ANSIスパークライン生成
- `src/terminal.rs` — Sixel/iTerm2プロトコル検出・画像表示
- `src/downsample.rs` — 時系列データのRMSダウンサンプリング
- `src/extract.rs` — telemetry-parserによるMP4からのクォータニオン抽出
- `src/error.rs` — エラー型定義

## Documentation

- `docs/spec.md` — 機能仕様書
- `docs/concept.md` — 構想書
- `docs/adr-004-visual-output.md` — ADR-004: ビジュアル出力仕様（HUDレイアウト詳細）
- `docs/architectural-decision.md` — ADR-001: Rust採用
- `docs/adr-002-single-file-input.md` — ADR-002: 単一ファイル入力
- `docs/adr-003-no-csv-output.md` — ADR-003: CSV出力不採用
- `docs/telemetry-parser-reference.md` — telemetry-parser技術リファレンス
- `docs/todo.md` — 未決定事項トラッカー
- `docs/recommendation-algorithm.ja.md` — 推奨パラメータ算出アルゴリズム（日本語）
- `docs/recommendation-algorithm.en.md` — 推奨パラメータ算出アルゴリズム（英語）

## Domain Knowledge

- DJIドローンはIMU生データではなくクォータニオン（カメラ3D姿勢）をMP4のprotobufトラック(`djmd`)に記録する
- DJI Neo: **4:3撮影が必須**（16:9ではEIS強制オン→モーションデータなし）。FOVは117.6°固定
- DJI Avata: EIS(Rocksteady)オフ + FOV Wide が必要
- 推奨パラメータはFFT/PSD周波数解析から算出（`docs/recommendation-algorithm.ja.md` 参照）

## Build Commands

```bash
cargo build
cargo run -- <FILE>
cargo test           # 79 tests
cargo clippy         # 0 warnings
```

## Key Dependencies

- `telemetry-parser` — DJI MP4からモーションデータ抽出（Gyroflowプロジェクト由来）
- `rustfft` — FFT/PSD計算（推奨パラメータ算出）
- `clap` — CLI引数パース
- `plotters` — チャート描画（BitMapBackend）
- `image` — PNG生成・Sixel向けリサイズ
- `base64` — iTerm2プロトコルエンコード
- `libc` — ターミナルサイズ取得（TIOCGWINSZ）
- `thiserror` — エラー型定義

## Test Data

- `testdata/DJI_20260228080801_0003_D.MP4` — DJI Neo 4:3撮影、232MB（.gitignoreでtestdata/は除外済み）

## Git運用ルール

- **mainブランチへの直接更新禁止**
- developブランチで開発、mainへはPR経由でマージ
- 現在のブランチ: develop
