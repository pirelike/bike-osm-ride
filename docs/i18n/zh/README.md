# bike-osm-ride — 中文

> **翻译说明。** 本文档的权威版本是匈牙利语 [`README.md`](../../../README.md)。截图显示的是原界面
> （匈牙利语；程序本身也支持中文）。同一目录下还有：[用户指南](USER_GUIDE.md) · [隐私与网络流量](PRIVACY.md)。

**语言：** [匈牙利语](../../../README.md) · [英语](../en/README.md) · [西班牙语](../es/README.md) ·
[葡萄牙语](../pt/README.md) · [德语](../de/README.md) · [意大利语](../it/README.md) · 中文 —
用户指南：[中文](USER_GUIDE.md) · [匈牙利语](../../USER_GUIDE.md) · [英语](../en/USER_GUIDE.md) ·
[西班牙语](../es/USER_GUIDE.md) · [葡萄牙语](../pt/USER_GUIDE.md) · [德语](../de/USER_GUIDE.md) ·
[意大利语](../it/USER_GUIDE.md) ·
隐私：[中文](PRIVACY.md) · [匈牙利语](../../PRIVACY.md) · [英语](../en/PRIVACY.md) ·
[西班牙语](../es/PRIVACY.md) · [葡萄牙语](../pt/PRIVACY.md) · [德语](../de/PRIVACY.md) ·
[意大利语](../it/PRIVACY.md)

<p align="center">
  <img src="../../../packaging/icons/128.png" width="96" alt="bike-osm-ride 图标">
</p>

<p align="center">
  <b>在你自己的 GPX 路线上虚拟骑行，配合智能骑行台——或者不用骑行台。</b><br>
  地图、海拔剖面、训练、幽灵、TCX 导出。快速、现代、2D。Rust。
</p>

<p align="center">
  <a href="https://github.com/pirelike/bike-osm-ride/releases/latest"><img alt="最新发行版" src="https://img.shields.io/github/v/release/pirelike/bike-osm-ride?display_name=tag&sort=semver"></a>
  <img alt="许可证：MIT 或 Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue">
  <img alt="平台" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-informational">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white">
  <img alt="界面：匈牙利语、英语、西班牙语、葡萄牙语、德语、意大利语和中文" src="https://img.shields.io/badge/interface-Hungarian%20%7C%20English%20%7C%20Spanish%20%7C%20Portuguese%20%7C%20German%20%7C%20Italian%20%7C%20Chinese-brightgreen">
</p>

<p align="center">
  <img src="../../screenshots/ride-dark.png" alt="深色主题下的骑行界面：地图、功率、海拔剖面、统计" width="100%">
</p>

> **In English:** bike-osm-ride is a free, open-source (MIT OR Apache-2.0) virtual-cycling app written in Rust. Load any
> GPX file or a [bikerouter.de](https://bikerouter.de) link, ride it on a 2D map with an elevation profile while a
> Bluetooth LE smart trainer follows the hills (or a built-in simulated rider pedals), run structured ERG workouts,
> race your own ghost and export TCX. Windows, Linux, macOS and Android. **The interface speaks Hungarian, English,
> Spanish, Portuguese, German, Italian and Chinese.**
>
> [English README](../en/README.md) · [user guide](../en/USER_GUIDE.md) ·
> [privacy](../en/PRIVACY.md) · *Settings → Appearance → Language*.
>
> **En español:** la aplicación está traducida al español: [README en español](../es/README.md) ·
> [manual del usuario](../es/USER_GUIDE.md) · [privacidad](../es/PRIVACY.md) ·
> *Ajustes → Apariencia → Idioma*.
>
> **Em português:** a aplicação está traduzida para português: [README em português](../pt/README.md) ·
> [manual do utilizador](../pt/USER_GUIDE.md) · [privacidade](../pt/PRIVACY.md) ·
> *Definições → Aspeto → Idioma*.
>
> **Auf Deutsch:** Die App ist auf Deutsch übersetzt: [README auf Deutsch](../de/README.md) ·
> [Benutzerhandbuch](../de/USER_GUIDE.md) · [Datenschutz](../de/PRIVACY.md) ·
> *Einstellungen → Erscheinungsbild → Sprache*.

**跳转：** [下载](#下载) · [快速开始](#快速开始) · [功能](#功能) · [截图](#截图) ·
[设备与状态](#设备与状态) · [文档](#文档) · [开发](#开发) · [许可证](#许可证)

## 为什么选它？

- **你的路线。** 不是预先造好的世界：任何 GPX 文件或 bikerouter 链接都能骑，有真实的海拔和爬坡，骑行台会跟随路线的坡度。
- **所有重要的数据。** 功率（3 秒平均、W/kg、区间）、踏频、心率、速度、坡度、NP/IF/TSS、累计爬升、预计到达时间：
  大而清晰的数字，没有 3D。
- **训练与分析。** 93 个内置训练，附来源，ERG 模式，带教练提示；幽灵和带 PR 的本地赛段；TCX 导出。
- **始终属于你。** 没有账户，没有遥测，数据留在你的电脑上（[隐私](../../PRIVACY.md)）。开源，宽松许可证。
- **没有硬件也能用。** 用模拟骑手，不用蓝牙也能把一切试一遍。

## 下载

最新发行版：**[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**。
文件名中的 `<版本>` 是发行版号（例如 `0.3.0`）。

| 平台 | 文件 | 安装 |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<版本>-windows-x64.exe` | 没有安装程序：直接运行。需要 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 自带）。程序没有数字签名，SmartScreen 可能会拦截。 |
| **Linux**（x86-64，glibc 2.35+） | `bike-osm-ride-<版本>-linux-x86_64.AppImage` | `chmod +x` 后运行；自带 WebKitGTK 和 GTK |
| | `bike-osm-ride-<版本>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<版本>-linux-amd64.deb`（会要求 `libwebkit2gtk-4.1-0`、`libgtk-3-0`、`bluez` 包） |
| **macOS 11+**（Intel 和 Apple Silicon） | `bike-osm-ride-<版本>-macos-universal.dmg` | 拖到*应用程序*里；首次启动时**右键 → 打开 → 打开**（未经公证）；首次搜索蓝牙时允许蓝牙 |
| **Android 7+** | `bike-osm-ride-<版本>-android.apk` | 复制到手机并打开（需要允许从未知来源安装），或者 `adb install -r` |

程序**会自己提示**有新版本：顶栏出现*更新*按钮（Windows 上和作为 AppImage 时它会自我替换，其他平台则打开发行版页面）。
Android 包用调试（debug）密钥签名，所以两个发行版之间可能需要先卸载旧版。

## 快速开始

1. **启动。** 会打开*路线*窗口：把 GPX 拖进去，粘贴 bikerouter 链接，或者选择*演示路线*。
2. **连接骑行台**（*设备* → *开始搜索* → 在*骑行台*角色下），或者在手边没有硬件时选择**模拟骑手**。心率监测器、
   功率计、踏频传感器和 Zwift Click/Play 也可以连接。
3. *（可选）***训练**：顶栏的*训练*按钮（`W`），可从 93 个训练中选择，也可以导入自己的训练。
4. **`开始`**（`空格`）。骑行台跟随爬坡（或保持训练的目标瓦数），数字开始跳动。
5. 结束时从**总结**页保存 TCX（Strava、Intervals.icu 和 TrainingPeaks 都接受）。

完整说明：**[用户手册](USER_GUIDE.md)**。

## 功能

**路线**
- GPX 文件（浏览、拖放、`Ctrl+O`）、bikerouter/BRouter 链接（路线方案、备选方案和避让选项也会一并带过来）、
  最近路线、内置演示路线。
- 容错的 GPX 读取器（BOM、命名空间、多个轨迹段、`<rte>`，文件损坏时有能看懂的提示）；缺失的海拔由 Open-Meteo
  补齐。爬坡识别从 4 级到 HC；写进文件里的赛段轨迹（例如 Strava 赛段）会以条带显示。

**骑行**
- 真实的物理模型（Martin 等，1998：重力、滚动阻力、轴承摩擦、辐条车轮风阻、惯性），骑行台会收到你前方路段的
  坡度（最大 ±20%，强度 0–150%）。
- **FTMS 骑行台**（爬坡和 ERG）、**功率计**（CPS）、**速度/踏频**（CSC）、**心率监测器**（HR），一个物理设备
  可以在一个连接上承担*多个角色*；**Zwift Click/Play** 虚拟变速；**模拟骑手**。
- **虚拟变速**：2 × 12 传动，23 档，改变骑行台的阻力。

**训练与分析**
- 12 个类别共 93 个内置训练，带标签、按区间着色的区块图和 943 条带时间点的教练提示；可导入 MyWhoosh 和
  Zwift 的训练（`.zwo`、XML、JSON）。
- Coggan 区间、NP、IF、TSS、kcal；心率区间；每秒记录。
- **幽灵**（最近一次、最好的一次，或 GPX 自带的时间戳）和**带 PR 的本地赛段**。
- **骑行回顾与比较**：同一路线上的骑行（最多四次）并排比较功率、心率、踏频、速度和时差；放大到某个路段
  （拖动或从爬坡列表选择），路段表格。从 Strava、Garmin 导出文件**导入 GPX 或 TCX 骑行**，含心率和踏频；
  如果文件里没有瓦数，就用模拟的物理模型反推（用你自己的或指定的体重、自行车和滚动阻力）。
- **功率页面**：5 秒到 60 分钟的峰值功率，根据最好的 20 分钟给出 FTP 建议（一键设置），每周和每月负荷。
  只计入实测瓦数。
- **备份**：路线、骑行、导入的训练和设置放在一个 zip 文件里（不含 Google 密钥），恢复时合并。不会有任何
  东西发到网络。
- **TCX 导出**（Garmin 模式）：Strava、Intervals.icu 和 TrainingPeaks 手动上传都能接受。没有内置上传：
  Strava 登录本来在每次骑行后都得重来，所以我们把它去掉了（0.3.2）。

**界面**
- **平铺布局**：面板像在平铺式窗口管理器里一样工作（不重叠、不留缝）；可以拖动调整大小、移动、关闭；有训练
  和没有训练时各有一套独立布局。
- 深色和浅色主题（两者都完整可用），文字大小 80–160%，**匈牙利语、英语和西班牙语界面**
  （*设置 → 外观 → 语言*，或自动跟随系统语言），内嵌字体。
- **手机和平板**：标签页视图，“骑行”标签页里有骑行时所需的一切。
- 可选的 **Street View**（用自己的 Google 密钥，离线下载）和一个**浏览器面板**（Windows，内嵌已安装的 Firefox）。
- 骑行时让电脑保持唤醒（Windows、Linux、macOS）。

## 截图

<table>
  <tr>
    <td width="50%"><img src="../../screenshots/ride-light.png" alt="浅色主题"><br><sub><b>浅色主题</b>：纸白主题与深色主题同等地位</sub></td>
    <td width="50%"><img src="../../screenshots/workout.png" alt="ERG 模式下的训练"><br><sub><b>ERG 模式下的训练</b>：训练卡片、实时图表、区块图</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/workout-picker.png" alt="训练选择器"><br><sub><b>93 个训练</b>，带筛选和按区间着色的区块图</sub></td>
    <td><img src="../../screenshots/summary.png" alt="骑行总结"><br><sub><b>总结</b>：NP、IF、TSS、赛段、区间；TCX 保存</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/devices.png" alt="设备窗口"><br><sub><b>设备</b>：一个设备承担多个角色</sub></td>
    <td align="center">
      <img src="../../screenshots/phone-ride.png" width="170" alt="手机，骑行标签页">
      <img src="../../screenshots/phone-map.png" width="170" alt="手机，地图标签页"><br>
      <sub><b>手机上</b>：骑行和地图标签页</sub>
    </td>
  </tr>
  <tr>
    <td><img src="../../screenshots/review.png" alt="骑行回顾"><br><sub><b>回顾</b>：同一段爬坡上的两次骑行，第二次是估算瓦数</sub></td>
    <td align="center">
      <img src="../../screenshots/review-phone.png" width="170" alt="手机，回顾页面">
      <br><sub><b>手机上</b>：骑行、地图、图表、路段标签页</sub>
    </td>
  </tr>
</table>

<sub>截图显示的是一条真实的 BRouter 路线（仓库中作为测试数据的 Hűvösvölgy–Dobogókő 路线的最后 12 km），使用模拟
骑手。路线和地图：© OpenStreetMap 贡献者，ODbL。图片由 `node scripts/readme_shots.mjs` 生成。</sub>

## 设备与状态

**不是每一项都用真实设备或真实服务试过。** 说实话：

| 项目 | 状态 |
|---|---|
| FTMS 智能骑行台，**ERG**（训练） | 已测试，**也用真实骑行台测试过**（一个型号：VE200） |
| FTMS **坡度模拟** | 充分测试过（用模拟传输，按规范的字节向量）；**尚未用真实骑行台确认** |
| 心率监测器、独立功率计、CSC 传感器 | 解码器已测试；骑行台的 CPS/CSC 数据是真实的；尚未用独立传感器试过 |
| Zwift Click / Play | 已实现（非官方），尚未用真实控制器试过 |
| 模拟骑手 | 没有硬件也能用，所有平台都可以 |
| Windows | 日常使用 |
| Linux、macOS | CI 会构建，（在 Linux 上）跑 20 秒的冒烟测试；还没有在真实机器上验证过（欢迎提供经验） |
| Android | 界面跑过（0.3.0 的 bug 就是运行时发现的）；系统文件选择器已实现；**蓝牙尚未在设备上试过** |
| Street View | 已实现，并用本地测试服务器测试过；尚未用真实服务试过 |

细节：[`docs/TESTING.md`](../../TESTING.md) 和 [`docs/BLUETOOTH.md`](../../BLUETOOTH.md)。欢迎在 Issues 里报告
bug、分享经验、提供兼容或不兼容的设备（附上 `ble.log` 会很有帮助）。

## 文档

| 读者 | 内容 |
|---|---|
| **用户** | [用户手册](USER_GUIDE.md) · [隐私与网络流量](PRIVACY.md) · [变更日志](../../../CHANGELOG.md) · [English](../en/README.md) · [Español](../es/README.md) · [Português](../pt/README.md) · [Deutsch](../de/README.md) |
| **开发者** | [架构](../../ARCHITECTURE.md) · [开发指南](../../DEVELOPMENT.md) · [测试](../../TESTING.md) · [发行](../../RELEASING.md) · [决策记录](../../DECISIONS.md) · [翻译（i18n）](../../I18N.md) |
| **专题** | [蓝牙](../../BLUETOOTH.md) · [模拟与物理](../../SIMULATION.md) · [路线与数据格式](../../ROUTES_AND_DATA.md) · [界面](../../UI.md) · [设计系统](../../DESIGN.md) · [Android](../../ANDROID.md) · [Street View](../../STREETVIEW.md) · [前作与复用代码](../../PRIOR_ART.md) |

完整目录：[`docs/README.md`](../../README.md)。

## 开发

```powershell
cargo run -p ride-app                      # debug 构建并启动（Windows、Linux、macOS）
cargo test --workspace                     # 完整测试集（2026-11-06：1076 个测试）
node scripts/readme_shots.mjs              # 重新生成截图
```

Rust 工作区，含十个库 crate 和应用（`ride-app`）：

```
crates/
  ride-model         共享数据模型（Route、Settings、RideSnapshot、Workout、…）——“契约”
  ride-route         GPX/bikerouter → Route、爬坡、持久化、骑行日志
  ride-sim           物理、RideEngine、训练执行器、幽灵、赛段、TCX（无 I/O 和时钟）
  ride-ble           BLE 中枢（FTMS/CPS/CSC/HR/Zwift）+ 模拟骑手
  ride-ui-kit        设计系统：token、调色板、图标、格式化
  ride-ui-map        瓦片地图 · ride-ui-profile  剖面与图表
  ride-ui-hud        仪表、卡片、控件       · ride-ui-dialogs  对话框
  ride-ui-streetview Street View 面板
  ride-app           外壳：状态、控制器、平铺、瓦片代理、更新器、Android
packaging/  scripts/  tests/fixtures/  docs/
```

代码基于 Dioxus 0.7（webview 界面）和 `btleplug`。Windows 上开发在 GNU 工具链上进行（发行版 exe 由 CI 用
MSVC 构建）。前置条件、开关（`RIDE_*`）、调试、打包：[`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md)。面向
Claude Code 的项目说明：[`CLAUDE.md`](../../../CLAUDE.md)。

**贡献。** 我们欢迎 bug 报告、实测日志和补丁。规则简述：只能移植宽松许可证的代码（见下），用户可见文本的
源语言是匈牙利语（界面的每个字符串都是 `crates/ride-i18n/locales/hu.json` 里的键，翻译放在旁边；见
[`docs/I18N.md`](../../I18N.md)），不对来自外部的数据 panic，新功能要附带测试和文档。提交的代码按项目许可证
（MIT 或 Apache-2.0）纳入，除非你另行说明。

## 许可证

### 项目自身的代码

`bike-osm-ride` 自身的代码**双许可证：[MIT](../../../LICENSE-MIT) 或 [Apache-2.0](../../../LICENSE-APACHE)**，
可任选其一（SPDX：`MIT OR Apache-2.0`）。如果你贡献代码，提交的代码也按这两个许可证纳入，除非你另行说明。

### 使用的开源代码

我们刻意只使用宽松（permissive）许可证的代码。来源和许可证同时保存在源文件和
[`NOTICE.md`](../../../NOTICE.md) 中。

| 来源 | 许可证 | 用途 |
|---|---|---|
| [`ftms`](https://github.com/deancochran/ftms) crate | MIT | 编解码蓝牙 FTMS（智能骑行台）数据和控制命令 |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | 解析功率计、踏频和心率传感器数据（已内置，源文件中有标注） |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | 骑行台连接和命令安全过滤器（移植的部分，已标注） |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | 心率监测器的搜索与重连（只作范例，未取代码，已标注） |
| [Barlow 和 Barlow Condensed](https://github.com/jpt/barlow) 字体 | SIL OFL 1.1 | 界面文字和数字；**内嵌**在程序中（子集化 WOFF2，latin + latin-ext，含匈牙利语 ő/ű），因此没有网络也能用；许可证文本：[`crates/ride-ui-kit/assets/fonts/OFL.txt`](../../../crates/ride-ui-kit/assets/fonts/OFL.txt)，细节见 [`NOTICE.md`](../../../NOTICE.md) |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense（公有领域） | Zwift Click/Play 和 Zwift Hub 的 BLE 协议（改写成 Rust）：`ride-ble` 的 `zwift` 模块，细节见 [`NOTICE.md`](../../../NOTICE.md) |
| [`p256`](https://github.com/RustCrypto/elliptic-curves)、[`hkdf`](https://github.com/RustCrypto/KDFs)、[`sha2`](https://github.com/RustCrypto/hashes)、[`aes`](https://github.com/RustCrypto/block-ciphers)、[`ccm`](https://github.com/RustCrypto/AEADs)（RustCrypto） | MIT / Apache-2.0 | Zwift Click 的密钥交换（ECDH P-256、HKDF-SHA256）和 AES-CCM 加密 |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0（程序**不包含**它，只启动机器上已安装的那份） | 浏览器面板：内嵌已安装的 Firefox 窗口，使用独立配置文件 |
| [Dioxus](https://dioxuslabs.com)、[btleplug](https://github.com/deviceplug/btleplug)、[tokio](https://tokio.rs)、[quick-xml](https://github.com/tafia/quick-xml) 及其他 Rust 依赖 | 主要是 MIT / Apache-2.0 | 完整的自动生成列表（638 个包）：[`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md) |

仅作**参考**（未取用代码）：开源 Martin 实现 `rouleur` 的一个算例数值用于物理测试，
[`nrenner/brouter-web`](https://github.com/nrenner/brouter-web)（MIT）用于核对 bikerouter 链接格式，以及
Makinolo 关于 Zwift Click/Play 协议的逆向笔记作为文档：
<https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>。这是非官方的互操作性工作：我们与 Zwift
没有任何关系，本程序不是 Zwift 的产品，Zwift 也不支持它。

我们**没有**使用 GPL/AGPL 许可证项目（例如 OpenCycling、suffice、GoldenCheetah、QDomyos-Zwift）的代码，
因为那会让整个程序落入 copyleft 的约束；研究细节：[`docs/PRIOR_ART.md`](../../PRIOR_ART.md)。

**许可证审计（2026-11-04，`cargo metadata --filter-platform`，普通依赖和构建依赖）。** 各平台的外部
（crates.io）crate 数量：Windows（GNU）396，Linux 458，macOS（Apple Silicon）400，Android（aarch64）449。
其中**没有 GPL、AGPL 或 LGPL，也没有未知许可证**；宽松许可证包括 MIT、Apache-2.0、BSD、ISC、Zlib、
Unicode、CC0、Unlicense 等。

**唯一的 copyleft 类型是 MPL-2.0**，它是**文件级**的，这些 crate 我们从 crates.io 原样取用，没有改动
（不是 vendor 的代码），其源码可以在 [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md) 给出的
仓库里找到：

- Linux 和 macOS 上是 `option-ext`（经 `dioxus-desktop → tray-icon → dirs` 链）；
- Android 上是 `selectors`、`cssparser`、`cssparser-macros` 和 `dtoa-short`（经 `wry → kuchikiki` 链）；
- Windows 上都没有。

这不影响项目自身的许可证：我们自己的代码仍然是 `MIT OR Apache-2.0`。`Cargo.lock` 包含所有平台的依赖；
重新生成列表：`.\scripts\third_party_licenses.ps1`，各平台的审计命令在
[`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md) 第 9 点。

### 地图与数据来源（署名）

程序运行时调用外部服务；这些数据按其自身条款使用，程序在地图上显示规定的署名（调用的完整列表：
[`docs/PRIVACY.md`](../../PRIVACY.md)）：

- **地图数据：** © [OpenStreetMap](https://www.openstreetmap.org/copyright) 贡献者，
  [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/)。
- **瓦片：** 标准的 [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles) 瓦片
  （`tile.openstreetmap.org`）。*深色*和*浅色*地图样式在本地对同一批瓦片做过滤（CSS 过滤），因此不需要
  API 密钥，也不需要第三方的样式服务器。下载的瓦片缓存在本地，以减轻服务器负担
  （[OSM 瓦片使用政策](https://operations.osmfoundation.org/policies/tiles/)）；没有批量预取。
- **Street View（可选）：** [Google Maps Platform](https://mapsplatform.google.com) 的服务：Street View
  Static API 的*元数据*端点（只有 JSON：状态、全景 ID、位置）、Static API 的图像端点（仅在你请求离线下载时）
  以及 Maps Embed API 的 Street View 视图。程序使用**你自己的** API 密钥（你的 Google Cloud 账户、你的配额
  和账单），密钥只在你机器的 `settings.json` 里，从不写入日志，只会发往 `maps.googleapis.com` /
  `www.google.com`。使用受 [Google Maps Platform 条款](https://cloud.google.com/maps-platform/terms)和
  [Street View 政策](https://developers.google.com/maps/documentation/streetview/policies)约束。程序
  **默认不下载图片**：只有你在面板上按下*下载*按钮，才会把路线的图片保存到你自己的电脑上，最多 30 天；
  这些图片只在没有密钥或网络出错时显示，其他时候显示的是实时的全分辨率视图。Street View 图片的长期保存受
  Google 条款限制，所以这是你的决定和责任；下载会消耗你自己的 Google 账户配额。政策允许保存*全景 ID*
  （它们会随时间变化，所以旁边还保存坐标和时间戳，内嵌视图在 ID 之外也会拿到坐标）。本程序不是 Google 的
  产品，与 Google 没有关联。
- **训练说明：** 内置训练库是根据公开的训练协议（British Cycling、TrainingPeaks、CTS、FasCat、TrainerRoad
  等）和训练科学研究（Rønnestad、Seiler、Helgerud、Billat、Gibala、Bossi、Karsten 等）编写的自有文本；
  每个训练用到的来源列在 `crates/ride-model/assets/workouts.json` 的 `sources` 字段里。
- **路线规划：** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de)（公共服务；只有你
  粘贴 bikerouter 链接时才会调用，路线由他们的服务器计算）。`tests/fixtures/brouter_*.gpx` 是 BRouter 的
  响应，来自 OSM 数据（ODbL），README 截图里也是这条路线。
- **缺失海拔数据的补齐（可选）：** [Open-Meteo](https://open-meteo.com) Elevation API，
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)，非商业使用。

我们自己的许可证不涵盖这些数据和服务。

### 法律声明

本软件“按原样”（as is）提供，不附带任何保证；细节见 [LICENSE-MIT](../../../LICENSE-MIT) 和
[LICENSE-APACHE](../../../LICENSE-APACHE) 文件。设备控制（自动调整骑行台的阻力）由你自行负责。
