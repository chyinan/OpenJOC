# OpenJOC v0.10.0 私有路径门禁协调摘要

审计对象：

- `artifacts/openjoc-lav-0.10.0-windows-x64.zip`
- `artifacts/openjoc-lav-0.10.0-corresponding-source.zip`

哈希复核通过，严格路径扫描复现 42 条命中：

- `runtime/LAVAudio.ax`：1 条 CodeView/RSDS `PdbFileName`，属于通用构建路径。
- `runtime/libbluray.dll`：1 条 CodeView/RSDS `PdbFileName`，另有 37 条 libbluray `BD_DEBUG` 宏通过 `__FILE__` 传入 `bdpriv_debug` 的源文件字面量；均为通用构建路径。
- `LAVFilters-OpenJOC/ffmpeg/ffbuild/config.log`：3 条 `USERPROFILE`/assembler 诊断构建根路径，属于预期源码构建元数据。

分类计数：

| 分类 | 数量 |
| --- | ---: |
| USER_IDENTIFYING_PRIVATE_PATH | 0 |
| PRIVATE_PROJECT_PATH | 0 |
| GENERIC_BUILD_PATH | 39 |
| TOOLCHAIN_PATH | 0 |
| EXPECTED_SOURCE_BUILD_METADATA | 3 |
| FALSE_POSITIVE | 0 |

项目安全核心对 `config.log` 返回 0 findings；默认完整 ZIP 扫描返回
`release security scan passed: findings=0`。未发现用户识别路径、私有项目
身份、凭据、token 或秘密赋值。

结论：`RELEASE_PATH_GATE=PASS`。本次仅完成只读协调，没有重建、重新打包、
发布、推送、打标签或创建 fork。
