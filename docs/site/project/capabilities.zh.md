!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 能力矩阵

这是当前能力状态的唯一权威页面。真实媒体验收已经确认：Logic 可以导入 OpenJOC 重建的 ADM，Logic 创作工程重新导出的文件也能被 Dolby Encoding Engine 接受。但项目不声明可以直接送入 DEE 的、由 OpenJOC 精确生成的文件。历史发布状态请看[CHANGELOG](https://github.com/chyinan/OpenJOC/blob/master/CHANGELOG.md)和[历史归档](https://github.com/chyinan/OpenJOC/tree/master/docs/archive)；带日期的工程证据请看研究记录和来源记录。

## 状态定义

显式的 `render-scene` 工作流支持调用方绑定的静态单声道声源、直接或均匀分区的双耳卷积，以及严格的 J5R8 SimpleFreeFieldHRIR/CDF-1 SOFA 子集。它不是 JOC 渲染器，也不是原始创作对象渲染器。

- `ADMITTED` —— 在所声明的约定内受支持；
- `ADMITTED_WITH_SCOPE` —— 只在明确限定的范围内受支持；
- `DIAGNOSTIC_ONLY` —— 用于分析，不构成语义产品声明；
- `PARTIAL` —— 支持明确的子集，不代表完整能力；
- `UNRESOLVED` —— 证据不足，不能作出实现声明；
- `NOT_ADMITTED` —— 有意排除在当前约定之外；
- `EXPECTED_STRICT_REJECTION` —— 对该输入来说，拒绝才是正确结果。

## 当前矩阵

| 领域 | 能力 | 状态 | 证据边界 | 重要范围 |
|---|---|---|---|---|
| 输入 | 原始 E-AC-3 解析和受限流式处理 | `ADMITTED` | 受控载体和公开语法 | 完整真实流的编解码保真度仍受范围限制 |
| 输入 | 支持随机访问的普通 MP4/M4A，且只有一个 E-AC-3 轨道 | `ADMITTED_WITH_SCOPE` | 容器和采样游标回归测试 | 使用 `ffprobe`/`ffmpeg`；不支持随机访问和分片 MP4 不在范围内 |
| 基础 E-AC-3 | 普通基础解码及声道/LFE 标记 | `ADMITTED_WITH_SCOPE` | 公开语法、拓扑、TDAC 和状态测试 | 不是扬声器渲染器；跨解码器保真度仍不完整 |
| 编码工具 | 耦合、SPX、AHT、重混矩阵 | `ADMITTED_WITH_SCOPE` | 规范/公开语法的数值和状态测试框架 | 一些真实制作工具的启用情况及完整 PCM 保真度仍待解决 |
| 子流 | 一个 I0 加可选 D0 的组装 | `ADMITTED_WITH_SCOPE` | 声道映射、原子组装和重置测试 | 多个从属流不在支持范围内 |
| OAMD | 规范元数据前缀和仅元数据时间线 | `ADMITTED_WITH_SCOPE` | 规范解析器和受控状态测试 | 完整的厂商扩展尾部不可用 |
| OAMD | `ETSI_STRICT` 配置 | `ADMITTED` | 已发布的 ETSI 校验规则 | 观察到的原始 `warp=3` 会表示为 `ReservedWarpMode`，并被拒绝 |
| OAMD | `OBSERVED_VENDOR_COMPAT` 配置 | `PARTIAL` | 明确的观测信令接受规则和偏差证据 | 扩展数据会原样保留，但不会解释其厂商语义 |
| 场景 | 仅元数据的 `ObjectScene` | `ADMITTED` | 模式、时间线、组装和原子性测试 | `ResolvedWithinCarrier` 只保留给下方精确的 decoded-JOC/OAMD 配置；原始创作身份仍未解决 |
| 重建 | `ReconstructionBasis` 行 | `ADMITTED_WITH_SCOPE` | 公开 JOC 输出对象约定、确定性数值/连续性测试和脱敏场景向量 | 这些行是受支持 JOC 配置内的解码输出对象 PCM，永远不是原始创作对象 PCM 或源分轨 |
| 绑定 | 解码 JOC PCM ↔ OAMD 动态元数据 | `ADMITTED_WITH_SCOPE` | 脱敏清洁室约定、带类型的序号检查、正/负向量和实际数量场景组装测试 | 恰好 15 个 JOC 对象、无床层、一个前置 Base LFE、无 ISF、15 个动态对象、总计 16 个；不使用猜测，也不声明原始创作身份 |
| 绑定 | 根据解码 JOC 对象生成重建动态 ADM 对象 | `ADMITTED_WITH_SCOPE` | 受限元数据预检、带类型绑定配置、确定性位置块导出、严格/尽力策略测试和独立 ADM 校验 | 生成对象只在普通精确配置或已观测 raw3 兼容配置内携带解码后的 OAMD 运动；结构/解码场景正确不保证原生 JOC 的听感定位等价；不支持的属性或配置会保持中性或被拒绝 |
| 绑定 | 未验证的 JOC 绑定配置 | `UNRESOLVED` | 明确拒绝并记录报告原因 | 带床层、带 ISF、备用 LFE、数量/顺序不匹配、无法识别的兼容性偏差、Base LFE 不完整以及其他未验证配置，不会进行动态绑定 |
| 组件 | 带类型的解码组件清单 | `ADMITTED` | `diagnostics/components.json` 区分 Base、Base LFE、带索引的 RB 坐标、RcLfe 边界和 JOC 数据内部绑定状态 | 只描述布局，不包含 PCM；不包含原始创作对象身份 |
| JOC 桥接层 | 编解码器域流式重建输入和准备度检查 | `ADMITTED_WITH_SCOPE` | `JocSpatialFrameBridge`、绝对 `SampleRange`、有限值/维度检查、线性/分块合成测试和准备度统计 | `T(t)` 仍未解决；不进行原始创作对象语义绑定 |
| JOC 桥接层 | 选择性启用的编解码器坐标空间投影和累加 | `ADMITTED_WITH_SCOPE` | `JocSpatialBridge`、拓扑绑定、空间投影、Q32 增益调度、线性累加、raw3 保留和分块测试 | 仍是实验性功能；桥接层/算子状态与范围受限的解码对象 ADM 绑定彼此独立且仍未解决；官方运行时基准尚未独立确认 |
| JOC 渲染 | 真实受支持的 E-AC-3 JOC 到预设扬声器 WAV/CAF 流程 | `ADMITTED_WITH_SCOPE` | `render-joc` 解码/桥接/输出集成测试、自动桥接控制组装测试、2.0 拓扑与 Lo/Ro/Lt/Rt 数值测试、预设几何、拓扑/数量/LFE/顺序/掩码/语义 CAF 检查、合成任意布局和 24 声道桥接测试 | 实验性的 2.0，以及 5.1、5.1.2、5.1.4、7.1、7.1.2、7.1.4、7.1.6、9.1、9.1.2、9.1.4、9.1.6 和 22.2 路径；全部使用同一通用的全 XYZ/N 层数据驱动投影器，并单独处理 LFE 归属；7.1.6 和 9.1 系列只能使用语义 CAF，22.2 使用显式无掩码 24 声道 WAV 或更丰富的 CAF 元数据；`--topology` 可选，仍是完整的显式覆盖/测试输入；通用库布局仍受支持；不进行原始创作对象绑定，也不声明厂商级保真度 |
| JOC 渲染 | 真实受支持的 E-AC-3 JOC 到立体声通用/用户 SOFA 双耳 WAV | `ADMITTED_WITH_SCOPE` | 虚拟扬声器集成测试、内置 SADIE II 资源往返/覆盖测试、精确 HRIR 身份、延迟对齐球面插值、方位角环绕和稀疏数据测试、采样率预检、直接后端参考等价、分区后端等价、LFE 策略、重置和尾部测试 | 默认虚拟场为 `7.1.4`，除非提供 `--virtual-layout`；`--binaural` 使用离线内置 SADIE II D1 资源，`--sofa` 可覆盖它；选定的 HRTF 数据必须为每个非 LFE 虚拟扬声器提供精确或可安全插值的方向；CLI 默认将 LFE 设为 `exclude`，也可显式选择 `equal-power-dual-mono`；输出始终是双声道 OpenJOC 虚拟扬声器结果，不是厂商级或直达对象双耳声明 |
| JOC 布局引擎 | 规范预设和最多 64 个输出声道的任意用户几何布局 | `ADMITTED_WITH_SCOPE` | 公开 `SpeakerLayout`/`SpatialLayout` 与 `JocSpatialBridge`、带版本的自定义 JSON、不规则 3/4/7/11/13/17/31 声道几何、拒绝校验、任意顺序和 24 声道测试 | 预设名称仍是 CLI 的常规路径；`--layout-file` 面向高级使用；自定义 WAV 有意使用无掩码，CAF 携带坐标；下游主机/设备的几何布局另行处理 |
| 集成 | 无界面 Rust `OpenJocSession` / `OpenJocConfig` | `ADMITTED_WITH_SCOPE` | 会话生命周期、完整访问单元数据包校验、拥有的交错 `f32` PCM、时间戳、重置/刷新/排空、多实例和延迟测试 | 每个会话只允许一个串行调用方；任意字节拆分、多访问单元推送、文件 I/O 和 CLI 解析不属于数据包 API |
| 集成 | 带版本的 C ABI 1.4 | `ADMITTED_WITH_SCOPE` | 公共 `openjoc.h`、保留的完整访问单元解码器、内存中的自定义扬声器几何描述、无需解码的受限分类器、受限数据包流句柄、拆分/多访问单元测试、延迟的正向 JOC 识别、语义布局/指纹访问、数值状态码、`struct_size` 回退、C11/C++ 编译、实例所有错误和 panic 保护 | 实验性 ABI；OpenJOC 0.x 期间兼容性可能演进 |
| 集成 | Windows DirectShow / LAV Filters OpenJOC Audio Decoder | `ADMITTED_WITH_SCOPE` | 公开的 `LAVFilters-OpenJOC` 分支/标签；严格的原始/MP4 DirectShow 捕获，证明 Stereo、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4 的精确媒体类型与采样传递；端点探测保留 VB-Audio WaveOut 成功、VB-Audio DirectSound 拒绝和 Realtek DirectSound 成功；另有 JOC 正向识别、普通 E-AC-3 隔离、直通优先级、跳转/EOS/重新打开/策略切换、并行安装、卸载和原有 LAV 回滚测试 | 每种显式方案只提供一个精确的 48 kHz `WAVEFORMATEXTENSIBLE` IEEE-float 格式，不提供备用方案；状态为 `AUTO_NOT_RELIABLE`；Stereo 为默认值；不根据端点名称推断、不执行低频管理、不进行物理低音炮路由；物理多声道硬件仍未提供或未验证 |
| 集成 | 原生 FFmpeg `libopenjoc` 源码包装器 | `ADMITTED_WITH_SCOPE` | 可复现的 FFmpeg 9.0.1/master 补丁、pkg-config 检测、接收帧回调、显式名称选择、原有 E-AC-3 安全性、原始/MP4/Matroska 私有媒体测试、跳转/刷新/排空/多实例测试，以及完整节目双耳/7.1.4/22.2 等价测试 | 需要带动态 OpenJOC 的定制 FFmpeg 构建；项目提供源代码补丁/构建，但不声明上游 FFmpeg 支持 |
| 集成 | mpv 播放器补丁集和 OpenJOC Player Bundle | `ADMITTED_WITH_SCOPE` | 干净的 mpv 0.41.0/master 补丁应用、受限的 JOC 正向分类器、普通 E-AC-3 隔离、显式解码器覆盖、双耳传输、物理 7.1.4/9.1.6/22.2 声道映射路径、直通分离，以及原生 macOS/Linux/Windows 软件包验证 | 项目提供定制的 mpv/FFmpeg 构建；软件包不是官方上游发行版；Linux/Windows 物理扬声器硬件不在验证范围内 |
| 互操作 | 重建 RIFF/RF64 ADM BWF 导出 | `ADMITTED_WITH_SCOPE` | 生产级受限内存压缩媒体预检/直接写入器、条件式解码 JOC 动态元数据路径、独立的基于跳转的 RIFF/RF64/ds64/Atmos 配置 XML/CHNA/公开 DBMD 校验器、合法的室内坐标式 5.1 LFE 传输床层、事务式清理、有符号 24 位 PCM、映射报告、缩放高水位和严格不支持配置拒绝 | 精确受支持的配置会导出带位置块的动态对象；不支持的配置在尽力模式下保留中性对象，严格模式下拒绝；不声明恢复原始 ADM 身份/母版，也不声明可以直接送入 DEE |
| 生态 | OpenJOC SDK、定制 FFmpeg 软件包和启用功能的 GStreamer 插件包 | `ADMITTED_WITH_SCOPE` | 提取的软件包清单/校验和、许可/私有路径扫描、C 用户程序编译运行、FFmpeg/ffprobe 冒烟测试、`gst-inspect-1.0` 和目标运行时基线 | 软件包由项目提供；FFmpeg 不是上游版本，GStreamer 运行时 ABI 必须匹配记录的基线，C ABI 1.4 仍是实验性的 |
| 诊断 | 公开的合成 JOC 测试样本和 `openjoc self-test` | `ADMITTED_WITH_SCOPE` | 项目自有测试样本生成、正向分类、解码、7.1.4 扬声器、内置 HRTF/双耳和 ADM 健康报告 | 测试样本按需生成；缺少可选样本时报告 `NOT_APPLICABLE`，而不是静默成功 |
| 输出策略 | Dialnorm Default/Digital/Analog 和可选采样峰值归一化 | `ADMITTED_WITH_SCOPE` | Dialnorm 数值映射、节目级所有权、最终联动扬声器余量、归一化恒增益等价和 WAV/CAF 输出测试 | Default 是推荐的校准解码行为；Analog 是高级的单位增益兼容策略；归一化只在离线阶段处理采样峰值，不是 LUFS 或真峰值处理 |
| 语义 | 恢复原始创作对象身份 | `NOT_ADMITTED` | 清洁室约定明确区分 JOC 数据内部的解码绑定和原始创作身份 | 生成的名称、编号、UID、层级、创作元数据和源分轨身份都不会恢复 |
| 语义 | 恢复原始 ADM 母版 | `NOT_ADMITTED` | 有损 JOC 输入和重建导出报告 | `original_adm_master_recovered=false`；无法从这种表示恢复原始母版 |
| 语义 | 已验证的创作 PCM 或创作对象渲染器保真度 | `NOT_ADMITTED` | JOC 数据内部的绑定无法识别源分轨，也无法复现专有渲染器 | `SemanticBindingState::ResolvedWithinCarrier` 永远不会升级为原始创作身份或渲染器等价 |
| 渲染 | 显式场景立体声和通用二维扬声器渲染器 | `ADMITTED_WITH_SCOPE` | `openjoc-render` 独立立体声/VBAP 基准、布局、轨迹、连续性和分块测试 | 只接受调用方提供的单声道声源；支持任意经验证的水平布局、相邻声道对摇移和绝对采样位置/增益轨迹；没有 JOC 桥接、HRTF、房间模型或 Dolby 渲染器保真声明 |
| 渲染 | 显式场景三维扬声器拓扑、VBAP 三元组渲染器和采样精确轨迹 | `ADMITTED_WITH_SCOPE` | `openjoc-render` 检查过的 3×3 公开数学和独立大圆基准、四面体/八面体/部分覆盖/歧义、连续性和分块测试 | 调用方必须显式提供扬声器顺序和三元组；只使用最短大圆分段和线性增益；不自动三角剖分，不推断 Delaunay/外壳/距离/多普勒/听音者方向/LFE/HRTF/JOC 或原始创作对象身份 |
| 渲染 | 静态显式声源双耳直接 FIR 渲染器 | `ADMITTED_WITH_SCOPE` | `openjoc-render` 精确方向 HRIR/提供者校验、独立完整卷积基准、耳朵顺序、历史、尾部、重置、失败原子性和输入/尾部分块测试 | 调用方提供有限值、等长的 HRIR 系数和精确静态方向；使用固定听音者方向和直接因果 f64 FIR 参考路径；SOFA 解析/插值由 `openjoc-sofa` 负责；不支持移动声源、房间、距离、HRTF 数据库或 JOC 桥接 |
| 渲染 | 静态显式声源均匀分区双耳卷积 | `ADMITTED_WITH_SCOPE` | `openjoc-render` 固定 FFT 后端、直接 FIR 等价、多种分区大小/声源、部分输入、精确尾部和生命周期回归测试 | 调用方选择一个固定的二次幂 `P`；FFT 大小为 `2P`，输入是精确的 `P` 采样点分区加一次最终不完整分区，调度延迟明确为 `P` 个采样点；不支持自适应选择、非均匀分区、SOFA、插值、移动声源或 JOC 桥接 |
| 渲染 | 严格的 `SimpleFreeFieldHRIR` SOFA 读取和受限 HRIR 插值 | `ADMITTED_WITH_SCOPE` | `openjoc-sofa` 合成 CDF-1 测试样本、坐标/耳朵/延迟/坏文件测试、精确身份、球面线段/三角形插值、延迟/ITD、方位角环绕、有限结果和稀疏覆盖测试，以及直接/分区构建集成 | 本地只读 NetCDF classic CDF-1 子集；SOFA 约定版本 1.0–1.2，恰好两个接收器，球面度/度/米声源位置和整数采样延迟；不支持 HDF5/NetCDF-4、重采样、下载、写入或任意覆盖声明；超出实测本地球面范围的插值会拒绝继续处理 |
| 渲染 | 绑定原始创作对象的 `ObjectScene` 或渲染器保真度 | `NOT_ADMITTED` | 范围受限的解码对象绑定无法识别原始声源，也无法复现专有渲染器 | 不声明原始创作对象身份、双耳等价或 Dolby 渲染器保真度 |
| 发布 | OpenJOC 0.14.0 平台软件包及现有 LAV 集成 | `ADMITTED_WITH_SCOPE` | GitHub Actions 源码/版本检查、原生平台质量门槛、软件包验证、C ABI 产物检查、汇总校验和验证、解码 JOC 动态 ADM 回归覆盖，以及现有分层 DirectShow/LAV 传输和端点证据 | 工作流面向 macOS arm64、Windows x86_64 和 GNU/Linux x86_64；Windows DirectShow/LAV 子集仍限于 v0.12.0 起的七种固定方案；不声明物理多声道硬件和自动语义协商；精确的 OpenJOC ADM 不支持或不声明直接送入 DEE；不保证重建 ADM 与原生渲染器在听感上等价 |

这个矩阵有意把生产状态和证据类别分开。一个数值上合法的重建行不是原始创作对象；某个真实载体被厂商配置接受，也不能证明 ETSI 严格语义有误。

## 面向用户的入口

```text
openjoc inspect FILE
openjoc decode FILE -o DIR [--internal-base] [--streaming]
openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR
openjoc diagnose-tools FILE --vector-id ID --json OUTPUT
openjoc census [MANIFEST] -o DIR
openjoc diagnose-oamd FILE [OPTIONS]
openjoc render-joc FILE [--topology TOPOLOGY.json] [--layout LAYOUT | --layout-file LAYOUT.json | --binaural [--sofa HRTF.sofa] [--virtual-layout LAYOUT]] --output OUTPUT.wav|OUTPUT.caf [--downmix auto|loro|ltrt] [--lfe-policy exclude|equal-power-dual-mono]
openjoc --version
```

CLI 会输出结构化的失败信息，永远不会悄悄降低所选的校验级别；诊断输出也会明确称为重建行，而不是原始创作对象分轨。解码输出目录只创建一次，稳定的机器可读清单会用 `openjoc.*.v1` 标记自己的模式版本。
