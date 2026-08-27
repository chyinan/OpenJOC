!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 安装

你可以使用 release 压缩包，也可以从仓库构建 CLI。

## Release 压缩包

从 [OpenJOC releases 页面](https://github.com/chyinan/OpenJOC/releases)下载对应平台的 asset。将其解压到你控制的目录并把 CLI 加入 `PATH`，或者使用完整路径调用。

v0.13.0 release workflow 面向 macOS arm64、Windows x86_64 和 GNU/Linux x86_64。生态系统 package 有各自的 runtime 与许可边界；请使用每个压缩包附带的 package quick-start 文档。

## 从源代码构建

workspace 在根目录 `Cargo.toml` 中声明 Rust edition、版本、仓库和最低支持 Rust 版本。从干净 checkout 开始：

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

在 Windows 上使用 `target\\release\\openjoc.exe` 中的等效二进制文件。

要安装到指定 prefix：

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
```

## 输入工具

原始 E-AC-3 由 OpenJOC 的有界 reader 处理。可 seek 的普通 MP4/M4A 使用仓库的 container boundary，可能需要 `ffprobe` 或 `ffmpeg`。非 seek 或 fragmented MP4 不属于文档规定的 streaming 路径。

## Windows LAV package

通过 DirectShow 的 Windows 播放是独立的可选 package。它会在 stock LAV 旁边安装 OpenJOC 自有 filter，不会自动修改 PotPlayer。解压 package 后，按照 [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) 操作。

## 检查安装

```sh
openjoc --help
openjoc self-test
```

如果缺少可选的 public fixture，`self-test` 会报告 `NOT_APPLICABLE`；这不是对依赖 fixture 的检查默默报告成功。
