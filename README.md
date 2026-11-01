# bike-osm-ride

Virtuális tekerés GPX-útvonalakon, okos trainerrel (vagy anélkül). Betöltesz egy
[bikerouter.de](https://bikerouter.de)-ről exportált vagy bármilyen meglévő GPX-fájlt
(vagy beillesztesz egy bikerouter-linket), és végigtekered ugyanazt az utat: térképpel,
magasságprofillal és a trainerek által küldött összes adattal. Nincs 3D, csak gyors,
modern, 2D-s felület.

**Rust** alkalmazás: [Dioxus](https://dioxuslabs.com) desktop (HTML/CSS felület WebView2-ben),
Bluetooth LE a [`btleplug`](https://github.com/deviceplug/btleplug) crate-tel.
Fejlesztési környezet: Windows 10/11, Rust (GNU vagy MSVC toolchain), WebView2 runtime.

A felület egy kerékpáros fedélzeti számítógép / nyomtatott versenylap nyelvét követi: vonalakkal
tagolt, éltől élig érő panelek, nagy kondenzált számjegyek (Barlow, beágyazva), egyetlen narancsvörös
kiemelőszín, a sötét (meleg grafit) és a világos (papírfehér) téma egyenrangú. A részletek:
[`docs/DESIGN.md`](docs/DESIGN.md).

## Mit tud

- **Útvonal betöltése** – GPX fájl tallózással vagy az ablakra húzva, bikerouter-link
  beillesztése (a link `#`-fragmentjéből profil, `alternativeidx`, elkerülők is átkerülnek),
  legutóbbi útvonalak listája, beépített demo útvonal.
- **Térkép** – saját csempeszolgáltatás (`tiles` protokoll) helyi gyorsítótárral, sötét/világos/OSM
  stílus, drag-pan, görgős nagyítás az egér körül, követő mód, emelkedő szerint színezett
  útvonal, rajt/cél és bringás jelölő.
- **Magasságprofil** – teljes vagy „előretekintő" ablak, szintén emelkedő szerinti színezéssel,
  km-beosztás, emelkedő-sávok kategóriával, kattintásra ugrás (seek), egérre adatkiolvasás.
- **Élő műszerek** – teljesítmény (3 s átlag, W/kg, zóna), kadencia, pulzus (zóna), sebesség,
  emelkedő; statisztika (táv, hátralévő, idő, ETA, szintemelkedés, átlag/NP/max, kJ, kcal, pulzus).
- **Tiling elrendezés** – a felület panelekből áll (térkép, profil, vezérlés, váltó, teljesítmény,
  következő emelkedő, statisztika, grafikonok, edzés, szimulált tekerő, böngésző), amelyek úgy
  működnek, mint egy tiling ablakkezelőben: nincs átfedés és nincs lyuk. A panelek közti
  **elválasztót húzva** méretezhetsz (a vonal egész rácscellákra ugrik, a panelek minimális méretét
  tiszteletben tartva). Egy panel **fejlécénél fogva** egy másik panel szélére húzva kettéosztja
  azt, a közepére húzva helyet cserélnek (szellemkeret mutatja, mi lesz). A panel **× gombja**
  bezárja: a szomszédja kitölti a helyét, a panel pedig chipként a felső sávba kerül; a chipre
  kattintva oda tér vissza, ahol volt, a felületre húzva oda, ahová ejted. Minden mentődik; a
  *Beállítások → Elrendezés* egy gombbal visszaállítja az alaphelyzetet.
- **Telefon és tablet** – 1000 px-nél keskenyebb ablakban (telefon, álló tablet) a csempék helyett
  füles nézet jelenik meg: alul a **Térkép · Tekerés · Profil · Adatok** fülek, fölöttük mindig a
  Kezdés/Szünet sáv; álló tableten a panelek két oszlopban. A párbeszédablakok telefonon teljes
  képernyősek. Fekvő tableten (≥ 1000 px) a megszokott csempés elrendezés marad.
- **Android** – ugyanez az alkalmazás APK-ként is elkészül (lásd *Android* lent). Androidon nincs
  fájlválasztó: útvonalat bikerouter-linkkel, a legutóbbiak közül vagy a bemutató útvonallal
  töltesz be, a TCX-mentés a *Letöltések* mappába kerül.
- **Automatikus frissítés** – induláskor a program megnézi a GitHub legújabb kiadását; ha újabb
  van, a felső sávban megjelenik a **Frissítés** gomb. Windowson letölti az új exe-t, kicseréli
  magát és újraindul; Androidon megnyitja az új APK letöltését, a telepítést a rendszer kéri.
- **Edzés** – Coggan zónák, NP/TSS/IF, 1 Hz-es felvétel, összegzés zóna-bontással, **TCX export**
  (Garmin séma: pozíció, magasság, pulzus, kadencia, watt, sebesség) Strava/Intervals/TrainingPeaks
  kompatibilisen.
- **Edzéskönyvtár és ERG** – 93 beépített, blokkos edzés (regeneráció, állóképesség, tempo,
  sweet spot, küszöb, VO2max, anaerob, sprint, hegymászás, versenyszimuláció, vegyes és tesztek:
  20 perces és 8 perces FTP-teszt, ramp tesztek, CP-, teljesítményprofil- és tartóssági teszt).
  Sok közülük publikált kutatás protokollját követi (Rønnestad 30/15, Seiler 4 × 8 és 4 × 16,
  Helgerud 4 × 4 és 15/15, Billat 30/30, Gibala sprintintervallum, változó intenzitású
  VO2max-intervallumok). Minden edzésnek van leírása (mit, miért, hogyan), blokkonkénti
  instrukciója és **címkéi** (mire jó, szint, jelleg, időszak), amelyek alapján szűrni lehet. A felső sáv `Edzés`
  gombjával (vagy `W`-vel) nyílik a választó; a célok %FTP-ből számolódnak, és tekerés közben
  a trainer **ERG-módban** tartja a célwattot. A HUD az aktuális blokkot, a hátralévő időt és a
  következő blokkot mutatja; a szünet/kilépés itt is semlegesre állítja a trainert.
- **Virtuális váltó** – *Beállítások → Trainer → Virtuális váltó*. 2×12-es hajtás (50/34 lánctányér,
  11–34 fogaskoszorú, 23 különböző fokozat); a `←` / `→` billentyűvel vagy a Váltó panelen váltasz.
  Egy okos trainernek nincs saját váltója, ezért a fokozat a trainerre küldött lejtőt módosítja
  (a legkönnyebb ~−5 %, a legnehezebb ~+5 %): ugyanazon az úton könnyebb vagy nehezebb a letekerés,
  a panel a fokozat áttételét, a kadenciából adódó sebességet és az ellenállás-eltolást mutatja.
  Edzés (ERG) közben a célwatt a döntő, a váltó ilyenkor nem módosít semmit.
- **Zwift Click / Play váltó** – a Zwift Click kontrollerrel Bluetoothon léptetheted a virtuális
  váltót: a `+` gomb egy fokozattal nehezebbre, a `−` gomb könnyebbre kapcsol. A Zwift Play
  kontrollereknél a jobb kontroller `Shift` vagy `Y` gombja a nehezebb, a bal kontrolleré a
  `Shift` vagy `B` gomb a könnyebb irány. Az első léptetés bekapcsolja a virtuális váltót és
  erről egy értesítés szól. A kapcsolat titkosított kézfogással jön létre; a Zwift Hub trainer
  nem váltó, hanem trainerként csatlakozik. A **Zwift Cog** fizikai, elektronika nélküli
  fogaskerék: nincs Bluetooth-ja, nem kell csatlakoztatni, a program a virtuális váltóval
  helyettesíti. Zwift Click/Play nélkül a `←` / `→` billentyű és a Váltó panel marad.
- **Böngésző (Firefox)** – a `Böngésző` panel a gépre telepített **Firefoxot** építi be az ablakba,
  hogy sorozatot vagy bármilyen tartalmat nézhess tekerés közben (a Firefox saját,
  `%LOCALAPPDATA%\bike-osm-ride\firefox-profile` profilt használ, így a bejelentkezéseid
  megmaradnak, és a rendes Firefoxodhoz nem nyúl). Windowson működik; a panel bezárása a
  Firefoxot is bezárja. A Firefoxnak min. ~500 px szélesség kell, ezért a panel nem húzható
  ennél keskenyebbre. Párbeszédablak megnyitásakor vagy panel húzásakor a Firefox ideiglenesen
  elrejtőzik (a natív ablak fölé a felület nem tud rajzolni).
- **Szenzorok** – FTMS okos trainer (ellenállás-vezérlés bemelegítéssel, keep-alive-val,
  biztonsági szűrővel, szünetnél/kilépésnél semleges beállítással), teljesítménymérő (CPS),
  sebesség/kadencia (CSC), pulzus (HR), és egy **szimulált tekerő**, amivel hardver nélkül is
  végig lehet tekerni. Egy fizikai eszköz **egyszerre több szerepet is elláthat**: ha a trainer
  a teljesítményt, kadenciát vagy a pulzust is sugározza (CPS/CSC/HR szolgáltatás), egyetlen
  BLE-kapcsolat tölti ki az összes érintett kártyát – nem kell ugyanazt az eszközt külön
  szerepenként csatlakoztatni.
- **Fizika** – valósághű gördülési, légellenállási és gravitációs modell; a trainer az előtted
  lévő ~20 m átlagos emelkedését kapja, nehézségi szorzóval (0–150 %) és ±20 %-os korláttal.
- **Street View (opcionális, saját Google-kulccsal)** – a **térkép panelen belül**: a térkép panel
  fejlécének `Street View` gombjával (vagy a `V` billentyűvel) a Street View kis ablakban jelenik
  meg a térkép jobb alsó sarkában. Az `X` billentyűvel (vagy a kis ablak cserélő gombjával) a
  két nézet **helyet cserél**: a Street View lesz nagy, a térkép pedig kicsi, jobb alul, és ott is
  követi a kerékpárost egy olvasható, utcaszintű nagyítással. A felvétel a bringás aktuális pozíciójáról
  készül, az út irányába nézve.
  Kulcs nélkül nem jelenik meg semmi: a Google ezt csak az API-jain keresztül engedi, ezért a
  *Beállítások → Street View* alatt a saját Google Maps API-kulcsodat kell megadnod (a lépések
  ott le vannak írva). A kép alapból élőben, a Google beágyazott (Maps Embed API) nézetében jön.
  Opcióként bekapcsolható a **„Panoráma-azonosítók mentése az útvonalhoz"** (csak az ID-k, ~1,5 km-rel
  előre feloldva), és a **„Street View képek letöltése"**: a panel fejlécének Letöltés gombja
  75–250 méterenként egy képet ment az útvonalhoz (legfeljebb 900 képet), így ugyanazt az utat
  legközelebb **API-kulcs és hálózat nélkül** is végigtekered. A letöltött kép a Google korlátja miatt
  legfeljebb 640 × 640 px, ezért nagy panelen lágyabb: ha van kulcs és hálózat, a program az éles,
  teljes felbontású beágyazott nézetet mutatja, és csak annak hibájakor (vagy kulcs nélkül) vált a
  letöltött képekre. A letöltött képek 30 napig
  érvényesek, a Beállításokban bármikor törölhetők, és a letöltés a te Google-kvótádat használja.
- Sötét/világos téma (a világos „papírfehér" téma egyenrangú), beállítások azonnali mentése, magyar
  felület, a beágyazott betűtípusoknak köszönhetően hálózat nélkül is ugyanúgy néz ki.
- **Hálózati forgalom** – csak amit kérsz: OSM-csempék (a térképhez, helyben gyorsítótárazva),
  bikerouter/BRouter (ha linket illesztesz be), Open-Meteo (csak hiányzó magasságadatnál) és,
  ha bekapcsolod és kulcsot adsz meg, a Google Street View (az offline letöltés is csak kérésre
  indul). Minden más a gépen marad.

## Követelmények

- Windows 10/11, [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (Windows 11-en alapból ott van).
- Bluetooth LE adapter csak akkor kell, ha valódi szenzort/trainert használsz.
- Fejlesztéshez Rust (a projekt GNU toolchainen készült és tesztelt, MSVC-vel is fordul).
- A debug/release exe mellé a `WebView2Loader.dll` kerül (a `ride-app/build.rs` másolja oda).

## Futtatás

```powershell
cargo run -p ride-app                 # debug build + indítás
.\scripts\run.ps1                     # ugyanez, kényelmes kapcsolókkal
.\scripts\run.ps1 -Demo -Sim 250      # demo útvonal + szimulált tekerő 250 W-on
cargo build --release -p ride-app     # kész exe: target\release\bike-osm-ride.exe
```

### Android

Kell hozzá: Android SDK + NDK 27, JDK 17+, `rustup target add aarch64-linux-android` és a Dioxus
CLI (`cargo install dioxus-cli@0.7.10 --locked`).

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:ANDROID_HOME\ndk\27.2.12479018"
cd crates\ride-app
dx bundle --platform android --package-types apk --release --target aarch64-linux-android
# az APK: target\dx\bike-osm-ride\release\android\app\app\build\outputs\apk\
```

Telepítés telefonra: másold át az APK-t, és nyisd meg (engedélyezni kell az ismeretlen forrásból
származó alkalmazások telepítését), vagy USB-n: `adb install -r <apk>`. Androidon a Bluetooth
egyelőre nem működik (a btleplug Android-háttere Java-kiegészítést igényel), ott a szimulált
tekerővel lehet tekerni.

### Kiadás (GitHub)

A `.github/workflows/release.yml` (egyelőre kézzel indítva: *Actions → release → Run workflow*, egy `v*` címkén) futtatja a
folyamatot: tesztek, Windows exe (MSVC, DLL nélkül futtatható) és Android APK, majd GitHub Release
`bike-osm-ride-<verzió>-windows-x64.exe` és `bike-osm-ride-<verzió>-android.apk` fájlokkal; ezeket
keresi a beépített frissítő. A frissítő a `Cargo.toml` `repository` mezőjéből tudja a repót.

## Használat

1. **Útvonal.** Indításkor a nyitóképernyő jön: húzd rá a GPX-et, tallózz (`Ctrl+O`),
   illeszd be a bikerouter-linket, vagy válassz a legutóbbiak közül / a demót.
2. **Eszközök.** A felső sáv `Eszközök` gombja: trainerek, pulzus, teljesítménymérő,
   kadenciaszenzor és Zwift Click / Play váltó. A lista első eleme a **Szimulált tekerő** – ezzel hardver nélkül is megy.
   Keresésnél a szűrés nélküli scan után a szolgáltatások alapján soroljuk be az eszközöket.
   Egy eszköz csatlakoztatásakor minden olyan szerepet kihasználunk, amit az eszköz hirdet
   (pl. egy trainer FTMS + CPS + HR szolgáltatással négy kártyát is elláthat); a kártyákon a
   `közös eszköz` jelzés mutatja, hogy ugyanarról a kapcsolatról van szó.
3. **Edzés (opcionális).** A felső sáv `Edzés` gombja (vagy `W`) megnyitja a 93 edzésből álló
   könyvtárat: kategória és címkék (pl. *VO2max*, *Kezdőknek*, *Kevés időre*, *Kutatáson alapul*)
   szerint szűrhetsz (a címkék ÉS-kapcsolatban szűkítenek, mindegyik mellett ott a találatszám), a jobb oldali panelen a blokkokat látod, a kiválasztás
   azonnal érvényes. Tekerés közben a trainer ERG-módban tartja a célwattot; a jobb sáv tetején
   jelenik meg az aktuális blokk, a cél és a hátralévő idő. Az FTP-t a *Beállítások → Versenyző*
   alatt állítsd be, a %FTP-s célok abból számolódnak.
4. **Tekerés.** `Kezdés` (vagy `Space`): a fizika a mért teljesítményből számol sebességet, a
   trainer megkapja az emelkedést. Szünetnél a trainer semlegesre áll, az idő áll.
5. **Street View (opcionális).** A felső sáv `Street View` gombja (vagy `V`) be-/kikapcsolja a panelt;
   a panel fejlécében a cserélő, a letöltés, a beállítások és a bezárás gombja van. A **cserélő
   gombbal** (vagy `X`-szel) a Street View és a térkép helyet cserél: ha a Street View a nagy, a
   térkép kerül a kis panel helyére, és viszont. A kis panel a bal felső fogantyújánál mozgatható,
   a jobb alsó sarkánál méretezhető; a jobb sáv és az alsó profil határa is húzható. Kulcs nélkül a
   panel megmondja, mit kell tenni, és egy gombbal megnyitja a *Beállítások → Street View* részt:
   ott létre kell hozni egy API-kulcsot a Google Cloud Console-ban (Maps Embed API + Street View
   Static API engedélyezve), és bemásolni. Hibáknál magyar üzenet jön (érvénytelen / elutasított
   kulcs, elfogyott kvóta, ezen a helyen nincs panoráma); a kulcsot a program sehová nem naplózza.
   **Ismételt tekeréshez** nyomd meg a fejléc Letöltés gombját: a program 75–250 méterenként
   letölti az útvonal képeit (a folyamatjelző a fejléc alatt látszik), utána ugyanez az útvonal
   kulcs és hálózat nélkül is végigtekered. A letöltött képek a *Beállítások → Street View* alatt
   törölhetők.
6. **Vége.** Az útvonal végén automatikusan zárul a túra; az összegzőből `Mentés TCX-ként`
   gombbal exportálhatsz (a javasolt fájlnév dátumból és útvonalnévből áll), vagy jöhet új út.

### Billentyűparancsok

| Gomb | Művelet |
|---|---|
| `Space` | indítás / szünet / folytatás |
| `M` | térkép követő mód be/ki |
| `P` | magasságprofil: teljes ↔ előretekintő |
| `T` | sötét/világos téma |
| `V` | Street View panel be/ki |
| `X` | térkép és Street View cseréje |
| `W` | edzésválasztó |
| `F11` | teljes képernyő |
| `Esc` | dialógus bezárása, illetve kilépés a teljes képernyőből |
| `Ctrl+O` | GPX megnyitása |
| `↑` / `↓` | szimulált tekerő teljesítménye ±10 W |

A szövegmezőkben gépelt billentyűk nem váltanak parancsot (kivéve `Esc`, `F11`, `Ctrl+O`).

## Adatok helye

Minden a `%LOCALAPPDATA%\bike-osm-ride\` alatt: `settings.json` (beállítások), `recent.json`
(legutóbbi útvonalak), `routes\` (a megnyitott útvonalak feldolgozott geometriája, hogy legközelebb
ne kelljen újra feldolgozni), `tiles\` (csempe-gyorsítótár, 30 nap), `streetview\<útvonal-azonosító>.json`
(csak ha bekapcsoltad a panoráma-azonosítók mentését: ~25 m-es útvonalszakaszonként az azonosító, az
irány és az időbélyeg; a kulcs nincs benne), `streetview\img\<útvonal-azonosító>\` (csak a letöltött
Street View képek és a hozzájuk tartozó `manifest.json`; a képek 30 napig érvényesek, a Beállításokban
törölhetők), `ble.log` (ha kéred),
`webview\` (WebView2 profil). A Google API-kulcs a `settings.json`-ban van, csak ezen a gépen. A `RIDE_DATA_DIR` környezeti változóval áthelyezhető.

## Fejlesztés

```
crates/
  ride-model        a közös adatmodell (Route, RideSnapshot, Settings, …)
  ride-route        GPX/bikerouter → Route, emelkedők, perzisztencia
  ride-sim          fizika, RideEngine, statisztika, felvétel, TCX
  ride-ble          BLE hub (FTMS/CPS/CSC/HR) + szimulált tekerő
  ride-ui-kit       design system: tokenek, paletta, ikonok, formázás
  ride-ui-map       csempés térkép, útvonal, bringás jelölő
  ride-ui-profile   magasságprofil + élő grafikonok
  ride-ui-hud       műszercsempék, statisztika, emelkedő- és edzéskártya, vezérlők
  ride-ui-dialogs   nyitóképernyő, eszközök, beállítások, edzésválasztó, összegzés, névjegy
  ride-ui-streetview Street View panel: metadata-lekérdezés, azonosító-gyorsítótár, offline képletöltés
  ride-app          shell: állapot, vezérlőciklus, csempe-proxy, elrendezés
```

- `cargo test --workspace` – a teljes tesztkészlet (fizika-referenciaértékek, GPX-pipeline,
  BLE-parserek differenciáltesztjei a `ftms` crate-pel szemben, hub-viselkedés fake átvitellel,
  UI-segédfüggvények).
- `cargo check -p <crate>` / `cargo clippy -p <crate>` – crate-enként.
- `.\scripts\screenshot.ps1` – elindítja az appot és PNG-t ment a főablakról (a `RIDE_*`
  változókkal vezérelhető: `RIDE_DEMO`, `RIDE_SIM`, `RIDE_GPX`, `RIDE_THEME`, `RIDE_WINDOW`,
  `RIDE_DIALOG`, `RIDE_TIMESCALE`, `RIDE_TILE_PROBE`, `RIDE_BLE_LOG`, `RIDE_STREETVIEW_BASE`). Részletek:
  `crates/ride-app/src/hooks.rs`.
- `RIDE_DEBUG_PORT=9333` + `node scripts/cdp_eval.mjs 9333 "<JS>"` – a futó WebView2 lap
  vizsgálata (elrendezés, méretek, kattintás szimulálása) a fejlesztéshez; a szkript a
  b64-vel átadott kifejezést is elfogadja, így idézőjelek nélkül is használható.
- `cargo run -p ride-ui-streetview --example offline_demo -- 48` – a demó útvonal első 48
  Street View képét tölti le a `RIDE_DATA_DIR`-be (mock szerverrel együtt az offline mód
  kipróbálásához).
- `node scripts/streetview_mock.mjs 9300` – a három Google-végpont (metadata + kép + beágyazott
  nézet) helyi utánzata kulcs és hálózat nélküli teszthez; az appot
  `RIDE_STREETVIEW_BASE=http://127.0.0.1:9300` értékkel indítva a panel ide beszél (csak helyi cím
  fogadható el, így a kulcs nem juthat máshová). Az
  oldal a kapott panoráma-azonosítót és irányt kiírja, így a csatlakozás szemmel ellenőrizhető; a
  `/__stats` címen a kérésszámlálók láthatók.
- `.\scripts\third_party_licenses.ps1` – újragenerálja a `THIRD_PARTY_LICENSES.md`-t (a
  `Névjegy` ablak ezt a fájlt fordításkor beágyazza).
- Ha a PowerShell végrehajtási házirend blokkolja a szkripteket:
  `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1`.
- A csempéket a `ride-ui-map::upstream_url` szerinti szerverekről tölti le az app, a
  felhasználói agenttel, helyben gyorsítótárazva (nincs tömeges előtöltés).

## Licenc

### A projekt saját kódja

A `bike-osm-ride` saját kódja **kettős licencű: [MIT](LICENSE-MIT) VAGY
[Apache-2.0](LICENSE-APACHE)**, tetszés szerint (SPDX: `MIT OR Apache-2.0`).
Hozzájárulás esetén a beküldött kód is ezen a két licencen kerül be, hacsak másképp
nem jelzed.

### Felhasznált nyílt forrású kód

Szándékosan csak megengedő (permissive) licencű kódot használunk fel. Az eredetet és a
licencet a forrásfájlokban és a [`NOTICE.md`](NOTICE.md)-ben is megőrizzük.

| Forrás | Licenc | Mire használjuk |
|---|---|---|
| [`ftms`](https://github.com/deancochran/ftms) crate | MIT | Bluetooth FTMS (okos trainer) adatok és vezérlőparancsok kódolása/dekódolása |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | teljesítménymérő, cadence- és pulzusszenzor adatok értelmezése (beépítve, forrásfájlokban jelölve) |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | trainer-kapcsolat és parancs-biztonsági szűrő (átültetett részek, jelölve) |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | pulzusmérő-keresés és újracsatlakozás (minta, ha átvett kódot tartalmaz, jelölve) |
| [Barlow és Barlow Condensed](https://github.com/jpt/barlow) betűtípusok | SIL OFL 1.1 | a felület szövege és számjegyei; a programba **beágyazva** (részhalmazolt WOFF2, latin + latin-ext, benne a magyar ő/ű), így hálózat nélkül is működnek; a licenc szövege: [`crates/ride-ui-kit/assets/fonts/OFL.txt`](crates/ride-ui-kit/assets/fonts/OFL.txt), a részletek a [`NOTICE.md`](NOTICE.md)-ben |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense (közkincs) | Zwift Click/Play és Zwift Hub BLE-protokoll (átírva Rustra): a `ride-ble` `zwift` modulja, részletek a [`NOTICE.md`](NOTICE.md)-ben |
| [`p256`](https://github.com/RustCrypto/elliptic-curves), [`hkdf`](https://github.com/RustCrypto/KDFs), [`sha2`](https://github.com/RustCrypto/hashes), [`aes`](https://github.com/RustCrypto/block-ciphers), [`ccm`](https://github.com/RustCrypto/AEADs) (RustCrypto) | MIT / Apache-2.0 | a Zwift Click kulcscseréje (ECDH P-256, HKDF-SHA256) és az AES-CCM titkosítás |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0 (a program **nem tartalmazza**, a gépre telepített példányt indítja) | a Böngésző panel: a telepített Firefox ablakát építjük be, külön profillal |
| [Dioxus](https://dioxuslabs.com), [btleplug](https://github.com/deviceplug/btleplug), [tokio](https://tokio.rs), [quick-xml](https://github.com/tafia/quick-xml) és a többi Rust-függőség | főleg MIT / Apache-2.0 | a teljes, automatikusan generált lista: [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) |

A Zwift Click/Play protokoll leírásához Makinolo visszafejtési jegyzeteit használtuk
dokumentációként: <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>. Ez nem
hivatalos, interoperabilitási munka: a Zwifttel nincs kapcsolatunk, a program nem a Zwift terméke,
és a Zwift nem támogatja.

GPL/AGPL licencű projektek (pl. OpenCycling, suffice, GoldenCheetah, QDomyos-Zwift)
kódját **nem** használtuk fel, mert az az egész programot copyleft hatálya alá vonná;
a kutatás részletei: [`docs/PRIOR_ART.md`](docs/PRIOR_ART.md).

**Licenc-audit (Windows build):** a `cargo metadata` szerint a Windowsos (`x86_64-pc-windows-gnu`)
build 373 külső crate-je mind megengedő licencű (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, CC0, Unlicense, 0BSD);
copyleft vagy ismeretlen licenc nincs közöttük.

A `Cargo.lock` minden platform függőségét tartalmazza (635 csomag, ezt listázza a
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md)). Ezek között néhány **MPL-2.0** licencű crate
is szerepel: `cssparser`, `selectors`, `dtoa-short` (a `wry` → `kuchikiki` láncon) és `option-ext`
(a `tray-icon` → `dirs` láncon). A cargo szerint ezek csak nem-Windowsos célplatformok buildjébe
kerülnek be, a Windowsos programba nem. Ha mégis más platformra fordítod: az MPL-2.0 fájlszintű
copyleft, módosítatlan könyvtárként használva nem érinti a projekt saját licencét.

### Térkép- és adatforrások (attribúció)

Az alkalmazás működés közben külső szolgáltatásokat hív; ezek adatai a saját
feltételeik szerint használhatók, és az alkalmazás a térképen megjeleníti az
előírt hivatkozásokat:

- **Térképadatok:** © [OpenStreetMap](https://www.openstreetmap.org/copyright)-közreműködők, [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Csempék:** a standard [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles)-csempék (`tile.openstreetmap.org`). A *Sötét* és a *Világos* térképstílus ugyanezeket a csempéket szűri helyben (CSS-szűrés), így nincs szükség API-kulcsra vagy harmadik féltől származó stílusszerverre. A letöltött csempéket helyben gyorsítótárazzuk, hogy kíméljük a szervereket ([OSM tile usage policy](https://operations.osmfoundation.org/policies/tiles/)).
- **Street View (opcionális):** a [Google Maps Platform](https://mapsplatform.google.com) szolgáltatásai: a Street View Static API *metadata* végpontja (csak JSON: állapot, panoráma-azonosító, helyzet), a Static API kép-végpontja (csak a kért offline letöltésnél) és a Maps Embed API Street View nézete. A **saját** API-kulcsodat használja a program (a Google Cloud-fiókod, a te kvótád és számlázásod), a kulcs csak a gépeden, a `settings.json`-ban van, soha nem kerül naplóba, és csak a `maps.googleapis.com` / `www.google.com` címekre megy el. A használatra a [Google Maps Platform feltételei](https://cloud.google.com/maps-platform/terms) és a [Street View irányelvei](https://developers.google.com/maps/documentation/streetview/policies) vonatkoznak. A program **alapból nem tölti le a képeket**: ha bekapcsolod az offline opciót és a panelen elindítod a letöltést, az útvonalhoz tartozó képeket a saját gépedre menti legfeljebb 30 napra; ezeket csak kulcs nélkül vagy hálózati hiba esetén mutatja, egyébként az éles, teljes felbontású nézet látszik. A Street View képek tartós tárolását a Google feltételei korlátozzák, ezért ez a te döntésed és felelősséged; a letöltés a saját Google-fiókod kvótáját használja. A *panoráma-azonosítók* tárolását az irányelvek engedik (ezek idővel változhatnak, ezért mellettük a koordinátát és az időbélyeget is megőrizzük, és a beágyazott nézet az azonosító mellé a koordinátát is megkapja, így ha az azonosító már nem létezik, a Google a legközelebbi panorámát mutatja). Ez a program nem a Google terméke, és nincs a Google-lel kapcsolatban.
- **Edzésleírások:** a beépített edzéskönyvtár nyilvános edzésprotokollok (British Cycling, TrainingPeaks, CTS, FasCat, TrainerRoad és mások) és edzéstudományi vizsgálatok (Rønnestad, Seiler, Helgerud, Billat, Gibala, Bossi, Karsten és mások) alapján szerkesztett saját szöveg; a felhasznált források edzésenként a `crates/ride-model/assets/workouts.json` `sources` mezőjében szerepelnek.
- **Útvonaltervezés:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de) (nyilvános szolgáltatás; csak akkor hívjuk, ha bikerouter-linket illesztesz be, az útvonalat az ottani szerver számolja).
- **Hiányzó magasságadatok pótlása (opcionális):** [Open-Meteo](https://open-meteo.com) Elevation API, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), nem kereskedelmi használatra.

A saját licenc nem terjed ki ezekre az adatokra és szolgáltatásokra.

### Jogi nyilatkozat

A szoftver „ahogy van” (as is) áll rendelkezésre, garancia nélkül; a részletek a
[LICENSE-MIT](LICENSE-MIT) és a [LICENSE-APACHE](LICENSE-APACHE) fájlokban találhatók.
Az eszközvezérlés (a trainer ellenállásának automatikus állítása) a saját felelősségedre
történik.
