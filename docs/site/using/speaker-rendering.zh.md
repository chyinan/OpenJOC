!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# JOC 扬声器与双耳渲染

本页说明当前 `render-joc` 渲染器、输出容器、时间线和输出电平的约定。当前支持状态请看[能力矩阵](../project/capabilities.md)；自定义 JSON/API 几何布局请看[自定义扬声器布局](custom-speaker-layouts.md)。

`render-joc` 会从受支持的 E-AC-3 JOC 数据载体中提取带内 EMDF，解码支持范围内的 OAMD 前缀/时间线与 JOC 重建载荷，生成 carrier-local `ReconstructionBasis` PCM。随后它会对齐 Base 和重建 PCM，根据解码后的 JOC/OAMD 状态组装桥接控制信息，再渲染为一种语义扬声器布局或双耳虚拟扬声器输出：

```text
原始 EC-3 / 支持随机访问的普通 ISO BMFF
  -> 受限的 E-AC-3 访问单元
  -> Base + RcLfe + JOC/OAMD 解码
  -> Base/RB 时间线对齐
  -> 自动组装桥接控制信息
  -> 持久化的 JocSpatialBridge
  -> 扬声器 FinalLinkedGain 或 SOFA 双耳卷积
  -> 事务式 WAV/CAF 输出
```

桥接层是受支持的普通域投影，目前仍是实验性功能。它不会解决原始创作对象身份，也不会解决编解码器域算子 `T(t)`。`ReconstructionBasis` 行仍然是解码器坐标，不是经过验证的对象分轨。

## 输入内存与预检

CLI 按访问单元读取原始 EC-3 或支持随机访问的普通 ISO BMFF 文件。完整预检后，CLI 会重新打开输入并开始渲染。预检保留原有的配置选择规则和进度总量，因此输出仍需等待预检完成。

普通渲染、性能报告、分区双耳卷积和峰值归一化均使用增量输入。整段压缩音轨不再受 512 MiB 大小上限约束；ISO BMFF 中的单个容器样本仍有 512 MiB 上限。
输出仍受容器大小限制：普通 WAV 使用 32 位 RIFF 长度字段，超出其容量的输出请使用 CAF。

增量读取保留解码器状态、DSP 精度、运算顺序和完整尾音。峰值归一化沿用原有的渲染器原生精度中间文件。性能报告仍保存逐访问单元的耗时记录，FFprobe 也会单独维护容器数据，因此输入缓冲受限不代表整个进程的内存恒定。

## 普通扬声器流程

普通渲染请使用预设：

```text
openjoc render-joc input.m4a --layout 7.1.4 -o output.wav
```

当前预设包括：

- `2.0`；
- `5.1`、`5.1.2`、`5.1.4`；
- `7.1`、`7.1.2`、`7.1.4`、`7.1.6`；
- `9.1`、`9.1.2`、`9.1.4`、`9.1.6`；
- `22.2`。

所有预设使用同一个通用的全 XYZ、多层投影器。预设定义语义扬声器身份、几何布局、LFE 归属和 PCM 顺序，但不会选择另一套专用渲染器实现。

`2.0` 表示物理上的 `FL, FR` 扬声器，不是双耳输出。Base 声道使用选定的 E-AC-3 立体声下混策略，重建声部则投影到物理立体声布局：

```text
openjoc render-joc input.m4a --layout 2.0 --downmix auto -o stereo.wav
openjoc render-joc input.m4a --layout 2.0 --downmix loro -o stereo-loro.wav
openjoc render-joc input.m4a --layout 2.0 --downmix ltrt -o stereo-ltrt.wav
```

普通流程会根据解码元数据和组件状态自动生成完整的桥接控制信息。`--topology bridge-control.json` 是高级的完整覆盖/测试输入。自动生成的控制信息和显式控制信息永远不会被隐式合并。

## 自定义扬声器几何

高级用户可以用带版本的 JSON 替换预设：

```sh
openjoc render-joc input.m4a \\
  --layout-file studio-layout.json \\
  -o studio.caf
```

`--layout` 和 `--layout-file` 不能同时使用。自定义布局使用同一个渲染器，最多支持 64 个有序输出声道。JSON 的 `speakers` 数组同时决定语义标签顺序和交错 PCM 顺序。LFE 条目仍然是排除在空间投影器之外的逻辑输出。

坐标范围、校验规则、投影覆盖、Rust 构造方式、C ABI 1.4 描述结构和示例，统一记录在[自定义扬声器布局](custom-speaker-layouts.md)中。渲染器支持不代表播放器、框架、音频设备或容器也能接受同样的任意几何布局。

## WAV 与 CAF 的准确表达

`22.2` 同时支持 `.wav` 和 `.caf`。两种容器用不同方式记录声道身份：

| 布局 | WAV（`.wav`） | CAF（`.caf`） |
|---|---|---|
| 可由 WAVEFORMATEXTENSIBLE 掩码表示的标准预设 | 使用含真实声道身份和掩码的 WAVEFORMATEXTENSIBLE | 语义声道标签 |
| `7.1.6` 或 `9.1.x` | 不支持；WAV 会拒绝写入 | 语义声道标签 |
| `22.2` | 按 canonical 顺序输出 24 声道 PCM，不写入声道掩码 | 语义化输出：用标准 CAF 声道标签（channel labels）表示可命名声道，用坐标描述（coordinate descriptions）表示其余位置，完整保存 22.2 声道身份 |
| 自定义布局 | 按声明顺序写入无掩码 PCM | 写入语义声道标签；自定义几何使用坐标描述 |

不会替换任何声道身份，也不会写入凭空生成的 WAV 掩码。容器中的顺序永远不会改变渲染器的 canonical 语义顺序。

22.2 预设对应 ITU-R BS.2051 Sound System H，包含 22 个空间扬声器和两个语义 LFE 输出目的地。LFE 声道永远不是投影顶点。

## 双耳与 SOFA

`--binaural` 会把虚拟扬声器场渲染为两个输出声道。如果没有 SOFA 路径，OpenJOC 会使用内置的离线 SADIE II D1 通用 HRTF。默认虚拟布局是 7.1.4：

```text
openjoc render-joc input.m4a --binaural -o headphones.wav
openjoc render-joc input.m4a \\
  --binaural --virtual-layout 9.1.6 \\
  -o headphones-916.wav
```

使用 `--sofa` 指定用户数据集：

```text
openjoc render-joc input.m4a \\
  --binaural --sofa listener.sofa --virtual-layout 7.1.4 \\
  --lfe-policy exclude -o custom-headphones.wav
```

加载器支持 CDF-1 和 NetCDF-4/HDF5 中的本地 `SimpleFreeFieldHRIR`。每个非 LFE 虚拟方向都必须精确匹配或能安全插值；方向覆盖不足时会拒绝继续处理。`sofa inspect` 显示文件的源采样率，渲染前会将 HRIR 转成 48 kHz。

LFE 策略必须明确选择：`exclude` 或 `equal-power-dual-mono`。CLI 默认使用 `exclude`。双耳输出始终是双声道虚拟扬声器结果，不代表直达对象或专有渲染器的听感保真度。

`--backend direct|partitioned` 选择受支持的卷积后端。直接 FIR 是数值参考；分区卷积使用请求的固定二次幂分区，并保留最终不完整输入和 FIR 尾部的处理行为。

Rust API 和 C ABI 还提供显式启用的实验性设备无关 3DoF 听音者姿态模式。它会旋转固定虚拟扬声器的 HRTF 查询方向，不会移动声源或改变投影/增益阶段，也不会读取传感器或连接音频设备。CLI、WASM 和 DirectShow/LAV 路径保持静态。identity 姿态回归会精确比较静态 HRIR 与 PCM。启用方式、序号、有限拉取和实测限制详见 [Rust API](../reference/rust-api.md) 与 [C ABI](../reference/c-abi.md)。

## DRC、Dialnorm 与文件电平

推荐的信号顺序是：

```text
编码后的 DRC 策略 -> 节目 Dialnorm -> JOC 渲染
  -> 扬声器 FinalLinkedGain（仅扬声器路径）
  -> 可选的静态采样峰值归一化 -> 文件
```

`--drc disabled|line|rf|custom` 控制 E-AC-3 中编码的动态范围元数据。自定义模式接受 0 到 100 的 `--drc-boost` 和 `--drc-cut` 百分比。DRC 不是通用压缩器，也不是输出归一化器。

`--dialnorm default` 是推荐的校准方式。`digital` 明确选择编码后的数字节目校准。`analog` 为高级兼容/诊断场景应用单位 Dialnorm；它不是更高质量、原始、无损或母带制作模式，在电平较高的素材上可能让 FinalLinkedGain 承担更多工作。

`--normalize-peak TARGET_DBFS` 是可选项，默认关闭。CLI 会完成一次标准渲染，同时暂存受限的渲染器原生 PCM，测量采样峰值，然后在渲染器处理后应用一个统一的静态缩放系数。它既可以提升也可以降低电平。它不是 DRC、Dialnorm、限幅器、压缩器、LUFS 归一化或真峰值归一化；样点之间的峰值仍可能超过目标值。

`--diagnostic-contribution base-only|reconstruction-only` 可以单独输出一种声部，用于工程证据。这是仅限诊断的功能，不得把结果描述成创作床层或对象分轨。

## 时间线、延迟、排空与重置

逻辑 PTS 始终位于解码后的 48 kHz 采样域，不会为了隐藏渲染器的可用性延迟而移动：

- 扬声器输出报告 609 个采样点：577 个采样点的 QMF/Base-RB 对齐延迟，加上受支持的 32 采样点因果 FinalLinkedGain 块；
- 双耳输出报告 577 个采样点；采样率不同时，还会计入 HRIR 转换的公共延迟。有限长度的 SOFA FIR 尾部会单独排空；
- Dialnorm 和文件静态归一化不会增加音频采样延迟。

排空会输出所有 QMF/重建、FinalLinkedGain 和双耳 FIR 尾部。Flush、reset 和不连续会在下一段数据前清除访问单元、解码器、时间线、增益和 HRTF 状态。集成层仍需负责容器跳转、预滚选择和丢弃输出策略。

## 进度、报告与输出安全

交互式进度写入 stderr；输出不是 TTY 时会自动关闭。使用 `--no-progress` 可以主动关闭进度，使用 `--performance-report report.json` 可以记录带版本的阶段耗时和实时诊断。

输出采用事务式写入。目标文件已经存在时，需要交互确认或使用 `--overwrite`；输入和输出指向同一位置会被拒绝。解码、渲染、校验、范围检查或写入失败，都不会发布一个看起来像成功的标准输出文件。

## 集成边界

Rust API 和受支持的框架适配器共享同一个会话渲染器，但每个主机负责自己的传输和布局协商。经过验证的 Windows DirectShow/LAV/PotPlayer 集成提供八种固定的 48 kHz IEEE-float PCM 策略：Stereo、Binaural、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4。Stereo 与 Binaural 都使用双声道 PCM，但只有 Binaural 会通过 HRTF 虚拟扬声器场。64 声道渲染上限和独立的预设矩阵，都不属于 DirectShow/LAV 输出声明。

当前不作出的保证请看[已知限制](../compatibility/known-limitations.md)，组件职责请看[生产架构](../concepts/architecture.md)。
