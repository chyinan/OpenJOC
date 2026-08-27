!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 快速开始

本指南带你把一个受支持的 JOC 节目从输入文件渲染成第一个输出文件。

## 渲染扬声器布局

```sh
openjoc render-joc input.m4a \\
  --layout 7.1.4 \\
  --output output.wav
```

输出文件的扩展名决定容器格式。只要标准布局的声道身份能够由 `WAVEFORMATEXTENSIBLE` 表示，就可以使用 WAV；需要保留语义声道描述时，应使用 CAF。

## 尝试立体声或双耳输出

```sh
openjoc render-joc input.m4a --layout 2.0 --output stereo.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
```

双耳模式默认使用 7.1.4 虚拟扬声器场，默认 LFE 处理策略为 `exclude`。如需自定义 SOFA 文件或其他虚拟布局，请参阅 [CLI 参考](../reference/cli-reference.md)中的选项说明。

## 渲染前先检查

```sh
openjoc inspect input.ec3
openjoc --help
```

用 `inspect` 查看输入是否被识别为受支持的 JOC 数据，以及它适用的配置范围或拒绝边界。需要元数据清单（manifest）或诊断用 `ReconstructionBasis` 行 WAV，而不是最终扬声器文件时，使用 `decode`。

## 导出重建 ADM 文件

```sh
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

导出器还会在同一目录生成 `.adm-report.json`。如果要在 DAW 或互操作流程中使用该文件，请先阅读[重建 ADM 导出](../using/reconstructed-adm-export.md)。

## 下一步

- 扬声器设置：[扬声器渲染](../using/speaker-rendering.md)
- 自定义几何布局：[自定义扬声器布局](../using/custom-speaker-layouts.md)
- 耳机或 SOFA：[双耳与 SOFA](../using/binaural-sofa.md)
- Windows 播放：[Windows LAV / PotPlayer](../using/windows-lav-potplayer.md)
- 输出语义：[输出格式](../reference/output-formats.md)
