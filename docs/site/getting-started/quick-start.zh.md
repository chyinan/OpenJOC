!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 快速开始

本指南将一个受支持的 JOC 节目从输入带到第一次文件渲染。

## 渲染扬声器布局

```sh
openjoc render-joc input.m4a \\
  --layout 7.1.4 \\
  --output output.wav
```

输出扩展名选择容器。标准布局在其声道身份可由 `WAVEFORMATEXTENSIBLE` 表示时可以使用 WAV；需要语义声道描述时使用 CAF。

## 尝试立体声或双耳输出

```sh
openjoc render-joc input.m4a --layout 2.0 --output stereo.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
```

默认双耳虚拟场是 7.1.4，默认 LFE policy 是 `exclude`。可以通过 [CLI 参考](../reference/cli-reference.md)中的选项使用自定义 SOFA 文件和不同的虚拟布局。

## 渲染前先检查

```sh
openjoc inspect input.ec3
openjoc --help
```

使用 `inspect` 查看输入是否被准入为 JOC carrier，以及对应的 profile 或拒绝边界。需要元数据 manifest 或诊断 ReconstructionBasis 行 WAV，而不是最终扬声器渲染时，使用 `decode`。

## 导出重建 ADM 文件

```sh
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

exporter 还会写入相邻的 `.adm-report.json`。在 DAW 或互换工作流中使用该文件前，请阅读[重建 ADM 导出](../using/reconstructed-adm-export.md)。

## 选择下一页

- 扬声器设置：[扬声器渲染](../using/speaker-rendering.md)
- 自定义几何：[自定义扬声器布局](../using/custom-speaker-layouts.md)
- 耳机或 SOFA：[双耳与 SOFA](../using/binaural-sofa.md)
- Windows 播放：[Windows LAV / PotPlayer](../using/windows-lav-potplayer.md)
- 输出语义：[输出格式](../reference/output-formats.md)
