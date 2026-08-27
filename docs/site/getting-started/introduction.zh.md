!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 简介

OpenJOC 是一个独立的 Rust 实现，提供 E-AC-3 JOC 解码和空间渲染流程。它接受受支持的原始 E-AC-3，或支持随机访问的普通 MP4/M4A 输入，解码基础节目和 JOC 重建数据，再按需要输出为扬声器、双耳或 ADM 文件。

普通用户主要会用到以下入口：

- `openjoc render-joc`：扬声器或双耳渲染；
- `openjoc export-adm`：导出重建的 ADM BWF 表示；
- `openjoc inspect` 和 `openjoc decode`：查看元数据和重建诊断信息；
- Rust `OpenJocSession` 会话 API 和带版本的 C ABI：嵌入应用；
- 项目提供的 FFmpeg、GStreamer、mpv 和 Windows DirectShow/LAV 集成。

这里有两个需要先分清的概念：

1. `ReconstructionBasis` 行是解码器内部、只对应当前 JOC 数据的输出信号，不是创作得到的 Atmos 分轨。
2. 重建 ADM 导出器只会在受支持的配置组合内，把解码后的 JOC 对象 PCM 与解码后的 OAMD 运动信息绑定起来；它不会恢复源 ADM 母版。

OpenJOC 使用 48 kHz 渲染域。常规 JOC 扬声器输出仍是实验性的，并受到文档规定的配置范围、布局、输出容器和验证状态约束。[能力矩阵](../project/capabilities.md)负责维护详细的状态定义。

## 先跑出第一个结果

安装 CLI，运行 `openjoc --help`，然后按照[快速开始](quick-start.md)操作。如果输入是 MP4 或 M4A，请确保文件支持随机访问，并按照[安装](installation.md)中的说明准备 `ffprobe` 和 `ffmpeg`。
