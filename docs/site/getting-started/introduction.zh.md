!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 简介

OpenJOC 是一个独立的 Rust E-AC-3 JOC 解码与渲染流水线实现。它接受受支持的原始 E-AC-3 或可 seek 的普通 MP4/M4A 输入，解码基础节目与 JOC 重建数据，并将所得场景提供给多种输出路径。

主要面向用户的入口包括：

- `openjoc render-joc`：扬声器或双耳渲染；
- `openjoc export-adm`：重建 ADM BWF 表示；
- `openjoc inspect` 和 `openjoc decode`：元数据与重建诊断；
- Rust `OpenJocSession` API 和有版本的 C ABI：嵌入式使用；
- 项目提供的 FFmpeg、GStreamer、mpv 与 Windows DirectShow/LAV 集成。

项目有两个重要的语义边界：

1. `ReconstructionBasis` 行是解码器域、载波本地的输出信号，不是创作得到的 Atmos stem。
2. 重建 ADM exporter 只在其准入 profile 内，将解码后的 JOC 对象 PCM 与解码后的 OAMD 运动绑定；它不会恢复源 ADM 母版。

OpenJOC 使用 48 kHz 渲染域。普通 JOC 扬声器路径是实验性的，并受文档规定的 profile、布局、输出容器和验证状态约束。[能力矩阵](../project/capabilities.md)负责详细的状态词汇。

## 第一次成功渲染

安装 CLI，运行 `openjoc --help`，然后按照[快速开始](quick-start.md)操作。如果输入是可 seek 的 M4A 或 MP4，请按照[安装](installation.md)中的说明准备 `ffprobe` 和 `ffmpeg`。
