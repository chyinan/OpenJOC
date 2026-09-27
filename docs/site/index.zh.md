!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

![OpenJOC](assets/openjoc-header.png){ .openjoc-hero }

# OpenJOC


OpenJOC 是一个使用 Rust 编写的开源、以净室方式开发的 **E-AC-3 JOC Object 解码器与空间渲染器**。它可直接从受支持的 Dolby Atmos E-AC-3 JOC 码流中提取带内 EMDF，解码 OAMD 对象元数据与 JOC 重建数据，再渲染为扬声器或双耳输出，或导出重建 ADM。

[快速开始](getting-started/quick-start.md){ .md-button .md-button--primary }
[安装](getting-started/installation.md){ .md-button }
[查看源代码](https://github.com/chyinan/OpenJOC){ .md-button }

!!! warning "使用导出的 ADM 前，请先了解它的边界"
    重建 ADM 不是原始 Atmos 创作母版的恢复。它只在文档规定的配置范围内，保留 JOC 数据中实际解码得到的对象场。如果你需要互操作或监听方面的指导，请先阅读[解码对象与创作对象](concepts/decoded-vs-authored-objects.md)和[渲染器等价性](compatibility/renderer-equivalence.md)。

## OpenJOC 能做什么

<div class="grid cards" markdown>

-   :material-waveform: **解码 JOC 对象**

    从受支持的 E-AC-3 JOC 码流中提取带内 EMDF，解码 OAMD 对象元数据和 JOC 重建数据，并重建仅对应当前载波的对象信号。

-   :material-speaker-multiple: **渲染扬声器布局**

    使用从双声道到 22.2 的受支持预设布局，也可以提供经过验证的自定义几何布局，最多输出 64 个声道。

-   :material-headphones: **生成双耳输出**

    使用内置 SADIE II D1 HRTF，或受支持的本地 SOFA 文件，把虚拟扬声器场渲染成双声道耳机输出。

-   :material-file-music: **导出重建 ADM**

    将带有受支持 OAMD 运动信息的解码 JOC 对象 PCM，写入经过验证的 RIFF/RF64 ADM BWF 表示。

-   :material-language-rust: **嵌入解码器**

    使用 Rust 会话 API 或带版本的 C ABI。FFmpeg、GStreamer、mpv 和 Windows LAV 适配器都基于同一套核心会话接口。

</div>

## 按目标开始

| 你的目标 | 从这里开始 |
| --- | --- |
| 渲染第一个节目 | [快速开始](getting-started/quick-start.md) |
| 安装 CLI 或从源代码构建 | [安装](getting-started/installation.md) |
| 在 Windows 上使用 PotPlayer | [Windows LAV / PotPlayer](using/windows-lav-potplayer.md) |
| 理解对象身份 | [解码对象与创作对象](concepts/decoded-vs-authored-objects.md) |
| 为其他工具导出 ADM | [重建 ADM 导出](using/reconstructed-adm-export.md) |
| 将 OpenJOC 集成到软件中 | [Rust API](reference/rust-api.md) 或 [C ABI](reference/c-abi.md) |

## 当前版本

本站以仓库 **v0.18.0** 为基线。支持范围是有意限定的；在把渲染或导出结果当作生产交付物前，请阅读[能力矩阵](project/capabilities.md)和[已知限制](compatibility/known-limitations.md)。

OpenJOC 与 Dolby Laboratories 没有隶属、认可或赞助关系。第三方名称归其各自所有者所有。
