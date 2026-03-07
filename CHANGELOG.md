# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - 2026-03-07

### Added

- `--version` / `-V` option to display version information
- Short options for all CLI flags: `-v` (visual), `-o` (output-image), `-s` (sparkline), `-x` (sixel), `-i` (iterm2)
- GitHub Actions workflow to verify release tag matches Cargo.toml version

### Changed

- All user-facing messages (help, errors, hints) localized to English
- Chart header version label now auto-derived from Cargo.toml via `env!("CARGO_PKG_VERSION")`

## [1.1.0-rc] - 2026-03-06

### Changed

- **Gyroflow recommended parameters: complete rewrite** — replaced heuristic smoothness/crop with FFT/PSD-based estimation of 5 Gyroflow parameters:
  - Smoothness (%) — from PSD shake power ratio + RMS angular velocity
  - Max smoothness (s) — from PSD cutoff frequency → time constant τ = 1/(2πfc)
  - Max smoothness at high velocity (s) — from high-velocity cutoff frequency
  - Zoom limit (%) — from smoothness + RMS angular velocity
  - Zooming speed (s) — from coefficient of variation of rolling RMS angular velocity
- Output format updated to show all 5 parameters with Gyroflow-compatible units
- HUD chart footer updated with new parameter display

### Added

- `src/spectrum.rs` — FFT/PSD frequency analysis module (using `rustfft` crate)
- `docs/recommendation-algorithm.ja.md` — detailed algorithm documentation (Japanese)
- `docs/recommendation-algorithm.en.md` — detailed algorithm documentation (English)
- `rustfft` dependency for spectral analysis

### Removed

- Heuristic smoothness mapping (piecewise linear from score)
- Crop parameter (no corresponding Gyroflow parameter exists)

## [0.1.0] - 2026-03-06

### Added

- **Core analysis**: quaternion attitude data extraction from MP4, RMS angular velocity based shake score (0-100)
- **4-level grading**: STABLE / MILD / MODERATE / SEVERE
- **Text output**: file info, score, level, RMS/Peak, per-axis stats, Gyroflow recommendations
- **HUD-style chart** (`--visual` / `--output-image`):
  - 1440x900px PNG, Tokyo Night cool-tone dark background
  - Score gauge (green→cyan→blue→purple→red gradient arc + glow effect)
  - 3-axis radar chart (Pitch/Roll/Yaw)
  - 4 line graphs (Composite/Pitch/Roll/Yaw with glow effect)
  - Conditional footer (Gyroflow parameters shown for MODERATE+ only)
- **Terminal inline display** (`--visual`):
  - Sixel / iTerm2 protocol auto-detection (TERM_PROGRAM / TERM env vars)
  - `--sixel` / `--iterm2` forced override
  - iTerm2: `width=100%` fit
  - Sixel: `ioctl(TIOCGWINSZ)` pixel width detection, aspect-ratio-preserving resize
- **ANSI sparkline** (`--sparkline`): lightweight visualization for SSH/pipe environments
- **Error handling**: shooting condition hints when no motion data found (Neo→4:3 required, Avata→EIS off + FOV Wide)
- **Documentation**: spec, concept, ADR-001–004, telemetry-parser reference

### Supported devices

- DJI Avata / Avata 2 (EIS off, FOV Wide)
- DJI Neo / Neo2 (4:3 aspect ratio)
