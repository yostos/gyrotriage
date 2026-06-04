# ADR-005: 推奨パラメータを Gyroflow プラグイン版に一本化する

## ステータス

承認済み（2026-06-04）

## コンテキスト

gyrotriage は当初、Gyroflow **スタンドアロンアプリ**向けに 5 つのスタビライゼーション
パラメータ（Smoothness / Max smoothness / Max smoothness at high velocity / Zoom limit /
Zooming speed）のベースライン値を提示していた。

その背景には、concept.md に記した次のワークフロー想定があった。

> 「編集で 1 本の動画を複数カットに分割して使う場合、カットごとにプラグインを適用する
> 手間が発生する。そのため編集前に Gyroflow スタンドアロンで元動画単位でスタビライズを
> 済ませ、処理済みファイルを Resolve に持ち込むほうが効率的である。」

しかし、この前提は **誤りだった**。実運用では DaVinci Resolve 上で Gyroflow OpenFX
プラグインを使い、編集タイムライン内で直接スタビライズするワークフローが実際的であり、
「先にスタンドアロンで安定化してから持ち込む」運用は採用しない。

加えて、プラグイン版パラメータの Feasibility 検証
（`docs/plugin-recommendation-feasibility.ja.md`）により、以下が判明した。

- プラグイン版「Adjust parameters」は単体版と**パラメータ構成が異なる**。
  単一の `Smoothness` スライダーに集約されており、Max smoothness / Max smoothness at
  high velocity / Zooming speed は UI に存在しない（`.gyroflow` プロジェクト側に焼かれる）。
- プラグインの `Smoothness` は **コア値 ×100**（min:1 / max:300 / default:50）。
  gyrotriage の従来 `smoothness_pct`（百分率表現）は**スケール変換なしでそのまま**
  プラグイン値として使える。
- 単体版と両対応すると Smoothness の入力スケールが 0–3（コア値）と 1–300（×100）で
  食い違い、同じ数値が桁違いの誤った設定になる事故リスクがある。

## 決定

**推奨パラメータの提示対象を Gyroflow プラグイン版（DaVinci Resolve OpenFX 等）に一本化する。
スタンドアロン版向けの提示はサポートしない。**

具体的には:

1. **単体版固有の 3 パラメータを廃止する**
   — Max smoothness / Max smoothness at high velocity / Zooming speed の算出・出力をやめる。
2. **プラグイン版「Adjust parameters」形式で提示する**
   — モーション解析から算出する **Smoothness**（=従来 `smoothness_pct`、変換不要）と
   **Zoom limit** を中心に、**FOV** はベースライン 1.0 を明示する。
3. **算出できないパラメータは固定推奨/ケース別ガイドを提示する**
   — 特に **Integration method = None**（DJI はクォータニオン記録済みのため）と
   **Lens correction = 100**（正しいレンズプロファイル読込前提）を主要項目として CLI 出力に
   簡潔に同梱する。残りの項目（Horizon lock/roll, Additional pitch/yaw, Video/Input rotation,
   Video speed, Disable stretch）はユーザー意図・機材依存のため、ドキュメントでケース別に
   設定指針を示す（`docs/plugin-recommendation-feasibility.ja.md` §5）。
4. **ツールの位置づけをプラグイン中心に改訂する**
   — concept.md のスタンドアロン推奨ワークフロー記述を撤回し、Resolve OpenFX プラグイン
   前提に書き換える。

## 根拠

- 実運用ワークフローが Resolve + OpenFX プラグインであり、提示するパラメータも
  プラグイン UI に直接入力できる形が最も実用的。
- gyrotriage の従来 `%` 出力がプラグイン Smoothness スケールにそのまま一致するため、
  移行コストが小さい。
- 両対応は Smoothness スケールの取り違えによる重大な設定ミスを招くため、一本化する方が安全。
- gyrotriage は DJI FPV 専用ツールであり、Integration method = None など機材特性に基づく
  固定推奨を確信を持って提示できる。

## 影響

- `src/recommend.rs`: `Recommendation` から 3 フィールドを削除、`fov`（=1.0）を追加。
  recommend() ロジックを簡素化。
- `src/output.rs`: テキスト出力をプラグイン形式へ。主要固定推奨（Integration method=None,
  Lens correction=100）を同梱。
- `src/chart.rs`: ビジュアル出力フッターをプラグイン形式へ（→ ADR-004 を更新済み）。
- ドキュメント: concept.md / spec.md / recommendation-algorithm.{ja,en}.md / README / CLAUDE.md
  をプラグイン版前提に更新。
- 既存テスト（recommend / output / chart）の更新が必要。

## 関連

- 裏付け分析: `docs/plugin-recommendation-feasibility.ja.md`
- 影響を受ける ADR: ADR-004（ビジュアル出力フッター仕様を更新）
