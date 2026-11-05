# Változásnapló

Minden jelentős változás verziónként, a [Keep a Changelog](https://keepachangelog.com/hu/1.1.0/) szellemében, és a
projekt a [szemantikus verziózást](https://semver.org/lang/hu/) követi (még 0.x: a felület és a fájlformátumok változhatnak,
de a beállítás-, útvonal- és tekerés-fájlok visszafelé kompatibilisek maradnak). A tartalom a commitüzenetekből készült;
a dátumok a commitok dátumai.

A **v0.2.0** és a **v0.2.2** címke megvan a tárolóban, de nincs hozzá kiadás: a kiadási futásuk elbukott, a tartalmuk a
következő verzióban jelent meg (0.2.1, illetve 0.2.3). Egy már kitett címkét nem mozgatunk; a javítás új patch-verzió.

## [Kiadatlan]

### Javítva
- **Android: a „Mentés TCX-ként” nem hagyott használható fájlt.** A fájl a Letöltések mappába próbált közvetlenül írni,
  amit az Android 11+ nem enged, ezért az app saját, kívülről nem látható mappájába került, a jelzés pedig „Mentve” volt.
  Most megnyílik a rendszer „Mentés másként” ablaka (mint a biztonsági mentésnél), és te választod ki a helyet.
- **Eszközök ablak: a szimulált tekerő is „Kapcsolódás…”-t mutatott, ha egy valódi eszközhöz csatlakoztál.** A szimulált
  tekerő minden szerepet felkínál, a sor pedig szerep alapján nézte, hogy folyik-e csatlakozás. Most az eszköz neve is számít.
- **Android: hosszabb csatlakozási időkorlát** (30 mp a korábbi 12 helyett, a szolgáltatás-felderítésé 12 a korábbi 6
  helyett): az Android saját közvetlen csatlakozása is kb. 30 mp után ad fel, és a btleplug maga is újrapróbálja.
- **Android: a keresés hiba nélkül sem talál semmit.** A `BLUETOOTH_SCAN` engedély `neverForLocation` jelzés nélkül volt
  a jegyzékben, ilyenkor az Android 12+ helyengedély híján némán üres keresési eredményt ad. A jelzés most ott van, és a régi
  (Android 11-ig) `BLUETOOTH` / `BLUETOOTH_ADMIN` engedély is. Ha egy sikeresen elindult keresés 30 másodpercig egyetlen
  eszközt sem lát, az app egyszer kiírja, mit érdemes ellenőrizni (Bluetooth, „Közeli eszközök” engedély, Androidon 11-ig a
  helymeghatározás).
- **Android: „A BLE-napló nem nyitható meg: Permission denied (os error 13)”.** Az app adatmappáját eddig csak a
  `/data/data/<csomag>/files` útvonalból találtuk ki, ami másodlagos felhasználónál, munkaprofilnál, Secure Foldernél vagy
  „kettős alkalmazásnál” nem a saját mappánk. Most az Activity `getFilesDir()` válasza az első, a kitalált útvonalak
  (`/data/user/<n>/…`, `/data/data/…`) csak tartalékok, és mindegyiket írási próba előzi meg. A hibaüzenet a fájl útvonalát
  is kiírja.
- **Android: „Bluetooth nem érhető el: failed to resolve java class … Adapter class not found”.** Két oka volt. A btleplug
  Java oldala nem került bele az APK-ba: a csomagoló szkript (`packaging/android/resoverlay.sh`) most bemásolja a
  Gradle-projektbe a használt btleplug-verzió Java forrását, és R8-szabályt is ad hozzá, hogy a kiadási build ki ne dobja
  az osztályokat. Ez egymagában nem volt elég (a telefonon ugyanaz a hiba jött): a Bluetooth-szál natív szál, és a
  rendszer class loaderével kereste az osztályokat, amely az app saját osztályait nem látja. Az indítás most az
  Activity class loaderét adja a szálnak.

## [0.4.0] – 2026-11-05

### Új
- **Edzések áttekintése** (`Áttekintés` gomb; telefonon az `Útvonal` ablak alján): teljes képernyős oldal, ahol egy útvonal
  mentett edzései (legfeljebb négy) összehasonlíthatók. Grafikon a távolság függvényében: teljesítmény, pulzus, kadencia,
  sebesség és az első edzéshez mért időkülönbség, közös kurzorral. Ráközelítés egy szakaszra a profilon vagy a grafikonon
  húzással, illetve az emelkedő-gombokkal: a térkép kiemeli és megmutatja a szakaszt, a grafikon és a szakasztábla (idő,
  sebesség, átlag- és NP-watt, pulzus, kadencia, emelkedés, VAM, időkülönbség) ugyanarra szűkül. Edzés törölhető.
- **Edzésimport**: más alkalmazás (Strava, Garmin, Wahoo) **GPX-** vagy **TCX-**fájlja, pulzussal, kadenciával és wattal.
  A nyomot az útvonalra vetítjük (30 m-en belüli pontok, a kitérők kimaradnak, az automatikus szünet nem számít az időbe), így
  az edzés ugyanazon a távolságtengelyen él, mint a többi. A fájl kézzel adott (tallózás vagy ráhúzás), semmi nem megy ki a
  gépről. Az importált edzés nem lesz PR és nem lesz szellem.
- **Becsült watt**: ha a fájlban nincs teljesítmény, a szimuláció fizikáját visszafelé futtatjuk (energiamérleg egy 9 s-os
  ablakra; a lejtőt az útvonal adja). A számolás a saját adataiddal vagy egyéni értékekkel történik (testsúly, kerékpár,
  gördülési ellenállás; a CdA, a hatásfok és a levegősűrűség a Beállításokból), az eredmény `~` jelű, és újraszámolható.
  Mért wattot soha nem írunk felül.
- Új, négyszínű adatpaletta a sorozatokhoz (`--series-1…4`, mindkét témában ellenőrizve).
- **Teljesítmény oldal** (az áttekintésben az *Összehasonlítás | Teljesítmény* váltó): a legjobb átlagos watt 5 mp-re, 1, 5, 20 és
  60 percre az összes edzésedből; az FTP-javaslat (a legjobb 20 perc 95 %-a) egy gombbal beállítható; a terhelés (TSS és idő) az
  utolsó 12 hétre vagy hónapra. Csak mért watt számít: a szimulált és a becsült wattú edzések kimaradnak. A régebbi
  edzések számait az első megnyitáskor számoljuk ki (`RideMeta.tss`, `RideMeta.mmp`), az *FTP-teszt 20 perc* leírása
  hivatkozik rá.
- **Biztonsági mentés** (*Beállítások → Biztonsági mentés*): egyetlen zip az útvonalakról, a mentett túrákról, az importált
  edzésekről és a beállításokról. A visszaállítás csak összefésül, semmit nem töröl és meglévő túrát nem ír felül. A
  Street View-kulcs, a csempék, a képek és a naplók nincsenek benne; a kulcs a visszaállításnál sem jön a fájlból. Hibás,
  túl nagy vagy gyanús nevű fájlt az app előre elutasít, és semmit nem ír. Androidon a rendszer fájlválasztója
  (Storage Access Framework) kezeli; telefonon még nem próbáltuk ki.
- **Androidon a kijelző bekapcsolva marad tekerés közben** (`FLAG_KEEP_SCREEN_ON`). Szünetben és a túra végén a rendszer
  saját időkorlátja él újra. Telefonon még nem próbáltuk ki.

### Javítva
- A telefonos felső sáv „Teljes képernyő” gombja nem tűnt el, ahogy kellett volna: az ékezetes `aria-label`-re illesztő CSS
  nem talált. Ékezetmentes osztályra váltottunk.

### Fejlesztőknek
- Új crate: `ride-ui-review`. Új modulok: `ride-sim::{analysis, estimate, compare, power_curve}`, `ride-route::{activity, matching, import, backup}`.
- Új függőség: `zip` 4.6 (MIT), csak a `deflate-flate2-zlib-rs` funkcióval, a `flate2` már a fában volt.
- `RideRecord` és `RideMeta` additívan bővült (`source`, `power_estimated`, `estimate`, átlagok, útvonalnév); a régi fájlok
  változatlanul betöltődnek.
- `RIDE_REVIEW=1` hook; `cargo run -p ride-route --example seed_review -- <mappa> [útvonal.gpx]` mintaedzéseket tölt egy
  adatmappába; a `readme_shots.mjs` új `review` és `review-phone` forgatókönyve.

### Eltávolítva
- **Strava-feltöltés**: az asztali rejtett WebView-s feltöltés, az Android-megosztás, a „Feltöltés Stravára” gomb, a Strava-sáv
  és az összegzőben a Strava adatait kérő űrlap (cím, leírás, sport, jegyzet, láthatóság). Ok: a bejelentkezést minden tekerés
  után újra el kellett végezni, így a funkció nem volt használható. A TCX-mentés marad (`Mentés TCX-ként`, korábban „Csak mentés
  TCX-ként”): a fájl kézzel feltölthető a Stravára, az Intervals.icu-ra vagy a TrainingPeaks-re.
- A már létrejött `strava` és `strava-webview` mappa az adatmappában ilyenkor feleslegessé válik; a program nem törli, kézzel
  törölhető (a `strava-webview` a Strava-bejelentkezési profilt tartalmazza).

## [0.3.1] – 2026-11-05

### Felület
- **Telefonos Térkép fül:** a térkép, a teljesítménysor és a magassági profil most ugyanúgy keretes csempe 8 px
  margóval és réssel, mint a Tekerés és a többi fül; eddig ezen a fülön teljes szélességű, keret nélküli sávok voltak.

### Dokumentáció
- Újraírt, képernyőképes README; új `CLAUDE.md`, `CHANGELOG.md`, `docs/README.md` (index), `USER_GUIDE`, `PRIVACY`,
  `DEVELOPMENT`, `TESTING`, `BLUETOOTH`, `SIMULATION`, `ROUTES_AND_DATA`, `UI`, `ANDROID`, `RELEASING`,
  `STREETVIEW`, `DECISIONS`; az `ARCHITECTURE` és a `DESIGN` frissítve.
- `scripts/readme_shots.mjs`: a képernyőképek újragenerálása tiszta adatmappából.

## [0.3.0] – 2026-11-03

### Android
- **Rendszer-fájlválasztó** (Storage Access Framework): a GPX a telefon bármely helyéről betölthető; a kiválasztott
  dokumentumot a program az alkalmazás gyorsítótárába másolja.
- **Bluetooth** Androidon: a `btleplug` Android-hátterét inicializálja a program, és kéri a futásidejű
  Bluetooth-engedélyeket; a hub nem hal el többé „Droidplug has not been initialized” hibával, a szimulált tekerő
  adapter nélkül is megmarad. *(Valódi eszközzel még nem próbáltuk.)*
- **A láncreakció gyökere megszűnt:** a TLS-ellenőrző (`rustls-platform-verifier`) Androidon pánikolt az eseményhurokban, a
  `dioxus-desktop` pedig minden eseményt egyetlen globális zár alatt futtat, így az első hálózati hívás (a frissítés-
  ellenőrzés, öt másodperccel az indulás után) után minden koppintás, ablak és fül is összeomlott. Androidon a program most a
  beágyazott Mozilla-gyökértanúsítványokkal ellenőriz.
- **Összeomlás-jelentés:** az elkapott pánik a `files/crash.log`-ba, a logcatbe és egy egysoros piros sávba kerül; a kezelők
  nem engednek ki pánikot, az alkalmazás használható marad.
- A `configChanges` minden konfigurációváltozást felsorol (téma, betűméret, nyelv, sűrűség), így ezek nem építik újra a
  `NativeActivity`-t (amit a folyamat nem élne túl). `adjustResize` és „kattintás a mezőn kívülre” szkript: a billentyűzet nem
  takarja el az ablakot.
- **Saját ikon és indítóképernyő** (a `dx` minden csomagolásnál a saját sablonját írja be).

### Felület
- Telefonon a felső sáv 52 px magas, az ikongombok 46 pxesek (ujjnyi méret).
- Ablakok: a lábléc telefonon kétoszlopos rács; a felesleges alsó *Bezárás* gomb mindenhol megszűnt (minden ablaknak
  van ×-e, és azonnal ment).
- „Talált eszközök”: a műveletgombok saját sorba kerülnek, nem fedik az eszközt.
- A térkép, ha követi a bringást, utcaszinten a bringásra nyílik meg, nem az egész útvonalra.

## [0.2.3] – 2026-11-03 *(a 0.2.2 tartalmával)*

### Javítva
- A Linux-AppImage lépés a kiadásban: a `linuxdeploy` már csak olyan ikont fogad el, amelynek neve megegyezik a `.desktop`
  fájl `Icon=` értékével.

### Új a 0.2.2-ben
- Egy **panel elválasztójának húzása csak a két szomszédos panelt** méretezi (az azonos irányú, egymásba ágyazott
  elválasztók a helyükön maradnak; a minimális méreten lévő panel eltolja a következőt).
- **Külön elrendezés és profilmód edzéssel és anélkül** (`tiles` / `tiles_workout`).
- **Szövegméret** (80–160 %): minden `--t-*` betűméret szorzódik; nagy szövegnél a telefonos fejegység négy száma 2 × 2-ben áll.
- Telefon: nincs logó a felső sávban; az **edzésválasztó két oldal** (Lista / Részletek), a kategóriák sávban, a címkék
  gomb mögött; az Adatok fül egy tartalom-magasságú oszlop; a Grafikonok fül kitölti a képernyőt.
- Az edzésdiagramok fejteret kapnak a legnehezebb blokk fölött; az ablakok jobban kihasználják a magas képernyőt.

## [0.2.1] – 2026-11-03 *(a 0.2.0 tartalmával)*

### Javítva (a 0.2.0 kiadási futásához képest)
- A `packaging/icons` PNG-i újragenerálva (egy sorvég-tisztítás levágta a PNG-aláírás CR-jét); a `.gitattributes` a képeket és
  betűtípusokat binárisnak jelöli.
- A Linux tartalék csomagoló abszolút kimeneti könyvtárat használ; az Android-lépésben a `setup-android` csak a
  `platform-tools`-t telepíti; a térképteszt és a Street View-teszt platformfüggetlen lett.

### Új a 0.2.0-ban
- **Tekerés-képernyő:** négyszer finomabb tiling rács (96 × 64 cella); a profil-panel *Magasság | Edzés* kapcsolója (az edzésnézet
  a blokkokat, a zónákat, az FTP-vonalat, a lovas helyét és a következő blokkig hátralévő időt mutatja); az előretekintő
  magassági tengely már nem ugrál (egy tesztútvonalon 94 átskálázás helyett 3); a GPX-ben lévő **nevesített szegmensek**
  (a főútvonalon fekvő extra track vagy `<type>segment</type>`) sávként a profilon; a térképgombok a sarkokba kerültek.
- **Edzések:** 943 időzített **edzői üzenet** a beépített könyvtárhoz (az edzéskártyán/telefonos fejegységen, felugró
  üzenet nélkül); a MyWhoosh/ZWO `textevent`-ek minden időzítéssel beolvasódnak; a virtuális váltó edzés (ERG) közben el van
  rejtve és nem hat.
- **Telefon (kompakt elrendezés):** *Tekerés / Térkép / Grafikonok / Adatok* fülek; a Tekerés fül egyetlen fejegység
  (edzésblokk, teljesítmény a célhoz mérve, zónasáv, négy mező) a profil fölött; a profil nézetei oldalra húzással váltanak;
  48 px-es gombok, váltógombok; a felső sáv a biztonsági sávval kitöltve; az Android célverzió 34-re rögzítve.
- **Szimuláció:** a teljes Martin és mtsai. 1998-as modell (csapágysúrlódás, küllők légellenállása), az open-source `rouleur`
  implementációval keresztbe ellenőrizve.
- A kiadással együtt kerültek be a korábbi munkák: **tekerés-napló, szellem és helyi szegmensek PR-rel** (összehasonlítás az
  összegzésben); **MyWhoosh/Zwift edzésimport**; **Strava-feltöltés** (asztalon WebView, Androidon megosztás) és a
  *Ride details* űrlap; Linux (AppImage, `.deb`) és macOS (univerzális `.dmg`) csomagolás és CI-lépések; frissítő minden platformra.

## [0.1.1] – 2026-11-01

### Javítva
- **Olyan FTMS-trainerek vezérlése, amelyek soha nem erősítik meg a vezérlést** (látott eset: VE200): három megválaszolatlan
  *Request Control* után a program feltételezi a vezérlést, és megerősítés nélkül küldi a parancsokat; a megtagadott *Start*,
  meredekség- és ERG-írást (ATT `0x11`) az írási szünet után újrapróbálja, nem a következő életben tartásnál. Az
  egyértelmű elutasítás továbbra is elutasítás.
- A **BLE hexnapló mindig íródik** (`ble.log`).
- A Windows-exe a C futtatókörnyezetet statikusan linkeli a CI-ben, így nem kell mellé DLL.

## [0.1.0] – 2026-11-01

Az első kiadás: virtuális tekerés GPX-útvonalakon BLE okos trainerrel (Dioxus desktop, Rust).

- **Tiling tekerés-képernyő** és kompakt telefon/tablet elrendezés, OSM-térkép, magassági profil, élő műszerek.
- **93 bizonyítékokra épülő, szerkezetes edzés** ERG-móddal és címkeszűrőkkel.
- **Virtuális váltó** Zwift Click/Play léptetéssel.
- **Street View** (saját Google-kulccsal), **Android-build**, a **GitHub-kiadásokat figyelő frissítő**.
- FTMS-trainer, teljesítménymérő, sebesség/kadencia-szenzor, pulzusmérő és szimulált tekerő; TCX-export.
