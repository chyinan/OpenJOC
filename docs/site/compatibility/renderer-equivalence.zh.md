!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 重建 ADM 渲染器等价性

OpenJOC 重建的是解码后 JOC 对象场的、面向互操作的 ADM 表示。它不恢复原始创作 Dolby Atmos master，也不保证 generic ADM renderer 渲染该文件时能产生与 native JOC renderer 感知上相同的定位。

## 已在范围内验证

当前 validation boundary 覆盖：

| Boundary | 状态 |
| --- | --- |
| Decoded object PCM scene | 在 tested scope 内已验证。 |
| Decoded JOC Object ↔ OAMD mapping | 对准入的 carrier-local profile 已验证。 |
| OAMD-to-ADM coordinate conversion | 已验证。 |
| Position interpolation 与 event timing | 对导出 profile 已验证。 |
| Physical BW64 / `chna` / TrackUID mapping | 在 tested real-media audit 中验证为 15/15。 |
| Public renderer-state coverage | `COMPLETE_WITH_SCOPE`。 |

这些检查建立 decoded-scene 与 ADM-structure correctness，但不建立 generic ADM renderer 与 native JOC renderer 最终 localization choices 相同。

## 剩余保证

一个 self-authored real-world validation programme 在 decoded-scene、mapping、coordinate、timing 和 file-structure 检查通过后仍出现 residual localization difference。该观察与具体素材有关，不能泛化；因此 exact perceptual equivalence 仍在保证范围之外。

需要 exact native-renderer localization 时，以 native JOC playback 为参考。不要因此把 ADM exporter 视为 fundamentally broken：它的 contract 是 reconstructed interoperability，而不是 native renderer emulation。

这也是 `ResolvedWithinCarrier` 不等于 authored identity 的原因。请参阅[解码 Objects 与创作 Objects](../concepts/decoded-vs-authored-objects.md)和[重建 ADM 导出](../using/reconstructed-adm-export.md)。
