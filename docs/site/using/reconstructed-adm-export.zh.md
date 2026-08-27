!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 导出可互操作的重建 ADM BWF

OpenJOC 提供一种基于公开标准的重建方式导出，方便与其他工具互操作；同时支持在明确限制内处理压缩媒体流：

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav
openjoc validate-adm OUTPUT.wav
```

## 这个文件到底代表什么

对于明确受支持的 JOC 配置组合，OpenJOC 可以把解码后的 JOC 对象音频信号，与同一个 JOC 节目中解码得到的运动元数据对应起来。因此，生成的 ADM 对象可以带有运动轨迹。这里重建的是 JOC 数据中实际保留下来的对象场，不是原始 Atmos 创作母版。

导出时生成的名称、编号和 UID 都属于 OpenJOC；它们不是原始 DAW/Logic 工程中的轨道身份、创作对象编号、ADM 对象 UID，也不是源分轨 PCM。JOC 是有损编码，解码后的运动信息可能与源工程中的自动化轨迹不同。不支持的配置组合在尽力模式下会保持中性，严格模式则会明确拒绝导出。

## 结构正确性不等于渲染器等价

OpenJOC 重建的是解码后 JOC 对象场的 ADM 表示，目的是实现互操作。它不会恢复原始创作的 Dolby Atmos 母版，也不保证通用 ADM 渲染器与原生 JOC 最终渲染器会做出听感上完全相同的定位选择。

验证覆盖解码数据和 ADM 结构边界：对象 PCM、JOC 数据内部的对象绑定、坐标、时序、受支持的增益/状态元数据、轨道身份、容器结构以及 ADM 关系。这些检查能够证明解码场景和文件结构符合当前范围，但不能证明与原生渲染器的听感定位一致。

在至少一个真实世界的验证节目中，重建 ADM 通过适用的技术检查后仍观察到残余定位差异。这个结果与具体素材有关，不能推广到所有节目。需要与原生渲染器保持一致时，仍应以原生 JOC 播放为参考。这是重建 ADM 互操作性的已知限制，并不表示解码对象场或导出结构无效。

## 重建的动态对象

对于明确受支持的 decoded-JOC/OAMD 配置组合，OpenJOC 会按 JOC 数据内部的序号，把每个解码 JOC 对象信号与对应的解码 OAMD 运动元数据绑定：

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

`+1` 只是因为完整 OAMD 列表开头有一个 Base LFE；它不是对创作 ID 的查询，也不是根据 PCM 猜出来的对应关系。`ResolvedWithinCarrier` 只能说明“解码 JOC PCM ↔ 解码 OAMD”这条关系在限定范围内通过了检查；更不能据此认为原始创作对象的身份已经找回。

导出的动态对象可以保留有意义的解码后运动，但它仍是 OpenJOC 生成的对象，而不是恢复出来的源对象。JOC 可能量化元数据、重新组织对象表示、改变编号或丢弃创作信息，因此轨迹可能与 DAW 工程中的自动化轨迹不同。

## 坐标转换

OAMD 的房间坐标与 ADM 的笛卡尔坐标属于不同的公开坐标域。受支持的室内配置使用 OAMD 坐标：X 从左墙 `0` 到右墙 `1`，Y 从前墙 `0` 到后墙 `1`，Z 从地面 `-1` 到天花板 `1`。ADM 使用以中心为原点的归一化立方体：X 向右为正，Y 向前为正，Z 向上为正。

```text
ADM X = 2 × OAMD X - 1
ADM Y = 1 - 2 × OAMD Y
ADM Z = OAMD Z
```

桥接层会检查有限值以及支持的归一化输入/输出范围；不支持的坐标会被拒绝，不会被静默截断。

## 不支持的配置组合与两种策略

带床层、带 ISF、alternate-LFE、数量或顺序不匹配、无法识别的兼容性偏差、缺少完整 Base LFE，以及其他未经验证的配置组合，都不能进行动态绑定。尽力模式会保留中性/静态位置，并记录 `unsupported_binding_reason`；严格模式会拒绝导出。

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy best-effort
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy strict
```

ADM 中不会凭空补出声场范围、增益、发散度、声道锁定、区域，以及当前不支持的激活/停用过渡等语义。若 PCM24 数据超出范围，或包含 NaN、无穷大等非有限值，写入器会在有符号 24 位量化边界处拒绝写入并明确报错；它不会擅自裁剪、饱和、归一化、限幅，也不会悄悄降低音量。详见 [PCM24 余量](../compatibility/pcm24-headroom.md)。

## 5.1 床层与工具互操作

如果存在已知的 Base LFE PCM，导出器会把它放入一个生成的、最小合法 5.1 DirectSpeakers 床层的 LFE 位置。L、R、C、Ls、Rs 是为了补齐合法传输结构而生成的静音占位声道，不是额外的创作对象内容。

生成的 ADM 可以由 Logic Pro 导入；维护者验证过的、由 Logic 创作工程重新导出的文件，也可以被 Dolby Encoding Engine 接受。但这不会让 OpenJOC 生成的文件获得 Dolby 制作来源，直接送入 DEE 仍不受支持，项目也不作此声明。完整的字段、报告和容器边界请参阅[英文原版页面](reconstructed-adm-export.md)。
