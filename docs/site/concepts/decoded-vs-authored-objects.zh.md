!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 解码对象与创作对象

下面三者不是一回事：

```text
原始创作 Atmos 对象
        ≠ JOC 解码输出对象
        ≠ 重建 ADM 对象
```

## 原始创作对象

原始创作对象位于源 DAW 或 ADM 母版中，可能带有名称、UID、层级、轨道身份、源分轨 PCM、未量化的自动化信息以及创作元数据。

OpenJOC 无法从有损的 JOC 传输流中恢复这些信息。

## JOC 解码输出对象

JOC 解码后得到的是只对应当前 JOC 数据的输出对象。OpenJOC 用 `ReconstructionBasis` 行表示这些信号。在明确受支持的配置组合中，每一行都可以按照 JOC 数据内部的序号，与对应的解码 OAMD 运动信息配对：

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

`+1` 是因为完整 OAMD 列表开头有一个 Base LFE。它不是对创作 ID 的查询，也不是根据 PCM 猜出来的对应关系。

## 重建 ADM 对象

`export-adm` 会根据解码场景生成 ADM 对象。`OpenJOC Reconstructed JOC Object 04` 这样的名称，只表示这是 OpenJOC 生成的输出；它不能证明这个信号就是源 Logic 或 ADM 工程中的第 04 个创作对象。

导出结果可以保留有意义的解码后运动。但 JOC 可能会量化元数据、重新组织对象表示、改变编号或丢弃创作信息，所以这些运动可能与源工程中的自动化轨迹不同。

## 恢复状态

导出器会明确记录以下边界：

| 声明 | 状态 |
| --- | --- |
| 已恢复原始创作身份 | `false` |
| 已恢复原始 ADM 母版 | `false` |
| JOC 到 ADM 的无损往返转换 | `false` |

`ResolvedWithinCarrier` 只能说明解码后的 JOC PCM 与解码后的 OAMD 运动信息在限定范围内成功对应；更不能据此认为原始创作对象的身份已经找回，也不表示结果与原生渲染器一致。

## 实际使用规则

用重建 ADM 文件查看 JOC 携带的解码场景，并与其他工具互操作。需要原生渲染器的定位结果时，以原生 JOC 播放为参考。关于验证范围和剩余保证，请阅读[重建 ADM 与渲染器的等价性](../compatibility/renderer-equivalence.md)。
