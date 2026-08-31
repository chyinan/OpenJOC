!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 已知限制

这里列出目前面向用户的限制，以及项目明确不作出的保证。当前支持情况请看[能力矩阵](../project/capabilities.md)；历史限制保留在变更记录、研究记录和归档资料中。

其中一些边界仍有待补充证据。如果你想参与贡献，请先阅读[开放问题与贡献方向](../project/open-problems.md)，不要把尚未解决的问题理解成可以凭经验提交猜测性修复的邀请。

OpenJOC 是一个独立的实验性互操作项目。不声明得到 Dolby 的认可或认证，不声明这是授权实现，也不声明输出与 Reference Player（参考播放器）逐比特一致，更不声明具备专有渲染器的全部特性。

## 解码器与语义边界

- `ObjectScene` 会把 `ReconstructionBasis` 行与原始创作对象区分开来。只有符合精确支持条件的 decoded-JOC/OAMD 配置组合，才会报告 `SemanticBindingState::ResolvedWithinCarrier`；这表示解码数据与 OAMD 运动信息可以在该 JOC 数据内部对应起来，并不表示原始创作对象身份已经恢复。其他配置均保持 `Unresolved`。
- 普通域的 `JocSpatialBridge` 会使用 OAMD 中的控制信息来渲染解码后的 Base/RB 声部，但不会恢复原始创作对象身份，也不实现编解码器域中的算子 `T(t)`。
- 对于精确支持的 common profile（一个前置 Base LFE、无 Bed/ISF、15 个动态 JOC 对象），物理 2.0 输出使用 E-AC-3 compatibility presentation，不再叠加第二份 reconstructed-object Stereo。现有 Lo/Ro 或 Lt/Rt 矩阵、规范要求的 `1 / max_sum` 溢出保护、元数据控制的 LFE、dialnorm、DRC 与 FinalLinkedGain 顺序均保持不变；该 ownership 结论不会推广到其他 profile。
- `ETSI_STRICT` 会拒绝已发布配置范围之外的语法，包括已观察到的 OAMD warp `raw=3`。`OBSERVED_VENDOR_COMPAT` 是明确限定范围的部分兼容策略：它只保留无法解释的扩展数据，不为这些数据臆造厂商语义。在测试范围内，一种精确的、包含 15 个对象的 raw3 兼容形式可以进入解码对象场景路径，因为该路径不需要额外的 raw3 专用变换。
- 对公开语法的编码工具支持只覆盖有限的数值和状态范围。项目不声称对所有制作工具、媒体格式、编码工具组合，以及各种异常输入组合，都能保持完整的保真度。

## 输入与流式处理

- JOC 渲染采用 48 kHz。原始 E-AC-3，以及普通（非分片）ISO BMFF 容器中支持随机访问的输入，只有在文档规定的拓扑和访问单元边界内才会被接受；不支持随机访问的 MP4 和分片 MP4 不在支持范围内。
- Rust `OpenJocSession` 的数据包接口每次接收一个完整的 General JOC 访问单元：I0 加按顺序排列的 D0..D7，受公开最大值约束。解复用、任意字节拆分，以及一次传入多个访问单元，属于有界的 C 流式解码器或框架适配器负责的范围。
- CMAF Annex E.3 仍限制为 I0 加可选 D0；原始 AC-3 Annex-J I0 组合也仍保持 D0-only，Type 2 不扩展从属子流支持。
- 标准 flat-7.X 仅由 JOC downmix index 1、七个 JOC 输入和 `L R C Ls Rs Lrs Rrs` Table-47 组装共同识别。JOC reconstruction 使用 I0+D0..Dn 组装后的七输入 plane，2.0 compatibility rendering 使用独立 I0 presentation；OpenJOC 不会臆造 Lrs/Rrs 到 Stereo 的直接系数。
- 原始语法 I0 的支持严格限定为公开的 48 kHz Annex-J/TS-103-420 结构：一个 CRC 有效的 AC-3 I0、一个 E-AC-3 D0、匹配的六 block 时序、有效语义 chanmap，以及位于最后 D0 的 JOC/OAMD。普通 AC-3 不会进入 OpenJOC。已知 malformed 真实文件仍会在 AU0 截断 EMDF 边界失败关闭，不构成完整真实流验证。
- 某些支持随机访问的容器和兼容 Base 的处理流程需要 `ffprobe` 或 `ffmpeg`；OpenJOC 的发行包并非完全无需额外依赖。

## 扬声器与双耳渲染

- 预设布局和自定义布局共用同一个通用渲染器。自定义几何布局最多支持 64 个有序输出声道，并且至少需要两个可用的全频段方向。渲染器接受某种布局，并不意味着主机、设备或容器格式也能传输相同的几何信息。
- `7.1.6` 和 `9.1` 系列需要使用能够保留声道语义描述的 CAF 文件，因为标准 `WAVEFORMATEXTENSIBLE` 声道掩码无法准确表示它们的声道身份。`22.2` 和自定义 WAV 使用显式的无掩码 PCM；需要保留坐标时，优先使用 CAF。
- OpenJOC 不执行分频、低频管理、房间校正、扬声器校准、头部跟踪、距离模型、多普勒处理或设备发现。LFE 的归属必须明确指定，不会根据声道数量推断物理设备。
- 双耳输出采用虚拟扬声器渲染，不承诺与专有的直达对象双耳渲染完全一致。内置 SADIE II 数据集是通用数据；某些听音者可能更适合使用自定义 SOFA 文件。
- 自定义 SOFA 仅支持严格限定的本地 `SimpleFreeFieldHRIR` NetCDF classic CDF-1 子集，要求固定的听音者姿态、两个接收器、统一采样率，并且方向覆盖必须落在规定范围内（支持精确方向或插值）。不支持 HDF5/NetCDF-4、自动重采样、下载、写入、移动声源或覆盖任意方向的数据集。

## 输出电平与同步

- DRC 使用编码在 E-AC-3 中的动态范围元数据。Dialnorm 负责节目响度校准；两者彼此独立，也与文件导出时的归一化彼此独立。
- `DialnormMode::Default` 是推荐的校准方式。`Digital` 明确选择编码数字校准；`Analog` 是高级的单位增益兼容/诊断策略，不代表更高音质，也不是母带制作模式。
- `--normalize-peak` 只应用一个静态的渲染后采样峰值缩放系数。它不是 LUFS 或真峰值归一化，也不是限幅器、压缩器或 DRC；样点之间的峰值仍可能超过请求值。
- 扬声器输出会报告 609 个采样点的可用性延迟（577 个 QMF/Base-RB 采样点加 32 个 FinalLinkedGain 采样点）。双耳输出会报告 577 个采样点，不包括有限长度的 FIR 尾部。逻辑 PTS 不会为了隐藏这个延迟而移动。

## ADM 互操作

- `export-adm` 写入的是重建得到的 RIFF/RF64 ADM BWF 文件，不是原始 ADM/BWF 母版。原始名称、层级、UID、创作绑定关系，以及已经丢弃的源信息都无法恢复。报告会明确保留 `original_authored_identity_recovered: false`、`original_adm_master_recovered: false` 和 `lossless_round_trip: false`。
- 对于精确的清洁室实现配置组合（15 个 JOC 对象、无床层、一个前置 Base LFE、无 ISF、15 个动态 OAMD 对象，共 16 个），解码后的 JOC PCM 会通过明确类型的 JOC 内部序号，与对应的 OAMD 动态元数据配对。这包括普通严格配置组合，以及精确的已观测 raw3 兼容配置组合。
- 结构检查和解码场景检查通过，并不保证通用 ADM 渲染器与原生 JOC 最终渲染器在听感上的定位完全一致。至少有一个真实世界的验证节目在技术检查通过后仍出现了残余定位差异；该结果与具体素材有关，不能推广到其他节目。需要原生渲染器的精确定位结果时，请以原生 JOC 播放为参考。
- 移动的重建对象表示的是从 JOC 节目中保留下来并解码得到的空间元数据。它的轨迹可能不同于原始 DAW 自动化；即使解码出的轨迹有实际意义，也不等于原始母版已经恢复。
- OpenJOC 不承诺恢复原始 DAW/Logic 轨道身份、创作对象编号、对象名称、源分轨 PCM、未量化的自动化、节目/内容层级、创作元数据、Dolby 制作来源信息，或 JOC 到 ADM 的无损往返转换。
- 在受限的动态处理路径中，导出器会在解码后的 OAMD 事件边界写出位置数据。ADM 中不会凭空补出范围、增益、发散度、声道锁定、区域，或当前不支持的激活/停用过渡等语义。对于不支持的元数据，尽力模式会退回中性输出并记录原因，严格模式则拒绝导出。
- 存在 Base LFE 时，导出器会创建一个最小合法的 5.1 传输床层。只有 LFE 携带解码得到的 Base LFE PCM；L、R、C、Ls、Rs 是为了补齐合法传输结构而生成的静音占位声道，并会在报告中标为生成结构。
- 生成的 `dbmd` 只包含公开的 EBU Supplement 6 封装。保留下来的 Atmos 专用片段负载和 Dolby 制作来源信息不会被复制、猜测或伪造。
- Logic Pro 可以导入重建文件；维护者验证过的 Logic 创作文件重新导出版本，也可以被 Dolby Encoding Engine 接受。但这不会让 OpenJOC 生成的文件获得 Dolby 制作来源，直接送入 DEE 仍不受支持，项目也不作此声明。

## API 与集成

- C ABI 1.4 在 OpenJOC 0.x 期间仍是实验性的。公共头文件、结构体大小、所有权规则、数值状态码和兼容初始化函数共同构成接口约定；ABI 仍可能演进。
- 外部 FFmpeg 桥接是供其他程序嵌入的接口，不是针对已安装 `ffmpeg` 可执行文件的独立插件。原生 `libopenjoc` 解码器需要使用打过补丁的定制 FFmpeg 构建，并且必须显式选择 JOC。
- GStreamer 使用 OpenJOC 专用的实验性 caps 特性，需要匹配的主机运行时，不会全局修改已安装的 GStreamer。
- mpv 和 OpenJOC Player Bundles 是项目提供的定制构建，不是官方上游的 mpv 或 FFmpeg 发行版。物理多声道播放仍需要音频输出和设备能够接受请求的声道映射。
- Windows DirectShow/LAV 集成会主动接受 JOC，把普通 E-AC-3 留在原有的 LAV/FFmpeg 路径，并保持直通优先级。它固定提供 48 kHz IEEE-float PCM 输出方案：Stereo、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4。每种方案只提出一种明确的 `WAVEFORMATEXTENSIBLE` 格式，不提供备用方案。Stereo 是默认值，其他布局需要显式选择；物理多声道硬件尚未验证。OpenJOC 不会根据音频端点名称推断布局，不执行低频管理，也不会把物理低音炮数量转换成逻辑 LFE 声道。独立的 7.1.6、9.1.x、22.2、自定义渲染器支持，都不属于 LAV 输出声明。

## 平台与发布范围

- 平台软件包覆盖当前发布元数据中记录的目标平台。多声道 PCM 的生成和传输已经完成相应验证；但并非每种 Linux 或 Windows 设备上的物理扬声器系统播放，都经过独立验证。
- macOS 构建文件在需要时使用临时签名，不是 Developer ID 签名，也没有经过公证。Linux 兼容性受记录的 glibc/运行时基线约束。Windows 软件包使用文档规定的旁置 DLL 或独立 LAV 安装方式。
- 私人或商业节目的测试素材及其派生 PCM 不会分发。因此，一些真实媒体的验收仍属于维护者的发布门槛。

明确支持的功能和证据边界请参阅[能力矩阵](../project/capabilities.md)与[扬声器渲染](../using/speaker-rendering.md)。
