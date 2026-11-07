# 中文译文审校提示（REVIEW_PROMPT）

> 目标语言：**简体中文（zh-Hans）**。本文件给母语审校者（或做回译的模型）使用；它不是应用的一部分。

## 你的角色

你是 bike-osm-ride 的简体中文母语审校者。译文由模型从英语 pivot（`locales/en.json`）和匈牙利语原文
（`locales/hu.json`）生成，**还没有母语者审过**。你的任务不是重译全文，而是找出会让骑行者误解、或让界面
显得不自然的地方，并把结论写回术语表。

## 输入

| 文件 | 内容 |
|---|---|
| `crates/ride-i18n/locales/zh.json` | 界面、错误消息、提示（1055 条） |
| `crates/ride-model/assets/workouts.zh.json` | 93 个训练的名称、目标、描述、步骤、教练提示（2065 条文本） |
| `crates/ride-i18n/GLOSSARY.tsv` | 术语表，`zh` 列是本次译文 |
| `crates/ride-i18n/STYLE.tsv` | 文体规则，`zh` 列是本次译文 |
| `docs/i18n/zh/README.md`、`USER_GUIDE.md`、`PRIVACY.md`、`CHANGELOG.md` | 骑行者文档 |

匈牙利语是源语言；英语是 pivot。遇到歧义时以匈牙利语为准，但不要把匈牙利语的格后缀、词序硬搬进中文。

## 必查项

1. **占位符**：`{name}`、`{n}`、`{err}`、`{}`、`{:.0}` 必须与源文数量、拼写一致；顺序可按中文语序调整。
2. **首尾空格**：有些字符串粘在链接或数字上；首尾空格不能丢。唯一例外是
   `ui_dialogs.settings.ban_es_kapcsold_be_a_szamlazast_a_google_havi_ingyen`（中文语序需要前导空格）。
3. **长度**：按钮、标签、页签、瓦片标签优先 2–4 字；如果译文比原文长很多，先缩短，不要靠省略号。
4. **称呼**：统一用「你」，不用「您」；教练提示要像教练说话，短、具体、鼓励，不要逐字直译。
5. **骑行者**：`骑行者`，绝不用「骑马 / 马术 / 赛马」。
6. **术语**：以 `GLOSSARY.tsv` 的 `zh` 列为准；同一概念在不同页面必须同词。
7. **产品名/单位**：bike-osm-ride、FTP、ERG、TCX、GPX、BRouter、Open-Meteo、Street View、Google、
   Zwift、MyWhoosh、FTMS、BLE、VO₂max、W、W/kg、km、m、%、kcal、kJ、bpm 不翻译。
8. **标点**：句子用全角（，。：；！？），短标签不加句号；`…` 保持一个字符。
9. **数字**：半角数字、小数点（`1234.5`），不加大千位分隔符；不要换算单位。
10. **繁体字**：必须全部是简体；测试 `chinese_uses_simplified_forms` 会拦截常见繁体字。

## 重点风险清单（先看这些）

### A. 固定宽度的短标签

这些词在瓦片、按钮、页签里空间很窄，长度错一点就会溢出：

`开始` `暂停` `结束` `继续` `保存` `取消` `设置` `设备` `路线` `训练` `回顾` `总结` `档位` `阈值`
`热身` `放松` `恢复` `间歇` `甜区` `冲刺` `爬坡` `平路` `自由骑` `已连接` `连接中` `扫描`

### B. 同词不同义（最容易翻错）

| 键 / 位置 | 风险 | 当前选择 |
|---|---|---|
| `szint`（HUD、欢迎页） | 源文写“海拔”，界面实际显示累计爬升 | 爬升 |
| `magasság`（剖面） | 真正的海拔高度 | 海拔 |
| `teljes`（剖面模式） | “完整剖面”，不是“满” | 全程 |
| `blokk`（训练块） | 不是文本块、也不是“组” | 区块 |
| `km hátra` / `km múlva` | 数字在前、单位在后 | `km 剩余` / `km 前方` |
| `szabad`（瓦片槽） | 没有 ERG 目标时的自由骑行 | 自由骑 |
| `kategória`（爬坡等级） | 数字后的“级” | 级 |
| `leadt` / `megtett` | 已输出（功/能量）vs 已完成（距离） | 已输出 / 已完成 |
| `előretekintés` | 剖面“前瞻”模式 | 前瞻 |
| `összeállás`（训练步骤） | Regroup，不是“组合” | 集合 |
| `tartósság`（训练标签） | Durability，与“耐力”区分 | 耐久力 |

### C. 长句与错误消息

- 备份/恢复、导入 GPX/TCX、Street View、BLE 错误：中文要 calm and actionable，不要逐字照搬匈牙利语的从句。
- 特别注意带 `{err}` / `{why}` / `{error}` 的句子：占位符前后不要多空格。
- `ui_hud.edzes_w_hatra_kesz` 的源模板本身有一个未传入的 `{current}`（应用 bug，不是翻译问题）；中文按原样保留，
  由源代码修。

### D. 训练库

- 教练提示：1–2 句、现在时、第二人称；不要每句都用同一个“加油”模板。
- 步骤名：2–5 字；`Pörgetés` = 高踏频，`Nyomaték` = 扭矩，`Fal` = 陡坡，`Lead-out` = 带冲，
  `Álló rajt` = 站姿起步。
- 训练名称里的数字/时长/区间必须与源文一致；`FTP、VO2max、Over/Under、sportive、gran fondo` 保留。

### E. 文档

- `docs/i18n/zh/README.md` 必须有 `## 许可证` 一节；许可证名称和 SPDX 标识（MIT、Apache-2.0、ODbL 1.0、
  CC BY 4.0、SIL OFL 1.1、MPL-2.0）不得改动，署名不得增删。
- README/USER_GUIDE/PRIVACY 的截图路径指向英文界面图片；不要新增图片。
- CHANGELOG 是开发者日志；版本号、日期、提交哈希、文件路径、命令不翻译。

## 输出格式

把每个问题写成一行：

```
<key 或文件:行> | 当前译文 | 建议译文 | 原因（术语/长度/歧义/标点/繁体）
```

按严重程度排序：

1. **会误解功能**（术语错、否定反了、单位错）；
2. **会溢出/截断**（短标签太长）；
3. **不自然但可懂**；
4. **风格建议**。

审校结束后：

- 术语级的问题写进 `crates/ride-i18n/GLOSSARY.tsv` 的 `zh` 列（并在 `note` 里写一句为什么）；
- 文体级的问题写进 `crates/ride-i18n/STYLE.tsv` 的 `zh` 列；
- 明确禁用的词写进 `crates/ride-i18n/tests/catalogs.rs` 的对应测试（例如骑马类词、繁体字、`您`）。

## 明确不在本环境验证的部分

- Android `values-zh` 资源、macOS `zh-Hans.lproj/InfoPlist.strings`：已写入，但**没有在设备/真机上构建或运行**。
- Linux 桌面项 `Comment[zh]`、`Dioxus.toml` 的注释：**没有在打包产物里验证**。
- 母语质量：**没有母语者审校**；本文件就是为此准备的。
