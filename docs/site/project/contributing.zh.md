!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 参与贡献

有关代码、清洁室实现、测试和仓库维护规则，请先阅读仓库中的[贡献指南](https://github.com/chyinan/OpenJOC/blob/master/CONTRIBUTING.md)。

## 文档工作流

本站使用 MkDocs 和 Material for MkDocs。Markdown 页面位于 `docs/site/`；导航由 `mkdocs.yml` 管理；少量视觉样式位于 `docs/site/assets/stylesheets/extra.css`；`.github/workflows/docs.yml` 负责构建和部署网站。

在仓库根目录运行：

```sh
py -3 -m venv .venv-docs
.venv-docs\\Scripts\\python -m pip install -r requirements-docs.txt
.venv-docs\\Scripts\\python -m mkdocs serve
.venv-docs\\Scripts\\python -m mkdocs build --strict
```

在 Unix-like 主机上，请激活虚拟环境后运行 `python -m mkdocs serve` 或 `python -m mkdocs build --strict`。

新增页面时，请把它放在最合适的信息架构分区，并在 `mkdocs.yml` 中加入导航；提交前运行严格构建。每个技术事实只应有一个 canonical 维护位置。带日期的研究、发布证据和内部实现计划应继续放在仓库原有目录中，除非它们有明确的面向用户用途。

## 研究类贡献

如果要处理编解码、渲染器、存储或硬件验证等较难的问题，请先阅读[开放问题与贡献方向](open-problems.md)。在开始大型研究实现前，建议先开一个 Discussion，让已有的证据边界和停止条件能够被看见。有价值的贡献需要可复现的测试、清楚的证据边界，并遵守清洁室来源规则。小型确定性修复和普通文档修改可以按正常流程直接提交。

## 翻译

英文是技术内容的 canonical 来源。每个页面都应在旁边维护一个简体中文版本，文件名使用 `page.zh.md`；命令、选项、标识符、类型名和文件名保持不变。翻译时要保留所有范围限定和限制说明。中文页面顶部的简短翻译提示是有意保留的：如果中英文出现差异，以英文版为准。
