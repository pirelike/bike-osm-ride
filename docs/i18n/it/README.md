# bike-osm-ride — in italiano

> **Traduzione.** La versione autoritativa è il [README.md](../../../README.md) ungherese.
> Gli screenshot mostrano l'interfaccia originale ungherese; l'app parla ungherese, inglese, spagnolo, portoghese, tedesco, italiano e cinese.
> Anche la guida completa e la pagina sulla privacy sono in italiano: [guida](USER_GUIDE.md) · [privacy](PRIVACY.md).

**Lingua:** [Ungherese](../../../README.md) · [English](../en/README.md) · [Español](../es/README.md) · [Português](../pt/README.md) · [Deutsch](../de/README.md) · Italiano · [中文](../zh/README.md)

<p align="center">
  <img src="../../../packaging/icons/128.png" width="96" alt="icona di bike-osm-ride">
</p>

<p align="center">
  <b>Pedalata virtuale sui tuoi percorsi GPX, con un rullo intelligente – o senza.</b><br>
  Mappa, profilo altimetrico, allenamenti, fantasma, esportazione TCX. Veloce, moderna, 2D. Rust.
</p>

<p align="center">
  <a href="https://github.com/pirelike/bike-osm-ride/releases/latest"><img alt="Ultima versione" src="https://img.shields.io/github/v/release/pirelike/bike-osm-ride?display_name=tag&sort=semver"></a>
  <img alt="Licenza: MIT o Apache-2.0" src="https://img.shields.io/badge/licenza-MIT%20OR%20Apache--2.0-blue">
  <img alt="Piattaforme" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-informational">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white">
  <img alt="Interfaccia: ungherese, inglese, spagnolo, portoghese, tedesco, italiano e cinese" src="https://img.shields.io/badge/interfaccia-ungherese%20%7C%20inglese%20%7C%20spagnolo%20%7C%20portoghese%20%7C%20tedesco%20%7C%20italiano%20%7C%20cinese-brightgreen">
</p>

<p align="center">
  <img src="../../screenshots/ride-dark.png" alt="La schermata di pedalata in tema scuro: mappa, potenza, profilo altimetrico, statistiche" width="100%">
</p>

**Vai a:** [Download](#download) · [Avvio rapido](#avvio-rapido) · [Cosa sa fare](#cosa-sa-fare) · [Screenshot](#screenshot) ·
[Dispositivi e stato](#dispositivi-e-stato) · [Documentazione](#documentazione) · [Sviluppo](#sviluppo) · [Licenza](#licenza)

## Perché questa app?

- **Il tuo percorso.** Non un mondo preconfezionato: puoi pedalare qualsiasi file GPX o link di bikerouter, con dislivello e
  salite reali, e il rullo segue la pendenza del percorso.
- **Tutti i numeri che contano.** Potenza (media di 3 s, W/kg, zona), cadenza, frequenza cardiaca, velocità, pendenza,
  NP/IF/TSS, dislivello, ETA: con numeri grandi e leggibili, senza 3D.
- **Allenamento e analisi.** 93 allenamenti inclusi, con fonti, in modalità ERG, con messaggi del coach; fantasma e segmenti
  locali con PR; esportazione TCX.
- **Resta tuo.** Nessun account, nessuna telemetria, i tuoi dati sono sul tuo computer ([privacy](PRIVACY.md)). Open
  source, con una licenza permissiva.
- **Funziona anche senza hardware.** Con il ciclista simulato puoi provare tutto senza Bluetooth.

## Download

La versione più recente: **[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**.
Nei nomi dei file, al posto di `<version>` c'è il numero della versione (per esempio `0.3.0`).

| Piattaforma | File | Installazione |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<version>-windows-x64.exe` | nessun installer: avvialo. Serve il [runtime WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (su Windows 11 c'è già). Il programma non è firmato digitalmente, quindi SmartScreen può bloccarlo. |
| **Linux** (x86-64, glibc 2.35+) | `bike-osm-ride-<version>-linux-x86_64.AppImage` | `chmod +x`, poi avvialo; porta con sé WebKitGTK e GTK |
| | `bike-osm-ride-<version>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<version>-linux-amd64.deb` (chiede i pacchetti `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `bluez`) |
| **macOS 11+** (Intel e Apple Silicon) | `bike-osm-ride-<version>-macos-universal.dmg` | trascinalo in *Applicazioni*; al primo avvio **clic con il tasto destro → Apri → Apri** (non è notarizzato); al primo scan Bluetooth consenti il Bluetooth |
| **Android 7+** | `bike-osm-ride-<version>-android.apk` | copialo sul telefono e aprilo (devi consentire l'installazione da origini sconosciute), oppure `adb install -r` |

Il programma **ti avvisa** quando c'è una nuova versione: nella barra superiore compare il pulsante *Aggiorna* (su Windows e
come AppImage si sostituisce da solo, sulle altre piattaforme apre la pagina della release). Le versioni Android richiedono una chiave di firma stabile; la compilazione si interrompe se manca. Disinstalla una vecchia versione firmata con una chiave di debug prima di installare l’APK di rilascio.

## Avvio rapido

1. **Avvia.** Si apre la finestra *Percorso*: trascinaci un GPX, incolla un link di bikerouter oppure scegli il
   *Percorso demo*.
2. **Collega il rullo** (*Dispositivi* → *Avvia ricerca* → nel ruolo *Rullo*), oppure scegli il
   **ciclista simulato** se non hai hardware a portata di mano. Puoi collegare anche un cardiofrequenzimetro, un misuratore di
   potenza, un sensore di cadenza e un Zwift Click/Play.
3. *(Facoltativo)* **Allenamento**: con il pulsante *Allenamento* della barra superiore (`W`) scegli tra 93 allenamenti, oppure
   importi il tuo.
4. **`Avvia`** (`Spazio`). Il rullo segue le salite (o mantiene i watt obiettivo dell'allenamento) e i numeri prendono vita.
5. Alla fine salvi un TCX dal **riepilogo** (Strava, Intervals.icu e TrainingPeaks lo accettano).

La descrizione completa: **[Guida utente](USER_GUIDE.md)**.

## Cosa sa fare

**Percorsi**
- File GPX (sfoglia, trascina, `Ctrl+O`), link di bikerouter/BRouter (arrivano anche il profilo, l'alternativa e le esclusioni),
  percorsi recenti, percorso demo incluso.
- Lettore GPX tollerante (BOM, namespace, più segmenti, `<rte>`, un messaggio comprensibile per un file corrotto); se manca
  l'altitudine, la integra Open-Meteo. Riconoscimento delle salite dalla 4ª categoria a HC; le tracce dei segmenti inserite nel file
  (per esempio un segmento Strava) si vedono come bande.

**Pedalata**
- Fisica realistica (Martin et al. 1998: gravità, resistenza al rotolamento, attrito dei cuscinetti, resistenza aerodinamica
  delle ruote a raggi, inerzia), il rullo riceve la pendenza del tratto che hai davanti (fino a ±20%, con un'intensità da 0 a
  150%).
- **Rullo FTMS** (salite ed ERG), **misuratore di potenza** (CPS), **velocità/cadenza** (CSC), **cardiofrequenzimetro** (HR); un
  singolo dispositivo fisico può svolgere *più ruoli* su una sola connessione; **Zwift Click/Play** come cambio virtuale;
  **ciclista simulato**.
- **Cambio virtuale**: trasmissione 2 × 12, 23 marce, modifica la resistenza del rullo.

**Allenamento e analisi**
- 93 allenamenti inclusi in 12 categorie, con etichette, un grafico a blocchi colorato per zona e 943 messaggi del coach
  cronometrati; importazione di allenamenti MyWhoosh e Zwift (`.zwo`, XML, JSON).
- Zone di Coggan, NP, IF, TSS, kcal; zone di frequenza cardiaca; registrazione ogni secondo.
- **Fantasma** (l'ultima uscita, la migliore, oppure i timestamp del GPX) e **segmenti locali con PR**.
- **Analisi e confronto delle uscite**: le uscite fatte sullo stesso percorso (al massimo quattro) affiancate per potenza,
  frequenza cardiaca, cadenza, velocità e differenza di tempo; zoom su un tratto (trascinando o dalle salite), tabella dei
  tratti. **Importazione di un'uscita GPX o TCX** da un export Strava o Garmin, con frequenza cardiaca e cadenza; se mancano i
  watt, la fisica della simulazione li stima a ritroso (con il tuo peso corporeo o quello indicato, la bici e la resistenza
  al rotolamento).
- **Pagina della potenza**: potenza di picco da 5 s a 60 minuti, una proposta di FTP dai tuoi migliori 20 minuti (impostabile con
  un pulsante), carico settimanale e mensile. Contano solo i watt misurati.
- **Backup**: i percorsi, le uscite, gli allenamenti importati e le impostazioni in un unico file zip (senza la chiave Google),
  con un ripristino che unisce i dati. Niente esce verso la rete.
- **Esportazione TCX** (schema Garmin): Strava, Intervals.icu e TrainingPeaks la accettano se la carichi a mano. Un caricamento
  integrato non c'è: l'accesso a Strava andava rifatto dopo ogni uscita, perciò l'abbiamo tolto (0.3.2).

**Interfaccia**
- **Layout a riquadri**: i pannelli funzionano come in un gestore di finestre a riquadri (nessuna sovrapposizione, nessun buco);
  si ridimensionano, si spostano e si chiudono trascinando; un layout separato con e senza allenamento.
- Tema scuro e chiaro (entrambi completi), dimensione del testo 80–160%, **interfaccia in ungherese, inglese, spagnolo,
  portoghese, tedesco, italiano e cinese** (*Impostazioni → Aspetto → Lingua*, oppure automaticamente seguendo la lingua di sistema),
  font incorporati.
- **Telefono e tablet**: vista a schede; nella scheda “Pedalata” c'è tutto quello che serve mentre pedali.
- **Street View** facoltativo (con la tua chiave Google, download offline) e un **pannello Browser** (Windows, incorpora il
  Firefox installato).
- Tiene sveglio il computer mentre pedali (Windows, Linux, macOS).

## Screenshot

<table>
  <tr>
    <td width="50%"><img src="../../screenshots/ride-light.png" alt="Tema chiaro"><br><sub><b>Tema chiaro</b>: il tema bianco carta è alla pari con quello scuro</sub></td>
    <td width="50%"><img src="../../screenshots/workout.png" alt="Allenamento in modalità ERG"><br><sub><b>Allenamento in modalità ERG</b>: card dell'allenamento, grafici dal vivo, grafico a blocchi</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/workout-picker.png" alt="Selettore degli allenamenti"><br><sub><b>93 allenamenti</b> con filtri e un grafico a blocchi colorato per zona</sub></td>
    <td><img src="../../screenshots/summary.png" alt="Il riepilogo dell'uscita"><br><sub><b>Riepilogo</b>: NP, IF, TSS, segmenti, zone; salvataggio TCX</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/devices.png" alt="Finestra Dispositivi"><br><sub><b>Dispositivi</b>: un dispositivo in più ruoli</sub></td>
    <td align="center">
      <img src="../../screenshots/phone-ride.png" width="170" alt="Telefono, scheda Pedalata">
      <img src="../../screenshots/phone-map.png" width="170" alt="Telefono, scheda Mappa"><br>
      <sub><b>Sul telefono</b>: schede Pedalata e Mappa</sub>
    </td>
  </tr>
  <tr>
    <td><img src="../../screenshots/review.png" alt="Analisi delle uscite"><br><sub><b>Analisi</b>: due uscite su una salita, la seconda con watt stimati</sub></td>
    <td align="center">
      <img src="../../screenshots/review-phone.png" width="170" alt="Telefono, analisi">
      <br><sub><b>Sul telefono</b>: schede Uscite, Mappa, Grafico, Tratto</sub>
    </td>
  </tr>
</table>

<sub>Gli screenshot mostrano un percorso BRouter reale (gli ultimi 12 km del percorso di prova che sta nel repository come dato
di test), con il ciclista simulato. Percorso e mappa: © collaboratori di OpenStreetMap, ODbL. Le immagini le produce
`node scripts/readme_shots.mjs`.</sub>

## Dispositivi e stato

**Non tutte le voci sono state provate con hardware reale o con un servizio reale.** Sinceramente:

| Voce | Stato |
|---|---|
| Rullo intelligente FTMS, **ERG** (allenamento) | testato, **anche con un rullo reale** (un modello: VE200) |
| **Simulazione delle salite** FTMS | testata a fondo (con un trasporto simulato, usando i vettori di byte della specifica); **con un rullo reale non ancora confermata** |
| Cardiofrequenzimetro, misuratore di potenza separato, sensore CSC | i decoder sono testati; i dati CPS/CSC del rullo sono reali; con un sensore separato non abbiamo ancora provato |
| Zwift Click / Play | implementato (non ufficiale), con un controller reale non ancora provato |
| Ciclista simulato | funziona senza hardware, su ogni piattaforma |
| Windows | in uso quotidiano |
| Linux, macOS | la CI li compila e (su Linux) esegue uno smoke test di 20 secondi; manca ancora una verifica su una macchina reale (esperienze benvenute) |
| Android | l'interfaccia ha girato (i bug della 0.3.0 li abbiamo trovati usandola); il selettore di file di sistema è implementato; **il Bluetooth su un dispositivo non è ancora stato provato** |
| Street View | implementato e testato con un server di prova locale; con il servizio reale non ancora provato |

Dettagli: [`docs/TESTING.md`](../../TESTING.md) e [`docs/BLUETOOTH.md`](../../BLUETOOTH.md). Segnalazioni di bug, esperienze e
dispositivi compatibili o incompatibili sono benvenute nelle Issues (vale la pena allegare il `ble.log`).

## Documentazione

| A chi | Cosa |
|---|---|
| **Utenti** | [Guida utente](USER_GUIDE.md) · [Privacy e traffico di rete](PRIVACY.md) · [Registro delle modifiche](../../../CHANGELOG.md) · [Ungherese](../../../README.md) · [English](../en/README.md) · [Español](../es/README.md) · [Português](../pt/README.md) · [Deutsch](../de/README.md) |
| **Sviluppatori** | [Architettura](../../ARCHITECTURE.md) · [Guida allo sviluppo](../../DEVELOPMENT.md) · [Test](../../TESTING.md) · [Rilascio](../../RELEASING.md) · [Registro delle decisioni](../../DECISIONS.md) · [Traduzioni (i18n)](../../I18N.md) |
| **Temi** | [Bluetooth](../../BLUETOOTH.md) · [Simulazione e fisica](../../SIMULATION.md) · [Percorsi e formati dei dati](../../ROUTES_AND_DATA.md) · [Interfaccia](../../UI.md) · [Design system](../../DESIGN.md) · [Android](../../ANDROID.md) · [Street View](../../STREETVIEW.md) · [Precedenti e codice riutilizzato](../../PRIOR_ART.md) |

L'indice completo: [`docs/README.md`](../../README.md).

## Sviluppo

```powershell
cargo run -p ride-app                      # build di debug e avvio (Windows, Linux, macOS)
cargo test --workspace                     # l'intera suite di test (2026-11-06: 1076 test)
node scripts/readme_shots.mjs              # rigenera gli screenshot
```

Un workspace Rust con dieci crate di libreria e l'applicazione (`ride-app`):

```
crates/
  ride-model         modello dati comune (Route, Settings, RideSnapshot, Workout, …) – il “contratto”
  ride-route         GPX/bikerouter → Route, salite, persistenza, registro delle uscite
  ride-sim           fisica, RideEngine, runner degli allenamenti, fantasma, segmenti, TCX (senza I/O e senza orologio)
  ride-ble           hub BLE (FTMS/CPS/CSC/HR/Zwift) + ciclista simulato
  ride-ui-kit        design system: token, palette, icone, formattazione
  ride-ui-map        mappa a tile · ride-ui-profile  profilo e grafici
  ride-ui-hud        strumenti, card, controlli       · ride-ui-dialogs  finestre
  ride-ui-streetview pannello Street View
  ride-app           la struttura: stato, controller, layout a riquadri, proxy dei tile, updater, Android
packaging/  scripts/  tests/fixtures/  docs/
```

Il codice si basa su Dioxus 0.7 (interfaccia basata su webview) e su `btleplug`. Su Windows lo sviluppo avviene sul toolchain
GNU (l'exe di release lo compila la CI con MSVC). Prerequisiti, opzioni (`RIDE_*`), debug, pacchettizzazione:
[`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md). La descrizione del progetto per Claude Code: [`CLAUDE.md`](../../../CLAUDE.md).

**Contribuire.** Segnalazioni di bug, registri di misurazione e correzioni sono benvenuti. Le regole in breve: si può riprendere
solo codice con licenza permissiva (vedi sotto), la lingua di partenza dei testi utente è l'ungherese (ogni testo
dell'interfaccia è una chiave in `crates/ride-i18n/locales/hu.json`, con le traduzioni accanto; vedi
[`docs/I18N.md`](../../I18N.md)), sui dati esterni non andiamo in panico, e una nuova funzionalità arriva con un test e la
documentazione. Il codice inviato entra con la licenza del progetto (MIT o Apache-2.0), se non indichi diversamente.

## Licenza

### Il codice del progetto

Il codice di `bike-osm-ride` ha **doppia licenza: [MIT](../../../LICENSE-MIT) OPPURE [Apache-2.0](../../../LICENSE-APACHE)**,
a tua scelta (SPDX: `MIT OR Apache-2.0`). Se contribuisci, anche il codice inviato entra con queste due licenze, se non indichi
diversamente.

### Codice open source riutilizzato

Usiamo di proposito solo codice con licenze permissive. L'origine e la licenza sono conservate nei file di origine e in
[`NOTICE.md`](../../../NOTICE.md).

| Origine | Licenza | A cosa ci serve |
|---|---|---|
| crate [`ftms`](https://github.com/deancochran/ftms) | MIT | codifica e decodifica dei dati FTMS Bluetooth (rullo intelligente) e dei comandi di controllo |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | interpretazione dei dati del misuratore di potenza, del sensore di cadenza e del cardiofrequenzimetro (integrato, segnalato nei file di origine) |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | connessione al rullo e filtro di sicurezza dei comandi (parti portate, segnalate) |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | ricerca e riconnessione del cardiofrequenzimetro (schema ripreso, senza codice, segnalato) |
| Font [Barlow e Barlow Condensed](https://github.com/jpt/barlow) | SIL OFL 1.1 | testo e cifre dell'interfaccia; **incorporati** nel programma (WOFF2 con sottoinsieme, latino + latino esteso, compresi i caratteri ungheresi accentati), così funzionano anche senza rete; il testo della licenza: [`crates/ride-ui-kit/assets/fonts/OFL.txt`](../../../crates/ride-ui-kit/assets/fonts/OFL.txt), i dettagli in [`NOTICE.md`](../../../NOTICE.md) |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense (pubblico dominio) | il protocollo BLE di Zwift Click/Play e Zwift Hub (riscritto in Rust): il modulo `zwift` di `ride-ble`, dettagli in [`NOTICE.md`](../../../NOTICE.md) |
| [`p256`](https://github.com/RustCrypto/elliptic-curves), [`hkdf`](https://github.com/RustCrypto/KDFs), [`sha2`](https://github.com/RustCrypto/hashes), [`aes`](https://github.com/RustCrypto/block-ciphers), [`ccm`](https://github.com/RustCrypto/AEADs) (RustCrypto) | MIT / Apache-2.0 | lo scambio di chiavi dello Zwift Click (ECDH P-256, HKDF-SHA256) e la cifratura AES-CCM |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0 (il programma **non lo include**, avvia la copia installata sul computer) | il pannello Browser: incorporiamo la finestra del Firefox installato, con un profilo separato |
| [Dioxus](https://dioxuslabs.com), [btleplug](https://github.com/deviceplug/btleplug), [tokio](https://tokio.rs), [quick-xml](https://github.com/tafia/quick-xml) e le altre dipendenze Rust | in gran parte MIT / Apache-2.0 | l'elenco completo, generato automaticamente (638 pacchetti): [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md) |

Come **riferimento** (senza riprendere codice) sono serviti: un valore numerico di un esempio svolto dell'implementazione open
source Martin chiamata `rouleur` nel test della fisica, [`nrenner/brouter-web`](https://github.com/nrenner/brouter-web) (MIT) per
verificare il formato dei link di bikerouter, e le note di reverse engineering di Makinolo sul protocollo Zwift Click/Play come
documentazione: <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>. Questo è un lavoro di interoperabilità non
ufficiale: non abbiamo alcun rapporto con Zwift, il programma non è un prodotto Zwift e Zwift non lo supporta.

**Non** abbiamo usato codice di progetti con licenza GPL/AGPL (per esempio OpenCycling, suffice, GoldenCheetah, QDomyos-Zwift),
perché metterebbe tutto il programma sotto copyleft; i dettagli della ricerca: [`docs/PRIOR_ART.md`](../../PRIOR_ART.md).

**Audit delle licenze (2026-11-04, `cargo metadata --filter-platform`, dipendenze normali e di build).** Il numero di crate
esterni (crates.io) per piattaforma: Windows (GNU) 396, Linux 458, macOS (Apple Silicon) 400, Android (aarch64) 449. Tra loro
**non c'è GPL, AGPL o LGPL, né una licenza sconosciuta**; le licenze permissive sono MIT, Apache-2.0, BSD, ISC, Zlib, Unicode,
CC0, Unlicense e simili.

**L'unico tipo di copyleft è MPL-2.0**, che è **a livello di file**, e prendiamo i crate senza modifiche da crates.io (niente
codice vendorizzato); il loro sorgente è disponibile nei repository indicati in
[`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md):

- su Linux e macOS `option-ext` (attraverso la catena `dioxus-desktop → tray-icon → dirs`);
- su Android `selectors`, `cssparser`, `cssparser-macros` e `dtoa-short` (attraverso la catena `wry → kuchikiki`);
- su Windows nessuno.

Questo non tocca la licenza del progetto: il nostro codice resta `MIT OR Apache-2.0`. Il `Cargo.lock` contiene le dipendenze di
ogni piattaforma; per rigenerare l'elenco: `.\scripts\third_party_licenses.ps1`, i comandi di audit per piattaforma sono al
punto 9 di [`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md).

### Mappe e fonti dei dati (attribuzione)

Durante il funzionamento l'applicazione chiama servizi esterni; i loro dati si possono usare secondo le loro condizioni, e
l'applicazione mostra sulla mappa le attribuzioni richieste (l'elenco completo delle chiamate: la [privacy](PRIVACY.md)):

- **Dati della mappa:** © collaboratori di [OpenStreetMap](https://www.openstreetmap.org/copyright), [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Tile:** i tile standard di [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles) (`tile.openstreetmap.org`). Gli stili
  di mappa *Scuro* e *Chiaro* filtrano gli stessi tile in locale (filtro CSS), quindi non serve nessuna chiave API né un server
  di stili di terze parti. I tile scaricati vengono messi in cache locale per risparmiare i server
  ([OSM tile usage policy](https://operations.osmfoundation.org/policies/tiles/)); non c'è alcun precaricamento di massa.
- **Street View (facoltativo):** i servizi di [Google Maps Platform](https://mapsplatform.google.com): l'endpoint di *metadata*
  della Street View Static API (solo JSON: stato, id del panorama, posizione), l'endpoint delle immagini della Static API (solo
  per il download offline che richiedi) e la vista Street View della Maps Embed API. Il programma usa **la tua** chiave API (il
  tuo account Google Cloud, la tua quota e la tua fatturazione); la chiave sta solo sul tuo computer, in `settings.json`, non
  finisce mai nei log e va solo verso `maps.googleapis.com` / `www.google.com`. All'uso si applicano i [termini di Google Maps
  Platform](https://cloud.google.com/maps-platform/terms) e le [linee guida di Street
  View](https://developers.google.com/maps/documentation/streetview/policies). Il programma **non scarica le immagini per
  impostazione predefinita**: le salva sul tuo computer solo se premi il pulsante *Scarica* nel pannello, per un massimo di 30
  giorni; le mostra solo senza chiave o in caso di errore di rete, altrimenti si vede la vista live a piena
  risoluzione. La conservazione permanente delle immagini di Street View è limitata dai termini di Google, quindi la decisione e
  la responsabilità sono tue; il download usa la quota del tuo account Google. Le linee guida permettono di conservare gli *id
  dei panorami* (possono cambiare nel tempo, perciò accanto teniamo anche la coordinata e il timestamp, e la vista incorporata
  riceve la coordinata accanto all'id). Questo programma non è un prodotto Google e non ha alcun rapporto con Google.
- **Descrizioni degli allenamenti:** la libreria di allenamenti inclusa è un testo nostro, redatto da protocolli di allenamento
  pubblici (British Cycling, TrainingPeaks, CTS, FasCat, TrainerRoad e altri) e studi di scienza dell'allenamento (Rønnestad,
  Seiler, Helgerud, Billat, Gibala, Bossi, Karsten e altri); le fonti usate compaiono, allenamento per allenamento, nel campo
  `sources` di `crates/ride-model/assets/workouts.json`.
- **Pianificazione dei percorsi:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de) (servizio pubblico; lo
  chiamiamo solo se incolli un link di bikerouter, e il percorso lo calcola il loro server). I file
  `tests/fixtures/brouter_*.gpx` sono risposte di BRouter, da dati OSM (ODbL), e lo stesso percorso si vede negli screenshot del
  README.
- **Completamento dei dati di quota mancanti (facoltativo):** [Open-Meteo](https://open-meteo.com) Elevation API,
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), per uso non commerciale.

La nostra licenza non copre questi dati e questi servizi.

### Avviso legale

Il software è fornito “così com'è”, senza garanzia; i dettagli sono nei file [LICENSE-MIT](../../../LICENSE-MIT) e
[LICENSE-APACHE](../../../LICENSE-APACHE). Il controllo dei dispositivi (l'impostazione automatica della resistenza del rullo)
avviene sotto la tua responsabilità.
