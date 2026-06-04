# Gyroflow推奨パラメータ算出アルゴリズム

## 概要

gyrotriageはMP4から抽出したクォータニオン時系列データをFFT/PSD（パワースペクトル密度）で
周波数解析し、**Gyroflow OpenFXプラグイン**（DaVinci Resolve等）の「Adjust parameters」に
入力する推奨値を算出する（→ ADR-005）。

これらの値はパラメータ調整の出発点（ベースライン）であり、最終的なパラメータはプラグインの
プレビューで映像を確認しながらユーザーが決定する。

## 出力パラメータ一覧

### 算出するパラメータ（モーション解析ベース）

| パラメータ | 単位/レンジ | プラグイン内部スケール | 算出根拠 |
|---|---|---|---|
| Smoothness | 数値 1–300 | コア値 ×100（default 50） | PSDのshake power ratio + RMS角速度 |
| Zoom limit | % 51–300 | `max_zoom` そのまま | Smoothness + RMS角速度からの推定 |
| FOV | 倍率 0.1–3.0 | コア `fov` そのまま | 算出せずベースライン **1.0** を提示（将来課題） |

> **スケールについて**: プラグインの Smoothness はコア値×100（min:1/max:300/default:50）。
> gyrotriage が従来 `smoothness_pct`（15–50）として算出していた値は、**変換なしでそのまま**
> プラグインの Smoothness 値として使える（28% → `28`）。詳細は
> `docs/plugin-recommendation-feasibility.ja.md` §3。

### 固定推奨パラメータ（DJI機特性ベース、算出なし）

| パラメータ | 推奨値 | 理由 |
|---|---|---|
| Integration method | None | DJI機はクォータニオン記録済みのため再積分不要 |
| Lens correction | 100 | 正しいDJIレンズプロファイル/プリセット読込を前提に完全適用 |

その他（Horizon lock/roll, Additional pitch/yaw, Video/Input rotation, Video speed,
Disable stretch）はユーザー意図・機材依存のためデフォルトのまま。ケース別の設定指針は
`docs/plugin-recommendation-feasibility.ja.md` §5 を参照。

## 入力データ

角速度時系列（Pitch/Roll/Yaw、単位: °/s）を`analyze.rs`で算出済み。元データはDJI MP4内の
クォータニオン姿勢データから連続フレーム間の差分回転をオイラー角に分解したもの。

## 算出パイプライン

```
MP4 → クォータニオン抽出 → 角速度時系列 → FFT/PSD → shake power ratio推定 → パラメータ変換
```

### ステップ1: PSD（パワースペクトル密度）計算

各軸（Pitch/Roll/Yaw）ごとにPSDを計算し、周波数ビンごとに加算して合成スペクトルを得る。

1. Hann窓を適用してスペクトルリーケージを抑制
2. `rustfft`クレートでFFTを実行
3. 片側PSDを計算: `PSD[k] = |X[k]|^2 / (N × fs) × 2`
4. 軸ごとのPSDを加算: `PSD[k] = PSD_pitch[k] + PSD_roll[k] + PSD_yaw[k]`

**なぜ軸ごと加算か（時間領域RSSではなく）**: 時間領域でRSS（`sqrt(p²+r²+y²)`）合成すると信号が常に非負に整流され、巨大なDC（直流）成分が混入する。このDCが全パワーを水増しし、shake power ratioを0付近まで潰すため、実際のブレ量に関係なくSmoothnessが下限15付近に張り付く。PSDの加算は線形で各軸のゼロ平均を保つため、余計なDCが生じない。

**なぜPSDを使うか**: 角速度信号は「意図した動き（低周波）」と「ブレ/振動（高周波）」の重ね合わせ
である。この2つは周波数帯域が明確に分離しており、PSDでその境界を客観的に検出できる。

FPVドローンの典型的な周波数帯域:

| 帯域 | 周波数 | 発生源 |
|---|---|---|
| 意図した動き | < 1 Hz | パン・チルト・旋回 |
| 手ブレ/風 | 3–10 Hz | 機体の姿勢変動 |
| モーター振動 | 20–80 Hz | プロペラ回転 |

### ステップ2: 主カットオフ周波数 fc の推定（shake power ratioのため）

0.5–5 Hzの範囲でPSD（移動平均平滑化済み）の極小値（谷）を探索する。この谷は意図した動きの
周波数帯とブレの周波数帯の間に現れる。最小値: 0.3 Hz。

このカットオフを境に、**shake power ratio**（カットオフ以上の周波数帯のパワーが全体に占める割合）
を計算する。これが Smoothness 算出の主入力になる。

### ステップ3: 各パラメータの算出

#### Smoothness（1–300）

PSDから算出した **shake power ratio** とRMS角速度から決定する。算出値はそのまま
プラグインの Smoothness 値として入力できる。

```
base = 15 + 35 × shake_power_ratio
velocity_factor = 0.85 (rms < 3°/s) ~ 1.15 (rms > 15°/s)
smoothness = clamp(base × velocity_factor, 15, 50)
```

**なぜshake power ratioか**: 信号のうちブレが占める割合が大きいほど、より強いスムージングが
必要になる。shake_power_ratio = 0なら信号は全て意図した動きであり最低限のスムージングで十分。
shake_power_ratio = 1なら全てブレであり最大のスムージングが必要。

**なぜvelocity_factorか**: 同じshake ratioでも、RMS角速度が小さい（全体的に動きが少ない）場合は
弱めのスムージングで十分であり、RMS角速度が大きい場合はより強いスムージングが効果的。

**FPVでの推奨範囲（20–35）との整合**: shake_power_ratioが0.15–0.60の範囲で20–35に収まる。
FPVの典型的なフライトデータはこの範囲に入る。

#### Zoom limit (%)

SmoothnessとRMS角速度から推定する。

```
base = 105 + (smoothness - 15) × 25 / 35
velocity_extra = min(rms_velocity / 20 × 5, 10)
zoom_limit = clamp(base + velocity_extra, 105, 140)
```

**なぜsmoothnessとRMS角速度の組み合わせか**: スムージングが強いほど、フレーム間の補正量が
大きくなり、黒縁を隠すために必要なズーム量が増加する。また、RMS角速度が大きい（ブレが大きい）
映像ではフレームごとの補正量の最大値も大きくなるため、追加のズーム余裕が必要になる。

**FPVでの目安（110–125%）との整合**: smoothness 20–35、RMS 5–15°/sの典型的なFPVデータで
110–120%に収まる。なおプラグインの Zoom limit レンジは 51–300% で、本算出の 105–140% は
その範囲内に収まる。

#### FOV（倍率）

モーション量から「どれだけ寄せる/引くか」を提案する余地はあるが、Zoom limit（動的クロップの
上限）との役割分担が難しく、過剰クロップを招きやすい。現状は算出ロジックを持たず、
**ベースライン 1.0**（プラグインのデフォルト）を提示する。FOV自動算出は将来課題とする。

## 実装ファイル

| ファイル | 役割 |
|---|---|
| `src/spectrum.rs` | FFT/PSD計算、shake power ratio算出 |
| `src/recommend.rs` | PSD結果からプラグインパラメータへの変換 |
| `src/analyze.rs` | クォータニオン→角速度変換、統計量算出 |

## 制約と注意事項

- 推奨値は信号処理に基づく客観的な推定であり、映像の主観的な品質評価は含まない
- パン・チルト等の意図した動きとブレの区別は周波数帯域の分離に依存しており、非常にゆっくりした
  ブレ（<0.3Hz）は意図した動きと区別できない
- 極端に短いクリップ（2秒未満）では周波数分解能が不足し、推奨精度が低下する
- プラグインの単一 Smoothness スライダーに対応する。Max smoothness / Max smoothness at high
  velocity / Zooming speed といった単体版固有の詳細パラメータは提示しない（→ ADR-005）
