!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# CLI 参考

本页根据 v0.17.0 可执行文件的输出进行核对，使用的命令是：

```sh
cargo run -p openjoc-cli --locked -- --help
cargo run -p openjoc-cli --locked -- inspect --help
cargo run -p openjoc-cli --locked -- render-joc --help
cargo run -p openjoc-cli --locked -- export-adm --help
```

CLI 源码 `crates/openjoc-cli/src/main.rs` 仍是唯一准确信息来源。命令语法发生变化时，请重新运行上述命令。

## 命令

```text
openjoc inspect <FILE> [--json] [--aus] [--objects] [--emdf] [--verbose] [--au N | --au-range START:END] [--trim-config-count N]
openjoc decode <FILE> -o <DIR> [--downmix <FILE> | --internal-base] [--streaming]
openjoc export-adm <INPUT|SCENE_DIR> -o <OUTPUT.wav|OUTPUT.bw64> [--adm-policy best-effort|strict] [--overwrite]
openjoc validate-adm <FILE> [--json]
openjoc self-test [--fixture <JOC.ec3>]
openjoc diagnose-tools <FILE> --vector-id <ID> --json <OUTPUT>
openjoc census [MANIFEST] -o <DIR>
openjoc diagnose-oamd <FILE> [-o <DIR>] [--access-unit N | --au START..END | --all-access-units]
openjoc render-scene <SCENE> --binaural-sofa <FILE> --output <DIR> --backend direct|partitioned
openjoc render-joc <FILE> (--layout <PRESET> | --layout-file <CUSTOM.json>) --output <OUTPUT.wav|OUTPUT.caf>
openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE> -o <DIR>
openjoc sofa inspect <FILE> [--json]
openjoc --version
```

## `inspect`

```text
usage: openjoc inspect <FILE> [--json] [--aus] [--objects] [--emdf] [--verbose] [--au-range START:END] [--trim-config-count N]
```

`inspect` 读取原始 EC-3，或可定位的普通 MP4/M4A E-AC-3 音轨，不写出音频文件。即使详细输出只选择部分访问单元，它仍会统计完整输入流。

- `--json` 向标准输出写入一个带 schema 版本的 JSON 文档；JSON 合约版本为 1。
- `--aus` 包含访问单元明细；`--au N` 是选择一个从零开始计数的访问单元的简写。
- `--au-range START:END` 选择包含首尾的、从零开始计数的访问单元范围，并自动启用访问单元明细。选择范围不会缩小完整流统计。
- `--objects` 包含每个 OAMD 槽位的活动和位置统计。
- `--emdf` 展开人类可读的 EMDF 配置细节。JSON 始终包含 EMDF 统计和配置。
- `--verbose` 增加组件配置和有界诊断解释。
- `--trim-config-count N` 覆盖规范中的 OAMD trim 配置数量；默认值为 9。

当遍历完整结束时，`inspect` 返回零，即使报告中包含格式错误的元数据。调用方应检查 `validation` 和 `diagnostics`，不要只看退出码。压缩流被截断或无法读取时，程序会输出安全可知的部分报告并返回非零。

## `render-joc`

```text
usage: openjoc render-joc <FILE> [--topology <TOPOLOGY.json>] (--layout <PRESET> | --layout-file <CUSTOM.json>) --output <OUTPUT.wav|OUTPUT.caf>
       [--downmix auto|loro|ltrt] (2.0 speaker output only; not binaural)
       [--dialnorm default|digital|analog] [--normalize-peak <TARGET_DBFS>]
       [--binaural [--sofa <HRTF.sofa>] [--virtual-layout <LAYOUT>] | --binaural-sofa <HRTF.sofa>]
       [--backend direct|partitioned --partition-size N]
       [--lfe-policy exclude|equal-power-dual-mono]
       [--validation-profile auto|etsi-strict|observed-vendor-compat]
       [--trim-config-count N] [--internal-base-policy current-default|codec-core]
       [--drc disabled|line|rf|custom] [--drc-boost 0..=100 --drc-cut 0..=100]
       [--reference-f64] [--diagnostic-contribution full|base-only|reconstruction-only]
       [--no-progress] [--performance-report <FILE.json>] [--overwrite]
```

支持的预设包括 `2.0`、`5.1`、`5.1.2`、`5.1.4`、`7.1`、`7.1.2`、`7.1.4`、`7.1.6`、`9.1`、`9.1.2`、`9.1.4`、`9.1.6` 和 `22.2`。`--layout-file` 接受带版本的自定义球面几何布局；预设布局仍是常规使用路径。

`--drc` 控制 E-AC-3 中编码的动态范围元数据。`--dialnorm` 控制节目响度校准。`--normalize-peak` 在渲染后对文件输出应用一个可选的静态缩放系数。这些选项都不是限幅器，也不是 LUFS/真峰值归一化器。

## `export-adm`

```text
usage: openjoc export-adm <INPUT|SCENE_DIR> -o <OUTPUT.wav|OUTPUT.bw64> [--adm-policy best-effort|strict] [--no-progress] [--overwrite]
```

该命令导出重建的 RIFF/RF64 ADM BWF 文件。它无法恢复原始 ADM 母版。尽力模式是默认策略；严格模式会拒绝不支持或无法确认绑定关系的动态数据。

## 命令边界

- `ETSI_STRICT` 永远不会自动降级；
- `OBSERVED_VENDOR_COMPAT` 必须显式选择，而且只提供部分兼容；
- 不支持随机访问或分片 MP4 的流式处理不在支持范围内；
- `render-scene` 只接受显式静态声源和严格的本地 SOFA 子集；
- `ReconstructionBasis` 行不是原始创作对象的 PCM；
- 在非交互执行中，如果输出文件已经存在，必须使用 `--overwrite`；替换过程仍保持事务性。
