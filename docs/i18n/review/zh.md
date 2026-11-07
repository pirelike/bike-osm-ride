# zh (Simplified Chinese) review flags: 1061 rows read, 45 flagged (no placeholder or spacing problems found; `tight` column is empty in every row, so length was checked against the brief's list only)

| # | key | hu | current | category | problem | suggestion | confidence |
|---|---|---|---|---|---|---|---|
| 311 | model.workout.lepes_osszeg_perc | {total} lépés-összeg != {minutes} perc | {total} 步数总和 != {minutes} 分钟 | meaning | "步数总和" reads as a count of steps, but the number is the summed step duration being compared with minutes. | {total} 步骤时长合计 ≠ {minutes} 分钟 | medium |
| 467 | ui_dialogs.about.az_alkalmazas_mukodes_kozben_ezeket_a_szolgaltatasok | ...az adatok a saját feltételeik szerint használhatók... | ...这些数据只能按各自的条款使用... | meaning | "只能" (only) adds a restriction the source does not state; the source says the data may be used under the services' own terms. | 这些数据可按其各自的条款使用 | low |
| 823 | ui_kit.format.h_m_02_o | {h}:{m} ó | {h}:{m} | meaning | The unit "ó" (hours) is dropped, so the duration shows as a bare clock time. | {h}:{m} 时 | medium |
| 1003 | ui_streetview.error.a_cim_nem_a_google_e_ezert_a_kerest_nem_kuldtuk_el | A cím nem a Google-é, ezért a kérést nem küldtük el. | 这个来源不属于 Google，所以没有发送请求。 | meaning | "cím" is the requested address; "来源" (origin) is a web-security term and reads differently. | 该地址不属于 Google，所以没有发送请求。 | low |
| 59 | app.controller.importalva_blokk_perc | „{name}” importálva: {blocks} blokk, {minutes} perc. | “{name}”已导入：{blocks} 段，{minutes} 分钟。 | terminology | blokk is 区块 in the glossary and the brief; here it becomes 段, which is used nowhere else for it. | “{name}”已导入：{blocks} 个区块，{minutes} 分钟。 | high |
| 104 | app.dock.szimulalt_kerekparos | Szimulált kerékpáros | 模拟骑手 | terminology | Glossary: rider = 骑行者; 骑手 is a different word (often a delivery or horse rider). | 模拟骑行者 | medium |
| 124 | app.shell.edzes_elore | Edzés · előre | 训练 · 前方 | terminology | "előre" is the look-ahead mode; settings and profile use 前瞻 for it (brief: előretekintés = 前瞻), here it is 前方. | 训练 · 前瞻 | medium |
| 149 | app.shell.szint_elore | Szint · előre | 海拔 · 前方 | terminology | Same look-ahead mode; 前方 here, 前瞻 in settings and profile. | 海拔 · 前瞻 | medium |
| 163 | app.state.pulzusmero | Pulzusmérő | 心率传感器 | terminology | Glossary maps pulzusmérő (heart rate monitor) to 心率带; the catalog uses 心率传感器 here and in other keys. | Pick one term: 心率带 (glossary) or change the glossary to 心率传感器, then apply everywhere | medium |
| 178 | ble.btle.a_bluetooth_kezelo_inditasa | a Bluetooth-kezelő indítása | 启动 Bluetooth 管理器 | terminology | English "Bluetooth" stays here while the rest of the catalog says 蓝牙 (e.g. 蓝牙服务不可用). | 启动蓝牙管理器 | low |
| 180 | ble.btle.nincs_bluetooth_adapter_a_gepen | nincs Bluetooth-adapter a gépen | 此设备上没有 Bluetooth 适配器 | terminology | Same mix; ble.error.nincs_bluetooth_adapter uses 没有蓝牙适配器. | 此设备上没有蓝牙适配器 | low |
| 207 | ble.hub.e_a_szimulalt_tekero_bluetooth_nelkul_is_hasznalhato | {e}. A szimulált tekerő Bluetooth nélkül is használható. | {e}。模拟骑手在没有蓝牙的情况下也能使用。 | terminology | tekerő (rider) is 骑行者 in the glossary; 模拟骑手 is a different word. | {e}。模拟骑行者在没有蓝牙的情况下也能使用。 | medium |
| 208 | ble.hub.szimulalt_tekero | Szimulált tekerő | 模拟骑手 | terminology | Same: simulated rider should follow the glossary rider term. | 模拟骑行者 | medium |
| 214 | ble.link.pulzusmero | Pulzusmérő | 心率传感器 | terminology | Same glossary conflict as #163 (心率带 in the glossary). | Same decision as #163 | medium |
| 272 | model.workout.alapozas | Alapozás | 基础 | terminology | Glossary: alapozás = 基础期 (the base period); the label shortens it to 基础. | 基础期 (if the tab width allows) | low |
| 279 | model.workout.egyenletes | Egyenletes | 稳定 | terminology | Glossary: steady = 平稳; the label uses 稳定. | 平稳 | low |
| 289 | model.workout.haladoknak | Haladóknak | 高级 | terminology | Glossary: haladóknak (advanced) = 进阶者; 高级 differs. | 进阶者 (or change the glossary if 高级 is the chosen tag) | medium |
| 291 | model.workout.hegymenet | Hegymenet | 上坡 | terminology | Glossary: hegymenet (the uphill tag) = 爬坡; here 上坡. | 爬坡 (or change the glossary) | low |
| 301 | model.workout.kezdoknek | Kezdőknek | 初级 | terminology | Glossary: kezdőknek (beginners) = 初学者; 初级 differs. | 初学者 (same decision as #289) | medium |
| 316 | model.workout.munka | Munka | 训练段 | terminology | Glossary: munka (work) = 做功; the step-kind label uses 训练段. | 做功, or change the glossary note for this label | low |
| 320 | model.workout.osszeallas | Összeállás | 重聚 | terminology | The brief fixes this step term as Regroup = 集合; 重聚 follows the pivot "Combined", which is not the cycling meaning. | 集合 | medium |
| 330 | model.workout.szabad | Szabad | 自由 | terminology | The free-ride tile uses 自由骑 (brief); this label says 自由. | 自由骑 | low |
| 446 | sim.workout_import.a_fajlban_nincs_felismerheto_edzesblokk_steadystate_ | A fájlban nincs felismerhető edzésblokk (SteadyState / Ramp / IntervalsT). | 文件里没有可识别的训练块（SteadyState / Ramp / IntervalsT）。 | terminology | "训练块" is not the glossary term for blokk (区块); other keys say 训练区块 or 区块. | 文件里没有可识别的训练区块（SteadyState / Ramp / IntervalsT）。 | medium |
| 447 | sim.workout_import.a_json_nem_tartalmaz_edzesblokkot_workoutstepsarray_ | A JSON nem tartalmaz edzésblokkot (WorkoutStepsArray üres). | JSON 里没有训练块（WorkoutStepsArray 为空）。 | terminology | Same blokk term issue: 训练块 instead of 区块. | JSON 里没有训练区块（WorkoutStepsArray 为空）。 | medium |
| 449 | sim.workout_import.az_edzes_nem_tartalmaz_hasznalhato_blokkot | Az edzés nem tartalmaz használható blokkot. | 训练里没有可用的块。 | terminology | Bare "块" for blokk; the glossary term is 区块. | 训练里没有可用的区块。 | medium |
| 495 | ui_dialogs.about.virtualis_tekeres_gpx_utvonalakon_okos_trainerrel_va | ...szimulált kerékpárossal... | ...或模拟骑手进行虚拟骑行... | terminology | Same simulated-rider term issue (rider = 骑行者). | ...或模拟骑行者进行虚拟骑行... | medium |
| 507 | ui_dialogs.devices.hardver_nelkul_a_szimulalt_kerekparossal_is_kiprobal | Hardver nélkül a szimulált kerékpárossal is kipróbálhatod. | 没有硬件时，也可以用模拟骑手试一试。 | terminology | Same simulated-rider term issue. | 没有硬件时，也可以用模拟骑行者试一试。 | medium |
| 545 | ui_dialogs.helpers.pulzusmero_ble | Pulzusmérő – BLE | 心率传感器 – BLE | terminology | Same glossary conflict as #163. | Same decision as #163 | medium |
| 554 | ui_dialogs.helpers.szabad | Szabad | 自由 | terminology | The brief fixes the tile slot szabad as 自由骑; the glossary says 自由骑行; this preset says 自由. | 自由骑 | low |
| 672 | ui_dialogs.virtual_rider.ennyi_wattot_teker_a_szimulalt_kerekparos | Ennyi wattot teker a szimulált kerékpáros. | 模拟骑手以这个功率踩踏。 | terminology | Same simulated-rider term issue. | 模拟骑行者以这个功率踩踏。 | medium |
| 673 | ui_dialogs.virtual_rider.szimulalt_kerekparos | Szimulált kerékpáros | 模拟骑手 | terminology | Same simulated-rider term issue (dialog heading). | 模拟骑行者 | medium |
| 792 | ui_hud.megtett_tav | Megtett táv | 已骑距离 | terminology | Glossary and brief: megtett (distance done) = 已完成; the "Megtett" group tile uses 已完成, this row does not. | 已完成距离 | low |
| 980 | ui_review.power.meg_nincs_mert_teljesitmenyu_edzes_a_szimulalt_teker | ...A szimulált tekerő és a becsült (~) wattok nem számítanak. | ...模拟骑手和估算（~）的功率都不计入。 | terminology | Same simulated-rider term issue. | ...模拟骑行者和估算（~）的功率都不计入。 | medium |
| 147 | app.shell.speed_ellenallas_offset_1 | {speed} · ellenállás {offset} % | {speed} · 阻力 {offset} % | style | STYLE percent rule: no space before % (55% FTP). | {speed} · 阻力 {offset}% | medium |
| 203 | ble.error.seconds_short | {s} mp | {s} s | style | English unit "s" where the rest of the catalog uses 秒 (e.g. "{n} 秒"). | {s} 秒 | low |
| 450 | sim.workout_import.az_edzes_tul_rovid_total_s_legalabb_min_total_s_s_ke | Az edzés túl rövid ({total} s); legalább {min} s kell. | 训练太短（{total} s）；至少需要 {min} s。 | style | Same English "s" unit instead of 秒. | 训练太短（{total} 秒）；至少需要 {min} 秒。 | low |
| 739 | ui_hud.climb_summary_ahead | {title}, {cat_label}: {hero} {hero_unit}, ... {grade} % átlagos meredekség ... | ...平均坡度 {grade} %，爬升 +{gain} m | style | Space before % (style rule: no space). | 平均坡度 {grade}% | medium |
| 740 | ui_hud.climb_summary_inside | ... {grade} % átlagos meredekség ... {pct} % kész | ...平均坡度 {grade} %，...已完成 {pct} % | style | Space before % in two places (style rule: no space). | 平均坡度 {grade}%，...已完成 {pct}% | medium |
| 824 | ui_kit.format.minutes_short | {m} p | {m} min | style | English "min" in a Chinese UI; other minute labels use 分钟 / 分 (e.g. "{minutes} 分钟"). | {m} 分 | low |
| 662 | ui_dialogs.summary.szellemhez_kepest | Szellemhez képest | 相对幽灵 | unnatural | Literal calque; the sense is "compared with the ghost". | 与幽灵对比 | medium |
| 304 | model.workout.kulcsedzes | Kulcsedzés | 关键课 | unnatural | "课" (a class or lesson) is an odd name for a workout type. | 关键训练 | medium |
| 101 | app.dock.kovetkezo_emelkedo | Következő emelkedő | 下一段爬坡 | length | Dock tile title is 5 characters; the brief asks for 2–4 in tile labels. | 下个爬坡 | low |
| 755 | ui_hud.dash.nincs_szenzoradat | Nincs szenzoradat | 无传感器数据 | length | 6-character tile message; the other dash tiles are 2–3 characters (e.g. 无功率). | 无数据 | low |
| 261 | i18n.test.items | {n} elem | (empty) | untranslated/empty | en and zh cells are empty. Looks like a test fixture; check whether that is intended. | Fill in if the UI uses it, otherwise leave | low |
| 262 | i18n.test.only_hu | Csak magyar | (empty) | untranslated/empty | en and zh cells are empty. Looks like a test fixture. | Same as #261 | low |

## Patterns

- **blokk**: most keys use 区块 as the brief asks, but #59 uses 段 and #446, #447, #449 use 训练块 or a bare 块. Grep the catalog for 块 and 段 and align them to 区块.
- **Simulated rider**: 模拟骑手 appears in nine rows (#104, #207, #208, #495, #507, #672, #673, #980 and the same phrase elsewhere) while the glossary and the settings use 骑行者 for rider. Rename to 模拟骑行者 in one pass.
- **Glossary vs catalog**: several terms disagree. szint is 海拔 in the glossary, but HUD, welcome and profile show 爬升 (the brief allows this, so the glossary row needs a note). pulzusmérő is 心率带 in the glossary but 心率传感器 in the UI. kezdőknek/haladóknak are 初学者/进阶者 in the glossary but 初级/高级 in the UI. szabad is 自由骑行 in the glossary, 自由骑 in the brief, and 自由 in several places. Also munka, egyenletes, alapozás and hegymenet. Decide each term and sync the glossary and catalog.
- **Units and spacing**: STYLE says no space before % (55% FTP), but #147, #739 and #740 have one. English "min" and "s" appear in #824, #203 and #450 where the rest of the catalog uses 分钟 and 秒.
- **Checks with no findings**: no placeholder mismatch in any of the 1061 rows; no lost leading or trailing space (the ban_es_kapcsold… leading space is the allowed exception and is present); no 您, no 骑马/马术/赛马, no Traditional characters found; ui_hud.edzes_w_hatra_kesz keeps its unpassed {current} as the brief says. The i18n.test.* keys are the only empty targets.
