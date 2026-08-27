!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 标准与参考资料

OpenJOC 面向公开 ADM 和容器互操作性的工作，建立在以下参考资料之上。项目不会把厂商私有实现细节复制到仓库或本站。

- [ITU-R BS.2076](https://www.itu.int/rec/R-REC-BS.2076/) —— Audio Definition Model（音频定义模型）；
- [ITU-R BS.2088-2](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2088-2-202511-I!!PDF-E.pdf) —— 长格式 WAVE 元数据块及其大小语义；
- [Dolby Atmos Master ADM Profile v1.0](https://developer.dolby.com/globalassets/documentation/technology/dolby_atmos_master_adm_profile_v1.0.pdf) —— 配置关系和互操作限制；
- [EBU Tech 3285 Supplement 6](https://tech.ebu.ch/publications/tech3285s6) —— 公开 `dbmd` 封装语义；
- [EBU Tech 3285 Supplement 7](https://tech.ebu.ch/publications/tech3285s7) —— `chna` 块的参考语义；
- ETSI TS 103 420 和 TS 103 190-2 —— 实现来源记录中收录的公开 E-AC-3 JOC 和 OAMD 语法子集。

[重建 ADM 导出](../using/reconstructed-adm-export.md)说明当前写入器和校验器实际使用了哪些部分；[清洁室实现方法](clean-room-methodology.md)说明证据类别和禁止使用的实现来源。
