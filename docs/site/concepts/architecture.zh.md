!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# OpenJOC 生产架构

这是当前生产数据流的 canonical 描述。它说明的是已经实现的边界，并不表示历史设计目标都已经完成。

OpenJOC 直接实现空间渲染 DSP，而不是把对象渲染交给平台专用的空间音频引擎。操作系统音频 API 可以用于集成和 I/O，但不会决定 OpenJOC 的空间渲染结果。所有平台使用同一个渲染器和同一套空间语义。

CLI、GStreamer 插件和 FFmpeg 外部桥接都只是同一个 `OpenJocSession` 之上的传输前端，不包含各自独立的渲染流程。GStreamer 负责自己的 buffer/caps/segment 生命周期；FFmpeg 桥接使用 libavformat 完成解复用，并使用公开的 libavutil AVFrame 分配方式。在这两条路径中，E-AC-3/JOC 解码、场景构建、DRC/Dialnorm、扬声器渲染、双耳 HRTF 渲染、延迟和排空状态都由 OpenJOC 保持负责。

## 数据流

显式 render-scene 工作流由 `openjoc-render-scene` 实现。它依赖 `openjoc-render`、`openjoc-sofa` 和 `openjoc-wave`，并且有意与 `openjoc-scene` 分开，这样解码器元数据和 `ReconstructionBasis` 行就不能进入调用方绑定的声源接口。

```text
原始 EC-3 / 支持随机访问的 ISO BMFF
          │
          ▼
输入/容器所有权与访问单元传递
          │
          ├── E-AC-3 基础解码 ──► 带声道标记的 PCM / RcLfe
          │
          └── EMDF 负载
                  ├── OAMD ──► 对象元数据和带时间状态
                  └── JOC  ──► ReconstructionBasis 行
                                      ├──► 编解码器域 JOC 桥接层
                                      │       （T(t) 仍未解决）
                                      │
                                      └──► 精确清洁室绑定检查
                                               │
                                               ▼
                                      已绑定的解码 JOC 对象
                                               │
                                               ▼
                                      重建动态 ADM
```

绑定分支的范围有意窄于解码器和渲染器分支。OAMD 提供解码后的动态元数据；JOC 提供解码后的对象音频行。只有精确符合支持条件的 JOC 配置，才会把两者配对。其他场景使用方仍然可以收到元数据和解码器坐标行，但不会自动把它们解释为原始创作对象。

重建 ADM 分支只在该配置范围内建立解码场景和 ADM 结构的正确性。它不会恢复原始创作 Atmos 母版，也不会证明与原生 JOC 最终渲染器在听感上等价；如果需要与原生渲染器完全一致的定位结果，仍应以原生 JOC 播放为参考。

`render-joc` 工作流会在解码组件边界之后，增加一条明确的实验性扬声器路径：

```text
原始 EC-3 / 支持随机访问的 ISO BMFF
          ↓
受限的访问单元传递 → E-AC-3 Base + RcLfe + JOC/OAMD 解码
          ↓
解码后的 Base/RB 编解码器坐标数据 + 自动组装的桥接控制信息
          ↓
持久化的 JocSpatialBridge → 活跃的 N 声道扬声器平面
  ├── Base LFE/RcLfe → 仅 LFE 平面
  └── 活跃平面 + LFE → 共享的最终联动扬声器增益
                              ↓
                      增量式语义 WAV/CAF 输出
```

CLI 预设只是注册在通用 `SpatialLayout` 和 `JocSpatialBridge` 投影路径上的数据。公共库可以接收调用方定义的声道注册表、几何布局和输出顺序，而不需要预设专用的投影算法；CLI 通过 `--layout-file` 接受同样的带版本几何布局。

`22.2` 预设对应 ITU-R BS.2051-3 Sound System H（9+10+3）：由底层、中层、上层和顶层组成的四层拓扑，包含 22 个空间扬声器和两个语义 LFE 输出目的地。它与已有布局使用同一套 N 层点投影和语义操作边界；LFE 声道永远不是投影顶点。

自动组装会根据经过验证的 OAMD 以及解码后的 Base/RB 状态，生成编解码器坐标控制信息。完整的 sidecar 是可选的；如果明确提供，它会作为覆盖或测试输入优先使用，自动来源和显式来源不会被隐式合并。拓扑/数量、坐标维度、元数据更新和 Base 拓扑变化，都会在集成边界检查；不会猜测并构造行/对象渲染器。公共语义 PCM 顺序由显式预设决定；精确的扬声器 WAV 掩码仍由后端决定。`5.1` 仍然是 `FL, FR, FC, LFE, Ls, Rs`。

解析器读取 JOC 数据中实际存在的内容，随后由明确的配置执行校验。解码器只消费已经接受的表示，不会隐藏厂商兼容性决策。

## 分层边界

### 输入与容器

原始 E-AC-3 使用进程内增量读取器。普通的、支持随机访问的 ISO BMFF 使用采样游标，容器所有权与访问单元消费者彼此分开。不支持随机访问和分片 MP4 不在当前约定范围内。

### E-AC-3 基础层

基础解码器会明确保存帧、音频块、耦合、SPX、AHT、重混矩阵、子流和 TDAC 状态。声道标记和 `RcLfe` 作为基础层携带的信息保留；`RcLfe` 不是动态重建行。

### OAMD 与配置

OAMD 元数据会解析为带类型的状态和带时间的更新。`ETSI_STRICT` 执行已发布的校验规则。`OBSERVED_VENDOR_COMPAT` 必须显式选择，而且只提供部分兼容：它保留原始元数据并记录偏差，但不会解释无法解决的厂商扩展数据。

面向用户的 `decode` 和 `decode-payload` 命令提供单独的 `AUTO` 选择策略。它只解析一次，先执行严格校验；只有当所有阻断性偏差都已列入白名单时，才使用已有的兼容策略。格式错误、不安全、未知或未列入白名单的失败仍然会失败。选择诊断会包括请求的配置、实际选择的配置、严格校验状态、偏差集合和原因。`AUTO` 不是解析器或渲染器配置；明确选择 `ETSI_STRICT` 时永远不会回退，而规范性检查本来就以严格为目的。

观察到的 OAMD `warp_mode` 值 `raw=3` 在 ETSI 严格解析下仍属于保留值。精确的已观测兼容配置会把 raw3 作为无法解释的扩展数据保留，并且可以在测试场景范围内直接使用解码后的 OAMD 空间元数据；实现中不存在生产环境的别名、变换、偏移或裁剪猜测。

### 场景与绑定

`ObjectScene` 会把对象元数据、带时间的位置和解码器坐标行保留在不同的数据域中。对于精确受支持的 decoded-JOC/OAMD 配置，`SemanticBindingState::ResolvedWithinCarrier` 表示重建 ADM 路径中的 JOC 数据内部配对。对于其他配置，它保持 `Unresolved`；不存在隐式的“行号等于创作对象”、槽位身份或主行回退规则。

### JOC ReconstructionBasis

JOC 重建会生成带结构索引、数值行为确定的行。这些行仍然是解码器坐标，不是原始创作对象的 PCM。在精确受支持的配置中，动态 ADM 路径还可以把一行解释为范围受限的解码 JOC 对象；这并不会把它变成创作分轨。诊断 WAV 导出路径为 `diagnostics/reconstruction_rows/row_NNN.wav`。

稳定的组件边界使用 `ReconstructionBasisRowIndex` 作为本地解码器坐标身份。`DecodedComponentLayout` 和 CLI 的 `diagnostics/components.json` 清单，会区分 Base 全频带声道、独立 Base LFE、带索引的 RB 行，以及 `SemanticBindingState::Unresolved` 或受支持的 JOC 数据内部状态，不会再保留一份 PCM 副本。需要原始创作对象音频身份的操作会明确失败，即使解码对象绑定已经受支持；组件域解码和流式处理仍然可用。

### JOC 空间重建桥接层

`openjoc-scene` 提供 `JocSpatialFrameBridge` 和带版本的 `openjoc.joc-spatial-reconstruction.v1` 编解码器域约定。借用的 `CodecBasisBlock` 携带明确标记的 Base 全频带 PCM、带索引的 ReconstructionBasis 行和独立的 RcLfe；`JocSpatialMetadataFrame` 携带当前 OAMD 负载和节目结构维度；`SampleRange` 为每个已提交的解码帧提供绝对的半开采样区间。桥接层采用流式处理，不保留与时长成比例的 PCM。

语义操作表示为 `o(t) = T(t)c(t)`，随后交给独立的渲染器算子。`T(t)` 尚未知，因此 `JocSpatialOperatorState` 保持 `Unresolved`，`require_resolved_operator()` 是硬性检查。解码组件不会自动转换成 `ExplicitSpatialScene`。范围受限的解码对象绑定，只解决重建 ADM 中解码 JOC 行 `j` 与解码 OAMD 动态序号 `j` 的 JOC 数据内部关联；它不会解决 `T(t)`、扬声器渲染算子或原始创作来源身份。不存在固定的 RB 行/创作对象映射，也不存在隐式矩阵或置换。准备度统计仍是仓库内部产物，可在[`docs/joc_reconstruction_readiness.json`](https://github.com/chyinan/OpenJOC/blob/master/docs/joc_reconstruction_readiness.json)查看。

明确激活的 `JocSpatialBridge` 是下游空间函数，目前仍属于实验性功能。它读取无损保留的拓扑/坐标快照，投影到调用方提供的公共布局，应用 Q32 增益调度器，并将结果线性累加到调用方拥有的缓冲区。它不会改变配置校验、分配原始创作对象身份，也不会为创作或渲染器语义解决 `SemanticBindingState`；raw warp-3 字段会作为不透明数据保留，不参与投影计算。普通域和激活路径的支持范围仍是实验性实现边界。[E-AC-3 JOC 概览](eac3-joc-overview.md)说明了这条路径与重建 ADM 导出的关系。

Base 和 ReconstructionBasis 声部累加到最终语义扬声器平面后，共享渲染器会对受支持的 48 kHz、32 采样点适配器块应用具有因果性的公共 FinalLinkedGain 阶段。它会把活跃 LFE 纳入联动声道集合，加入一块扬声器输出历史，并随流/时间线生命周期重置。SOFA 双耳路径不使用这一阶段；Base 下混音过载保护和增益前声部线性仍属于独立约定。

#### Base 全频带场景组合检查

解码后的非 LFE Base/全频带声道已知可以作为 JOC 重建输入。但这与独立的 Base 声部是否会保留到最终 JOC 交付场景，是两回事。实验性桥接层可以同时累加 Base 坐标和 ReconstructionBasis 行，但在 `T(t)` 仍未解决时，这种编解码器坐标组合不能证明最终场景方程成立。尤其是，解码后的 Base C 出现较强能量，并不能证明应该再次导出 Base C；原始创作床层也不能用来解释或授权这次导出。Base 加对象必须先通过明确的重复计数校验，才能改变 ADM 场景组合。

### 显式空间渲染器

`openjoc-render` crate 是独立的 Layer-A/Layer-B 基础组件。它只接受调用方提供的 `ExplicitSpatialSource` 数据块：其中包含不透明的声源 ID、单声道 PCM、明确的笛卡尔坐标和明确的线性增益。初始渲染器使用公开的等功率规律，把前方水平半球映射到 `FL, FR`，并把借用的数据块混入调用方拥有的浮点缓冲区。它不依赖 `openjoc-scene`、`DecodedJocComponents` 或 `ReconstructionBasis`，因此未解决的解码器行不会通过隐式转换变成创作空间声源。

初始的 `StereoRenderer` 会拒绝后半球和未定义的水平面方向；立体声渲染忽略仰角，且默认不裁剪。`SpeakerLayout2d` 和 `LayoutRenderer2d` 增加了任意经过验证的水平布局，以及确定性的相邻声道对、经过检查的 2×2 VBAP 风格增益。调用方提供的扬声器顺序就是公开的平面输出顺序；不支持的角度间隔会明确失败。二维渲染器忽略仰角，也没有 LFE/低频管理路径。独立的双耳渲染器和实验性 JOC 空间桥接层不会改变这套二维约定，也不会提供自动 JOC 语义绑定。

`SpatialState2d`、`TrajectorySegment2d` 和 `SourceTrajectory2d` 增加了明确的分段线性自动化约定。分段端点是包含在内的绝对采样索引；方位角遵循明确的最短/递增/递减路径策略，声源增益在线性域中插值。`StereoRenderer::render_trajectory_block` 和 `LayoutRenderer2d::render_trajectory_block` 会逐采样点评估状态，因此在相同绝对时间线下，整块、不规则分块和单采样点分块的结果一致。轨迹块借用 PCM 和调用方拥有的输出平面，执行受限的预检，不会按采样点分配内存，也不会为整条时间线分配内存。轨迹只描述方向：不会渲染半径、Z、距离、多普勒、房间效果、仰角或 HRTF。

`Speaker3d`、`SpeakerTriplet3d` 和 `SpeakerLayout3d` 增加了明确的三维拓扑约定。调用方提供公开的扬声器顺序和每个可用的 VBAP 三元组；`LayoutRenderer3d` 永远不会猜测 Delaunay 三角剖分、外壳、覆盖范围或“最佳三元组”。每个声明的三元组都按公开的 3×3 系统 `S g = p` 求解，并检查有限值、非奇异性、非负增益和单位能量归一化。精确命中某个扬声器时使用确定性 one-hot 增益。如果一个方向被多个声明的三元组覆盖，它们完整的公开顺序增益向量必须一致，否则渲染会因歧义错误失败。部分布局遇到不支持的方向时会明确失败；LFE 和低频管理不在此渲染器约定内。三维渲染器只接受显式声源和调用方拥有的平面 `f64` 输出。`SpatialState3d`、`TrajectorySegment3d`、`SourceTrajectory3d` 和 `TrajectorySourceBlock3d` 在同一不可变拓扑上增加采样精确的动态路径。每个分段使用规范化单位方向之间的最短大圆 SLERP、稳定的小角度分支、线性增益插值，并明确拒绝对跖方向歧义。更长的路线需要调用方提供中间关键帧，不会自动推断路径。`LayoutRenderer3d::render_trajectory_block` 会评估绝对采样索引，保持静态输出等价、端点/关键帧连续性和与分块方式无关的逐字节一致性。它会在清空调用方拥有的平面 `f64` 输出前预检每个采样点，也不会按采样点分配堆内存。距离、多普勒、听音者方向、房间效果、LFE、HRTF/双耳渲染、JOC、ObjectScene 和原始创作对象桥接，都不在此约定内。

`HrirPair`、`HrirEntry` 和 `HrirBank` 提供紧凑的、由调用方提供的精确方向 HRIR 约定，并可保留明确的逐耳延迟元数据，供构建时插值使用。`StaticBinauralSource` 绑定固定的显式声源 ID、规范方向、线性增益和 HRIR 条目；它不会推断原始创作对象身份。`BinauralRenderer` 使用固定的听音者约定（`+Y` 向前、`+X` 向右、`+Z` 向上），先输出 `LEFT_EAR`，再输出 `RIGHT_EAR`，并执行直接的因果时域 FIR 卷积。它保留 HRIR 开头延迟，在调用方拥有的输入块之间保留有界的逐声源历史，并提供明确的尾部排空和重置语义。`openjoc-sofa` 会先解析精确方向，再执行有界且与延迟对齐的球面插值，然后注册静态声源。

`PartitionedBinauralRenderer` 是一个额外的、由调用方选择的均匀重叠相加后端：它的固定二次幂分区 `P` 使用 `2P` FFT，报告一个分区的调度延迟，准确接受 `P` 个采样点的输入分区和一次明确的最终不完整操作，并准确排空已注册声源中最大的 `M-1` 个因果尾部。它会预先计算 HRIR 频谱，只保留与滤波器长度相关的有界频域/时域状态，不保存与时长成比例的 PCM 历史，也不会自动选择后端。直接 FIR 仍是数值基准，因此跨后端验证是数值验证，不承诺逐比特一致。

独立的 `openjoc-sofa` crate 是一个构建时只读适配器，负责把有意保持狭窄的 `SimpleFreeFieldHRIR` SOFA 约定转换成 `HrirBank`。它依赖 `openjoc-render`；渲染器本身不依赖文件解析、NetCDF/HDF5 库或操作系统专用 API。当前可移植读取器接受项目测试过的 NetCDF classic CDF-1 子集、固定听音者姿态、以球面度/度/米表示的声源位置、恰好两个接收器、统一采样率和整数采样延迟。左右耳映射由接收器几何位置决定，而不是由数组顺序决定。优先使用精确查找；非精确请求使用确定性的本地球面线段/三角形，并共享两耳权重；稀疏或超出范围的覆盖会拒绝继续处理。构建完成后不会保留 SOFA 文件句柄，两个渲染器也不会在每个音频块执行文件 I/O。HDF5/NetCDF-4 仍不在可移植运行时读取器范围内；内置 SADIE II 资源会在离线阶段转换到同一 CDF-1 路径，渲染时不需要网络。重采样、移动声源、写入 SOFA 和任何 JOC 语义桥接，都不在此边界内。

### 捕获与流式处理

捕获模式可以保留元数据和诊断产物。流式模式使用有界的访问单元/帧状态，并逐步完成输出；不会悄悄捕获无界的 ObjectScene 或重建行向量。

格式错误或截断的原始 E-AC-3，以及已经读取的 ISO BMFF 结构，都会以有界诊断的方式拒绝。流式输出会先暂存，只有完整解码成功后才会提升为正式输出，因此失败不会发布一个不完整的 canonical 文件。

### 范围受限的 decoded-JOC/OAMD 绑定

负载边界只有一条明确的清洁室结构检查，适用于普通严格配置和精确的已观测 raw3 兼容配置：15 个解码 JOC 对象、无 OAMD 床层、总索引 0 处有一个 Base LFE、无 ISF、15 个动态 OAMD 对象，以及总计 16 个 OAMD 条目。只有检查通过后，规范的带类型映射才会产生 `joc_ordinal = dynamic_ordinal` 和 `oamd_total_index = joc_ordinal + 1`。

`+1` 集中定义在 `DecodedJocBindingProfile` 中；它不是元素 ID 查询、音频内容匹配，也不是 PCM 猜测。

`SemanticBindingState::ResolvedWithinCarrier` 只能说明解码后的 JOC PCM 与解码后的 OAMD 元数据在受支持的 JOC 配置内完成配对。它不会恢复原始创作 ADM 身份。带床层、带 ISF、备用 LFE、数量/顺序不匹配、无法识别的兼容性偏差以及 Base LFE 不完整的情况，仍然无法解决；动态 ADM 元数据导出器也不接受停用过渡。

动态 ADM 路径复用已有的绝对场景采样域。它会为导出计划保留元数据事件，但永远不会重复节目 PCM：每条生成的 `OpenJOC Reconstructed JOC Object NN` 轨道，都在已有的受限流式写入过程中消费对应的重建行，而 ADM 块则由对应的 OAMD 事件边界生成。重置、不连续、flush 和流重新打开都会创建新的解码器/场景时期；过期元数据和 PCM 不会被合并。

## 错误与证据模型

格式错误的输入、不支持的容器形态、严格配置违规和输出失败会分别分类。诊断结果或经验性结果不能提升语义绑定状态。当前声明边界汇总在[能力矩阵](../project/capabilities.md)和[已知限制](../compatibility/known-limitations.md)中。
