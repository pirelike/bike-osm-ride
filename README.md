<p align="center">
  <img src="packaging/icons/128.png" width="96" alt="bike-osm-ride ikon">
</p>

<h1 align="center">bike-osm-ride</h1>

<p align="center">
  <b>Virtuális tekerés a saját GPX-útvonalaidon, okos trainerrel – vagy anélkül.</b><br>
  Térkép, magassági profil, edzések, szellem, TCX-export. Gyors, modern, 2D. Rust.
</p>

<p align="center">
  <a href="https://github.com/pirelike/bike-osm-ride/releases/latest"><img alt="Legfrissebb kiadás" src="https://img.shields.io/github/v/release/pirelike/bike-osm-ride?display_name=tag&sort=semver"></a>
  <img alt="Licenc: MIT vagy Apache-2.0" src="https://img.shields.io/badge/licenc-MIT%20OR%20Apache--2.0-blue">
  <img alt="Platformok" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-informational">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white">
  <img alt="Felület: magyar, angol, spanyol, portugál, német, olasz és kínai" src="https://img.shields.io/badge/felület-magyar%20%7C%20angol%20%7C%20spanyol%20%7C%20portug%C3%A1l%20%7C%20n%C3%A9met%20%7C%20olasz%20%7C%20k%C3%ADnai-brightgreen">
</p>

<p align="center">
  <img src="docs/screenshots/ride-dark.png" alt="A tekerés-képernyő sötét témában: térkép, teljesítmény, magassági profil, statisztika" width="100%">
</p>

> **In English:** bike-osm-ride is a free, open-source (MIT OR Apache-2.0) virtual-cycling app written in Rust. Load any
> GPX file or a [bikerouter.de](https://bikerouter.de) link, ride it on a 2D map with an elevation profile while a
> Bluetooth LE smart trainer follows the hills (or a built-in simulated rider pedals), run structured ERG workouts,
> race your own ghost and export TCX. Windows, Linux, macOS and Android. **The interface speaks Hungarian, English,
> Spanish, Portuguese, German, Italian and Chinese.**
>
> [English README](docs/i18n/en/README.md) · [user guide](docs/i18n/en/USER_GUIDE.md) ·
> [privacy](docs/i18n/en/PRIVACY.md) · *Settings → Appearance → Language*.
>
> **En español:** la aplicación está traducida al español: [README en español](docs/i18n/es/README.md) ·
> [manual del usuario](docs/i18n/es/USER_GUIDE.md) · [privacidad](docs/i18n/es/PRIVACY.md) ·
> *Ajustes → Apariencia → Idioma*.
>
> **Em português:** a aplicação está traduzida para português: [README em português](docs/i18n/pt/README.md) ·
> [manual do utilizador](docs/i18n/pt/USER_GUIDE.md) · [privacidade](docs/i18n/pt/PRIVACY.md) ·
> *Definições → Aspeto → Idioma*.
>
> **Auf Deutsch:** Die App ist auf Deutsch übersetzt: [README auf Deutsch](docs/i18n/de/README.md) ·
> [Benutzerhandbuch](docs/i18n/de/USER_GUIDE.md) · [Datenschutz](docs/i18n/de/PRIVACY.md) ·
> *Einstellungen → Erscheinungsbild → Sprache*.
>
> **In italiano:** L'app è tradotta in italiano: [README in italiano](docs/i18n/it/README.md) ·
> [guida dell'utente](docs/i18n/it/USER_GUIDE.md) · [privacy](docs/i18n/it/PRIVACY.md) ·
> *Impostazioni → Aspetto → Lingua*.
>
> **中文：** 本应用已翻译成简体中文：[中文 README](docs/i18n/zh/README.md) ·
> [用户指南](docs/i18n/zh/USER_GUIDE.md) · [隐私与网络流量](docs/i18n/zh/PRIVACY.md) ·
> *设置 → 外观 → 语言*。

**Nyelv / Language / Idioma:** magyar · [English](docs/i18n/en/README.md) · [Español](docs/i18n/es/README.md) ·
[Português](docs/i18n/pt/README.md) · [Deutsch](docs/i18n/de/README.md) · [Italiano](docs/i18n/it/README.md) ·
[中文](docs/i18n/zh/README.md) —
[Kézikönyv](docs/USER_GUIDE.md) / [User guide](docs/i18n/en/USER_GUIDE.md) / [Manual](docs/i18n/es/USER_GUIDE.md) /
[Manual (pt)](docs/i18n/pt/USER_GUIDE.md) / [Handbuch](docs/i18n/de/USER_GUIDE.md) /
[Guida](docs/i18n/it/USER_GUIDE.md) / [用户指南](docs/i18n/zh/USER_GUIDE.md) ·
[Adatvédelem](docs/PRIVACY.md) / [Privacy](docs/i18n/en/PRIVACY.md) / [Privacidad](docs/i18n/es/PRIVACY.md) /
[Privacidade](docs/i18n/pt/PRIVACY.md) / [Datenschutz](docs/i18n/de/PRIVACY.md) /
[Privacy (it)](docs/i18n/it/PRIVACY.md) / [隐私](docs/i18n/zh/PRIVACY.md)

**Ugrás:** [Letöltés](#letöltés) · [Gyors kezdés](#gyors-kezdés) · [Mit tud](#mit-tud) · [Képek](#képek) ·
[Eszközök](#eszközök-és-állapot) · [Dokumentáció](#dokumentáció) · [Fejlesztés](#fejlesztés) · [Licenc](#licenc)

## Miért ez?

- **A te útvonalad.** Nem előre gyártott világ: bármely GPX-et vagy bikerouter-linket végigtekerheted, valós
  szinttel és emelkedőkkel, a trainer az útvonal meredekségét követi.
- **Minden adat, ami számít.** Teljesítmény (3 mp-es átlag, W/kg, zóna), kadencia, pulzus, sebesség, meredekség,
  NP/IF/TSS, szintemelkedés, ETA: nagy, olvasható számokkal, 3D nélkül.
- **Edzés és elemzés.** 93 beépített, forrásokkal ellátott edzés ERG-módban, edzői üzenetekkel; szellem és helyi
  szegmensek PR-rel; TCX-export.
- **A tiéd marad.** Nincs fiók, nincs telemetria, az adataid a gépeden vannak ([adatvédelem](docs/PRIVACY.md)). Nyílt
  forrású, megengedő licenccel.
- **Hardver nélkül is megy.** A szimulált tekerővel mindent kipróbálhatsz Bluetooth nélkül.

## Letöltés

A legfrissebb kiadás: **[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**.
A fájlnevekben a `<verzió>` helyén a kiadás száma áll (például `0.3.0`).

| Platform | Fájl | Telepítés |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<verzió>-windows-x64.exe` | nincs telepítő: indítsd el. Kell a [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (Windows 11-en alapból megvan). A program nincs digitálisan aláírva, a SmartScreen megállíthatja. |
| **Linux** (x86-64, glibc 2.35+) | `bike-osm-ride-<verzió>-linux-x86_64.AppImage` | `chmod +x`, majd indítsd; magával hozza a WebKitGTK-t és a GTK-t |
| | `bike-osm-ride-<verzió>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<verzió>-linux-amd64.deb` (a `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `bluez` csomagokat kéri) |
| **macOS 11+** (Intel és Apple Silicon) | `bike-osm-ride-<verzió>-macos-universal.dmg` | húzd az *Alkalmazások* közé; az első indításnál **jobb klikk → Megnyitás → Megnyitás** (nincs hitelesítve); az első Bluetooth-keresésnél engedélyezd a Bluetooth-t |
| **Android 7+** | `bike-osm-ride-<verzió>-android.apk` | másold a telefonra és nyisd meg (engedélyezni kell az ismeretlen forrásból való telepítést), vagy `adb install -r` |

A program **magától szól**, ha új kiadás van: a felső sávban megjelenik a *Frissítés* gomb (Windowson és AppImage-ként
önmagát cseréli, a többi platformon a kiadás oldalát nyitja meg). Az Android-kiadáshoz állandó kiadási aláírókulcs szükséges; a kiadási folyamat hiányzó kulccsal megáll.
Egy régi debug-aláírt változatot a kiadási APK telepítése előtt el kell távolítani.

## Gyors kezdés

1. **Indítsd el.** Az *Útvonal* ablak nyílik meg: húzz rá egy GPX-et, vagy illessz be egy bikerouter-linket, vagy
   válaszd a *Bemutató útvonalat*.
2. **Csatlakoztasd a trainert** (*Eszközök* → *Keresés indítása* → *Trainer* szerepben), vagy válaszd a
   **Szimulált tekerőt**, ha nincs kéznél hardver. Pulzusmérő, teljesítménymérő, kadenciaszenzor és Zwift Click/Play is
   csatlakoztatható.
3. *(Opcionális)* **Edzés**: a felső sáv *Edzés* gombja (`W`), 93 edzésből választhatsz, vagy importálhatod a sajátodat.
4. **`Kezdés`** (`Szóköz`). A trainer követi az emelkedőket (vagy tartja az edzés célwattját), a számok élnek.
5. A végén az **összegzésből** TCX-et mentesz (a Strava, az Intervals.icu és a TrainingPeaks is elfogadja).

A teljes leírás: **[Felhasználói kézikönyv](docs/USER_GUIDE.md)**.

## Mit tud

**Útvonalak**
- GPX-fájl (tallózás, ráhúzás, `Ctrl+O`), bikerouter/BRouter-link (a profil, az alternatíva és az elkerülők is átjönnek),
  legutóbbi útvonalak, beépített bemutató útvonal.
- Tűrő GPX-olvasó (BOM, névterek, több szakasz, `<rte>`, hibás fájlra érthető üzenet); hiányzó magasságot az Open-Meteo
  pótol. Emelkedő-felismerés 4-től HC-ig; a fájlba tett szegmenstrackek (pl. Strava-szegmens) sávként látszanak.

**Tekerés**
- Valósághű fizika (Martin és mtsai. 1998: gravitáció, gördülés, csapágysúrlódás, küllős légellenállás, tehetetlenség),
  a trainer az előtted lévő szakasz meredekségét kapja (±20 %-ig, 0–150 % erősséggel).
- **FTMS-trainer** (emelkedők és ERG), **teljesítménymérő** (CPS), **sebesség/kadencia** (CSC), **pulzusmérő** (HR), egy
  fizikai eszköz akár *több szerepet* is ellát egy kapcsolaton; **Zwift Click/Play** virtuális váltó; **szimulált tekerő**.
- **Virtuális váltó**: 2 × 12-es hajtás, 23 fokozat, a trainer ellenállását módosítja.

**Edzés és elemzés**
- 93 beépített edzés 12 kategóriában, címkékkel, zónaszínes blokkvázlattal és 943 időzített edzői üzenettel; MyWhoosh-
  és Zwift-edzések (`.zwo`, XML, JSON) importja.
- Coggan-zónák, NP, IF, TSS, kcal; pulzuszónák; másodpercenkénti felvétel.
- **Szellem** (legutóbbi, legjobb, vagy a GPX saját időbélyegei) és **helyi szegmensek PR-rel**.
- **Edzések áttekintése és összehasonlítása**: az ugyanazon az útvonalon tett edzések (legfeljebb négy) egymás mellett
  teljesítmény, pulzus, kadencia, sebesség és időkülönbség szerint; ráközelítés egy szakaszra (húzással vagy az emelkedőkre),
  szakasztábla. Strava-, Garmin-exportból **GPX- vagy TCX-edzés importja** pulzussal, kadenciával; ha nincs benne watt, a
  szimuláció fizikája visszabecsüli (saját vagy megadott testsúllyal, kerékpárral, gördülési ellenállással).
- **Teljesítmény oldal**: csúcsteljesítmény 5 mp-től 60 percig, FTP-javaslat a legjobb 20 percből (egy gombbal beállítható), heti és
  havi terhelés. Csak mért watt számít.
- **Biztonsági mentés**: az útvonalak, a túrák, az importált edzések és a beállítások egyetlen zip-fájlban (a Google-kulcs nélkül),
  összefésülő visszaállítással. Semmi nem megy ki a hálózatra.
- **TCX-export** (Garmin-séma): a Strava, az Intervals.icu és a TrainingPeaks kézzel feltöltve elfogadja. Beépített feltöltés nincs: a Strava-bejelentkezés minden tekerés után újra kellett volna, ezért kivettük (0.3.2).

**Felület**
- **Tiling elrendezés**: a panelek úgy működnek, mint egy tiling ablakkezelőben (nincs átfedés, nincs lyuk); húzással
  méretezhetők, mozgathatók, bezárhatók; külön elrendezés edzéssel és anélkül.
- Sötét és világos téma (mindkettő teljes értékű), szövegméret 80–160 %, **magyar, angol, spanyol, portugál, német,
  olasz és kínai felület**
  (*Beállítások → Megjelenés → Nyelv*, vagy automatikusan a rendszer nyelvét követve), beágyazott betűtípusok.
- **Telefon és tablet**: füles nézet, a „Tekerés” fülön minden, ami tekerés közben kell.
- Opcionális **Street View** (saját Google-kulccsal, hálózat nélküli letöltéssel) és egy **Böngésző panel** (Windows, a
  telepített Firefoxot építi be).
- Ébren tartja a gépet tekerés közben (Windows, Linux, macOS).

## Képek

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/ride-light.png" alt="Világos téma"><br><sub><b>Világos téma</b>: a papírfehér téma egyenrangú a sötéttel</sub></td>
    <td width="50%"><img src="docs/screenshots/workout.png" alt="Edzés ERG-módban"><br><sub><b>Edzés ERG-módban</b>: edzéskártya, élő grafikonok, blokkprofil</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/workout-picker.png" alt="Edzésválasztó"><br><sub><b>93 edzés</b> szűrőkkel, zónaszínes blokkvázlattal</sub></td>
    <td><img src="docs/screenshots/summary.png" alt="A tekerés összegzése"><br><sub><b>Összegzés</b>: NP, IF, TSS, szegmensek, zónák; TCX-mentés</sub></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/devices.png" alt="Eszközök ablak"><br><sub><b>Eszközök</b>: egy eszköz több szerepben is</sub></td>
    <td align="center">
      <img src="docs/screenshots/phone-ride.png" width="170" alt="Telefon, Tekerés fül">
      <img src="docs/screenshots/phone-map.png" width="170" alt="Telefon, Térkép fül"><br>
      <sub><b>Telefonon</b>: Tekerés és Térkép fül</sub>
    </td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/review.png" alt="Edzések áttekintése"><br><sub><b>Áttekintés</b>: két edzés egy emelkedőn, a másodiknál becsült watt</sub></td>
    <td align="center">
      <img src="docs/screenshots/review-phone.png" width="170" alt="Telefon, áttekintés">
      <br><sub><b>Telefonon</b>: Edzések, Térkép, Grafikon, Szakasz fül</sub>
    </td>
  </tr>
</table>

<sub>A képernyőképeken egy valódi BRouter-útvonal látható (a repóban tesztadatként szereplő Hűvösvölgy–Dobogókő útvonal
utolsó 12 km-e), szimulált tekerővel. Útvonal és térkép: © OpenStreetMap-közreműködők, ODbL. A képeket
`node scripts/readme_shots.mjs` állítja elő.</sub>

## Eszközök és állapot

**Nem minden elem lett valódi eszközzel vagy valódi szolgáltatással kipróbálva.** Őszintén:

| Elem | Állapot |
|---|---|
| FTMS okos trainer, **ERG** (edzés) | tesztelve, **valódi trainerrel is** (egy típus: VE200) |
| FTMS **emelkedő-szimuláció** | alaposan tesztelve (szimulált átvitellel, a specifikáció bájtvektoraival); **valódi trainerrel még nem igazolt** |
| Pulzusmérő, külön teljesítménymérő, CSC-szenzor | a dekóderek tesztelve; a trainer CPS/CSC-adata valós; külön szenzorral még nem próbáltuk |
| Zwift Click / Play | megvalósítva (nem hivatalos), valódi kontrollerrel még nem próbáltuk |
| Szimulált tekerő | működik hardver nélkül, minden platformon |
| Windows | napi használatban |
| Linux, macOS | a CI építi és (Linuxon) 20 másodperces füsttesztet futtat; valódi gépen futtatott ellenőrzés még nincs (tapasztalatot szívesen fogadunk) |
| Android | a felület futott (a 0.3.0 hibáit futtatás közben találtuk meg); a rendszer-fájlválasztó megvalósítva; **a Bluetooth eszközön még nem volt kipróbálva** |
| Street View | megvalósítva és tesztelve helyi próbaszerverrel; a valódi szolgáltatással még nem próbáltuk |

Részletek: [`docs/TESTING.md`](docs/TESTING.md) és [`docs/BLUETOOTH.md`](docs/BLUETOOTH.md). Hibát, tapasztalatot, kompatibilis
vagy nem kompatibilis eszközt szívesen fogadunk az Issues-ban (érdemes csatolni a `ble.log`-ot).

## Dokumentáció

| Kinek | Mit |
|---|---|
| **Felhasználóknak** | [Felhasználói kézikönyv](docs/USER_GUIDE.md) · [Adatvédelem és hálózati forgalom](docs/PRIVACY.md) · [Változásnapló](CHANGELOG.md) · [English](docs/i18n/en/README.md) · [Español](docs/i18n/es/README.md) · [Português](docs/i18n/pt/README.md) · [Deutsch](docs/i18n/de/README.md) · [Italiano](docs/i18n/it/README.md) · [中文](docs/i18n/zh/README.md) |
| **Fejlesztőknek** | [Architektúra](docs/ARCHITECTURE.md) · [Fejlesztői útmutató](docs/DEVELOPMENT.md) · [Tesztelés](docs/TESTING.md) · [Kiadás](docs/RELEASING.md) · [Döntésnapló](docs/DECISIONS.md) · [Fordítás (i18n)](docs/I18N.md) |
| **Témák** | [Bluetooth](docs/BLUETOOTH.md) · [Szimuláció és fizika](docs/SIMULATION.md) · [Útvonalak és adatformátumok](docs/ROUTES_AND_DATA.md) · [Felület](docs/UI.md) · [Design rendszer](docs/DESIGN.md) · [Android](docs/ANDROID.md) · [Street View](docs/STREETVIEW.md) · [Előzmények, felhasznált kód](docs/PRIOR_ART.md) |

A teljes jegyzék: [`docs/README.md`](docs/README.md).

## Fejlesztés

```powershell
cargo run -p ride-app                      # debug build és indítás (Windows, Linux, macOS)
cargo test --workspace                     # a teljes tesztkészlet (2026-11-06: 1076 teszt)
node scripts/readme_shots.mjs              # a képernyőképek újragenerálása
```

Rust-workspace tíz könyvtár-crate-tel és az alkalmazással (`ride-app`):

```
crates/
  ride-model         közös adatmodell (Route, Settings, RideSnapshot, Workout, …) – a „szerződés”
  ride-route         GPX/bikerouter → Route, emelkedők, perzisztencia, tekerés-napló
  ride-sim           fizika, RideEngine, edzésfutó, szellem, szegmensek, TCX (I/O és óra nélkül)
  ride-ble           BLE-hub (FTMS/CPS/CSC/HR/Zwift) + szimulált tekerő
  ride-ui-kit        design rendszer: tokenek, paletta, ikonok, formázás
  ride-ui-map        csempés térkép · ride-ui-profile  profil és grafikonok
  ride-ui-hud        műszerek, kártyák, vezérlők       · ride-ui-dialogs  ablakok
  ride-ui-streetview Street View panel
  ride-app           a keret: állapot, vezérlő, tiling, csempe-proxy, frissítő, Android
packaging/  scripts/  tests/fixtures/  docs/
```

A kód Dioxus 0.7-re (webview-alapú felület) és `btleplug`-ra épül. Windowson a fejlesztés GNU-toolchainen zajlik
(a kiadási exe-t a CI MSVC-vel építi). Előfeltételek, kapcsolók (`RIDE_*`), hibakeresés, csomagolás:
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md). A Claude Code-nak szóló projektleírás: [`CLAUDE.md`](CLAUDE.md).

**Közreműködés.** Örülünk a hibajelentésnek, a mérési naplónak és a javításnak. A szabályok röviden: csak megengedő
licencű kód vihető át (lásd lent), a felhasználói szöveg forrása a magyar (a felület minden szövege kulcs a
`crates/ride-i18n/locales/hu.json`-ban, a fordítások mellette; lásd [`docs/I18N.md`](docs/I18N.md)), külső adatra nem
panikolunk, új funkcióhoz teszt és dokumentáció tartozik. A beküldött kód a projekt licencén (MIT vagy Apache-2.0) kerül
be, hacsak másképp nem jelzed.

## Licenc

### A projekt saját kódja

A `bike-osm-ride` saját kódja **kettős licencű: [MIT](LICENSE-MIT) VAGY [Apache-2.0](LICENSE-APACHE)**, tetszés szerint
(SPDX: `MIT OR Apache-2.0`). Hozzájárulás esetén a beküldött kód is ezen a két licencen kerül be, hacsak másképp nem
jelzed.

### Felhasznált nyílt forrású kód

Szándékosan csak megengedő (permissive) licencű kódot használunk fel. Az eredetet és a licencet a forrásfájlokban és a
[`NOTICE.md`](NOTICE.md)-ben is megőrizzük.

| Forrás | Licenc | Mire használjuk |
|---|---|---|
| [`ftms`](https://github.com/deancochran/ftms) crate | MIT | Bluetooth FTMS (okos trainer) adatok és vezérlőparancsok kódolása/dekódolása |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | teljesítménymérő, cadence- és pulzusszenzor adatok értelmezése (beépítve, forrásfájlokban jelölve) |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | trainer-kapcsolat és parancs-biztonsági szűrő (átültetett részek, jelölve) |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | pulzusmérő-keresés és újracsatlakozás (minta, kód nélkül átvéve, jelölve) |
| [Barlow és Barlow Condensed](https://github.com/jpt/barlow) betűtípusok | SIL OFL 1.1 | a felület szövege és számjegyei; a programba **beágyazva** (részhalmazolt WOFF2, latin + latin-ext, benne a magyar ő/ű), így hálózat nélkül is működnek; a licenc szövege: [`crates/ride-ui-kit/assets/fonts/OFL.txt`](crates/ride-ui-kit/assets/fonts/OFL.txt), a részletek a [`NOTICE.md`](NOTICE.md)-ben |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense (közkincs) | Zwift Click/Play és Zwift Hub BLE-protokoll (átírva Rustra): a `ride-ble` `zwift` modulja, részletek a [`NOTICE.md`](NOTICE.md)-ben |
| [`p256`](https://github.com/RustCrypto/elliptic-curves), [`hkdf`](https://github.com/RustCrypto/KDFs), [`sha2`](https://github.com/RustCrypto/hashes), [`aes`](https://github.com/RustCrypto/block-ciphers), [`ccm`](https://github.com/RustCrypto/AEADs) (RustCrypto) | MIT / Apache-2.0 | a Zwift Click kulcscseréje (ECDH P-256, HKDF-SHA256) és az AES-CCM titkosítás |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0 (a program **nem tartalmazza**, a gépre telepített példányt indítja) | a Böngésző panel: a telepített Firefox ablakát építjük be, külön profillal |
| [Dioxus](https://dioxuslabs.com), [btleplug](https://github.com/deviceplug/btleplug), [tokio](https://tokio.rs), [quick-xml](https://github.com/tafia/quick-xml) és a többi Rust-függőség | főleg MIT / Apache-2.0 | a teljes, automatikusan generált lista: [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) |

Csak **hivatkozásként** (kód átvétele nélkül) szolgált: a `rouleur` nevű, nyílt forrású Martin-féle implementáció egyik
kidolgozott példájának számértéke a fizika tesztjében, az [`nrenner/brouter-web`](https://github.com/nrenner/brouter-web) (MIT) a
bikerouter-linkek formátumának ellenőrzéséhez, valamint Makinolo visszafejtési jegyzetei a Zwift Click/Play protokollhoz
dokumentációként: <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>. Ez nem hivatalos, interoperabilitási
munka: a Zwifttel nincs kapcsolatunk, a program nem a Zwift terméke, és a Zwift nem támogatja.

GPL/AGPL licencű projektek (pl. OpenCycling, suffice, GoldenCheetah, QDomyos-Zwift) kódját **nem**
használtuk fel, mert az az egész programot copyleft hatálya alá vonná; a kutatás részletei:
[`docs/PRIOR_ART.md`](docs/PRIOR_ART.md).

**Licenc-audit (2026-11-04, `cargo metadata --filter-platform`, normál és build-függőségek).** A külső (crates.io) crate-ek
száma platformonként: Windows (GNU) 396, Linux 458, macOS (Apple Silicon) 400, Android (aarch64) 449. A listában nincs ismeretlen licenc; az `r-efi` LGPL-t is felsoroló OR-licencéből
a megengedő MIT vagy Apache-2.0 választható. Nem használunk kötelező GPL/AGPL/LGPL licencű kódot; a megengedő licencek: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, CC0,
Unlicense és hasonlók.

**Az egyetlen copyleft típus az MPL-2.0**, amely **fájlszintű**, és a crate-eket módosítás nélkül, a crates.io-ról vesszük (nem
vendorolt kód), a forrásuk a [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md)-ben megadott tárolókban elérhető:

- Linuxon és macOS-en az `option-ext` (a `dioxus-desktop → tray-icon → dirs` láncon át);
- Androidon a `selectors`, `cssparser`, `cssparser-macros` és `dtoa-short` (a `wry → kuchikiki` láncon át);
- Windowson egyik sem.

Ez a projekt saját licencét nem érinti: a saját kódunk továbbra is `MIT OR Apache-2.0`. A `Cargo.lock` minden platform
függőségét tartalmazza; a lista újragenerálása: `.\scripts\third_party_licenses.ps1`, a platformonkénti audit parancsai a
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) 9. pontjában vannak.

### Térkép- és adatforrások (attribúció)

Az alkalmazás működés közben külső szolgáltatásokat hív; ezek adatai a saját feltételeik szerint használhatók, és az
alkalmazás a térképen megjeleníti az előírt hivatkozásokat (a hívások teljes listája: [`docs/PRIVACY.md`](docs/PRIVACY.md)):

- **Térképadatok:** © [OpenStreetMap](https://www.openstreetmap.org/copyright)-közreműködők, [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Csempék:** a standard [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles)-csempék (`tile.openstreetmap.org`). A *Sötét* és
  a *Világos* térképstílus ugyanezeket a csempéket szűri helyben (CSS-szűrés), így nincs szükség API-kulcsra vagy harmadik
  féltől származó stílusszerverre. A letöltött csempéket helyben gyorsítótárazzuk, hogy kíméljük a szervereket
  ([OSM tile usage policy](https://operations.osmfoundation.org/policies/tiles/)); tömeges előtöltés nincs.
- **Street View (opcionális):** a [Google Maps Platform](https://mapsplatform.google.com) szolgáltatásai: a Street View Static API
  *metadata* végpontja (csak JSON: állapot, panoráma-azonosító, helyzet), a Static API kép-végpontja (csak a kért offline
  letöltésnél) és a Maps Embed API Street View nézete. A **saját** API-kulcsodat használja a program (a Google Cloud-fiókod, a te
  kvótád és számlázásod), a kulcs csak a gépeden, a `settings.json`-ban van, soha nem kerül naplóba, és csak a
  `maps.googleapis.com` / `www.google.com` címekre megy el. A használatra a
  [Google Maps Platform feltételei](https://cloud.google.com/maps-platform/terms) és a
  [Street View irányelvei](https://developers.google.com/maps/documentation/streetview/policies) vonatkoznak. A program **alapból nem
  tölti le a képeket**: csak ha a panelen a *Letöltés* gombot nyomod, ment az útvonalhoz képeket a saját gépedre legfeljebb 30
  napra; ezeket csak kulcs nélkül vagy hálózati hiba esetén mutatja, egyébként az éles, teljes felbontású nézet látszik. A Street
  View képek tartós tárolását a Google feltételei korlátozzák, ezért ez a te döntésed és felelősséged; a letöltés a saját
  Google-fiókod kvótáját használja. A *panoráma-azonosítók* tárolását az irányelvek engedik (ezek idővel változhatnak, ezért
  mellettük a koordinátát és az időbélyeget is megőrizzük, és a beágyazott nézet az azonosító mellé a koordinátát is megkapja).
  Ez a program nem a Google terméke, és nincs a Google-lel kapcsolatban.
- **Edzésleírások:** a beépített edzéskönyvtár nyilvános edzésprotokollok (British Cycling, TrainingPeaks, CTS, FasCat, TrainerRoad
  és mások) és edzéstudományi vizsgálatok (Rønnestad, Seiler, Helgerud, Billat, Gibala, Bossi, Karsten és mások) alapján szerkesztett
  saját szöveg; a felhasznált források edzésenként a `crates/ride-model/assets/workouts.json` `sources` mezőjében szerepelnek.
- **Útvonaltervezés:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de) (nyilvános szolgáltatás; csak akkor
  hívjuk, ha bikerouter-linket illesztesz be, az útvonalat az ottani szerver számolja). A `tests/fixtures/brouter_*.gpx` BRouter-
  válaszok, OSM-adatokból (ODbL), és a README képernyőképein is ez az útvonal látszik.
- **Hiányzó magasságadatok pótlása (opcionális):** [Open-Meteo](https://open-meteo.com) Elevation API,
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), nem kereskedelmi használatra.

A saját licenc nem terjed ki ezekre az adatokra és szolgáltatásokra.

### Jogi nyilatkozat

A szoftver „ahogy van” (as is) áll rendelkezésre, garancia nélkül; a részletek a [LICENSE-MIT](LICENSE-MIT) és a
[LICENSE-APACHE](LICENSE-APACHE) fájlokban találhatók. Az eszközvezérlés (a trainer ellenállásának automatikus állítása)
a saját felelősségedre történik.
