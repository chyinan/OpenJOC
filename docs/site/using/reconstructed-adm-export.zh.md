!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 重建的可互操作 ADM BWF 导出

OpenJOC 提供一种基于标准的重建互换导出，并支持有界的压缩媒体 streaming 路径：

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav
openjoc validate-adm OUTPUT.wav
```

## 这个导出代表什么

对于明确准入的 JOC profile，OpenJOC 可以把解码后的 JOC 对象音频与同一个 JOC 节目携带的解码后运动元数据关联起来。因此生成的 ADM Objects 可以移动。这里重建的是 JOC 所携带的对象场，不是原始 Atmos 创作母版。

生成的名称、编号和 UID 属于本次导出，并不是原始 DAW/Logic track identity、创作 Object 编号、ADM Object UID 或源 stem PCM。JOC 是有损的，解码后的运动可能与源 automation 不同。不支持的 profile 在 best-effort 模式下保持 neutral，或在 strict 模式下 fail closed。

## 结构正确性与渲染器等价性

OpenJOC 重建的是解码后 JOC 对象场的、面向互操作的 ADM 表示。它不恢复原始创作 Dolby Atmos 母版，也不保证 generic ADM renderer 与 native JOC final renderer 产生感知上相同的定位。

验证边界包括解码数据与 ADM 结构：对象 PCM、载波本地对象绑定、坐标、时序、受支持的 gain/state 元数据、track identity、容器结构和 ADM 关系。这些检查建立了结构和解码场景的正确性，但不能建立 native-renderer 的感知等价性。

在至少一个真实世界验证节目中，重建 ADM 通过适用的技术检查后仍观察到残余定位差异。该观察与具体素材有关，不能泛化。需要 renderer-identical 空间定位时，native JOC playback 仍是参考。这是重建 ADM 互操作性的已知限制，不是说解码对象场或导出结构无效。

## 重建的动态 Objects

对于明确准入的 decoded-JOC/OAMD profile，OpenJOC 可以按照载波本地 ordinal，把每个解码 JOC 对象信号与对应的解码 OAMD 运动元数据绑定：

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

`+1` 只表示 total OAMD list 中前置 Base LFE 带来的域偏移，不是 authored ID lookup，也不是 PCM heuristic。`ResolvedWithinCarrier` 只表示这条解码 JOC PCM ↔ 解码 OAMD 关系通过了限定的准入 gate，绝不升级为 authored identity。

导出的动态 Object 可以保留有意义的解码后运动，但它是生成的 OpenJOC identity，不是恢复出来的源 Object。JOC 可能量化元数据、重排对象表示、改变编号或丢弃创作信息，因此轨迹可以不同于 DAW automation。

## 坐标转换

OAMD room position 与 ADM Cartesian position 是不同的 public coordinate domain。准入的 in-room profile 使用 OAMD：X 从左墙 `0` 到右墙 `1`，Y 从前墙 `0` 到后墙 `1`，Z 从地面 `-1` 到天花板 `1`。ADM 使用居中的 normalized cube：X 向右为正，Y 向前为正，Z 向上为正。

```text
ADM X = 2 × OAMD X - 1
ADM Y = 1 - 2 × OAMD Y
ADM Z = OAMD Z
```

bridge 会检查 finite 值以及支持的 normalized 输入/输出范围；不支持的坐标会被拒绝，而不是静默 clamp。

## 不支持的 profile 与 policy

带 bed、带 ISF、alternate-LFE、计数/顺序不匹配、未知 compatibility deviation、缺少完整 Base LFE 或其他未验证的 profile，不能进行动态 binding。best-effort 会保留 neutral/static positions 并记录 `unsupported_binding_reason`；strict 会拒绝。

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy best-effort
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy strict
```

extent、gain、divergence、channel lock、zones 以及不支持的 inactive transition 等语义不会被伪造进 ADM。出现 PCM24 range 或 non-finite error 时，writer 不会 clipping、saturating、normalizing、limiting 或静默衰减，而是在 signed 24-bit quantization 边界 fail closed。详见 [PCM24 余量](../compatibility/pcm24-headroom.md)。

## 5.1 bed 与工具互操作

当已知 Base LFE PCM 存在时，导出器把它放入生成的最小合法 5.1 DirectSpeakers bed 的 LFE 位置；L、R、C、Ls、Rs 是为了完成 transport shape 而生成的静音 placeholder，不是额外的创作对象内容。

生成的 ADM 可以被 Logic Pro 导入；维护者验证的 Logic-authored re-export 可被 Dolby Encoding Engine 接受。但 OpenJOC 自己生成的字节并不因此取得 Dolby authoring provenance，直接 DEE ingest 仍未支持且不作声明。要了解完整的字段、report 和容器边界，请阅读[英文 canonical 页面](reconstructed-adm-export.md)。
