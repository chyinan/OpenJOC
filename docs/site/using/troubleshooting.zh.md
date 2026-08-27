!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 故障排除

先判断是哪一道边界失败，再修改配置或归因于 renderer。

## 首先检查

```sh
openjoc --version
openjoc --help
openjoc self-test
```

测试 MP4/M4A 时，让输入保持本地且可 seek。文档规定的输入路径是 raw E-AC-3 与 seekable ordinary MP4/M4A；fragmented 或 non-seekable MP4 不属于 streaming path。container input 按[安装](../getting-started/installation.md)中的说明准备 `ffprobe` 和 `ffmpeg`。

## 渲染前先 inspect

```sh
openjoc inspect input.ec3
openjoc decode input.ec3 --output-dir decode-report
```

`inspect` 报告 carrier classification、profile、topology、timing 和 rejection boundary，不会让 speaker renderer 猜测。`decode` 生成 metadata manifests 与 diagnostic ReconstructionBasis row WAV。decode 成功不表示每个 speaker layout 或 container 都能表示该结果。

## render command 失败

在 [CLI 参考](../reference/cli-reference.md)中检查精确的 option spelling，先运行最小准入命令：

```sh
openjoc render-joc input.ec3 --layout 2.0 --output stereo.wav
```

然后再切换到目标布局。只有在 channel identity 能由 `WAVEFORMATEXTENSIBLE` 表示时，才可使用对应的 standard mask。`7.1.6` 和 `9.1` family 需要 CAF 来保留 semantic channel description；`22.2` 与 custom geometry 使用 explicit unmasked PCM，这不等于任意硬件播放的声明。

选择双耳输出时要记住它是 virtual-speaker rendering：

```sh
openjoc render-joc input.ec3 --binaural --output headphones.wav
```

内置 SADIE II D1 HRTF 是 generic 的。custom local SOFA 必须符合文档规定的 strict `SimpleFreeFieldHRIR` subset；不支持 HDF5/NetCDF-4 和 automatic resampling。详见[双耳与 SOFA](binaural-sofa.md)。

## ADM export 失败或 Objects 不动

独立验证输出：

```sh
openjoc export-adm input.ec3 --output reconstructed.wav --adm-policy best-effort
openjoc validate-adm reconstructed.wav
```

除非完整的 decoded-JOC/OAMD binding profile 被准入，否则 strict export 会 fail closed。best-effort 会保留 neutral/static output，并在无法证明 correspondence 时记录 `unsupported_binding_reason`。在准入 profile 内，移动的 reconstructed Objects 表示 decoded carrier-local movement，不是 authored Atmos master recovery。阅读[解码 Objects 与创作 Objects](../concepts/decoded-vs-authored-objects.md)和[重建 ADM 导出](reconstructed-adm-export.md)。

如果 export 报告 PCM24 range 或 non-finite error，不要期待 clipping、normalization 或 hidden limiter。writer 会在 signed 24-bit boundary fail closed；参阅 [PCM24 headroom](../compatibility/pcm24-headroom.md)。

## Windows playback 没有使用 OpenJOC

运行 package 的 `verify.bat`，确认 OpenJOC filter 已注册，并在 PotPlayer 中将 **LAV Audio Decoder (OpenJOC)** 设为 **Prefer**。普通 E-AC-3 与 compressed passthrough 有意保持 stock path。只有正向确认的 JOC 才会进入 OpenJOC filter。详见 [Windows LAV / PotPlayer](windows-lav-potplayer.md)。

## 收集有用的 issue report

包含 OpenJOC version、platform、exact command、经过清理的 `inspect` 或 validator output、selected layout/container，以及 failure 是否 deterministic。未经许可不要附带 private/commercial media 或 derived PCM。结构 validator 通过不等于 native JOC renderer equivalence；请分别报告两类观察。
