# Adatvédelem és hálózati forgalom

A bike-osm-ride-nak **nincs fiókja, nincs telemetriája, nincs elemzője és nincs hirdetése**. Az adataid (tekerések,
beállítások, szenzoradatok) a saját gépeden maradnak; a program csak azt hívja meg a hálózaton, amihez te
kérést adsz. Ez a lap pontosan felsorolja, mi mehet ki, és mi van a gépeden. Az állítások a forráskód alapján
készültek (2026-11-04, 0.3.0-s verzió).

## Mi megy ki a gépedről, és mikor

| Cím | Mikor | Mit kap | Megjegyzés |
|---|---|---|---|
| `tile.openstreetmap.org` | amikor a térkép csempéket kér | a csempe koordinátái (zoom, x, y) és a program neve a User-Agentben (`bike-osm-ride/0.1 (desktop app)`) | helyben, 30 napig gyorsítótárazva; tömeges előtöltés nincs (az OSM csempe-szabályzata miatt) |
| `brouter.de` | ha bikerouter-linket illesztesz be | a linkből kiolvasott pontok (hosszúság, szélesség), a profil és az elkerülők | az útvonalat az ő szerverük számolja |
| `api.open-meteo.com` | csak ha a GPX-ben nincs használható magasságadat | az útvonal mintapontjainak koordinátái (kérésenként legfeljebb 100) | CC BY 4.0, nem kereskedelmi használatra |
| `api.github.com` | induláskor, öt másodperc múlva | a legfrissebb kiadás lekérdezése, `User-Agent: bike-osm-ride/<verzió>` | csendben elhallgat, ha nem sikerül; **a beállításokban nem kapcsolható ki** |
| `github.com` (letöltés) | csak ha a *Frissítés* gombot nyomod | az új csomag letöltése | |
| `maps.googleapis.com`, `www.google.com` | csak ha bekapcsoltad a Street View-t, és megadtál API-kulcsot | a bringás pozíciója (koordináta) és a **saját** Google-kulcsod | a Google feltételei vonatkoznak rá; lásd [`STREETVIEW.md`](STREETVIEW.md) |
| a Firefoxban megnyitott oldalak | csak ha a Böngésző panelt használod (Windows) | amit te nézel benne | külön Firefox-profil, a rendes böngésződhöz nem nyúl |

Nincs más. Nincs bejelentkezés a programba, nincs hibajelentés-küldés, nincs használati statisztika. A
Bluetooth-forgalom csak a trainer, a szenzorok és a gép között zajlik.

## Mi van a gépeden

Minden egyetlen mappában van (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride`, Androidon az alkalmazás privát mappája). A felépítést a
[`ROUTES_AND_DATA.md`](ROUTES_AND_DATA.md) írja le; röviden:

- **beállítások** (`settings.json`): testsúly, FTP, max. pulzus, megjelenés, elrendezés, a Google API-kulcsod;
- **útvonalak és tekerés-napló**: a megnyitott útvonalak feldolgozott változata, minden befejezett menet teljes
  másodpercenkénti felvétele (pozíció, watt, pulzus, kadencia);
- **csempék**, a letöltött **Street View képek** (legfeljebb 30 napig) és panoráma-azonosítók;
- **naplók**: `ble.log` (a trainer és a szenzorok nyers adatcsomagjai: watt, pulzus, vezérlőparancsok; eszköznév vagy cím
  nincs bennük) és `crash.log` (elkapott hibák); mindkettő forgatott, a gépen marad;
- a Böngésző panel Firefox-profilja;
- a **0.3.1-es és régebbi** verzió Strava-feltöltésének maradékai: a `strava` és a `strava-webview` mappa (utóbbiban a
  Strava-bejelentkezésed sütijei vannak). A program már nem használja és nem is törli: nyugodtan töröld őket.

**Biztonsági mentés.** A *Beállítások → Biztonsági mentés* egyetlen zip-fájlt készít az útvonalaidról, a túráidról, az
importált edzéseidről és a beállításaidról. A fájl oda kerül, ahova te mented (Androidon a rendszer „Mentés másként”
ablakával), a program semmit nem küld sehová, és nem is tölt fel. A Google API-kulcs **nincs benne**, és visszaállításkor
a fájlból érkező kulcsot sem fogadja el; a csempék, a Street View képek és a naplók sincsenek benne. A fájl a túráid
pontos útvonalait és az egészségi adataidat (pulzus, watt) tartalmazza, ezért úgy kezeld, mint a többi személyes fájlodat.

**Titkok.** Az egyetlen titok a Google API-kulcs. A `settings.json`-ban *egyszerű szövegként* áll (nincs titkosítva),
ezért a mappát úgy védd, mint a többi személyes fájlodat. A program soha nem naplózza, nem kerül hibaüzenetbe és
nem jelenik meg a hibakeresési kimenetben, és csak a Google felé megy ki (vagy egy helyi, `127.0.0.1`-es próbaszerver
felé fejlesztéskor).

**Törlés.** A mappa törlésével minden eltűnik. A Street View képek és azonosítók a *Beállítások → Street View*
alatt külön is törölhetők; a legutóbbi útvonalak listájából törölt útvonal tekerés-naplója megmarad.

### Importált edzésfájlok

Az **Edzés importálása** egy általad kiválasztott (vagy ráhúzott) GPX- vagy TCX-fájlt olvas be. A fájlt a program a saját
gépén dolgozza fel, és csak az eredményt (a másodpercenkénti felvételt: pozíció az útvonalon, magasság, sebesség, pulzus,
kadencia, watt) menti a `rides` mappába, ugyanúgy, mint a saját tekeréseidet. A fájl eredeti tartalma (például a Strava
által beírt név és időbélyegek) nem kerül máshová, és semmi nem megy ki a hálózatra. A becsült watt kiszámítása is helyben
történik.

## Amit érdemes tudni

- Az OpenStreetMap, a BRouter, az Open-Meteo, a GitHub és a Google látja a saját kéréseidet (IP-cím,
  időpont); ez minden hálózati kliensnél így van.
- A frissítés-ellenőrzés kikapcsolására jelenleg nincs beállítás. Ha ezt nem szeretnéd, tűzfallal tiltsd a
  `api.github.com` címet; a program ettől nem romlik el.
- A program nem a Google vagy a Zwift terméke, és nincs velük kapcsolatban.
