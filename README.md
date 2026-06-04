# gyrotriage

A Rust CLI tool that extracts motion data (quaternions) from DJI FPV drone MP4 files (Avata/Neo series), scores shake severity, and recommends Gyroflow stabilization parameters.

Renders futuristic HUD-style graphics directly in your terminal.

![HUD output example](docs/images/hud_example.png)

## Features

- Extracts quaternion attitude data from MP4 (via telemetry-parser)
- RMS angular velocity based shake score (0–100) with 4-level grading (STABLE/MILD/MODERATE/SEVERE)
- **FFT/PSD-based Gyroflow plugin parameter recommendation** — produces values for the Gyroflow OpenFX plugin's "Adjust parameters" panel (DaVinci Resolve, etc.):
  - Smoothness (computed; plugin value 1–300)
  - Zoom limit (computed; %)
  - FOV (baseline 1.0)
  - Integration method = None, Lens correction = 100 (fixed DJI recommendations)
- HUD-style graphical output (score gauge, radar chart, 4-axis line graphs)
- Sixel / iTerm2 protocol terminal inline image display
- ANSI sparkline lightweight visualization

## Installation

```bash
cargo install --path .
```

Or via Homebrew (macOS):

```bash
brew install gyrotriage
```

## Usage

```bash
# Basic text output
gyrotriage clip.MP4

# HUD graphic display in terminal
gyrotriage clip.MP4 --visual

# Export as PNG image file
gyrotriage clip.MP4 --output-image shake.png

# Both
gyrotriage clip.MP4 --visual --output-image shake.png

# Text output with ANSI sparklines
gyrotriage clip.MP4 --sparkline
```

### Options

| Option | Short | Description |
|---|---|---|
| `--visual` | `-v` | Display HUD graph in terminal via Sixel/iTerm2 |
| `--output-image <PATH>` | `-o` | Export as PNG image file |
| `--sparkline` | `-s` | Append ANSI sparklines to text output |
| `--sixel` | `-x` | Force Sixel protocol (use with `--visual`) |
| `--iterm2` | `-i` | Force iTerm2 protocol (use with `--visual`) |
| `--version` | `-V` | Print version |

### Output example

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
Gyroflow plugin (Adjust parameters):
  Smoothness:           21
  Zoom limit:           118
  FOV:                  1.000
  Integration method:   None
  Lens correction:      100
```

## Supported Devices

| Device | Requirements |
|---|---|
| DJI Avata / Avata 2 | EIS (Rocksteady) OFF, FOV Wide |
| DJI Neo / Neo2 | Aspect ratio 4:3 (EIS is automatically off) |

Motion data is not recorded when shooting in 16:9 (Neo/Neo2) or with EIS enabled (Avata series), making analysis impossible.

## Scoring

| Level | Score | RMS angular velocity | Meaning |
|---|---|---|---|
| STABLE | 0–25 | < 5 deg/s | Almost no shake, no stabilization needed |
| MILD | 26–50 | 5–10 deg/s | Slight shake, stabilization optional |
| MODERATE | 51–75 | 10–15 deg/s | Noticeable shake, Gyroflow recommended |
| SEVERE | 76–100 | > 15 deg/s | Heavy shake, Gyroflow strongly recommended |

## Gyroflow Plugin Parameter Recommendation

gyrotriage produces values for the **Gyroflow OpenFX plugin** ("Adjust parameters" panel) using FFT/PSD (Power Spectral Density) analysis of the angular velocity time series. The approach is based on signal processing — no training data or subjective quality assessment is required. Standalone-app parameter sets are not supported (see ADR-005).

Computed from motion analysis:

| Parameter | Range | How it's estimated |
|---|---|---|
| Smoothness | 1–300 | PSD shake power ratio + RMS angular velocity (used directly as the plugin value) |
| Zoom limit | 51–300% | Derived from smoothness + RMS angular velocity |
| FOV | 0.1–3.0 | Not computed; baseline 1.0 |

Fixed recommendations for DJI footage:

| Parameter | Value | Reason |
|---|---|---|
| Integration method | None | DJI records quaternions, so no re-integration is needed |
| Lens correction | 100 | Full correction, assuming the correct DJI lens profile is loaded |

For the full algorithm details, see:
- [docs/recommendation-algorithm.en.md](docs/recommendation-algorithm.en.md) (English)
- [docs/recommendation-algorithm.ja.md](docs/recommendation-algorithm.ja.md) (Japanese)

## Documentation

- [docs/spec.md](docs/spec.md) — Functional specification (Japanese)
- [docs/concept.md](docs/concept.md) — Concept document (Japanese)
- [docs/recommendation-algorithm.en.md](docs/recommendation-algorithm.en.md) — Recommendation algorithm (English)
- [docs/recommendation-algorithm.ja.md](docs/recommendation-algorithm.ja.md) — Recommendation algorithm (Japanese)
- [docs/adr-004-visual-output.md](docs/adr-004-visual-output.md) — Visual output specification (ADR-004)
- [docs/adr-005-plugin-target.md](docs/adr-005-plugin-target.md) — Gyroflow plugin target decision (ADR-005)
- [docs/plugin-recommendation-feasibility.ja.md](docs/plugin-recommendation-feasibility.ja.md) — Plugin parameter feasibility study (Japanese)
- [docs/adr-001-rust-language.md](docs/adr-001-rust-language.md) — Rust adoption decision (ADR-001)

## Development

```bash
cargo build          # Build
cargo test           # Tests (79 tests)
cargo clippy         # Lint
cargo run -- <FILE>  # Run
```

## License

TBD
