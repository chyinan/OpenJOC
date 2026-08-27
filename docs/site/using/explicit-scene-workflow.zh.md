!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 显式 render-scene 工作流

OpenJOC 可以把调用方指定的单声道 WAV 声源和本地提供的 `SimpleFreeFieldHRIR` SOFA 文件，渲染成一个可移植的静态场景。场景版本为 `openjoc.render-scene.v1`；它不包含 JOC 对象、`ReconstructionBasis` 行、OAMD 槽位或后端设置。

```json
{
  "schema": "openjoc.render-scene.v1",
  "sample_rate_hz": 48000,
  "source_semantics": "explicit_spatial_sources",
  "sources": [
    {"id":"voice","input_wav":"audio/voice.wav","start_sample":0,
     "position":{"x":0.0,"y":1.0,"z":0.0},"gain":1.0}
  ]
}
```

声源路径相对于场景文件。绝对路径、跳出父目录、符号链接逃逸、重复 ID、未知字段、不支持的方向和采样率不匹配，都会在正式生成输出前被拒绝。支持的声源 WAV 包括单声道 PCM16/24/32 和单声道 IEEE-float32；不会执行重采样、归一化、裁剪或抖动。

先检查受支持的 SOFA 文件：

```text
openjoc sofa inspect listener.sofa --json
```

使用明确指定的后端渲染：

```text
openjoc render-scene scene.json --binaural-sofa listener.sofa \\
  --backend direct --output render-direct
openjoc render-scene scene.json --binaural-sofa listener.sofa \\
  --backend partitioned --partition-size 256 --output render-partitioned
```

输出目录采用事务式写入，包含 `binaural.wav`（立体声 IEEE-float32，先左耳后右耳）和 `render.json`（`openjoc.render-result.v1`）。输出长度等于场景输入时间线加上完整的因果 HRIR 尾部（`N + M - 1`）；不会隐藏开头延迟，也不会裁掉尾部。后端必须明确选择，永远不会自动切换。

SOFA 的支持边界有意保持狭窄：只支持 SimpleFreeFieldHRIR、1.0/1.1/1.2 版本，以及来自 J5R8 的可移植 NetCDF classic CDF-1 子集。不支持 HDF5/NetCDF-4、其他约定、插值、最近方向回退、移动声源或下载。用户需要自行负责本地 SOFA 数据的许可和来源。

该工作流独立于尚未解决的 JOC 语义绑定；结果清单中的 `joc_semantic_binding` 为 `unresolved_not_used`。
