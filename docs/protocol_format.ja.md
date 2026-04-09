# プロトコル定義 TOML リファレンス

Glass は `protocols/` ディレクトリ配下の `*.toml` を起動時にスキャンし、設定画面から選択できるようにします。各ファイルは 1 つのプロトコル定義を表します。

ファイルは大きく分けて以下の 3 セクションから構成されます。

1. `[protocol]` — メタ情報・フレーム抽出ルール・シーケンス図設定
2. `[[protocol.frame_rules]]` — 生バイト列からフレームを切り出すルール（複数可）
3. `[[messages]]` — 切り出されたフレームを識別・デコードするメッセージ定義（複数可）

---

## 1. `[protocol]` セクション

```toml
[protocol]
title = "My Protocol"          # 必須。設定 UI の選択肢に表示される名前
frame_idle_threshold_ms = 1.7  # 省略時 5.0
```

| キー | 型 | 既定 | 説明 |
|---|---|---|---|
| `title` | string | (必須) | プロトコル名。重複する場合はファイル名で区別される |
| `frame_idle_threshold_ms` | float | `5.0` | この秒数未満の IDLE はフレーム途中の小ギャップとみなして同一フレーム扱いする |

### 1.1 `[protocol.sequence]` — シーケンス図設定 (省略可)

シーケンス図ビューでフレームの送信元 / 宛先 / マスタ表示を制御します。

```toml
[protocol.sequence]
master = "Main"        # 常にシーケンス図の左端に固定する参加者名
broadcast = "99"       # 宛先がこの値のとき [Broadcast] ノートとして描画
source = "{Src}"       # メッセージごとに sequence_source を持たない場合のデフォルト式
destination = "{Dst}"  # 同様に sequence_destination のデフォルト式
```

| キー | 型 | 説明 |
|---|---|---|
| `master` | string | シーケンス図の最左端に固定する参加者。マスタ-スレーブ系で常に左に置きたい場合に指定 |
| `broadcast` | string | 解決後の宛先文字列がこの値と一致したらブロードキャスト扱いする |
| `source` | string | 各メッセージの `sequence_source` が未設定のときのデフォルト式 |
| `destination` | string | 同上、宛先のデフォルト式 |

`source` / `destination` の **式 (expression) 構文** はメッセージ側 (`sequence_source` / `sequence_destination`) と共通で、後述の「式構文」を参照。

---

## 2. `[[protocol.frame_rules]]` — フレーム抽出ルール

受信バイト列から「フレームの先頭バイト (trigger)」を見つけると、対応する `frame_rule` の方式で 1 フレーム分のバイト列を切り出します。複数のルールを書くと、最初にマッチした trigger に対応するルールが使われます。

### 2.1 固定長フレーム

```toml
[[protocol.frame_rules]]
trigger = "05"  # ENQ
length = 4      # trigger 含めて 4 バイトで 1 フレーム
```

### 2.2 終端バイト + 追加バイトの可変長フレーム

```toml
[[protocol.frame_rules]]
trigger = "02"     # STX
end = "03"         # ETX を見つけたら終了
end_extra = 2      # ETX の後ろ 2 バイトもフレームに含める (例: CRC)
max_length = 256   # 安全のため最大長
```

### 2.3 全フィールド一覧

| キー | 型 | 既定 | 説明 |
|---|---|---|---|
| `trigger` | hex string | (必須) | フレーム先頭バイト。`"02"` のように HEX 2 桁 |
| `length` | int | — | 固定長指定。trigger を含めた合計バイト数 |
| `end` | hex string | — | 可変長フレーム用の終端バイト |
| `end_extra` | int | `0` | `end` バイト後に取り込む追加バイト数 (チェックサム/CRC など) |
| `max_length` | int | `512` | 安全上限。これを超えると当該 trigger からのフレームは破棄される |
| `checksum` | table | — | チェックサム / CRC 検証仕様 (後述) |

`length` と `end` は通常どちらか一方を指定します。両方ある場合は `length` が優先されます。

### 2.4 `checksum = { ... }` — チェックサム / CRC 検証

フレーム末尾の検査値を自動検証し、不一致なら UI 上でハイライトします。

```toml
checksum = { algorithm = "crc16_arc", range = "after_trigger_to_end", size = 2, endian = "big" }
```

| キー | 型 | 既定 | 説明 |
|---|---|---|---|
| `algorithm` | enum | (必須) | 後述のアルゴリズム識別子 |
| `range` | enum | (必須) | 検査範囲 |
| `size` | int | アルゴリズム既定 | 検査値のバイト数 (フレーム末尾から取得) |
| `endian` | enum | `"big"` | 16bit 系のみ意味あり。`"big"` または `"little"` |

#### `algorithm` (snake_case)

| 値 | 説明 |
|---|---|
| `crc16_arc` | poly=0x8005, init=0x0000, refin/refout=true (2 byte) |
| `crc16_modbus` | poly=0x8005, init=0xFFFF, refin/refout=true (2 byte) |
| `crc16_ccitt_false` | poly=0x1021, init=0xFFFF (2 byte) |
| `crc16_xmodem` | poly=0x1021, init=0x0000 (2 byte) |
| `crc8` | CRC-8/SMBus, poly=0x07, init=0x00 (1 byte) |
| `sum8` | 8bit 加算の下位 8bit (1 byte) |
| `xor8` | 単純 XOR (1 byte) |
| `bcc` | 8bit 加算の二の補数 (1 byte) |

#### `range` (snake_case)

| 値 | 計算対象 |
|---|---|
| `whole_frame_excluding_checksum` | フレーム全体から末尾検査値を除いたバイト列 |
| `trigger_to_end` | trigger を含めて末尾検査値の直前まで |
| `after_trigger_to_end` | trigger の次のバイトから末尾検査値の直前まで |
| `after_trigger_before_end` | trigger の次のバイトから `end` バイトの直前まで (`end` を除外) |

---

## 3. `[[messages]]` — メッセージ定義

切り出された 1 フレームを HEX 文字列化し、`pattern`（正規表現）でマッチしたメッセージ定義によりタイトル・色・フィールド分解が決まります。複数定義があれば先頭からマッチを試し、最初にマッチしたものが採用されます。

### 3.1 最小例

```toml
[[messages]]
id = "msg_status"
title = "Status<01>"
color = "5B9BD5"
pattern = "^02[0-9A-F]{6}303103[0-9A-F]{4}$"
```

### 3.2 全フィールド一覧

| キー | 型 | 既定 | 説明 |
|---|---|---|---|
| `id` | string | (必須) | プロトコル内で一意の識別子。検索 / 状態保持に使われる |
| `title` | string | (必須) | UI に表示される電文名 |
| `pattern` | regex string | (必須) | フレームの **HEX 大文字文字列** に対する正規表現 (例: `^02[0-9A-F]{6}03[0-9A-F]{4}$`) |
| `color` | hex RGB string | — | タイトル表示色。`"FF8800"` のような 6 桁 HEX |
| `first_byte` | hex string | — | マッチ高速化のための先頭バイトヒント。指定するとパターン照合前にバイト一致を確認する |
| `sequence_source` | expr string | — | シーケンス図の送信元式（`[protocol.sequence].source` を上書き） |
| `sequence_destination` | expr string | — | シーケンス図の宛先式（同上） |
| `fields` | table array | `[]` | フィールド定義 (後述) |

### 3.3 `[[messages.fields]]` — フィールド分解

```toml
[[messages.fields]]
name = "Addr"
offset = 2          # フレーム先頭からのバイトオフセット (0 始まり)
size = 2            # バイト数
inline = true       # true ならリスト表示の電文タイトル行に並べて表示
description = "宛先アドレス"  # 詳細表示時の備考 (省略可)
```

| キー | 型 | 既定 | 説明 |
|---|---|---|---|
| `name` | string | (必須) | フィールド名。シーケンス図式 `{name}` で参照される |
| `offset` | int | (必須) | フレーム先頭 (trigger を含む) からのバイトオフセット |
| `size` | int | (必須) | バイト数 |
| `inline` | bool | `false` | true でリスト行へインライン表示。省略時は詳細表示のみ |
| `description` | string | — | フィールドの説明文。詳細表示で表示 |

フィールド値はバイト列を **ASCII 文字列としてそのまま** 取り出した結果です。値が空の場合は `—` 扱いになります。

---

## 4. シーケンス図用の式構文 (expression)

`sequence_source` / `sequence_destination` および `[protocol.sequence]` の `source` / `destination` で使われる文字列は、以下のいずれかとして評価されます。

| 形式 | 例 | 意味 |
|---|---|---|
| `=リテラル` | `"=Main"` | 先頭が `=` なら以降をそのままリテラル参加者名として使う |
| `{field}` を含む | `"{Type}:{Addr}"` | `{name}` 部分をフィールド値で置換 (テンプレート) |
| プレーンなフィールド名 | `"Addr"` | そのフィールドの値をそのまま使う |

参加者名に `:` が含まれていた場合は全角コロン (`：`) に、空白は `_` に自動変換されます (Mermaid 構文と衝突するため)。

### 例

```toml
[protocol.sequence]
master = "Main"

[[messages]]
id = "req"
title = "Request"
pattern = "^..$"
sequence_source = "=Main"               # 必ず "Main" から
sequence_destination = "{Type}:{Addr}"  # フィールド値からテンプレ生成
```

---

## 5. 完全な最小例

```toml
[protocol]
title = "Demo"
frame_idle_threshold_ms = 2.0

[protocol.sequence]
master = "M"
broadcast = "99"

[[protocol.frame_rules]]
trigger = "05"
length = 4

[[protocol.frame_rules]]
trigger = "02"
end = "03"
end_extra = 2
max_length = 256
checksum = { algorithm = "crc16_arc", range = "after_trigger_to_end", size = 2, endian = "little" }

[[messages]]
id = "enq"
title = "ENQ"
color = "78A0DC"
pattern = "^05[0-9A-F]{6}$"
sequence_source = "=M"
sequence_destination = "{Type}:{Addr}"

[[messages.fields]]
name = "Type"
offset = 1
size = 1
inline = true

[[messages.fields]]
name = "Addr"
offset = 2
size = 2
inline = true

[[messages]]
id = "msg_01"
title = "Status<01>"
color = "5B9BD5"
pattern = "^02[0-9A-F]{6}303103[0-9A-F]{4}$"
sequence_source = "=M"
sequence_destination = "{Type}:{Addr}"

[[messages.fields]]
name = "Type"
offset = 1
size = 1
inline = true

[[messages.fields]]
name = "Addr"
offset = 2
size = 2
inline = true
```

実際に動く長めの例として `protocols/sample.toml` を参照してください。
