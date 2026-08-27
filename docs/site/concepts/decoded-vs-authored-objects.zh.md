!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 解码 Objects 与创作 Objects

以下三者不同：

```text
原始创作 Atmos Objects
        ≠ 解码后的 JOC 输出 Objects
        ≠ 重建的 ADM Objects
```

## 原始创作 Objects

创作 Objects 位于源 DAW 或 ADM master 中。它们可能具有名称、UID、层级、track identity、源 stem PCM、未量化 automation 以及 authoring metadata。

OpenJOC 无法从有损的 JOC delivery stream 中恢复这些属性。

## 解码后的 JOC 输出 Objects

JOC reconstruction 产生载波本地的 decoder outputs。OpenJOC 使用 `ReconstructionBasis` rows 表示这些信号。在明确准入的 profile 内，每行都可以通过 typed carrier-local ordinal 与对应的解码 OAMD movement 配对：

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

`+1` 用于计入 total OAMD list 中前置的 Base LFE。它不是 authored ID lookup，也不是 PCM heuristic。

## 重建的 ADM Objects

`export-adm` 从解码场景中序列化生成的 ADM Objects。诸如 `OpenJOC Reconstructed JOC Object 04` 的名称表示 OpenJOC output identity，并不能证明这个信号是源项目中的 authored Object 04。

导出可以保留有意义的解码后运动。由于 JOC 可能量化元数据、重排对象表示、改变编号或丢弃创作信息，该运动可能不同于源 automation。

## Recovery state

exporter 会明确保留以下声明：

| 声明 | 状态 |
| --- | --- |
| 原始 authored identity 已恢复 | `false` |
| 原始 ADM master 已恢复 | `false` |
| 无损 JOC-to-ADM round trip | `false` |

`ResolvedWithinCarrier` 只表示 decoded JOC PCM ↔ decoded OAMD 关系通过了限定的 admission gate。它永远不会升级为 authored identity 或 renderer parity。

## 实际规则

使用重建 ADM 文件来检查 JOC 所携带的解码场景并进行互操作。需要 native-renderer 定位时，以 native JOC playback 为参考。关于验证范围和剩余保证，请阅读[重建 ADM 渲染器等价性](../compatibility/renderer-equivalence.md)。
