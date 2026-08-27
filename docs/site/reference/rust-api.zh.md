!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 无界面 Rust 流式 API

`openjoc-api` 提供当前的高级嵌入式接口。它仍是实验性的，解码器和渲染器的语义受当前能力矩阵与限制约定约束。

## 生命周期

```rust
use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession};

let mut session = OpenJocSession::new(OpenJocConfig::default())?;
let status = session.push_packet(OpenJocPacket {
    data: complete_access_unit,
    pts_samples: Some(0),
    discontinuity: false,
    preroll: false,
})?;
while let Some(frame) = session.receive_frame() {
    consume_interleaved_f32(frame.interleaved_f32, frame.sample_rate);
}
let _ = session.drain()?;
while let Some(frame) = session.receive_frame() {
    consume_interleaved_f32(frame.interleaved_f32, frame.sample_rate);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

一个会话只能由一个调用方串行访问。不同会话彼此独立，可以并发运行。不可变配置会被复制进会话；不存在进程全局的解码器、布局、SOFA 对象或错误缓冲区。

## 输入约定

`OpenJocPacket` 只在 `push_packet` 调用期间借用。`data` 必须是一个完整的 E-AC-3 JOC 访问单元：独立子流 0 加可选的从属子流 0。这是 JOC 元数据、Base/RB 对齐和 E-AC-3 继承机制所需的最小可靠单元。会话不会保留压缩输入缓冲区。

任意字节拆分、文件路径、MP4/Matroska 解复用，以及一次调用传入多个访问单元，有意排除在这第一版约定之外。

## 输出与布局

canonical 格式是交错的 IEEE-754 `f32`。每个 `OpenJocPcmFrame` 都拥有自己的向量，Rust 调用方可以保留它。帧会报告采样率、采样点数量、采样域 PTS、渲染模式、布局名称和有序语义声道标签。扬声器布局使用仓库的 canonical 公共预设：2.0 是 `FL, FR`，5.1 是 `FL, FR, FC, LFE, Ls, Rs`，22.2 是 canonical 的 24 声道 `FL, FR, FC, LFE1, ... , BtFR` 顺序；即使虚拟布局是多声道，双耳输出仍报告 `Left Ear, Right Ear`。

物理扬声器会话也可以使用 `SpeakerLayout::custom(...)` 和 `OpenJocConfig::with_speaker_layout(...)`。自定义布局保留调用方扬声器数组的顺序，作为 PCM/声道顺序；它会校验有限的球面几何，并把 LFE 声道排除在空间投影器之外。JSON/CLI 形式记录在[自定义扬声器布局](../using/custom-speaker-layouts.md)中；这是高级功能，不会扩大下游主机/设备的声道布局支持。

对于双耳会话，`BinauralConfig::builtin_generic("7.1.4")` 会选择离线内置的 SADIE II 通用 HRTF，不需要文件系统路径。显式使用用户 SOFA 时，请使用 `BinauralConfig::from_sofa_bytes(...)`；严格 SOFA 校验和超出覆盖范围时拒绝的行为不变。

`output_info()` 可以在第一个数据包之前调用。直到第一个访问单元确定流格式之前，采样率都是 `None`。

## 时间、延迟、排空与跳转

PTS 使用解码后的采样域。如果第一个数据包的 PTS 是 `P`，逻辑采样点 `n` 的输出会报告 `P + n`；滤波器组或最终联动增益的延迟不会悄悄移动 PTS。扬声器输出报告 609 个采样点的延迟：577 个采样点的 QMF/Base-RB 延迟，加上受支持的 32 采样点因果扬声器阶段块延迟。双耳输出报告 577 个采样点，因为它不使用扬声器 FinalLinkedGain 阶段。这些是公开的同步约定；Dialnorm 和离线静态归一化不会增加音频采样延迟。这样可以明确暴露可用性延迟，而不必让调用方从帧数反向推断。

- `drain()` 会刷新 QMF/重建状态以及直接 SOFA FIR 尾部；
- `flush()` 会丢弃等待中的 PCM，并重置流派生状态，但保留配置和准备好的 SOFA 数据；
- `reset()` 具有相同的可复用会话语义，是跳转或不连续边界的推荐用法；
- `discontinuity = true` 的数据包会在解码前执行流重置；
- `preroll = true` 可以用于预热解码器状态；第一版 ABI 不会自动隐藏这次预热产生的延迟帧。

输出队列有大小上限。调用方必须先接收等待中的 PCM，才能继续推送下一个数据包；否则会返回 `OpenJocStatus::OutputPending`。

## 策略

`DrcPolicy` 直接映射到已有的 E-AC-3 `InternalBasePolicy`，支持 disabled、line、RF 和自定义 boost/cut。DRC 改变节目动态，不是最终音量或响度控制。`DownmixPolicy` 为立体声输出支持 auto、Lo/Ro 和 Lt/Rt。公共库类型不会复用 CLI 枚举。

`OpenJocConfig::dialnorm` 选择解码器/节目校准策略：`DialnormMode::Default`（校准后的默认行为）是默认值，推荐用于普通播放/解码。`Digital` 明确选择编码后的数字节目级校准。`Analog` 使用单位 Dialnorm 因子，是高级兼容/诊断策略；它不是推荐的增大音量方式，也不是母带制作模式。Dialnorm 与 `DrcPolicy` 分开；DRC 仍然处理编码的动态范围元数据。

选定的 Dialnorm 节目缩放系数，会在扬声器投影、FinalLinkedGain 或 SOFA 卷积之前，对完整解码节目只应用一次。FinalLinkedGain 是内部渲染器余量处理，不是用户的母带制作控制。`OpenJocSession` 永远不会执行文件导出峰值归一化或其他面向文件的输出变换；应用可以在收到 PCM 后自行应用最终增益策略。CLI 的 `--normalize-peak` 是独立的离线便利功能：它会在渲染器处理后应用一次静态采样峰值增益，不是 DRC、Dialnorm、限幅、压缩、LUFS 或真峰值归一化。流式 API 不会执行文件导出峰值归一化，也不会为了文件级变换暂存完整节目。

`BinauralConfig` 接受完整的内存中 SimpleFreeFieldHRIR SOFA 缓冲区、虚拟扬声器布局和明确的 LFE 策略。会话不会保留文件系统路径。公共 API 当前使用直接卷积；分区卷积留待之后的 ABI 扩展。

如果需要审计不同前端的配置是否一致，`OpenJocConfig::effective_config_descriptor()` 和 `effective_config_fingerprint()` 会公开经过归一化的会话边界字段。`trace_access_units()` 会记录每组访问单元的精确字节长度、SHA-256、采样域 PTS、采样率，以及独立/从属帧数量。

## 错误与状态

状态码是数值型的非错误生命周期结果：`NeedMoreInput`、`FrameAvailable`、`OutputPending` 和 `EndOfStream`。Rust 错误是带类型的 `OpenJocError` 值。C 适配器会把它们映射为数值状态码和归属于当前实例的诊断消息。

格式错误的数据包、格式变化、时间戳不连续、配置变化和渲染失败，都不会被悄悄转换成不匹配的 PCM。
