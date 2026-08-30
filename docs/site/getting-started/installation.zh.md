!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 安装

你可以直接使用发行版压缩包，也可以从仓库源代码构建 CLI。

## 发行版压缩包

从 [OpenJOC 发行版页面](https://github.com/chyinan/OpenJOC/releases)下载与你的平台匹配的软件包。把它解压到自己有权限管理的目录，然后将 CLI 加入 `PATH`，或者直接使用完整路径调用。

v0.14.0 提供 macOS arm64、Windows x86_64 和 GNU/Linux x86_64 版本。不同平台的软件包有各自的运行时和许可要求；请参考压缩包附带的快速开始说明。

## 从源代码构建

Rust 工作区使用的版本、仓库地址和最低支持 Rust 版本，都在根目录的 `Cargo.toml` 中声明。请从一份干净的代码副本开始：

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

在 Windows 上运行 `target\\release\\openjoc.exe`。

要安装到指定目录：

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
```

## 容器输入所需工具

原始 E-AC-3 由 OpenJOC 的受限读取器处理。支持随机访问的普通 MP4/M4A 由仓库的容器处理流程负责，可能需要 `ffprobe` 或 `ffmpeg`。不支持随机访问的 MP4，以及分片 MP4，不属于文档规定的流式处理路径。

## Windows LAV 软件包

通过 DirectShow 播放 Windows 媒体是一个独立的可选软件包。它会在原有的 LAV 旁边安装 OpenJOC 自有筛选器，不会自动修改 PotPlayer。解压软件包后，按照 [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) 操作。

## 检查安装

```sh
openjoc --help
openjoc self-test
```

如果缺少可选的公开测试样例，`self-test` 会报告 `NOT_APPLICABLE`；对于依赖测试样例的检查，这不代表它被静默判定为成功。
