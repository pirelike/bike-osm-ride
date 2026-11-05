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
| `www.strava.com` | csak ha a *Feltöltés Stravára* gombot nyomod | a TCX-fájl és a kitöltött mezők (cím, leírás, jegyzet, láthatóság…) | a bejelentkezést a Strava oldala kezeli, a program a jelszavadat nem látja |
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
- a **Strava**-feltöltés másolatai és bejelentkezési profilja, a Böngésző panel Firefox-profilja.

**Titkok.** Az egyetlen titok a Google API-kulcs. A `settings.json`-ban *egyszerű szövegként* áll (nincs titkosítva),
ezért a mappát úgy védd, mint a többi személyes fájlodat. A program soha nem naplózza, nem kerül hibaüzenetbe és
nem jelenik meg a hibakeresési kimenetben, és csak a Google felé megy ki (vagy egy helyi, `127.0.0.1`-es próbaszerver
felé fejlesztéskor).

**Törlés.** A mappa törlésével minden eltűnik. A Street View képek és azonosítók a *Beállítások → Street View*
alatt külön is törölhetők; a legutóbbi útvonalak listájából törölt útvonal tekerés-naplója megmarad.

## Amit érdemes tudni

- Az OpenStreetMap, a BRouter, az Open-Meteo, a GitHub, a Google és a Strava látja a saját kéréseidet (IP-cím,
  időpont); ez minden hálózati kliensnél így van.
- A frissítés-ellenőrzés kikapcsolására jelenleg nincs beállítás. Ha ezt nem szeretnéd, tűzfallal tiltsd a
  `api.github.com` címet; a program ettől nem romlik el.
- A program nem a Google, a Strava vagy a Zwift terméke, és nincs velük kapcsolatban.
