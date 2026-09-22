!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 解码与检查

CLI 将输入检查、诊断解码和最终渲染分开处理。

## 检查 JOC 数据

```sh
openjoc inspect input.ec3
```

`inspect` 会报告访问单元结构、JOC 元数据、配置选择和受限解析结果，但不会写出最终音频文件。

如果需要机器可读输出或更多检查细节：

```sh
openjoc inspect input.ec3 --json
openjoc inspect input.ec3 --aus --objects --emdf
openjoc inspect input.ec3 --au-range 10:20 --json
openjoc inspect input.ec3 --verbose
```

`--json` 会向标准输出写入带版本的报告。`--aus` 包含访问单元明细。`--au N` 选择一个从零开始计数的访问单元，`--au-range START:END` 选择包含首尾的范围；这两种选择都不会缩小完整流统计。`--objects` 增加每个 OAMD 槽位的统计，`--emdf` 展开人类可读的 EMDF 配置细节，`--verbose` 增加有界诊断解释。完整语法见 [CLI 参考](../reference/cli-reference.zh.md#inspect)。

## 捕获解码器诊断输出

```sh
openjoc decode input.ec3 --output decoded
```

捕获流程会写出元数据清单、真实反映解码结果的组件清单，以及用于诊断的 ReconstructionBasis 行 WAV。这些行是解码器输出，不是原始创作对象的分轨。

如果需要带内部 Base 诊断的受限流式解码：

```sh
openjoc decode input.ec3 \\
  --output decoded \\
  --internal-base \\
  --streaming
```

Rust 数据包 API 每次推送接受一个完整的 E-AC-3 JOC 访问单元：I0 加可选 D0。解复用、任意字节拆分和多个访问单元，属于受限的流式解码器或框架适配器负责的范围。

## 选择校验配置

`auto` 是解码的默认策略。`etsi-strict` 永远不会回退。`observed-vendor-compat` 是明确的部分兼容策略：它会保留无法解释的扩展数据，但不会赋予这些数据厂商语义。

```sh
openjoc decode input.ec3 \\
  --output decoded \\
  --validation-profile etsi-strict
```

如果流中使用了保留的 OAMD 值，例如原始 `warp=3`，严格模式拒绝是该配置下预期的结果，不是程序悄悄降级造成的。
