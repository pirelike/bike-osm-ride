# bike-osm-ride

Virtuális tekerés GPX-útvonalakon, okos trainerrel (vagy anélkül). Betöltesz egy
[bikerouter.de](https://bikerouter.de)-ről exportált vagy bármilyen meglévő GPX-fájlt
(vagy beillesztesz egy bikerouter-linket), és végigtekered ugyanazt az utat: térképpel,
magasságprofillal és a trainerek által küldött összes adattal. Nincs 3D, csak gyors,
modern, 2D-s felület.

**Rust** alkalmazás: [Dioxus](https://dioxuslabs.com) desktop (HTML/CSS felület a rendszer
webview-jában: WebView2 Windowson, WKWebView macOS-en, WebKitGTK Linuxon), Bluetooth LE a
[`btleplug`](https://github.com/deviceplug/btleplug) crate-tel. Windows 10/11, Linux és macOS
(Intel és Apple Silicon) alatt ugyanaz a kód fut.

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
  km-beosztás, emelkedő-sávok kategóriával, kattintásra ugrás (seek), egérre adatkiolvasás. Az
  előretekintő ablak skálája csak akkor vált, ha muszáj (és egy ablaknyit előre is számol), így
  tekerés közben nem ugrál. Ha edzés van kiválasztva, a **Magasság | Edzés** kapcsolóval a profil
  helyén az edzés blokkjai látszanak: zónaszínes blokkok, az FTP-vonal, hol tartasz, mennyi van
  hátra a blokkból és milyen blokk következik (visszaszámlálóval).
- **Szegmensek a GPX-ből** – ha a GPX-ben a fő track mellett van egy rövidebb track, amely az
  útvonalon fekszik (pl. egy Strava-szegmens GPX-e beletéve a fájlba), vagy egy track
  `<type>segment</type>` jelöléssel, az nem toldódik az útvonalhoz, hanem nevesített szegmens lesz:
  a profilon sávként látszik, tekerés közben pedig a „Szegmens: … X km hátra” kiírás. (A Strava
  saját GPX-exportja szegmenst nem tartalmaz.)
- **Élő műszerek** – teljesítmény (3 s átlag, W/kg, zóna), kadencia, pulzus (zóna), sebesség,
  emelkedő; statisztika (táv, hátralévő, idő, ETA, szintemelkedés, átlag/NP/max, kJ, kcal, pulzus).
- **Tiling elrendezés** – a felület panelekből áll (térkép, profil, vezérlés, váltó, teljesítmény,
  következő emelkedő, statisztika, grafikonok, edzés, szimulált tekerő, böngésző), amelyek úgy
  működnek, mint egy tiling ablakkezelőben: nincs átfedés és nincs lyuk. A panelek közti
  **elválasztót húzva** méretezhetsz (a vonal egy 96 × 64-es rács celláira ugrik, a panelek
  minimális méretét tiszteletben tartva). Egy panel **fejlécénél fogva** egy másik panel szélére húzva kettéosztja
  azt, a közepére húzva helyet cserélnek (szellemkeret mutatja, mi lesz). A panel **× gombja**
  bezárja: a szomszédja kitölti a helyét, a panel pedig chipként a felső sávba kerül; a chipre
  kattintva oda tér vissza, ahol volt, a felületre húzva oda, ahová ejted. Minden mentődik; a
  *Beállítások → Elrendezés* egy gombbal visszaállítja az alaphelyzetet. Az elrendezést (és a
  profil Teljes / Előretekintés módját) az app **külön jegyzi edzéssel és edzés nélkül**: a
  kiválasztott edzéshez saját elrendezés tartozik, amely addig a szabadot követi, amíg ott nem
  alakítod át. Az elválasztó húzása csak a két szomszédos panelt méretezi, a mögöttük lévőket nem.
- **Szövegméret** – *Beállítások → Megjelenés → Szövegméret* (80–160 %): minden szöveg és szám
  együtt nagyobb vagy kisebb, kis vagy nagy kijelzőn.
- **Telefon és tablet** – 1000 px-nél keskenyebb ablakban (telefon, álló tablet) a csempék helyett
  füles nézet jelenik meg: alul a **Tekerés · Térkép · Grafikonok · Adatok** fülek, fölöttük mindig
  a nagy Kezdés/Szünet/Befejezés gombsor (bekapcsolt virtuális váltónál, edzésen kívül a váltó
  gombjaival; Zwift Click/Play esetén csak a fokozat látszik). A **Tekerés** fülön egy képernyőn
  van minden, ami tekerés közben kell: az edzés blokkja (hátralévő idő, következő blokk, az edző
  üzenete), a teljesítmény a célhoz mérve (célban / kevés / sok), a zónasáv, kadencia, pulzus,
  sebesség, meredekség, alatta a profil. A **Térkép** fülön ugyanez a műszerfal, a térkép és a
  szintgrafikon. A profil nézetei telefonon **oldalra húzással** váltanak (pöttyök mutatják, hol
  vagy; az első húzásig egy „húzd oldalra” tipp segít). A párbeszédablakok telefonon teljes
  képernyősek (az edzésválasztó telefonon két oldal: *Lista* és *Részletek*, a címkék külön gomb
  mögött). Telefonon a felső sávban nincs logó, hogy elférjen az útvonal hossza. Fekvő tableten
  (≥ 1000 px) a megszokott csempés elrendezés marad.
- **Android** – ugyanez az alkalmazás APK-ként is elkészül (lásd *Android* lent). Androidon nincs
  fájlválasztó: útvonalat bikerouter-linkkel, a legutóbbiak közül vagy a bemutató útvonallal
  töltesz be, a TCX-mentés a *Letöltések* mappába kerül.
- **Automatikus frissítés** – induláskor a program megnézi a GitHub legújabb kiadását; ha újabb
  van, a felső sávban megjelenik a **Frissítés** gomb. Windowson letölti az új exe-t, Linuxon
  pedig — ha AppImage-ként indult — az új AppImage-t, kicseréli magát és újraindul. Androidon az
  új APK letöltését nyitja meg (a telepítést a rendszer kéri), .deb-es telepítésnél és macOS-en
  pedig a kiadás oldalát nyitja meg a böngészőben, mert ezeket a csomagkezelő, illetve a Finder
  telepíti, nem a program.
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
  A választóban a kijelölt edzés **zónaszínes blokkvázlata** látszik (minden blokk magassága a
  %FTP, színe a Coggan-zóna, a rámpa trapéz), a **blokklista** alatta pedig blokkonként az időt,
  a célt és az edzői instrukciót. **Saját edzés importálható**: a *Beállítások* melletti
  *Edzés importálása…* gomb MyWhoosh- vagy Zwift-fájlt (`.xml`, `.zwo`) és a MyWhoosh JSON-
  exportját is beolvassa, elmenti a `workouts/` mappába, és onnantól ugyanúgy választható, mint
  a beépített edzések (a „Legutóbbi” és a címkék is működnek vele). Az **edzői üzenetek**
  (a fájlok `textevent`-jei, illetve a beépített edzések blokk-instrukciói és időzített
  megjegyzései) a megfelelő pillanatban az Edzés kártyán jelennek meg, 12 másodpercig.
- **Virtuális váltó** – *Beállítások → Trainer → Virtuális váltó*. 2×12-es hajtás (50/34 lánctányér,
  11–34 fogaskoszorú, 23 különböző fokozat); a `←` / `→` billentyűvel vagy a Váltó panelen váltasz.
  Egy okos trainernek nincs saját váltója, ezért a fokozat a trainerre küldött lejtőt módosítja
  (a legkönnyebb ~−5 %, a legnehezebb ~+5 %): ugyanazon az úton könnyebb vagy nehezebb a letekerés,
  a panel a fokozat áttételét, a kadenciából adódó sebességet és az ellenállás-eltolást mutatja.
  Edzés (ERG) közben a célwatt a döntő: a váltó ilyenkor el sincs, és a váltás nem hat.
- **Tekerés-napló, szellem és szegmensek** – minden befejezett menet automatikusan elmentődik az
  útvonalhoz (1 Hz-es felvétel, összegzés, szegmensidők), és ugyanezen az útvonalon a korábbi
  menet **szellemként** visszajátszható: már a rajt előtt ott vár a rajtvonalon, tekerés közben
  pedig a térképen és a profilon egy áttetsző, saját színű jelölő mutatja, hol járt akkor, amikor
  te most; az előny/hátrány másodpercben (a profilon és az összegzésben is). A *Beállítások →
  Térkép és profil → Szellem* alatt választható **Ki / Az útvonal ideje / Legutóbbi menet /
  Legjobb (PR)**: az „Az útvonal ideje” a betöltött GPX saját időbélyegeit játssza vissza, tehát a
  Strava/Garmin exportból származó track már a legelső menet előtt szellemként versenyez (a
  BRouter-útvonalakban nincs idő, ott ez a mód üres). A „Legjobb” a legkisebb idejű **teljes**
  menet; ha még egy menet sem ért célba, a legtovább jutott (részleges) menetet játssza vissza, és
  a szellem a saját végénél megáll — így az első menet után sem maradsz szellem nélkül. A választó
  alatti sor megmondja, miért nincs éppen szellem: nincs még menet, csak szimulált menet van,
  nincs időbélyeg a GPX-ben, vagy egyik menet sem indult a rajtból. A felismert emelkedők **helyi
  szegmensek**: a rajt előtt a legjobb idő (PR) látszik, benne tekerve pedig a másodpercre mért
  különbség; az összegzés szegmenstáblája menetenként mutatja az időt, a korábbi PR-t és az „Új
  PR!” jelvényt. A PR-idők a mentett menetekből jönnek; a szimulált tekerővel (vagy a `RIDE_DEMO`
  / `RIDE_SIM` hookkal) mentett menet is megmarad, de nem lesz belőle PR és nem lesz „Legjobb”
  szellem.
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
  megmaradnak, és a rendes Firefoxodhoz nem nyúl). **Csak Windowson érhető el:** a panel a Firefox
  ablakát Win32 gyerekablakként építi be, ezért macOS-en és Linuxon nem is jelenik meg (sem
  csempeként, sem chipként). A panel bezárása a Firefoxot is bezárja. A Firefoxnak min. ~500 px
  szélesség kell, ezért a panel nem húzható ennél keskenyebbre. Párbeszédablak megnyitásakor vagy
  panel húzásakor a Firefox ideiglenesen elrejtőzik (a natív ablak fölé a felület nem tud rajzolni).
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

- **Windows 10/11** – [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (Windows 11-en alapból ott van). A debug/release exe mellé a `WebView2Loader.dll` kerül (a
  `ride-app/build.rs` másolja oda).
- **Linux** – 64 bites rendszer glibc 2.35 vagy újabb (Ubuntu 22.04+), GTK 3 és WebKitGTK 4.1
  (`libwebkit2gtk-4.1-0`, `libgtk-3-0`), valamint BlueZ a Bluetooth-hoz. Az AppImage ezeket
  magával hozza, a .deb függőségként kéri. NVIDIA-kártyán vagy Wayland alatt a program maga
  állítja be a `WEBKIT_DISABLE_DMABUF_RENDERER=1`-et (az ismert üres-ablak hiba ellen).
- **macOS 11 (Big Sur) vagy újabb** – Intel és Apple Silicon is (a kiadás univerzális bináris).
  Az első Bluetooth-keresésnél a rendszer engedélyt kér.
- Bluetooth LE adapter csak akkor kell, ha valódi szenzort/trainert használsz.
- Fejlesztéshez Rust (a projekt GNU toolchainen készült és tesztelt, MSVC-vel is fordul).

## Futtatás

```powershell
cargo run -p ride-app                 # debug build + indítás
.\scripts\run.ps1                     # ugyanez, kényelmes kapcsolókkal
.\scripts\run.ps1 -Demo -Sim 250      # demo útvonal + szimulált tekerő 250 W-on
cargo build --release -p ride-app     # kész exe: target\release\bike-osm-ride.exe
```

Linuxon és macOS-en ugyanez `cargo run -p ride-app` (a `scripts\*.ps1` szkriptek PowerShellt
kérnek). Linuxon a fejlesztéshez kell a
`libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev pkg-config` csomag (Debian/Ubuntu:
`sudo apt install …`), macOS-en a Xcode Command Line Tools.

### Android

Kell hozzá: Android SDK + NDK 27, JDK 17+, `rustup target add aarch64-linux-android` és a Dioxus
CLI (`cargo install dioxus-cli@0.7.10 --locked`). Az SDK licenceit egyszer el kell fogadni
(`sdkmanager --licenses`).

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:ANDROID_HOME\ndk\27.2.12479018"
cd crates\ride-app
dx bundle --platform android --package-types apk --release --target aarch64-linux-android
# az APK: target\dx\bike-osm-ride\release\android\app\app\build\outputs\apk\release\app-release.apk
```

Egy APK **egy ABI-t** tartalmaz: telefonra `aarch64-linux-android` kell, az x86_64-es emulátorhoz
ugyanez `--target x86_64-linux-android`-dal (`rustup target add x86_64-linux-android`).

Amit a `dx bundle` elvégez, hogy Rust kódból APK legyen:

| lépés | mi történik |
|---|---|
| 1 | a `ride-app` binárisát keresztfordítja a választott ABI-ra (a linker az NDK clang-ja) |
| 2 | `-Wl,--export-dynamic`-kal linkel, hogy a JNI szimbólumok (`Java_dev_dioxus_main_*`) látszódjanak — enélkül `UnsatisfiedLinkError` |
| 3 | a binárist `app/src/main/jniLibs/<abi>/libmain.so` néven beemeli (ezt tölti be a generált `WryActivity`, `System.loadLibrary("main")`) |
| 4 | legenerálja a Gradle projektet: manifest (`android/AndroidManifest.xml`), activity (`android/MainActivity.kt`), erőforrások |
| 5 | `gradlew assembleRelease`: aapt2 + d8/R8 + apksigner |

Ha a `dx` elakad, vagy gyorsan akarsz iterálni (a Rust-oldal újrafordítása ~10 s, a csomagolás
~5 s), a lépések kézzel is megcsinálhatók:

```powershell
# 1. a natív library (a dx ugyanezt teszi, csak a saját target könyvtárába)
$env:CARGO_TARGET_DIR = "$PWD\target\android-dev"
cargo rustc -p ride-app --target x86_64-linux-android -- `
  -C link-arg=-landroid -C link-arg=-llog -C link-arg=-lOpenSLES -C link-arg=-lc++abi `
  -C link-arg=-Wl,--export-dynamic
Copy-Item target\android-dev\x86_64-linux-android\debug\bike-osm-ride `
  target\dx\bike-osm-ride\release\android\app\app\src\main\jniLibs\x86_64\libmain.so -Force
# 2. újracsomagolás (a Kotlin- és erőforrás-változást is felveszi)
cd target\dx\bike-osm-ride\release\android\app
.\gradlew.bat assembleDebug
```

**Az ikon és a nyitóképernyő** nem a `dx`-ből jön: a `dx` 0.7.10 minden buildnél a saját
sablon-ikonjait (kék Dioxus-jel) és a világos témáját írja a generált projektbe, az `[android] icon`
kulcsot pedig nem használja. Ezért a `dx bundle` **után** a `packaging/android/resoverlay.sh`
kicseréli ezeket a sajátunkra (sötét csempe + teal gerinc ikon, sötét nyitóképernyő a logóval) és
újracsomagolja az APK-t; a CI is ezt futtatja. A képeket a `packaging/android/make-icons.py`
generálja a `packaging/icons/1024.png`-ból (Pillow kell hozzá).

**Aláírás.** Saját kulcs nélkül a Gradle a debug kulccsal írja alá (`CN=Android Debug`): ez
telepíthető, de csak debug-aláírt példány fölé — egy kulccsal aláírt release APK előtt előbb
`adb uninstall hu.borbelyhuba.bikeosmride` kell. A kiadást a `.github/workflows/release.yml` a
`ANDROID_KEYSTORE_BASE64` / `ANDROID_KEYSTORE_PASSWORD` / `ANDROID_KEY_ALIAS` secretekből írja alá.

Telepítés telefonra: másold át az APK-t, és nyisd meg (engedélyezni kell az ismeretlen forrásból
származó alkalmazások telepítését), vagy USB-n: `adb install -r <apk>`.

Androidon a **GPX-fájl a rendszer saját fájlválasztójával** nyílik meg (Storage Access Framework): a
kiválasztott dokumentumot az alkalmazás a saját gyorsítótárába másolja, onnan tölti be az útvonalat.
A Bluetooth-hoz az első indításkor megkérdezi a szükséges engedélyeket (`BLUETOOTH_SCAN`/`_CONNECT`,
Android 11 és régebbi rendszeren a helymeghatározást); ha ezt elutasítottad, az Eszközök ablak
„Keresés indítása” gombja újra kérdez. Hardver nélkül a szimulált tekerővel lehet tekerni.

Ha az alkalmazás összeomlik, a hiba oka nem tűnik el: a szöveg (veremmel együtt) a
`/data/data/hu.borbelyhuba.bikeosmride/files/crash.log` fájlba kerül, ugyanaz megy a logcatbe, és a
képernyő tetején egy piros sáv mutatja az első sorát (a „Rendben” gombbal eltüntethető — az
alkalmazás utána is használható marad, mert a kezelőréteg elkapja a hibát). Lekérdezés USB-n:

```bash
adb logcat -s bike-osm-ride          # élőben, összeomlás közben is
adb shell run-as hu.borbelyhuba.bikeosmride cat files/crash.log   # csak debug buildnél
```

Ez azért kell, mert Androidon nincs konzol: egy Rust-pánik nyom nélkül viszi le a folyamatot, és a
`dioxus-desktop` Android-háttere egyetlen globális mutex alatt futtat minden eseményt és
újrarajzolást — ha a pánik elszökik, az *első* hiba után minden további koppintás is elszáll.

### Linux (AppImage és .deb)

A kiadás két csomagot tartalmaz, mindkettő Ubuntu 22.04-en (glibc 2.35) épül, hogy régebbi
disztribúciókon is elinduljon.

```bash
# AppImage: egyetlen futtatható fájl, nem kell telepíteni
chmod +x bike-osm-ride-0.3.0-linux-x86_64.AppImage
./bike-osm-ride-0.3.0-linux-x86_64.AppImage

# .deb: a rendszer csomagkezelőjével
sudo apt install ./bike-osm-ride-0.3.0-linux-amd64.deb
```

A .deb a `libwebkit2gtk-4.1-0`, `libgtk-3-0` és `bluez` csomagokat kéri; az AppImage ezeket
magában hozza. Ha az AppImage-t letöltés után közvetlenül futtatod, és a rendszer nem engedné,
a `chmod +x` a megoldás.

Helyi build:

```bash
cargo build --release -p ride-app     # target/release/bike-osm-ride
cd crates/ride-app
dx bundle --platform linux --package-types appimage --package-types deb --release
```

Ha a `dx` linuxos bundlere nem működik, a `packaging/linux/fallback.sh <verzió>` ugyanezt a két
fájlt állítja elő `dpkg-deb`-bel és `linuxdeploy`-jal (GTK plugin).

### macOS (univerzális .app, .dmg-ben)

A `.dmg` egy univerzális (Intel + Apple Silicon), **aláíratlan** alkalmazást tartalmaz, ezért az
első indítás így megy:

1. Másold a `bike-osm-ride.app`-ot az *Alkalmazások* mappába.
2. Nyisd meg **jobb klikk → Megnyitás → Megnyitás**. (A Gatekeeper karanténja miatt a dupla
   kattintás elutasítja; utána már rendesen indul.) Parancssorból ugyanez:
   `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`
3. Az első Bluetooth-keresésnél macOS engedélyt kér a Bluetooth használatára — engedélyezd
   (*Rendszerbeállítások → Adatvédelem és biztonság → Bluetooth*). Engedély nélkül a program a
   szenzorokat nem látja.

Helyi build (a szkript Apple Siliconon az Intel felét is keresztfordítja, majd `lipo`-val összefűzi):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
packaging/macos/bundle.sh 0.3.0
# dist/bike-osm-ride.app és dist/bike-osm-ride-0.3.0-macos-universal.dmg
```

### Kiadás (GitHub)

A `.github/workflows/release.yml` (egyelőre kézzel indítva: *Actions → release → Run workflow*, egy
`v*` címkén) futtatja a folyamatot: tesztek mindhárom platformon (Windows, Linux, macOS), majd a
csomagok, végül GitHub Release az alábbi fájlokkal — pontosan ezeket keresi a beépített frissítő,
ezért az átnevezésük az `updater.rs`-t is érinti:

| Fájl | Platform | Telepítés |
|---|---|---|
| `bike-osm-ride-<verzió>-windows-x64.exe` | Windows | MSVC, DLL nélkül futtatható exe |
| `bike-osm-ride-<verzió>-android.apk` | Android | APK |
| `bike-osm-ride-<verzió>-linux-x86_64.AppImage` | Linux | AppImage (ezt frissíti magától) |
| `bike-osm-ride-<verzió>-linux-amd64.deb` | Linux | Debian-csomag |
| `bike-osm-ride-<verzió>-macos-universal.dmg` | macOS | univerzális .app DMG-ben |

A Linuxos feladat a csomagolás előtt egy 20 másodperces füsttesztet is futtat (`xvfb` alatt,
`RIDE_DEMO=1`-gyel): ha az alkalmazás magától kilép, a kiadás elbukik. A macOS-es feladat
ellenőrzi, hogy az `Info.plist`-ben ott van-e a Bluetooth-engedély szövege, és hogy a bináris
valóban univerzális-e. A frissítő a `Cargo.toml` `repository` mezőjéből tudja a repót.

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
4. **Tekerés.** `Kezdés` (vagy `Space`): a fizika a mért teljesítményből számol sebességet (Martin
   és mtsai. 1998-as, közúti mérésekkel validált modellje: gravitáció, gördülés, csapágysúrlódás,
   légellenállás küllőkkel, tehetetlenség; a nyílt forrású `rouleur` implementációval tesztelve), a
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
   Az összegzőben kitöltheted, amit a Strava is kér (cím, leírás, virtuális túra vagy sima
   kerékpározás, hogy esett, privát jegyzet, ki láthatja, hírfolyam-némítás), alul pedig kétféleképp
   zárhatod: `Csak mentés TCX-ként`, vagy `Feltöltés Stravára`. A cím üresen a napszakból lesz
   (pl. „Délutáni edzés").
   Asztali gépen a feltöltés a háttérben megy a Strava saját feltöltő oldalán át (API-kulcs és
   Strava-előfizetés nem kell): egyszer be kell jelentkezned a Stravába (a program a jelszavadat
   nem látja, a belépés megmarad), utána egy rejtett ablak tölti fel a túrát a megadott adatokkal.
   Ha a Strava oldala megváltozik és valami nem megy, az ablak megjelenik, a TCX-másolat pedig
   a `%LOCALAPPDATA%\bike-osm-ride\strava` mappában marad (a hibanapló is ott van).
   Androidon a gomb a Strava appnak adja át a TCX-et (Android 10+), a címet és a többit ott
   kell megadni.

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

Minden egyetlen mappában van — a beállítások, a legutóbbi útvonalak, a csempe-gyorsítótár és a
webview profilja is:

| Platform | Mappa |
|---|---|
| Windows | `%LOCALAPPDATA%\bike-osm-ride\` |
| macOS | `~/Library/Application Support/bike-osm-ride/` |
| Linux | `$XDG_DATA_HOME/bike-osm-ride/`, alapból `~/.local/share/bike-osm-ride/` |
| Android | `/data/data/<csomagnév>/files/` (az app privát mappája) |

Alatta: `settings.json` (beállítások), `recent.json` (legutóbbi útvonalak), `routes/` (a megnyitott
útvonalak feldolgozott geometriája, hogy legközelebb ne kelljen újra feldolgozni), `rides/`
(a tekerés-napló: útvonalanként egy mappa, benne menetenként egy `<kezdés>.json` a teljes 1 Hz-es
felvétellel és egy `index.json` az összegzésekkel; a menetek nem törlődnek), `workouts/`
(az importált edzések, edzésenként egy JSON), `tiles/`
(csempe-gyorsítótár, 30 nap), `streetview/<útvonal-azonosító>.json` (csak ha bekapcsoltad a
panoráma-azonosítók mentését: ~25 m-es útvonalszakaszonként az azonosító, az irány és az időbélyeg;
a kulcs nincs benne), `streetview/img/<útvonal-azonosító>/` (csak a letöltött Street View képek és
a hozzájuk tartozó `manifest.json`; a képek 30 napig érvényesek, a Beállításokban törölhetők),
`ble.log` (ha kéred) és `webview/` (a webview profilja). A Google API-kulcs a `settings.json`-ban
van, csak ezen a gépen. A `RIDE_DATA_DIR` környezeti változóval az egész mappa áthelyezhető.

## Fejlesztés

```
crates/
  ride-model        a közös adatmodell (Route, RideSnapshot, Settings, …)
  ride-route        GPX/bikerouter → Route, emelkedők, perzisztencia, tekerés-napló
  ride-sim          fizika, RideEngine, statisztika, felvétel, szellem, szegmensek, TCX
  ride-ble          BLE hub (FTMS/CPS/CSC/HR) + szimulált tekerő
  ride-ui-kit       design system: tokenek, paletta, ikonok, formázás, protokoll-URL-ek
  ride-ui-map       csempés térkép, útvonal, bringás jelölő
  ride-ui-profile   magasságprofil + élő grafikonok
  ride-ui-hud       műszercsempék, statisztika, emelkedő- és edzéskártya, vezérlők
  ride-ui-dialogs   nyitóképernyő, eszközök, beállítások, edzésválasztó, összegzés, névjegy
  ride-ui-streetview Street View panel: metadata-lekérdezés, azonosító-gyorsítótár, offline képletöltés
  ride-app          shell: állapot, vezérlőciklus, csempe-proxy, elrendezés
packaging/
  icons/            a program ikonjai (32…1024 px), a src/icon.rs-ből generálva
  linux/            .desktop fájl és a `dx` helyetti tartalék csomagoló szkript
  macos/            Info.plist és bundle.sh (univerzális .app + .dmg)
```

- `cargo test --workspace` – a teljes tesztkészlet (fizika-referenciaértékek, GPX-pipeline,
  BLE-parserek differenciáltesztjei a `ftms` crate-pel szemben, hub-viselkedés fake átvitellel,
  UI-segédfüggvények).
- `cargo check -p <crate>` / `cargo clippy -p <crate>` – crate-enként.
- `cargo run -p ride-app --example export_icon` – újrarajzolja a `packaging/icons/*.png` ikonokat
  a `crates/ride-app/src/icon.rs` kódjából (ugyanaz a függvény, ami az ablak ikonját rajzolja).
  A PNG-k commitolva vannak; csak akkor kell futtatni, ha az ikon rajzolása változik.
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

**Ugyanez Linuxon és macOS-en.** Ezek a buildek ugyanazt a kódot fordítják, egyetlen különbséggel:
a `wry` a nem-Windowsos célokon a `kuchikiki` HTML-parseolón keresztül behúzza a **MPL-2.0**
licencű `cssparser`, `selectors` és `dtoa-short` crate-eket, a `tray-icon` → `dirs` láncon pedig az
`option-ext`-et. A Windowsos buildbe ezek nem kerülnek be, a Linuxba és a macOS-be igen. Ez a
projekt licencét nem érinti: az MPL-2.0 **fájlszintű** copyleft, a crate-eket módosítás nélkül,
a crates.io-ról vesszük (nem vendoroljuk őket), és a saját kódunk továbbra is `MIT OR
Apache-2.0`. Az MPL csak a módosított MPL-fájlok közzétételét írná elő; ilyen nincs.

A `Cargo.lock` minden platform függőségét tartalmazza (635 csomag, ezt listázza a
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md)); a lista újragenerálása:
`.\scripts\third_party_licenses.ps1`.

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
