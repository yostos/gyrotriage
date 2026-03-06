# Gyroflow Recommended Parameter Estimation Algorithm

## Overview

gyrotriage extracts quaternion time series data from MP4 files and performs FFT/PSD (Power Spectral Density) frequency analysis on the derived angular velocity to estimate recommended values for five Gyroflow stabilization parameters.

These values serve as a starting point (baseline) for parameter adjustment in Gyroflow. Final parameters should be determined by the user while previewing the footage in Gyroflow.

## Output Parameters

| Parameter | Unit | Gyroflow JSON key | Recommended range |
|---|---|---|---|
| Smoothness | % | `smoothing_params[0].value` | 15–50% |
| Max smoothness | seconds | `smoothing_params[1].value` | 0.3–2.0s |
| Max smoothness at high velocity | seconds | `smoothing_params[2].value` | 0.03–0.3s |
| Zoom limit | % | `max_zoom` | 105–140% |
| Zooming speed | seconds | `adaptive_zoom_window` | 2.0–6.0s |

## Input Data

Per-axis angular velocity time series (Pitch/Roll/Yaw, in °/s) are computed in `analyze.rs`. The source data is quaternion attitude data from DJI MP4 files, decomposed into Euler angle differences between consecutive frames.

## Estimation Pipeline

```
MP4 → quaternion extraction → angular velocity time series → FFT/PSD → cutoff frequency estimation → parameter conversion
```

### Step 1: PSD (Power Spectral Density) Computation

FFT is performed on the 3-axis composite angular velocity (RSS: Root Sum Square).

1. Apply Hann window to suppress spectral leakage
2. Execute FFT using the `rustfft` crate
3. Compute one-sided PSD: `PSD[k] = |X[k]|^2 / (N × fs) × 2`

**Why PSD**: The angular velocity signal is a superposition of "intentional motion (low frequency)" and "shake/vibration (high frequency)". These two components have clearly separated frequency bands, and PSD allows objective detection of the boundary.

Typical frequency bands for FPV drones:

| Band | Frequency | Source |
|---|---|---|
| Intentional motion | < 1 Hz | Pan, tilt, turns |
| Hand shake / wind | 3–10 Hz | Attitude disturbances |
| Motor vibration | 20–80 Hz | Propeller rotation |

### Step 2: Cutoff Frequency Estimation

Two types of cutoff frequencies are estimated from the PSD.

#### Primary cutoff frequency fc (intentional motion vs. shake boundary)

Search for the minimum (valley) in the smoothed PSD within the 0.5–5 Hz range. This valley appears between the intentional motion frequency band and the shake frequency band.

**Why valley detection**: The lowpass filter cutoff should be placed at the "boundary between signal to keep and signal to remove". The PSD valley indicates exactly this boundary.

Minimum: 0.3 Hz (below this, no meaningful stabilization is possible).

#### High-velocity cutoff frequency fc_hv

Uses the 80% cumulative power point, clamped to at least `fc × 2` and at most 10 Hz. During fast rotation, low-frequency components dominate, so higher frequencies need to pass through.

### Step 3: Conversion to Time Constants

Compute the lowpass filter time constant from the cutoff frequency:

```
τ = 1 / (2π × fc)
```

**Why this formula**: Gyroflow's smoothing uses an Exponential Moving Average (EMA) filter, and the relationship between its time constant τ and cutoff frequency fc is `τ = 1/(2πfc)`. Gyroflow's internal implementation:

```
alpha = 1 - exp(-(1 / sample_rate) / smoothness)
```

where `smoothness` corresponds to τ.

### Step 4: Parameter Estimation

#### Smoothness (%)

Determined from the **shake power ratio** (fraction of total power above the cutoff frequency) computed from PSD, combined with RMS angular velocity.

```
base = 15 + 35 × shake_power_ratio
velocity_factor = 0.85 (rms < 3°/s) to 1.15 (rms > 15°/s)
smoothness = clamp(base × velocity_factor, 15, 50)
```

**Why shake power ratio**: The greater the proportion of shake in the signal, the stronger the smoothing needed. If shake_power_ratio = 0, the signal is entirely intentional motion and minimal smoothing suffices. If shake_power_ratio = 1, the signal is entirely shake and maximum smoothing is needed.

**Why velocity_factor**: Even with the same shake ratio, lower RMS angular velocity (less overall movement) requires less smoothing, while higher RMS angular velocity benefits from stronger smoothing.

**Consistency with FPV recommended range (20–35%)**: shake_power_ratio of 0.15–0.60 produces smoothness values of 20–35%. Typical FPV flight data falls within this range.

#### Max smoothness (seconds)

Uses the time constant τ derived from the primary cutoff frequency fc directly.

```
max_smoothness = clamp(τ, 0.3, 2.0)
```

**Why use τ directly**: Max smoothness is the upper limit of "how much smoothing to apply at maximum". The time constant derived from the cutoff frequency corresponds exactly to "the maximum filter strength needed to remove shake".

**Clamp range rationale**: Practical Gyroflow range. Below 0.3s, the effect is negligible; above 2.0s, crop becomes excessive.

#### Max smoothness at high velocity (seconds)

Uses the time constant derived from the high-velocity cutoff fc_hv.

```
max_smoothness_at_high_velocity = clamp(τ_hv, 0.03, 0.3)
```

**Why a shorter time constant**: If smoothing is too strong during fast FPV turns, a "rubber band" effect occurs (the footage appears to lag behind unnaturally). During fast rotation, intentional motion dominates, so tracking responsiveness should be prioritized over shake removal.

**Clamp range rationale**: Gyroflow's default is 0.1s. Below 0.03s is effectively no smoothing; above 0.3s, tracking responsiveness during fast rotation is lost.

#### Zoom limit (%)

Estimated from Smoothness and RMS angular velocity.

```
base = 105 + (smoothness_pct - 15) × 25 / 35
velocity_extra = min(rms_velocity / 20 × 5, 10)
zoom_limit = clamp(base + velocity_extra, 105, 140)
```

**Why the combination of smoothness and RMS angular velocity**: Stronger smoothing increases the per-frame correction amount, requiring more zoom to hide black borders. Additionally, footage with higher RMS angular velocity (more shake) has larger maximum per-frame corrections, requiring additional zoom headroom.

**Consistency with FPV guidelines (110–125%)**: Typical FPV data with smoothness 20–35% and RMS 5–15°/s produces zoom limits of 110–120%.

#### Zooming speed (seconds)

Estimated from the **temporal variability** of angular velocity (coefficient of variation of rolling RMS).

1. Compute rolling RMS over 1-second windows of 3-axis composite angular velocity (50% overlap)
2. Calculate coefficient of variation: `CV = σ(rolling_rms) / μ(rolling_rms)`
3. Map CV to zooming speed:

```
zooming_speed = clamp(6.0 - 4.0 × min(CV, 1.0), 2.0, 6.0)
```

**Why coefficient of variation**: Zooming speed controls the FOV transition speed of Dynamic zooming. If shake is temporally uniform, FOV can change slowly without issues. If shake is intermittent (alternating between intense and calm periods), FOV needs to adapt quickly. CV quantifies this "temporal non-uniformity".

- CV ≈ 0 (uniform shake) → 6.0s (slow FOV transitions are sufficient)
- CV ≈ 0.5 (moderate variation) → 4.0s (equivalent to Gyroflow default)
- CV ≈ 1.0+ (intermittent shake) → 2.0s (fast FOV adaptation needed)

## Implementation Files

| File | Role |
|---|---|
| `src/spectrum.rs` | FFT/PSD computation, cutoff frequency estimation |
| `src/recommend.rs` | Conversion from PSD results to Gyroflow parameters |
| `src/analyze.rs` | Quaternion-to-angular-velocity conversion, statistics |

## Limitations and Caveats

- Recommended values are objective estimates based on signal processing and do not include subjective video quality assessment
- Distinction between intentional motion and shake depends on frequency band separation; very slow shake (<0.3Hz) cannot be distinguished from intentional motion
- Assumes Gyroflow's "Default" algorithm. Recommended values may not apply directly to "Plain 3D" or per-axis modes
- For extremely short clips (under 2 seconds), frequency resolution is insufficient and estimation accuracy degrades
