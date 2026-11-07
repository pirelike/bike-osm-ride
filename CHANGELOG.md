# Változásnapló

Minden jelentős változás verziónként, a [Keep a Changelog](https://keepachangelog.com/hu/1.1.0/) szellemében, és a
projekt a [szemantikus verziózást](https://semver.org/lang/hu/) követi (még 0.x: a felület és a fájlformátumok változhatnak,
de a beállítás-, útvonal- és tekerés-fájlok visszafelé kompatibilisek maradnak). A tartalom a commitüzenetekből készült;
a dátumok a commitok dátumai.

A **v0.2.0** és a **v0.2.2** címke megvan a tárolóban, de nincs hozzá kiadás: a kiadási futásuk elbukott, a tartalmuk a
következő verzióban jelent meg (0.2.1, illetve 0.2.3). Egy már kitett címkét nem mozgatunk; a javítás új patch-verzió.

## [0.5.0] – 2026-11-07

### Új
- **Spanyol felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető spanyolul: a
  Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”, „Magyar”, „Español”). Az „Automatikus” a készülék
  nyelvét használja, ha annak a szövege hiánytalan; különben magyarul indul, hogy senki ne lásson félkész felületet.
  A szövegek nyelvi katalógusokban élnek (`crates/ride-i18n/locales/`), a magyar a forrás. A spanyol fordítás
  végig „ciclista” (soha nem „lovas”), a mértékegységek és a márkanevek (FTP, ERG, TCX, GPX, BRouter, Open-Meteo,
  Street View) változatlanok. Részletek: `docs/I18N.md`.
- **A spanyol edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és
  minden edzőüzenete (2 065 szöveg). A fordítás a magyar szövegekhez kötött térképből generálódik
  (`workouts.es.map.json` → `workouts.es.json`), és a generátor nem ír ki félkész fájlt; egy teszt
  őrzi, hogy a fordítás ne változtasson egyetlen blokk hosszán, célján vagy számán sem. Az edzéslista
  spanyolul a spanyol nevek szerint rendeződik.
- **A nyelvváltás azonnal érvénybe lép**, a mentett nevek (útvonal, tekerés) nem követik: azok maradnak, ahogy
  elnevezted őket.
- **A felhasználói dokumentumok is elérhetők spanyolul**: a README mellett a teljes kézikönyv
  (`docs/i18n/es/USER_GUIDE.md`) és az adatvédelmi lap (`docs/i18n/es/PRIVACY.md`). A magyar dokumentumok
  tetején hivatkozás mutat a spanyol változatra.
- **Angol felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető angolul is:
  a Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”, „Magyar”, „English”, „Español”). A spanyol
  mellé a teljes katalógus elkészült (`crates/ride-i18n/locales/en.json`, 1 055 szöveg), a mértékegységek és a
  márkanevek változatlanok, a „tekerő” végig „rider” (soha nem „horse rider”). Az angol egyben a fordítási sor
  köztes nyelve: a következő nyelvek innen indulnak. Részletek: `docs/I18N.md`.
- **Az angol edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és minden
  edzőüzenete (2 065 szöveg) a `workouts.en.map.json` térképből generálva (`workouts.en.json`). Ugyanaz a generátor
  és ugyanaz a teszt őrzi, mint a spanyolnál: blokk-hossz, cél és darabszám nem változhat.
- **A felhasználói dokumentumok angolul is elérhetők**: `docs/i18n/en/README.md`,
  `docs/i18n/en/USER_GUIDE.md` és `docs/i18n/en/PRIVACY.md`. A magyar dokumentumok tetején hivatkozás mutat rájuk.
- **A Linuxos asztali bejegyzés angol leírást is kapott** (`Comment[en]` a `.desktop` fájlban).
- **Portugál felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető
  portugálul (európai portugál): a Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”,
  „Magyar”, „English”, „Español”, „Português”). A katalógus hiánytalan (`crates/ride-i18n/locales/pt.json`,
  1 066 szöveg), a „tekerő” végig „ciclista” (soha nem „cavaleiro”), a mértékegységek és a márkanevek
  változatlanok. A brazil alakok (`você`, `arquivo`, `salvar`, `excluir`, `tela`, `configuração`) ellen a
  katalógus-teszt is őrködik. Részletek: `docs/I18N.md`.
- **A portugál edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és
  minden edzőüzenete (2 065 szöveg) a `workouts.pt.map.json` térképből generálva (`workouts.pt.json`).
  Ugyanaz a generátor és ugyanaz a teszt őrzi, mint a spanyolnál és az angolnál: blokk-hossz, cél és
  darabszám nem változhat. Az edzéslista portugálul a portugál nevek szerint rendeződik.
- **A felhasználói dokumentumok portugálul is elérhetők**: `docs/i18n/pt/README.md`,
  `docs/i18n/pt/USER_GUIDE.md` és `docs/i18n/pt/PRIVACY.md`; a portugál README `## Licença` szakasza a
  magyar `## Licenc` szakaszt tükrözi, a licencnevek és SPDX-azonosítók változatlanok. A magyar
  dokumentumok tetején hivatkozás mutat rájuk.
- **Csomagolás portugálul**: `Comment[pt]` a Linuxos `.desktop` bejegyzésben, `pt.lproj/InfoPlist.strings`
  a macOS-bundle Bluetooth-kérdéséhez, `values-pt/strings.xml` az Android-hoz, a `Dioxus.toml`
  egynyelvű mezői mellett pedig kommentben a portugál szöveg. Az Android-összeállítás és a macOS futás
  ebben a környezetben nem volt kipróbálható: ezek **nem ellenőrzöttek**, és az anyanyelvi lektorálás is
  hátravan.
- **Német felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető
  németül: a Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”, „Magyar”, „English”,
  „Español”, „Português”, „Deutsch”). A katalógus hiánytalan (`crates/ride-i18n/locales/de.json`,
  1 064 szöveg), a „tekerő” végig „Fahrer” (soha nem „Reiter” vagy „Pferd”), a megszólítás végig „du”
  (soha nem „Sie”), a tizedesvessző és a százalékjel előtti szóköz a német helyesírást követi, a
  mértékegységek és a márkanevek változatlanok. A `de` oszlop a `GLOSSARY.tsv`-ben és a `STYLE.tsv`-ben
  is ott van, a katalógus-teszt pedig a megszólítást és a ló szavakat is őrzi. Részletek: `docs/I18N.md`.
- **A német edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és
  minden edzőüzenete (2 065 szöveg) a `workouts.de.map.json` térképből generálva (`workouts.de.json`).
  Ugyanaz a generátor és ugyanaz a teszt őrzi, mint a többi nyelvnél: blokk-hossz, cél és darabszám nem
  változhat. Az edzéslista németül a német nevek szerint rendeződik.
- **A felhasználói dokumentumok németül is elérhetők**: `docs/i18n/de/README.md`,
  `docs/i18n/de/USER_GUIDE.md` és `docs/i18n/de/PRIVACY.md`; a német README `## Lizenz` szakasza a
  magyar `## Licenc` szakaszt tükrözi, a licencnevek és SPDX-azonosítók változatlanok. A magyar
  dokumentumok tetején hivatkozás mutat rájuk.
- **Csomagolás németül**: `Comment[de]` a Linuxos `.desktop` bejegyzésben, `de.lproj/InfoPlist.strings`
  a macOS-bundle Bluetooth-kérdéséhez, a `Dioxus.toml` egynyelvű mezői mellett pedig kommentben a német
  szöveg. Androidhoz nem kellett külön német erőforrás: az egyetlen felhasználónak látszó szöveg ott a
  terméknév. Az Android-összeállítás és a macOS futás ebben a környezetben nem volt kipróbálható: ezek
  **nem ellenőrzöttek**, és az anyanyelvi lektorálás is hátravan.
- **Kínai felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető
  egyszerűsített kínaiul (zh-Hans): a Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”,
  „Magyar”, „English”, „Español”, „Português”, „Deutsch”, „Italiano”, „简体中文”). A katalógus hiánytalan
  (`crates/ride-i18n/locales/zh.json`, 1 057 szöveg), a „tekerő” végig „骑行者” (soha nem „骑马 / 马术 /
  赛马”), a megszólítás végig „你” (soha nem „您”), a tizedespont és a százalékjel előtti szóköz nélküli
  írás a kínai helyesírást követi, a mértékegységek és a márkanevek változatlanok. A `zh` oszlop a
  `GLOSSARY.tsv`-ben és a `STYLE.tsv`-ben is ott van, a katalógus-teszt pedig a megszólítást, a ló
  szavakat és a hagyományos írásjeleket is őrzi. Részletek: `docs/I18N.md`.
- **A kínai edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és
  minden edzőüzenete (2 065 szöveg) a `workouts.zh.map.json` térképből generálva (`workouts.zh.json`).
  Ugyanaz a generátor és ugyanaz a teszt őrzi, mint a többi nyelvnél: blokk-hossz, cél és darabszám nem
  változhat. Az edzéslista kínaiul a kínai nevek szerint rendeződik.
- **A felhasználói dokumentumok kínaiul is elérhetők**: `docs/i18n/zh/README.md`,
  `docs/i18n/zh/USER_GUIDE.md`, `docs/i18n/zh/PRIVACY.md` és `docs/i18n/zh/CHANGELOG.md`; a kínai README
  `## 许可证` szakasza a magyar `## Licenc` szakaszt tükrözi, a licencnevek és SPDX-azonosítók változatlanok.
  A magyar dokumentumok tetején hivatkozás mutat rájuk.
- **Csomagolás kínaiul**: `Comment[zh]` a Linuxos `.desktop` bejegyzésben, `zh-Hans.lproj/InfoPlist.strings`
  a macOS-bundle Bluetooth-kérdéséhez, `values-zh/strings.xml` az Androidhoz, a `Dioxus.toml` egynyelvű
  mezői mellett pedig kommentben a kínai szöveg. Az Android-összeállítás és a macOS futás ebben a
  környezetben nem volt kipróbálható: ezek **nem ellenőrzöttek**, és az anyanyelvi lektorálás is hátravan.
- **A kínai betűkészlet rendszerbetűre vált.** A beágyazott Barlow nem tartalmaz han jeleket, ezért a
  `:lang(zh-Hans)` szabály a szöveghez és a címekhez rendszerbetűt ad (`--font-cjk`), a latin betűk és a
  számok viszont maradnak Barlow, illetve Barlow Condensed alatt. A CSS-token és a rá épülő teszt a
  `ride-ui-kit`-ben van; a nyelvváltó nyolc lehetőséggel már nem fér el egy sorban a telefonon, ezért a
  szegmentált vezérlő törhető lett.
- **Olasz felület.** Az app minden felirata, hibaüzenete, tippje és a beállítások szövege elérhető
  olaszul: a Beállítások → Megjelenés → Nyelv alatt választható („Automatikus”, „Magyar”, „English”,
  „Español”, „Português”, „Deutsch”, „Italiano”, „简体中文”). A katalógus hiánytalan
  (`crates/ride-i18n/locales/it.json`, 1 061 szöveg), a „tekerő” végig „ciclista” (soha nem „cavaliere”
  vagy „fantino”), a megszólítás végig „tu” (soha nem a formális „Lei”), a tizedesvesszőt használja, a
  mértékegységek és a márkanevek változatlanok. Az `it` oszlop a `GLOSSARY.tsv`-ben és a `STYLE.tsv`-ben
  is ott van, a katalógus-teszt pedig a ló szavakat is őrzi. Részletek: `docs/I18N.md`.
- **Az olasz edzéskönyvtár is elkészült**: mind a 93 edzés neve, célja, leírása, blokkneve, megjegyzése és
  minden edzőüzenete (2 065 szöveg) a `workouts.it.map.json` térképből generálva (`workouts.it.json`).
  Ugyanaz a generátor és ugyanaz a teszt őrzi, mint a többi nyelvnél: blokk-hossz, cél és darabszám nem
  változhat. Az edzéslista olaszul az olasz nevek szerint rendeződik.
- **A felhasználói dokumentumok olaszul is elérhetők**: `docs/i18n/it/README.md`,
  `docs/i18n/it/USER_GUIDE.md`, `docs/i18n/it/PRIVACY.md`, valamint a fordítói `PROMPT.md` és
  `REVIEW_PROMPT.md`; az olasz README `## Licenza` szakasza a magyar `## Licenc` szakaszt tükrözi, a
  licencnevek és SPDX-azonosítók változatlanok. A magyar dokumentumok tetején hivatkozás mutat rájuk.
- **Csomagolás olaszul**: `Comment[it]` a Linuxos `.desktop` bejegyzésben, `it.lproj/InfoPlist.strings`
  a macOS-bundle Bluetooth-kérdéséhez, `values-it/strings.xml` az Androidhoz, a `Dioxus.toml` egynyelvű
  mezői mellett pedig kommentben az olasz szöveg. Az Android-összeállítás és a macOS futás ebben a
  környezetben nem volt kipróbálható: ezek **nem ellenőrzöttek**, és az anyanyelvi lektorálás is hátravan.
- **Olaszul is elférnek a feliratok.** A 412 × 860-as ellenőrzés egyetlen olyan elemet talált, amely csak
  olaszul csordult túl: az adatok táblázatának pulzusfejlécét; ezért a keskeny HUD-csempe rövid felirata
  „FC” lett. A többi vágott elem (`profile-readout`, a térkép vászna, a Névjegy licenctáblázata)
  magyarul is vág.
- **Néhány keménykódolt felirat a katalógusba került.** Az eszközlista „Cadence-szenzor” szerepneve és a
  beállítások „Google Maps API-kulcs” címkéje eddig a forráskódban élt; most kulcs, és minden kész
  katalógusban megvan (a magyar forrás is „Kadenciaszenzor”). A `check_ui.mjs` szólistája ezzel bővült.
- **A felső sáv bal sarkában az app saját ikonja látszik** (sötét lekerekített négyzet, türkiz hegygerinc) a korábbi
  általános kerékpár-jel helyett: ugyanaz a kép, mint az ablak, a Linux/macOS csomagok és az APK ikonja. Ugyanazokból
  az állandókból rajzolódik (`crates/ride-app/src/icon.rs`), így a négy nem tud elcsúszni egymástól.
- **Áttekintés oldal: egységes panelek és olvashatóbb grafikonok.** Asztalon minden szakasz (Edzések, Becsült watt,
  Térkép, Szakasz, Grafikon, A szakasz adatai) a tekerés képernyő paneljeivel azonos címsoros keretet kapott; a
  távolságtengelyek első számán ott a `km`, a grafikon felső vonalán a mértékegység, a szélső számok nem vágódnak
  félbe. Kurzor nélkül a jelmagyarázat az ablak átlagát mutatja („Ø 258 W”; időkülönbségnél az ablak végén mért
  különbséget), nem egy sor gondolatjelet.
- **Áttekintés oldal telefonon.** Egy fejlécsor (vissza, Összehasonlítás | Teljesítmény, importálás ikonként) a korábbi
  három helyett; a mérőszám-gombokon a mértékegység áll (`W · bpm · rpm · km/h · ±s`), a grafikon fölött a teljes név;
  a szakasz adatai elforgatott táblázatban (soronként egy szám, oszloponként egy edzés), így négy edzés is elfér
  oldalra görgetés nélkül; a csúcsteljesítmény-táblában a dátum alá kerül az útvonal, a sávoszlop elmarad.
- **Telefonon a felső sávban ott az Áttekintés gombja**, a téma-kapcsoló helyén (a téma a Beállítások → Megjelenés
  alatt állítható). Eddig csak az Útvonal ablakon át lehetett eljutni oda.
- **Gépi fordítás-átnézés mind a hat nyelvre**: nyelvenként egy agent a teljes felületi katalógust átolvasta, és
  megjelölte a kérdéses fordításokat (`docs/i18n/review/<nyelv>.md`, összesen 224 jelölés). Ez egy anyanyelvi lektor
  munkalistája, nem ítélet; a téves jelöléseket és a már javítottakat a `docs/i18n/review/README.md` sorolja fel.

### Javítva
- **Áttekintés oldal telefonon: a grafikonok fölött nem lehetett görgetni, és a koppintott érték azonnal eltűnt.** A
  profil és a grafikon minden érintést magának tartott (`touch-action: none`), pedig a telefon képernyőjének nagy
  részét kitöltik. Most a függőleges húzás görget, a vízszintes kijelöl egy szakaszt, a koppintás pedig leteszi a
  kurzort, és ott is marad (az ujjnak nincs „lebegése”, ami mutatná az értékeket).
- **Áttekintés oldal telefonon: a térkép rossz méretre igazodott**, és az útvonal eleje a méretarány alá került. A
  térkép most akkor igazodik a szakaszra, amikor a Térkép fülön először valódi mérete van. Minden térkép-ráigazítás
  (a tekerés képernyőn is) alul helyet hagy a méretaránynak és a forrásmegjelölésnek.
- **Az élő grafikonok üresek maradtak, ha az előzmény ritkábban került rögzítésre, mint másodpercenként** (lassú
  eszköz, vagy a `RIDE_TIMESCALE` tesztkapcsoló): a vonal minden mintánál „szenzorkiesést” látott és szétesett.
  A szakadás határa most a rögzítés saját üteméhez igazodik; egy valódi kiesés továbbra is megszakítja a vonalat.
- **Telefon, Adatok fül: a statisztika utolsó sora és a szimulált kerékpáros gombjai levágódtak**, a szimulált
  kerékpáros panel pedig keskenyebb volt a többinél, dupla kerettel. Most mindkettő a tartalma szerinti magas, a
  panel teljes szélességű.
- **Telefon: a profil fejlécében félbevágott feliratok** („A” az „Ablak 3.00 km” helyett, „Követke” a „Következik”
  helyett). A keskeny fejléc most egész értékpárokat hagy el jobbról.
- **Telefon: a Beállítások „Elrendezés” csoportja nem jelenik meg**: a telefonon fülek vannak, nincs húzható panel.
- **Eszközök ablak: a „Nincs kapcsolat” gombnak látszott.** Az állapot most ugyanolyan pill, mint a „Csatlakoztatva”.
- **Edzéslista: a 30 perces edzés hossza „30” volt** az „1:00” mellett; most „0:30”.
- **Olasz felület: „1 immagini”, „1 percorsi”.** Az olasz katalógusból hiányoztak az egyes számú alakok; most megvannak,
  és a katalógus-teszt minden egyes számot ismerő nyelvnél megköveteli őket (a teljesség tesztje is mind a hat nyelvre
  kiterjed, eddig csak az angolra és a spanyolra).
- **Fordítási hibák**, egy gépi átnézés jelölései alapján (`docs/i18n/review/`): portugálul a „km múlva” „megtett
  km” volt, és a terhelés-összegzés második mondata hiányzott; angolul, németül és spanyolul „az utolsó 12 hét”
  egyes számban állt („12 week”, „12 Woche”, „12 mes”); németül „Rennynamik” → „Renndynamik”.
- **Előretekintő magassági profil: egy-egy pillanatra eltűnt a görbe, a pötty pedig a bal szélre ugrott.** Az ablak
  azt nézte, hogy „kezdet + hossz − kezdet” rövidebb-e a hossznál, és a lebegőpontos kerekítés miatt ez a távolságok
  nagyjából harmadánál igaz lett: az app ilyenkor azt hitte, az ablak az útvonal végébe ütközött, és egy képkockára
  oda ugrott (a vonalzó például 9,5–12 km-t mutatott 2,4 km-nél). Most az ablak végét hasonlítja az útvonal hosszához;
  egy teszt tört méteres távolságokon végigsöpörve őrzi.
- **Néhány felirat kimaradt a fordításból.** A katalóguson kívül maradt szövegek (a szellemválasztó „Ki”
  gombja, a „besorolatlan” emelkedő-kategória, a „2. kat.” típusú jelölések, az útvonal-betöltés hibaüzenetei,
  a képernyőolvasóknak szóló leírások, a Street View letöltés állapotsorai és a hibaüzenetei) most a katalógusból
  jönnek, így spanyolul is spanyolul jelennek meg. A `scripts/i18n/check_ui.mjs` a renderelt felületet vizsgálja,
  ez fogta meg őket.
- **Az „Automatikus” nyelv leírása elavult volt**: azt írta, hogy ismeretlen nyelvnél angolul indul, pedig a
  program magyarul indul (félkész katalógust senki ne lásson). A szöveg most a valóságot írja.
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
