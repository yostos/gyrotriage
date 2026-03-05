# gyrotriage

DJI FPVドローン（Avata/Neo系）のMP4からモーションデータを解析し、ブレ度合いのスコアリング・判定・Gyroflow推奨パラメータ提示を行うRust製CLIツール。

ターミナルに未来的なHUDスタイルのグラフィック出力ができます。

![HUD output example](docs/images/hud_example.png)

## Features

- MP4からクォータニオン姿勢データを抽出（telemetry-parser利用）
- RMS角速度ベースのブレスコア（0-100）と4段階レベル判定（STABLE/MILD/MODERATE/SEVERE）
- Gyroflow推奨パラメータ（smoothness / crop）の自動算出
- HUDスタイルのグラフィカル出力（スコアゲージ、レーダーチャート、4軸折れ線グラフ）
- Sixel / iTerm2プロトコルによるターミナルインライン画像表示
- ANSIスパークラインによる簡易可視化

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# テキスト出力（基本）
gyrotriage clip.MP4

# ターミナルにHUDグラフィック表示
gyrotriage clip.MP4 --visual

# PNG画像ファイルに書き出し
gyrotriage clip.MP4 --output-image shake.png

# 両方
gyrotriage clip.MP4 --visual --output-image shake.png

# ANSIスパークライン付きテキスト出力
gyrotriage clip.MP4 --sparkline
```

### Options

| Option | Description |
|---|---|
| `--visual` | Sixel/iTerm2でターミナルにHUDグラフ表示 |
| `--output-image <PATH>` | PNG画像ファイルに書き出し |
| `--sparkline` | ANSIスパークラインをテキスト出力に追加 |
| `--sixel` | Sixelプロトコルを強制（`--visual`と併用） |
| `--iterm2` | iTerm2プロトコルを強制（`--visual`と併用） |

### Output example (text)

```
File:        DJI_20260228080801_0003_D.MP4
Duration:    30.3s (60593 samples @ 2000Hz)
Score:       100 / 100
Level:       SEVERE
RMS:         34.8 °/s
Peak:        644.0 °/s
Pitch:       avg=1.4°/s  std=2.2°/s  max=47.9°/s
Roll:        avg=11.7°/s  std=13.5°/s  max=251.1°/s
Yaw:         avg=3.3°/s  std=4.4°/s  max=89.0°/s
---
Gyroflow:    smoothness=1.5  crop≈1.4x
```

## Supported devices

| Device | Requirements |
|---|---|
| DJI Avata / Avata 2 | EIS (Rocksteady) OFF, FOV Wide |
| DJI Neo / Neo2 | Aspect ratio 4:3 (EIS is automatically off) |

16:9撮影（Neo/Neo2）やEISオン（Avata系）ではモーションデータが記録されないため解析できません。

## Scoring

| Level | Score | RMS angular velocity | Meaning |
|---|---|---|---|
| STABLE | 0-25 | < 5 deg/s | Almost no shake, no stabilization needed |
| MILD | 26-50 | 5-10 deg/s | Slight shake, stabilization optional |
| MODERATE | 51-75 | 10-15 deg/s | Noticeable shake, Gyroflow recommended |
| SEVERE | 76-100 | > 15 deg/s | Heavy shake, Gyroflow strongly recommended |

## Architecture

詳細は `docs/` を参照:

- `docs/spec.md` — 機能仕様書
- `docs/concept.md` — 構想書
- `docs/adr-004-visual-output.md` — ビジュアル出力仕様（ADR-004）
- `docs/architectural-decision.md` — Rust採用の決定（ADR-001）

## Development

```bash
cargo build          # ビルド
cargo test           # テスト（69件）
cargo clippy         # lint
cargo run -- <FILE>  # 実行
```

## License

TBD
