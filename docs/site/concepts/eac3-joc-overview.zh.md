!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# E-AC-3 JOC 概览


E-AC-3 JOC 对象解码从受支持的数据载体开始。OpenJOC 会从码流中提取并解析带内 EMDF，再分别解码支持范围内的 OAMD 前缀/时间线与 JOC 重建载荷。JOC 重建使用已解码的 E-AC-3 声道音频，生成只对应当前数据载体的 `ReconstructionBasis` PCM 行。

```text
受支持的 E-AC-3 JOC 访问单元
  +-- E-AC-3 帧/音频块 --------------------------> 基础声道 PCM
  +-- 带内辅助数据 ------------------------------> 提取并解析 EMDF
        +-- OAMD 载荷 ---------------------------> 支持范围内的前缀/时间线
        |                                             -> 解码空间元数据
        +-- JOC 载荷 + 已解码的基础声道 PCM ------> JOC 重建
                                                      -> carrier-local ReconstructionBasis PCM 行

已解码的基础 PCM + OAMD 元数据 + 重建行
  +-- render-joc -> Base/RB PCM + OAMD 桥接控制
  |                 -> 有范围限制的实验性扬声器/双耳路径
  +-- export-adm -> 精确的 JOC/OAMD 绑定门槛
                    -> 解码 JOC 对象 PCM + 受支持的 OAMD 运动信息
                    -> 重建 ADM BWF
```

这是 OpenJOC 的概念处理图，不是 Dolby 规范解码器架构图。OAMD 元数据和 JOC 重建行是分开的解码结果；只有精确通过门槛的配置才会把它们关联为 JOC 解码对象，其他配置仍保持未绑定。这种关联不能恢复原始创作对象身份、源分轨 PCM、未量化自动化信息或原始 Atmos 母版。

`render-joc` 的扬声器/双耳路径使用实验性的 JOC 空间桥接层，其编解码器域算子 `T(t)` 仍未解决。双耳输出是对受支持扬声器场的虚拟化，不代表直接对象双耳渲染或与原生渲染器等效。重建 ADM 导出有独立的精确绑定和验证范围。

实现职责与状态边界见 [OpenJOC 架构](architecture.md)；对象身份边界见[解码对象与创作对象](decoded-vs-authored-objects.md)；各能力的状态和证据见[能力矩阵](../project/capabilities.md)。
