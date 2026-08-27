!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

![OpenJOC](assets/openjoc-header.png){ .openjoc-hero }

# OpenJOC

使用 Rust 编写的开源、清洁室实现 E-AC-3 JOC 解码器与空间渲染器。

OpenJOC 可解码 E-AC-3 JOC 节目，重建解码后的对象信号，并将其渲染到受支持的扬声器布局或双声道双耳输出。它还可以导出面向互操作的、由解码对象场重建而成的 ADM BWF 表示。

[开始使用](getting-started/quick-start.md){ .md-button .md-button--primary }
[安装](getting-started/installation.md){ .md-button }
[查看源代码](https://github.com/chyinan/OpenJOC){ .md-button }

!!! warning "使用导出的 ADM 前请先阅读边界"
    重建 ADM 并不是原始 Atmos 创作母版的恢复。它只在文档规定的 profile 内保留解码后的、载波本地的对象场。如果你需要互换或监听指导，请先阅读[解码 Objects 与创作 Objects](concepts/decoded-vs-authored-objects.md)以及[渲染器等价性](compatibility/renderer-equivalence.md)。

## OpenJOC 能做什么

<div class="grid cards" markdown>

-   :material-waveform: **解码 JOC 节目**

    解码原始 E-AC-3，或包含 E-AC-3 的可 seek 普通 MP4/M4A，并使用有界的访问单元与 profile 处理。

-   :material-speaker-multiple: **渲染扬声器布局**

    从双声道到 22.2 的受支持 preset，或使用最多 64 个输出声道的、经过验证的自定义几何布局。

-   :material-headphones: **渲染双耳输出**

    使用内置 SADIE II D1 HRTF 或受支持的本地 SOFA 文件，将虚拟扬声器场渲染为双声道耳机输出。

-   :material-file-music: **导出重建 ADM**

    将带有受支持 OAMD 运动的解码 JOC 对象 PCM 写入经过验证的 RIFF/RF64 ADM BWF 表示。

-   :material-language-rust: **嵌入解码器**

    使用 Rust session API 或有版本的 C ABI。FFmpeg、GStreamer、mpv 与 Windows LAV adapter 共享同一个核心 session 边界。

</div>

## 选择下一步

| 你的目标 | 从这里开始 |
| --- | --- |
| 渲染第一个节目 | [快速开始](getting-started/quick-start.md) |
| 安装 CLI 或从源代码构建 | [安装](getting-started/installation.md) |
| 在 Windows 上使用 PotPlayer | [Windows LAV / PotPlayer](using/windows-lav-potplayer.md) |
| 理解对象身份 | [解码 Objects 与创作 Objects](concepts/decoded-vs-authored-objects.md) |
| 为其他工具导出 ADM | [重建 ADM 导出](using/reconstructed-adm-export.md) |
| 将 OpenJOC 集成到软件中 | [Rust API](reference/rust-api.md) 或 [C ABI](reference/c-abi.md) |

## 当前版本

本站以仓库 **v0.13.0** 为基线。支持范围是有意限定的；在把渲染或导出结果当作生产交付物前，请阅读[能力矩阵](project/capabilities.md)和[已知限制](compatibility/known-limitations.md)。

OpenJOC 与 Dolby Laboratories 没有隶属、认可或赞助关系。第三方名称归其各自所有者所有。
