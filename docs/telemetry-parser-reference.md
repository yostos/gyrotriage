# telemetry-parser 技術リファレンス

gyrotraigeプロジェクトに関連する telemetry-parser crate の API・データ型・DJI固有技術情報。

## Public API

### `Input` 構造体

メインエントリポイント。MP4ファイルからテレメトリデータを読み取る。

```rust
let mut stream = std::fs::File::open(path)?;
let filesize = stream.metadata()?.len() as usize;
let input = Input::from_stream(&mut stream, filesize, "mp4", options)?;
```

主要フィールド・メソッド:

| フィールド/メソッド | 型 | 説明 |
|---|---|---|
| `Input::from_stream()` | コンストラクタ | ストリームからテレメトリを読み取る |
| `camera_type()` | `String` | カメラメーカー名（例: "DJI"） |
| `camera_model()` | `Option<String>` | カメラモデル名（例: "DJI Neo"） |
| `samples` | `Option<Vec<SampleInfo>>` | サンプル群（各サンプルにタグマップを含む） |

### `InputOptions`

```rust
// gyrotraigeではデフォルトで十分
InputOptions::default()
```

## データ型

### `Quaternion<T>`

クォータニオン（四元数）。カメラの3D姿勢を表す。

```rust
pub struct Quaternion<T> {
    pub w: T,  // スカラー部
    pub x: T,  // ベクトル部 i
    pub y: T,  // ベクトル部 j
    pub z: T,  // ベクトル部 k
}
```

実装済みトレイト: `Mul`, `Sub`, `Neg`, `norm_squared()`

**注意**: `conjugate()` メソッドは未実装。自前で実装する必要がある: `Quaternion { w: q.w, x: -q.x, y: -q.y, z: -q.z }`

### `TimeQuaternion<T>`

タイムスタンプ付きクォータニオン。

```rust
pub struct TimeQuaternion<T> {
    pub t: f64,           // タイムスタンプ（マイクロ秒）
    pub v: Quaternion<T>, // クォータニオン値
}
```

### `TimeVector3`

3Dベクトル（加速度計データ等に使用）。本プロジェクトでは直接使用しない。

## タグシステム

テレメトリデータはタグのツリー構造で格納される。

```rust
type GroupedTagMap = BTreeMap<GroupId, TagMap>;
```

### クォータニオン抽出パターン

```rust
for sample in input.samples.unwrap_or_default() {
    let tag_map = sample.tag_map.as_ref().unwrap();
    if let Some(quat_group) = tag_map.get(&GroupId::Quaternion) {
        if let Some(tag) = quat_group.get(&TagId::Data) {
            let quats: &Vec<TimeQuaternion<f64>> = tag.get_t(TagId::Data);
            // quats を処理
        }
    }
}
```

### 関連する GroupId / TagId

| GroupId | TagId | 内容 |
|---|---|---|
| `GroupId::Quaternion` | `TagId::Data` | `Vec<TimeQuaternion<f64>>` |
| `GroupId::Accelerometer` | `TagId::Data` | `Vec<TimeVector3>` （本プロジェクトでは不使用） |
| `GroupId::Gyroscope` | `TagId::Data` | `Vec<TimeVector3>` （本プロジェクトでは不使用） |

## DJI固有のパース処理

### djmd コーデックタグ

DJI MP4のデータトラックは `codec_tag: "djmd"` で識別される。telemetry-parser が内部で検出・処理する。

### プロトコル

| プロトコル | 対象機種 |
|---|---|
| WM169 | DJI Neo / Neo2 |
| WA530 | DJI Avata 2 |

### protobuf 構造

```
ProductMeta → FrameMeta → DeviceAttitude → Quaternion (w, x, y, z)
```

### 座標変換

telemetry-parser内部で2段階のクォータニオン変換が行われる:

1. `q × (0.5, -0.5, -0.5, 0.5)` — 座標系の回転
2. `q × (0, 0, 1, 0)` — 最終的なカメラ座標系への変換

ユーザーコード側で追加の変換は不要。

### 不連続性処理

クォータニオンの符号反転（q と -q は同じ回転を表す）による不連続性を検出・修正:

- `norm_squared` 距離 > 1.5 で符号反転と判定
- 自動的に符号を揃えて連続的な時系列を提供

## タイムスタンプ

タイムスタンプの計算方法:

```
timestamp = frame_timestamp + (index / count) × frame_duration
```

- `frame_timestamp`: そのフレームの開始時刻
- `index`: フレーム内のサンプルインデックス
- `count`: フレーム内のサンプル数
- `frame_duration`: フレームの持続時間

単位: マイクロ秒（μs）

## サンプルレート

| 機種 | サンプルレート |
|---|---|
| DJI Neo / Neo2 | 2000 Hz（実測値。フレームあたり複数サンプルが格納される） |
| DJI Avata / Avata 2 | 400 Hz |
| DJI FPV | ～50 Hz（不適切） |

注: telemetry-parserのタイムスタンプ（`TimeQuaternion.t`）はミリ秒単位。

## バージョン情報

| ソース | バージョン | 備考 |
|---|---|---|
| crates.io | v0.2.6 | 安定版 |
| GitHub (git) | v0.3.0相当 | Avata 2 (WA530) 追加サポート |

主要依存: `mp4parse`, `prost`（protobufデコード）

本プロジェクトではGit版を使用（Avata 2対応のため）。
