# Gyroflow Recommended Parameter Estimation Algorithm

## Overview

gyrotriage extracts quaternion time series data from MP4 files and performs FFT/PSD (Power Spectral Density) frequency analysis on the derived angular velocity to estimate recommended values for the **Gyroflow OpenFX plugin** (DaVinci Resolve, etc.) "Adjust parameters" panel (see ADR-005).

These values serve as a starting point (baseline) for parameter adjustment. Final parameters should be determined by the user while previewing the footage in the plugin.

## Output Parameters

### Computed parameters (motion-analysis based)

| Parameter | Unit/Range | Plugin internal scale | Basis |
|---|---|---|---|
| Smoothness | value 1–300 | core value ×100 (default 50) | PSD shake power ratio + RMS angular velocity |
| Zoom limit | % 51–300 | `max_zoom` as-is | Derived from Smoothness + RMS angular velocity |
| FOV | factor 0.1–3.0 | core `fov` as-is | Not computed; baseline **1.0** presented (future work) |

> **About the scale**: The plugin's Smoothness is the core value ×100 (min:1/max:300/default:50). The value gyrotriage historically computed as `smoothness_pct` (15–50) can be used **without conversion** as the plugin's Smoothness value (28% → `28`). See `docs/plugin-recommendation-feasibility.ja.md` §3.

### Fixed recommendations (DJI hardware based, not computed)

| Parameter | Recommended | Reason |
|---|---|---|
| Integration method | None | DJI drones record quaternions (camera attitude), so re-integration is unnecessary |
| Lens correction | 100 | Full correction, assuming the correct DJI lens profile/preset is loaded |

Others (Horizon lock/roll, Additional pitch/yaw, Video/Input rotation, Video speed, Disable stretch) depend on user intent and hardware, so they are left at defaults. Case-by-case guidance is in `docs/plugin-recommendation-feasibility.ja.md` §5.

## Input Data

Per-axis angular velocity time series (Pitch/Roll/Yaw, in °/s) are computed in `analyze.rs`. The source data is quaternion attitude data from DJI MP4 files, decomposed into Euler angle differences between consecutive frames.

## Estimation Pipeline

```
MP4 → quaternion extraction → angular velocity time series → FFT/PSD → shake power ratio estimation → parameter conversion
```

### Step 1: PSD (Power Spectral Density) Computation

A PSD is computed for each axis (Pitch/Roll/Yaw) and the three PSDs are summed bin-by-bin to form the composite spectrum.

1. Apply Hann window to suppress spectral leakage
2. Execute FFT using the `rustfft` crate
3. Compute one-sided PSD: `PSD[k] = |X[k]|^2 / (N × fs) × 2`
4. Sum the per-axis PSDs: `PSD[k] = PSD_pitch[k] + PSD_roll[k] + PSD_yaw[k]`

**Why per-axis summation (not time-domain RSS)**: Combining axes in the time domain via RSS (`sqrt(p²+r²+y²)`) rectifies the signal to non-negative values, injecting a large DC offset. That DC inflates total power and collapses the shake power ratio toward zero, pinning Smoothness near its 15 floor regardless of actual shake. PSD addition is linear and preserves each axis' zero mean, so no spurious DC is introduced.

**Why PSD**: The angular velocity signal is a superposition of "intentional motion (low frequency)" and "shake/vibration (high frequency)". These two components have clearly separated frequency bands, and PSD allows objective detection of the boundary.

Typical frequency bands for FPV drones:

| Band | Frequency | Source |
|---|---|---|
| Intentional motion | < 1 Hz | Pan, tilt, turns |
| Hand shake / wind | 3–10 Hz | Attitude disturbances |
| Motor vibration | 20–80 Hz | Propeller rotation |

### Step 2: Primary Cutoff Frequency fc (for shake power ratio)

Search for the minimum (valley) in the smoothed PSD within the 0.5–5 Hz range. This valley appears between the intentional motion frequency band and the shake frequency band. Minimum: 0.3 Hz.

Using this cutoff as the boundary, the **shake power ratio** (fraction of total power above the cutoff frequency) is computed. This is the primary input to the Smoothness estimation.

### Step 3: Parameter Estimation

#### Smoothness (1–300)

Determined from the **shake power ratio** computed from PSD, combined with RMS angular velocity. The computed value can be entered directly as the plugin's Smoothness value.

```
base = 15 + 35 × shake_power_ratio
velocity_factor = 0.85 (rms < 3°/s) to 1.15 (rms > 15°/s)
smoothness = clamp(base × velocity_factor, 15, 50)
```

**Why shake power ratio**: The greater the proportion of shake in the signal, the stronger the smoothing needed. If shake_power_ratio = 0, the signal is entirely intentional motion and minimal smoothing suffices. If shake_power_ratio = 1, the signal is entirely shake and maximum smoothing is needed.

**Why velocity_factor**: Even with the same shake ratio, lower RMS angular velocity (less overall movement) requires less smoothing, while higher RMS angular velocity benefits from stronger smoothing.

**Consistency with FPV recommended range (20–35)**: shake_power_ratio of 0.15–0.60 produces smoothness values of 20–35. Typical FPV flight data falls within this range.

#### Zoom limit (%)

Estimated from Smoothness and RMS angular velocity.

```
base = 105 + (smoothness - 15) × 25 / 35
velocity_extra = min(rms_velocity / 20 × 5, 10)
zoom_limit = clamp(base + velocity_extra, 105, 140)
```

**Why the combination of smoothness and RMS angular velocity**: Stronger smoothing increases the per-frame correction amount, requiring more zoom to hide black borders. Additionally, footage with higher RMS angular velocity (more shake) has larger maximum per-frame corrections, requiring additional zoom headroom.

**Consistency with FPV guidelines (110–125%)**: Typical FPV data with smoothness 20–35 and RMS 5–15°/s produces zoom limits of 110–120%. The plugin's Zoom limit range is 51–300%, so the computed 105–140% fits within it.

#### FOV (factor)

While there is room to suggest a zoom factor from the amount of motion, separating its role from Zoom limit (the upper bound of dynamic crop) is difficult and tends to cause excessive cropping. Currently no estimation logic exists; the **baseline 1.0** (the plugin default) is presented. Automatic FOV estimation is future work.

## Implementation Files

| File | Role |
|---|---|
| `src/spectrum.rs` | FFT/PSD computation, shake power ratio estimation |
| `src/recommend.rs` | Conversion from PSD results to plugin parameters |
| `src/analyze.rs` | Quaternion-to-angular-velocity conversion, statistics |

## Limitations and Caveats

- Recommended values are objective estimates based on signal processing and do not include subjective video quality assessment
- Distinction between intentional motion and shake depends on frequency band separation; very slow shake (<0.3Hz) cannot be distinguished from intentional motion
- For extremely short clips (under 2 seconds), frequency resolution is insufficient and estimation accuracy degrades
- Maps to the plugin's single Smoothness slider. Standalone-specific detailed parameters such as Max smoothness / Max smoothness at high velocity / Zooming speed are not presented (see ADR-005)
