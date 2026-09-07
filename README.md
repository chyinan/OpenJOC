<p align="center">
  <img src="docs/site/assets/openjoc-header.png" alt="OpenJOC — open-source E-AC-3 JOC decoder and renderer" width="100%">
</p>

# OpenJOC

[English](#openjoc) · [简体中文](#中文说明)

OpenJOC is an independent, clean-room E-AC-3 JOC decoder and spatial renderer written in Rust. It decodes supported E-AC-3/JOC input, reconstructs carrier-local object signals, and renders speaker or binaural output through one platform-neutral engine.

Download the [latest release](https://github.com/chyinan/OpenJOC/releases/latest). OpenJOC is not affiliated with, endorsed by, or sponsored by Dolby Laboratories.

## Documentation

**[Read the OpenJOC documentation site](https://chyinan.github.io/OpenJOC/)** · **[阅读简体中文文档](https://chyinan.github.io/OpenJOC/zh/)**

## What it supports

- E-AC-3 JOC decoding with bounded reconstruction;
- supported speaker presets from `2.0` through `22.2` and custom geometry up to 64 output channels;
- two-channel virtual-speaker binaural rendering with the bundled SADIE II D1 HRTF or a supported local SOFA file;
- reconstructed ADM BWF interoperability output with decoded JOC/OAMD binding within a documented profile;
- Rust and versioned C ABI embedding surfaces;
- project-provided FFmpeg, GStreamer, mpv, and Windows DirectShow/LAV integrations.

Read the [capability matrix](docs/site/project/capabilities.md) for the evidence boundary behind each claim.

## Quick start

Build or download OpenJOC, then render a JOC programme:

```sh
openjoc render-joc input.m4a --layout 7.1.4 --output output.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

Use `openjoc inspect input.ec3` to inspect a carrier before rendering. The [quick-start guide](docs/site/getting-started/quick-start.md) covers the first render and points to the detailed output contracts.

For custom geometry, use `--layout-file LAYOUT.json`; the documented limit is 64 output channels.

## Windows playback

The optional Windows package provides an isolated OpenJOC-enabled LAV Audio Decoder. It installs beside stock LAV and does not change PotPlayer automatically:

1. Extract the package from the [OpenJOC releases page](https://github.com/chyinan/OpenJOC/releases).
2. Run `install.bat`, then require `verify.bat` to report **PASS**.
3. In PotPlayer, add **LAV Audio Decoder (OpenJOC)** in **Filter Control** → **Filter Priority (Overall)** and set it to **Prefer**.

The [Windows LAV / PotPlayer guide](docs/site/using/windows-lav-potplayer.md) documents the seven fixed PCM policies, passthrough behavior, rollback, and hardware boundary.

## Important boundaries

Reconstructed ADM is an interoperability-oriented representation of the decoded JOC object scene. It is not recovery of the original authored Atmos master. OpenJOC does not recover original authoring identity, source-stem PCM, unquantized automation, Dolby authoring provenance, or a lossless JOC-to-ADM round trip.

The [decoded Objects vs authored Objects](docs/site/concepts/decoded-vs-authored-objects.md) page explains the identity boundary. The [renderer-equivalence limitation](docs/site/compatibility/renderer-equivalence.md) explains why a generic ADM renderer is not guaranteed to localize exactly like native JOC playback.

## API and integrations

- [Rust API](docs/site/reference/rust-api.md) — serial `OpenJocSession` lifecycle and owned interleaved `f32` output.
- [C ABI](docs/site/reference/c-abi.md) — opaque handles, bounded stream decoding, custom geometry, and panic containment.
- [Integration overview](docs/site/project/integrations.md) — current FFmpeg, GStreamer, mpv, player-bundle, and Windows contracts.

## Build from source

Use the Rust toolchain declared in `Cargo.toml`:

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

Contributors should follow [CONTRIBUTING.md](CONTRIBUTING.md) and run the workspace quality gates before committing.

## Help wanted

OpenJOC is usable today, but several research and validation problems remain
open. Contributions are especially welcome around native-renderer equivalence,
reconstructed-PCM headroom, and physical multichannel hardware validation.

See [Open Problems & Contribution Opportunities](docs/site/project/open-problems.md)
before starting work on codec or renderer semantics. For difficult research,
starting a Discussion first is recommended so work does not repeat an already
investigated path.

## License and notices

OpenJOC core code is licensed under [Apache-2.0](LICENSE). Integration bundles may include components under additional terms; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and package-specific notices. Dolby, Dolby Atmos, SADIE, FFmpeg, GStreamer, mpv, LAV Filters, PotPlayer, Windows, and related names are marks of their respective owners.

## 中文说明

OpenJOC 是一个用 Rust 编写的独立、采用清洁室方式实现的 E-AC-3 JOC 解码器和空间渲染器。它可以解码受支持的 E-AC-3/JOC 输入，重建解码得到的对象信号，并通过与平台无关的引擎输出扬声器或双耳声道结果。

下载[最新版本](https://github.com/chyinan/OpenJOC/releases/latest)。OpenJOC 与 Dolby Laboratories 没有隶属、认可或赞助关系。

## 文档

**[阅读 OpenJOC 中文文档站点](https://chyinan.github.io/OpenJOC/zh/)** · **[Read the English documentation](https://chyinan.github.io/OpenJOC/)**

## 支持的功能

- 在有界重建范围内解码 E-AC-3 JOC；
- 支持从 `2.0` 到 `22.2` 的扬声器预设布局，也支持最多输出 64 个声道的自定义几何布局；
- 使用内置 SADIE II D1 HRTF 或受支持的本地 SOFA 文件，进行双声道虚拟扬声器双耳渲染；
- 在文档规定的配置范围内，输出带有已解码 JOC/OAMD 绑定信息的重建 ADM BWF 互操作表示；
- 提供 Rust 和带版本的 C ABI 嵌入接口；
- 提供项目维护的 FFmpeg、GStreamer、mpv 以及 Windows DirectShow/LAV 集成。

每项声明背后的证据范围，请参阅[能力矩阵](docs/site/project/capabilities.md)。

## 快速开始

构建或下载 OpenJOC，然后渲染一个 JOC 节目：

```sh
openjoc render-joc input.m4a --layout 7.1.4 --output output.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

使用 `openjoc inspect input.ec3` 可以在渲染前检查载体文件。[快速开始指南](docs/site/getting-started/quick-start.md)介绍了首次渲染流程，并指向详细的输出契约。

如需使用自定义几何布局，请传入 `--layout-file LAYOUT.json`；文档规定的上限是 64 个输出声道。

## Windows 播放

可选的 Windows 安装包提供一个独立的、启用 OpenJOC 的 LAV Audio Decoder。它会与原版 LAV 并列安装，也不会自动修改 PotPlayer 的设置：

1. 从 [OpenJOC releases 页面](https://github.com/chyinan/OpenJOC/releases)下载并解压安装包。
2. 运行 `install.bat`，然后确认 `verify.bat` 报告 **PASS**。
3. 在 PotPlayer 的 **Filter Control** → **Filter Priority (Overall)** 中添加 **LAV Audio Decoder (OpenJOC)**，并将其设置为 **Prefer**。

[Windows LAV / PotPlayer 指南](docs/site/using/windows-lav-potplayer.md)说明了七种固定 PCM 策略、直通行为、回滚方式和硬件边界。

## 重要边界

重建 ADM 是对已解码 JOC 对象场的互操作表示，不是对原始 Atmos 创作母版的恢复。OpenJOC 不会恢复原始创作身份、源分轨 PCM、未量化的自动化数据、Dolby 创作来源信息，也不能实现无损的 JOC 到 ADM 往返转换。

[解码对象与创作对象](docs/site/concepts/decoded-vs-authored-objects.md)介绍了对象身份边界。[渲染器等价性限制](docs/site/compatibility/renderer-equivalence.md)解释了为什么通用 ADM 渲染器不保证与原生 JOC 播放具有完全相同的定位结果。

## API 与集成

- [Rust API](docs/site/reference/rust-api.md) — 串行的 `OpenJocSession` 生命周期管理，以及由调用方拥有的交错 `f32` 输出；
- [C ABI](docs/site/reference/c-abi.md) — 不透明句柄、有界流式解码、自定义几何布局和 panic 隔离；
- [集成概览](docs/site/project/integrations.md) — 当前的 FFmpeg、GStreamer、mpv、播放器安装包和 Windows 契约。

## 从源代码构建

使用 `Cargo.toml` 中声明的 Rust 工具链：

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

贡献代码前，请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，并运行工作区质量检查。

## 欢迎贡献

OpenJOC 目前已经可以使用，但仍有一些研究和验证问题需要解决。我们尤其欢迎围绕原生渲染器等价性、重建 PCM 余量以及实体多声道硬件验证的贡献。

开始处理编解码器或渲染器语义之前，请先阅读[开放问题与贡献方向](docs/site/project/open-problems.md)。对于较复杂的研究，建议先发起 Discussion，以免重复已经完成的探索。

## 许可证与声明

OpenJOC 核心代码采用 [Apache-2.0](LICENSE) 许可证。集成安装包可能包含采用其他条款授权的组件，详情请参阅 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 及各安装包的声明文件。Dolby、Dolby Atmos、SADIE、FFmpeg、GStreamer、mpv、LAV Filters、PotPlayer、Windows 及相关名称均为其各自所有者的商标。
