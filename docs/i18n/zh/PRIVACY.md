# 隐私与网络流量

> **其他语言：** [Magyar](../../PRIVACY.md) · [English](../en/PRIVACY.md) · [Español](../es/PRIVACY.md) ·
> [Português](../pt/PRIVACY.md) · [Deutsch](../de/PRIVACY.md) · [Italiano](../it/PRIVACY.md) —
> 本文档是匈牙利语原文的中文译本。

bike-osm-ride **没有账户、没有遥测、没有分析工具，也没有广告**。你的数据（骑行、设置、传感器数据）留在这台电脑上；
程序只会在你发起请求时访问网络。本页准确列出什么会离开你的电脑，以及电脑上保存了什么。这些说明基于源代码
（2026-11-04，0.3.0 版）。

## 什么会离开你的电脑，什么时候

| 地址 | 何时 | 发送什么 | 备注 |
|---|---|---|---|
| `tile.openstreetmap.org` | 当地图请求瓦片时 | 瓦片坐标（zoom、x、y）以及 User-Agent 中的程序名（`bike-osm-ride/0.1 (desktop app)`） | 地图瓦片在 30 天内有效；过期的离线副本可保留最长 90 天，总计最多 512 MiB。启动时和使用期间每小时清理一次。 |
| `brouter.de` | 当你粘贴 bikerouter 链接时 | 从链接解析出的点（经度、纬度）、配置文件和避让选项 | 路线由他们的服务器计算 |
| `api.open-meteo.com` | 仅当 GPX 中没有可用的海拔数据时 | 路线采样点的坐标（每次请求最多 100 个） | CC BY 4.0，非商业使用 |
| `api.github.com` | 启动五秒后 | 查询最新版本，`User-Agent: bike-osm-ride/<版本>` | 失败时静默；**设置中无法关闭** |
| `github.com`（下载） | 仅当你按下*更新*按钮时 | 下载新安装包 | |
| `maps.googleapis.com`、`www.google.com` | 仅当你开启 Street View 并提供了 API 密钥时 | 骑行者的位置（坐标）和**你自己的** Google 密钥 | 受 Google 条款约束；见 [`STREETVIEW.md`](../../STREETVIEW.md) |
| 在 Firefox 中打开的页面 | 仅当你使用浏览器面板时（Windows） | 你在其中查看的内容 | 独立的 Firefox 配置文件，不会动你日常使用的浏览器 |

没有别的了。没有登录、没有崩溃上报、没有使用统计。蓝牙流量只在你、骑行台和传感器之间进行。

## 你的电脑上有什么

一切都在一个文件夹里（`%LOCALAPPDATA%\bike-osm-ride`、`~/Library/Application Support/bike-osm-ride`、
`$XDG_DATA_HOME/bike-osm-ride`，Android 上是应用的私有目录）。结构见
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md)；简而言之：

- **设置**（`settings.json`）：体重、FTP、最大心率、外观、布局、你的 Google API 密钥；
- **路线和骑行日志**：已打开路线的处理版本，每次完成骑行的逐秒完整记录（位置、瓦数、心率、踏频）；
- **瓦片**、下载的 **Street View 图片**（最多 30 天）和全景 ID；
- **日志**：`ble.log`（骑行台和传感器的原始数据包：瓦数、心率、控制命令；不含设备名或地址）和
  `crash.log`（捕获的错误）；两者都会轮转，留在电脑上；
- 浏览器面板的 Firefox 配置文件；
- **0.3.1 及更早版本** Strava 上传的残留：`strava` 和 `strava-webview` 文件夹（后者包含你的 Strava 登录
  cookie）。程序已不再使用也不再删除它们：可以放心删掉。

**备份。** *设置 → 备份* 会生成一个 zip 文件，包含你的路线、骑行、导入的训练和设置。文件保存到你选择的位置
（Android 上通过系统的“另存为”对话框），程序不会发送或上传任何东西。Google API 密钥**不在其中**，恢复时也不会
接受文件里的密钥；瓦片、Street View 图片和日志也不在其中。文件包含你骑行的精确路线和健康数据（心率、瓦数），
请像其他个人文件一样对待。

**机密。** 唯一的机密是 Google API 密钥。它在 `settings.json` 中以*明文*保存（未加密），请像保护其他个人
文件一样保护该文件夹。程序从不记录它，不会写入错误消息，不会出现在调试输出中，只会发往 Google
（开发时或发往本机 `127.0.0.1` 测试服务器）。

**删除。** 删除该文件夹即可清除一切。Street View 图片和 ID 也可以在 *设置 → Street View* 下单独删除；
从最近路线列表中删除的路线，其骑行日志仍会保留。

### 导入的训练文件

**导入训练**会读取你选择（或拖入）的 GPX 或 TCX 文件。程序在本机处理该文件，只把结果（逐秒记录：路线上的位置、
海拔、速度、心率、踏频、瓦数）保存到 `rides` 文件夹，和你自己的骑行一样。文件的原始内容（例如 Strava 写入的
名称和时间戳）不会保存到别处，也不会有任何数据发到网络。估算瓦数也在本地计算。

## 需要知道的事

- OpenStreetMap、BRouter、Open-Meteo、GitHub 和 Google 会看到你自己的请求（IP 地址、时间）；所有网络客户端
  都是如此。
- 目前没有关闭更新检查的设置。如果你不希望这样，可以用防火墙屏蔽 `api.github.com`；程序不会因此损坏。
- 本程序不是 Google 或 Zwift 的产品，与它们没有关联。
