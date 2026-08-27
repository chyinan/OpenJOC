!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 重建 ADM 语义

ADM 导出器写出的是解码后 JOC 对象信号，以及 OpenJOC 能在受支持配置内绑定的空间元数据；这是一种便于传输的表示。

它不会导出扬声器渲染结果、FinalLinkedGain 输出或 HRTF 输出。导出发生在解码器内部的场景边界：

```text
解码后的 JOC 对象 PCM + 解码后的 OAMD
                │
                ▼
       JOC 数据内部的绑定检查
                │
                ▼
       重建的 ADM 对象
```

## 会被序列化的内容

- 作为有符号 24 位小端 PCM 的解码对象 PCM；
- 生成的 ADM 对象、声道、流、轨道和 TrackUID 身份；
- 受支持配置内的解码 OAMD 位置事件；
- 存在 Base LFE 时生成的最小合法 5.1 DirectSpeakers 床层；
- RIFF/RF64 记账信息、`chna`、公开的 `dbmd` 和 EBUCore XML 关系；
- 同目录的 JSON 报告，其中包含映射、未写入内容、余量和恢复状态字段。

在受支持的动态路径中，OpenJOC 会把有限的归一化 OAMD 房间坐标映射为归一化 ADM 笛卡尔坐标：

```text
ADM X = 2 × OAMD X - 1
ADM Y = 1 - 2 × OAMD Y
ADM Z = OAMD Z
```

ADM 位置块使用解码采样域中的事件边界。导出器会写入目标配置要求的跳转插值元数据：第一个块为 `0`，后续块为 `250` 个采样点；不会把源 OAMD 的 ramp 数值凭空复制到 ADM 字段中。

## 不会被当作已恢复事实写入的内容

生成的名称、编号、UID 和轨道分配都属于 OpenJOC。导出器不会恢复 DAW/Logic 中的原始创作身份、原始 ADM 层级、源分轨 PCM、未量化的自动化、Dolby 制作来源，或 JOC 到 ADM 的无损逆向转换。

请参阅[解码对象与创作对象](../concepts/decoded-vs-authored-objects.md)了解身份模型，参阅[重建 ADM 导出](reconstructed-adm-export.md)了解完整的文件和策略约定。
