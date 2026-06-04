# Gyroflow プラグイン版 推奨パラメータ Feasibility 検証

> Status: 検証完了 → 方針決定済み（**ADR-005** として正式化）
> 対象ブランチ: develop
> 検証日: 2026-06-04
>
> 本ドキュメントは Feasibility 検証の詳細分析である。決定の正式な記録は
> `docs/adr-005-plugin-target.md`（ADR-005）を参照。

## 0. 方針決定（2026-06-04）

**単体版（スタンドアロンアプリ）向けの推奨パラメータ提示はサポートを終了し、
Gyroflow プラグイン版に一本化する。**

理由:
- 単体版とプラグイン版で Smoothness の入力スケールが異なり（§3）、同じ数値が
  単体版では桁違いに誤った値になるリスクがある。両対応は混乱と事故の元。
- プラグイン版は gyrotriage の `%` 出力をスケール変換なしでそのまま使える（§3）。
- プラグイン専用にすることで、単体版固有の Max smoothness 系3パラメータの
  算出・提示が不要になり、ロジック・出力ともに単純化できる（§4）。

影響:
- 単体版向けの Max smoothness / Max smoothness at high velocity / Zooming speed の
  算出・出力は**廃止対象**（`src/recommend.rs`、`src/output.rs`、
  `docs/recommendation-algorithm.*`）。
- 出力は「プラグイン版の値」としてのみ提示する（単体版/プラグイン版の出し分けは不要）。

## 1. 背景と目的

現在の gyrotriage は **Gyroflow 単体版（スタンドアロンアプリ）** 向けに 5 パラメータ
（Smoothness / Max smoothness / Max smoothness at high velocity / Zoom limit / Zooming speed）
を算出・提示している（`src/recommend.rs`、`docs/recommendation-algorithm.ja.md`）。

一方、DaVinci Resolve / Adobe / OpenFX 上で動作する **Gyroflow プラグイン版** は
「Adjust parameters」パネルに別構成のパラメータを持つ。本ドキュメントは
**プラグイン版パラメータを gyrotriage で算出可能か** を検証し、
算出できないパラメータについては **どのケースでどう設定すべきか** の指針をまとめる。

## 2. プラグイン版パラメータ定義（ソース根拠）

以下は Gyroflow プラグイン公式ソース `gyroflow-plugins/common/src/lib.rs`
（`get_param_definitions()`, line 280-296）から取得した正確な定義。

| # | ラベル | 内部ID | min | max | default | 単位/スケール |
|---|--------|--------|-----|-----|---------|--------------|
| 1 | Smoothness | `Smoothness` | 1.0 | 300.0 | **50.0** | コア値 ×100（後述） |
| 2 | Zoom limit | `ZoomLimit` | 51.0 | 300.0 | **130.0** | %（`max_zoom`そのまま） |
| 3 | Lens correction | `LensCorrectionStrength` | 0.0 | 100.0 | **100.0** | %（補正適用率） |
| 4 | Horizon lock | `HorizonLockAmount` | 0.0 | 100.0 | **0.0** | %（水平ロック強度） |
| 5 | Horizon roll | `HorizonLockRoll` | -100.0 | 100.0 | **0.0** | ロック水平線の傾き調整 |
| 6 | Additional pitch | `AdditionalPitch` | -180.0 | 180.0 | **0.0** | 度 |
| 7 | Additional yaw | `AdditionalYaw` | -180.0 | 180.0 | **0.0** | 度 |
| 8 | Video rotation | `Rotation` | -360.0 | 360.0 | **0.0** | 度（出力映像の回転） |
| 9 | Input rotation | `InputRotation` | -360.0 | 360.0 | **0.0** | 度（入力の取付向き補正） |
| 10 | FOV | `Fov` | 0.1 | 3.0 | **1.0** | 倍率（<1で寄り） |
| 11 | Video speed | `VideoSpeed` | 0.0001 | 1000.0 | **100.0** | %（再生速度） |
| 12 | Disable Gyroflow's stretch | `DisableStretch` | — | — | **false** | bool |
| 13 | Integration method | `IntegrationMethod` | enum | — | **"VQF"** | None/Complementary/VQF/Simple gyro/Simple gyro+accel/Mahony/Madgwick |

> 添付スクリーンショットの値（Smoothness 35.8, Zoom limit 130.0, Lens correction 100.0,
> FOV 1.000, Video speed 100, Integration method None）はすべてこの定義レンジ内に収まり、
> 多くがデフォルト値であることを確認済み。
> Integration method がデフォルトの "VQF" ではなく "None" なのは DJI 機（後述）の特性による。

## 3. 単体版 ↔ プラグイン版 スケール・マッピング

プラグインが各値を Gyroflow コアへ渡す際のスケール係数を、ソースから特定した
（`gyroflow-plugins/common/src/lib.rs` line 521、`openfx/src/gyroflow.rs` 経由）。

```
// プロジェクト読込時、コア値 → プラグイン表示値
params.set_f64(Params::Smoothness, smoothness * 100.0)   // line 705
cache_key!(KeyframeType::SmoothingParamSmoothness, Params::Smoothness, 100.0)  // line 521
cache_key!(KeyframeType::MaxZoom,                  Params::ZoomLimit,   1.0)   // line 520
```

**確定したマッピング:**

| プラグイン値 | = コア値 | 係数 |
|-------------|---------|------|
| Smoothness | コア `smoothness` | **×100** |
| Zoom limit | コア `max_zoom` | ×1（=%） |
| FOV | コア `fov` | ×1 |

### gyrotriage 現行出力との対応

gyrotriage は現在テキストで `smoothness=28%`, `zoom_limit=115%` のように出力する
（`src/output.rs` line 25-26）。これらは **そのままプラグイン値として使える**:

| gyrotriage 現行出力 | プラグイン版 入力値 | 根拠 |
|--------------------|-------------------|------|
| `smoothness_pct`（15–50） | Smoothness（同値） | 両者ともコア値×100。例: 28% → `28.0` |
| `zoom_limit_pct`（105–140） | Zoom limit（同値） | 両者とも%。例: 115% → `115.0` |
| （未算出） | FOV | デフォルト 1.0 |

> **重要な利点**: gyrotriage の `smoothness_pct`（百分率表現）は、
> **プラグイン版の Smoothness スケール（1–300、例 28 と入力）にそのまま一致する**。
> 値変換が不要で直感的。これが単体版を切り捨ててプラグイン版に一本化する後押しになった
> （§0）。単体版の Smoothness 欄は 0–3 のコア値スケール（例 0.28 と入力）で、
> 同じ数値を入れると桁違いの事故になっていた。
>
> ⚠️ **要確認の前提**: 上表は gyrotriage の `smoothness_pct` が「コア値×100」に等しい前提。
> プラグイン版に一本化するなら、`recommend()` の出力がそのままプラグイン Smoothness 値
> （1–300 レンジ内）として妥当かを実機で一度確認すること。

### gyrotriage が算出しているがプラグインに無いパラメータ

- **Max smoothness** / **Max smoothness at high velocity** / **Zooming speed** の3つは
  プラグイン UI に存在しない。プラグインの単一 `Smoothness` スライダーに集約されており、
  詳細な smoothing アルゴリズム設定は `.gyroflow` プロジェクトファイル側に焼かれる。
- → プラグイン版に一本化する方針（§0）に伴い、**この3パラメータの算出・出力は廃止する**。

## 4. 算出可否の分類

| パラメータ | 算出可否 | 区分 | 備考 |
|-----------|---------|------|------|
| Smoothness | ✅ 可 | 📊 モーション解析 | 既存ロジック流用、変換不要 |
| Zoom limit | ✅ 可 | 📊 モーション解析 | 既存ロジック流用、変換不要 |
| FOV | △ 条件付き可 | 📊 モーション解析 | 新規設計が必要。§6 参照 |
| Lens correction | ❌ 不可 | ⚙️ 機材依存 | デフォルト 100 推奨 |
| Horizon lock | ❌ 不可 | 🎨 演出意図 | §5 ガイド参照 |
| Horizon roll | ❌ 不可 | 🎨 演出意図 | §5 |
| Additional pitch | ❌ 不可 | 🎨 演出意図 | §5 |
| Additional yaw | ❌ 不可 | 🎨 演出意図 | §5 |
| Video rotation | ❌ 不可 | 🎨 演出/機材 | §5 |
| Input rotation | ❌ 不可 | ⚙️ 取付依存 | §5 |
| Video speed | ❌ 不可 | 🎨 編集意図 | §5 |
| Disable stretch | ❌ 不可 | ⚙️ レンズ/撮影設定 | §5 |
| Integration method | ❌ 不可（固定推奨） | ⚙️ 機材依存 | DJI は **None** 固定。§5 |

凡例: 📊 = ブレ解析から算出可能 / 🎨 = ユーザーの演出意図 / ⚙️ = 機材・レンズ・撮影設定依存

**算出できるのは実質 Smoothness・Zoom limit の2つ（+ 設計次第で FOV）。**
プラグイン版対応は「新アルゴリズムの追加」ではなく、主に
**既存2パラメータをプラグイン用語・スケールで再提示する**作業になる。

## 5. 算出できないパラメータの設定ガイド（ケース別）

モーション解析からは決定できないが、**gyrotriage は DJI FPV 専用ツール**であるため、
機材特性に基づいて「推奨デフォルト」と「調整が必要なケース」を提示できる。
以下を出力やドキュメントに同梱することを想定する。

### Lens correction（推奨: 100 のまま）
- **通常**: `100`。レンズ歪み補正を完全適用。Gyroflow のスタビライズ精度はレンズプロファイル
  に依存するため、DJI 機の正しいプロファイル/プリセットを読み込んだ上で 100 が基本。
- **下げるケース**: 補正後に画面端が不自然に伸びる/魚眼の残りが気になる等、
  プロファイルが完全一致しない場合のみ 80〜100 の範囲で微調整。

### Horizon lock（推奨: 0 / 撮影意図次第）
- **通常（FPV らしさ重視）**: `0`。FPV の倒し込み・バンクをそのまま残す。
- **水平を保ちたい場合**: `50〜100`。風景・シネマティック用途で地平線を水平に固定。
  ⚠️ ロックを強めるほど追加ズームが必要になり、Zoom limit を上げる必要が出る。
- gyrotriage としては FPV 既定の `0` を推奨デフォルトとし、用途次第で上げる旨を注記する。

### Horizon roll（推奨: 0）
- Horizon lock を有効化したときのみ意味を持つ。ロック後に地平線が傾いて見える場合に
  `-100〜100` で微調整。Horizon lock = 0 なら触らない。

### Additional pitch / Additional yaw（推奨: 0）
- 安定化後の構図を後から振りたいときの微調整（度）。
- **使うケース**: カメラがやや下/上を向いていた、被写体を中央に寄せたい等。
- モーション解析とは無関係なので常に `0` を初期提示。

### Video rotation（推奨: 0）
- 出力映像自体の回転（度）。
- **使うケース**: 縦位置素材、上下逆さマウント、90/180 度の編集上の回転が必要な場合。
- DJI 横位置の通常撮影では `0`。

### Input rotation（推奨: 0、ただし取付向き次第）
- 入力（カメラ取付向き）の補正（度）。
- **使うケース**: カメラを縦向き/逆さに搭載した、素材が回転して読み込まれる場合に
  `90 / 180 / 270` 等。安定化が破綻している場合は真っ先に疑う項目。
- 標準的な DJI 機の搭載姿勢では `0`。

### Video speed（推奨: 100）
- 再生速度（%）。エディタ側で速度変更/スローモーをかけた際にスタビライズと同期させる。
- **使うケース**: タイムラインでクリップをスロー/早送りした場合にその倍率を反映。
- 等速編集なら `100`。

### Disable Gyroflow's stretch（推奨: false / 撮影設定次第）
- レンズプロファイルで Input stretch（アナモルフィック等）を使い、かつエディタ側で
  別途デストレッチした場合にのみ `true`。
- DJI Neo/Avata の標準 4:3 / 16:9 撮影では通常 **false（オフ）**。
- DJI 機特有の注意: Neo は **4:3 撮影が必須**（16:9 は EIS 強制 ON でモーションデータなし）。
  この撮影要件は stretch とは別問題だが、素材取り込み時の前提として併記する価値がある。

### Integration method（推奨: None ← DJI 固定）
- **DJI 機は IMU 生データではなくクォータニオン（カメラ3D姿勢）を記録**している
  （`djmd` protobuf トラック）。再積分は不要で、記録済み姿勢をそのまま使うため **`None`**。
- プラグインのデフォルトは `VQF`（生ジャイロ前提）だが、**DJI 素材では必ず `None` に変更**する。
- gyrotriage は DJI 専用ツールなので、ここは **確信を持って `None` を推奨**できる。
  （添付スクリーンショットでも None が選択されている = 正しい設定）

## 6. FOV の算出について（要設計）

FOV はモーション量から「どれだけ寄せる/引くか」を提案できる余地があるが、
現状ロジックが無く、Zoom limit との役割分担を整理する必要がある。

- Zoom limit = 安定化のための**動的クロップの上限**（自動調整の天井）
- FOV = **静的な視野倍率**（全体の寄り/引き）

両者を独立に推奨すると過剰クロップになりやすいため、安易な算出は推奨しない。
基本は **FOV = 1.0（デフォルト）** を提示し、FOV 自動算出は将来課題とする。

## 7. Feasibility 結論

- **結論: 部分的に実現可能。低コストで価値あり。**
- プラグイン版で意味のある自動算出ができるのは **Smoothness・Zoom limit の2つ**。
  いずれも既存ロジックを**スケール変換なしで**流用でき、実装コストは小さい。
- 残り 11 項目はモーション解析からは決定不能だが、**DJI 専用ツールという強みを生かし、
  推奨デフォルト + ケース別ガイド（§5）を提示**することで実用的な価値を出せる。
  特に **Integration method = None** は DJI 素材で確信を持って提示できる重要項目。
- 単体版サポート終了の方針（§0）により、Max smoothness 系3パラメータの算出は廃止し、
  ロジック・出力ともに単純化できる。

### 推奨する実装方針（案）

1. **単体版向け出力を廃止し、プラグイン版の値のみを提示**する（出し分けフラグは設けない）。
   - `recommend()` から Max smoothness / Max smoothness at high velocity / Zooming speed を削除。
   - `src/output.rs` の出力フォーマットをプラグイン版パラメータ名に差し替え。
2. Smoothness・Zoom limit を**そのままの数値**で提示（プラグインの Adjust parameters に直接入力可）、
   FOV は 1.0。
3. §5 の固定推奨・ケース別ガイドを出力に同梱（特に **Integration method = None** を明示）。
4. §3 の ⚠️ を実機で確認: `recommend()` の Smoothness 出力がプラグインの 1–300 レンジ内で
   妥当か検証する。
5. 単体版に言及している既存ドキュメント（`docs/recommendation-algorithm.*`, `README`, `CLAUDE.md`,
   `docs/spec.md` 等）をプラグイン版前提に更新する。

## 8. 出典

- Gyroflow プラグイン公式ソース（パラメータ定義・スケール係数）:
  - https://github.com/gyroflow/gyroflow-plugins （`common/src/lib.rs`, `openfx/src/gyroflow.rs`）
- Gyroflow 公式ドキュメント:
  - https://docs.gyroflow.xyz/app/getting-started/basic-usage/stabilization
  - https://docs.gyroflow.xyz/app/video-editor-plugins/davinci-resolve-openfx
- 関連: `docs/recommendation-algorithm.ja.md`（単体版アルゴリズム）、`src/recommend.rs`
