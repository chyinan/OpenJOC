# R00 Logic 坐标复核

这是坐标语义修复后的人工互操作复核包。它不是原始 authored ADM master，也不要求恢复原始 Object identity。

文件：

- `reconstructed-dynamic.bw64`
- `reconstructed-dynamic.adm-report.json`
- ADM SHA-256：`85592FAFF43A2CF00A7DB2539413298744AE26B69CE041A98ABEC8DB9B37D4CF`

请在 Logic Pro 中导入 `reconstructed-dynamic.bw64`，确认：

1. ADM 导入成功。
2. 15 个 reconstructed JOC Object 仍存在，且 generated 5.1 bed 仍存在。
3. 原先缺失的左侧 Object 活动现在有合理分布。
4. 前/后方向听感正确；vocals 不再整体错误地位于听者后方。
5. 开启 head tracking 后，不再出现全局坐标反向/偏置被夸大的现象。
6. 动态轨迹和高度变化仍然存在。

这一步是最终人工门。即使 XML、结构、确定性和 PCM 不变性均通过，未完成本次导入前，`PRIMARY_LOGIC_INTEROP` 仍为 `PENDING_HUMAN_RECHECK`。
