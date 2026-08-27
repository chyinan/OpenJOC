!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 自定义扬声器布局

对于普通渲染，仍然推荐使用内置预设：

```sh
openjoc render-joc input.m4a --layout 7.1.4 --output render.wav
```

高级用户可以提供带版本的 JSON 布局，不需要更换渲染器，也不会创建第二套 DSP 路径：

```sh
openjoc render-joc input.m4a \\
  --layout-file fixtures/speaker-layouts/studio-irregular.json \\
  --output render.caf
```

`--layout` 和 `--layout-file` 不能同时使用。自定义布局严格按照 `speakers` 数组排序；这个顺序同时也是交错 PCM 的顺序，以及 Rust 和 C API 报告的语义标签顺序。

## 格式与坐标

当前格式是 JSON `version: 1`：

```json
{
  "version": 1,
  "name": "My Studio",
  "speakers": [
    {"name": "FL", "azimuth": 35.0, "elevation": 0.0},
    {"name": "FR", "azimuth": -35.0, "elevation": 0.0},
    {"name": "Sub", "azimuth": 0.0, "elevation": -20.0, "role": "lfe"}
  ]
}
```

渲染器使用 OpenJOC 现有的归一化笛卡尔坐标约定。在球面坐标输入中，方位角以度为单位，正值朝向 OpenJOC 左侧，正前方为 `0`；仰角也以度为单位，听音者上方为正。有效范围是方位角 `-180..=180`、仰角 `-90..=90`。内部坐标中，前后为 `y=0..1`，左右为 `x=0..1`，上下为带符号的 `z=-QMAX..QMAX`。投影器没有第二套坐标系。

`role` 默认是 `full_range`，也可以明确设置为 `lfe`。LFE 声道是逻辑输出声道，会保留声明顺序，但不会参与空间摇移。解码得到的 Base LFE 会复制到每个声明的逻辑 LFE 输出，这与现有的多 LFE 预设行为一致。本功能不会增加分频、低频管理、延迟、增益校准或房间校正。

实现最多接受 64 个输出声道，并且要求至少两个全频段扬声器。名称必须唯一且不能为空。坐标必须是有限值并处于有效范围内；重复或接近退化的全频段方向、空布局、格式错误的 JSON、未知字段和未知版本，都会在渲染前被拒绝。JSON 数字绝不会变成 NaN 或无穷大 PCM。

## 投影覆盖策略

现有通用投影器为结构合法的自定义布局定义覆盖范围，不会额外加入第二个回退渲染器。对于落在布局矩形支持范围之外的有限声源坐标，`x` 会限制到所选行的第一个/最后一个锚点，`y` 会限制到第一行/最后一行。对于多层布局，`z` 会限制到最低/最高层。相邻层之间的目标向量使用现有的等功率余弦/正弦规律混合。最终动态目标会由现有投影器归一化，因此受支持的有限扫动仍然具有确定性、有限值和有界结果。结构上不可用的布局会在构造阶段被拒绝；有限但越界的声源位置属于有定义的边界投影，不是未定义行为。

## API 与容器边界

Rust 调用方可以直接构造同一个经过校验的对象：

```rust
use openjoc_api::{OpenJocConfig, OpenJocSession};
use openjoc_scene::{SpeakerGeometry, SpeakerLayout};

let layout = SpeakerLayout::custom(
    "studio",
    vec![
        SpeakerGeometry::full_range("A", -40.0, 0.0),
        SpeakerGeometry::full_range("B", 8.0, 6.0),
        SpeakerGeometry::full_range("C", 48.0, 0.0),
    ],
)?;
let session = OpenJocSession::new(OpenJocConfig::default().with_speaker_layout(layout))?;
```

C ABI 1.4 在 `openjoc_decoder_config` 中追加了 `custom_speaker_layout`。它指向一个有序的 `openjoc_custom_speaker` 记录数组，并会在创建解码器时复制和校验。继续使用预设的 ABI 调用方可以把它留空。ABI 不要求生成临时 JSON 文件。

对于自定义物理布局，WAV 会按照声明的声道顺序写入确定且真实的无掩码 PCM；使用标准 WAVEFORMATEXTENSIBLE 扬声器掩码会错误地声称这些是标准声道身份。如果下游互操作需要保留几何信息，建议使用 CAF，因为 OpenJOC 会在那里写入带坐标的声道描述。下游播放器、FFmpeg 声道布局协商、GStreamer、DirectShow/LAV 和物理设备，仍可能有更窄的几何布局约定；渲染器支持不代表主机或设备也支持。
