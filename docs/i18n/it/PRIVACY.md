# Privacy e traffico di rete

> **Traduzione.** La versione autorevole di questo documento è la [pagina sulla privacy](../../PRIVACY.md) in
> ungherese; è disponibile anche in [inglese](../en/PRIVACY.md), [spagnolo](../es/PRIVACY.md),
> [portoghese](../pt/PRIVACY.md), [tedesco](../de/PRIVACY.md) e [cinese](../zh/PRIVACY.md).

bike-osm-ride **non ha account, non ha telemetria, non ha strumenti di analisi e non ha pubblicità**. I tuoi dati (uscite,
impostazioni, dati dei sensori) restano sul tuo computer; il programma chiama sulla rete solo quello che richiedi tu. Questa
pagina elenca esattamente cosa può uscire e cosa c'è sul tuo computer. Le affermazioni si basano sul codice sorgente
(2026-11-04, versione 0.3.0).

## Cosa esce dal tuo computer, e quando

| Indirizzo | Quando | Cosa riceve | Nota |
|---|---|---|---|
| `tile.openstreetmap.org` | quando la mappa richiede delle tile | le coordinate della tile (zoom, x, y) e il nome del programma nello User-Agent (`bike-osm-ride/0.1 (desktop app)`) | Le tessere sono aggiornate per 30 giorni; le copie offline restano fino a 90 giorni / 512 MiB totali, con pulizia all’avvio e ogni ora durante l’uso. |
| `brouter.de` | se incolli un link di bikerouter | i punti letti dal link (longitudine, latitudine), il profilo e le opzioni di evitamento | il percorso lo calcola il loro server |
| `api.open-meteo.com` | solo se nel GPX non ci sono dati di altitudine utilizzabili | le coordinate dei punti campione del percorso (al massimo 100 per richiesta) | CC BY 4.0, per uso non commerciale |
| `api.github.com` | all'avvio, cinque secondi dopo | la richiesta dell'ultima release, `User-Agent: bike-osm-ride/<version>` | se fallisce resta in silenzio; **non si può disattivare nelle Impostazioni** |
| `github.com` (download) | solo se premi il pulsante *Aggiorna* | il download del nuovo pacchetto | |
| `maps.googleapis.com`, `www.google.com` | solo se hai attivato Street View e inserito una chiave API | la posizione del ciclista (coordinata) e la **tua** chiave Google | valgono i termini di Google; vedi [`STREETVIEW.md`](../../STREETVIEW.md) |
| le pagine aperte in Firefox | solo se usi il pannello Browser (Windows) | quello che guardi lì dentro | un profilo Firefox separato, il tuo browser normale resta intatto |

Non c'è altro. Non c'è nessun login al programma, nessun invio di segnalazioni di errore, nessuna statistica d'uso. Il traffico
Bluetooth passa solo tra il rullo intelligente, i sensori e il computer.

## Cosa c'è sul tuo computer

Tutto sta in un'unica cartella (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride`, su Android la cartella privata dell'applicazione). La struttura è descritta in
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); in breve:

- **impostazioni** (`settings.json`): peso corporeo, FTP, frequenza cardiaca massima, aspetto, layout, la tua chiave API di Google;
- **percorsi e registro delle uscite**: la versione elaborata dei percorsi aperti, la registrazione completa secondo per secondo di
  ogni uscita conclusa (posizione, watt, frequenza cardiaca, cadenza);
- **tile**, le **immagini Street View** scaricate (per un massimo di 30 giorni) e gli id dei panorami;
- **log**: `ble.log` (i pacchetti di dati grezzi del rullo intelligente e dei sensori: watt, frequenza cardiaca, comandi di controllo;
  non contengono il nome né l'indirizzo del dispositivo) e `crash.log` (errori intercettati); entrambi vengono ruotati e restano sul computer;
- il profilo Firefox del pannello Browser;
- i residui del caricamento su Strava delle versioni **0.3.1 e precedenti**: le cartelle `strava` e `strava-webview` (in quest'ultima ci sono
  i cookie del tuo accesso a Strava). Il programma non le usa più e non le elimina: puoi eliminarle tranquillamente.

**Backup.** *Impostazioni → Backup* crea un unico file zip con i tuoi percorsi, le tue uscite, gli allenamenti che hai importato e le tue
impostazioni. Il file va dove lo salvi tu (su Android con la finestra “Salva con nome” del sistema); il programma non manda niente da nessuna
parte e non lo carica nemmeno. La chiave API di Google **non c'è**, e al ripristino non accetta nemmeno la chiave che arriva dal file; non ci
sono nemmeno le tile, le immagini Street View e i log. Il file contiene i percorsi esatti delle tue uscite e i tuoi dati di salute (frequenza
cardiaca, watt), quindi trattalo come gli altri tuoi file personali.

**Segreti.** L'unico segreto è la chiave API di Google. In `settings.json` sta *in chiaro* (non è cifrata), quindi proteggi la cartella come
gli altri tuoi file personali. Il programma non la registra mai nei log, non finisce in un messaggio di errore e non compare nell'output di
debug, e va solo verso Google (o verso un server di test locale su `127.0.0.1` durante lo sviluppo).

**Eliminazione.** Eliminando la cartella sparisce tutto. Le immagini Street View e gli id si possono eliminare anche separatamente in
*Impostazioni → Street View*; il registro delle uscite di un percorso rimosso dall'elenco dei percorsi recenti resta.

### File delle uscite importate

**Importa un'uscita** legge un file GPX o TCX che scegli tu (o che trascini dentro). Il programma elabora il file sul tuo computer e salva
solo il risultato (la registrazione secondo per secondo: posizione sul percorso, altitudine, velocità, frequenza cardiaca, cadenza, watt) nella
cartella `rides`, proprio come le tue uscite. Il contenuto originale del file (per esempio il nome e i timestamp scritti da Strava) non finisce
da nessun'altra parte, e non esce niente sulla rete. Anche il calcolo dei watt stimati avviene in locale.

## Cose da sapere

- OpenStreetMap, BRouter, Open-Meteo, GitHub e Google vedono le tue richieste (indirizzo IP, orario); è così con qualsiasi client di rete.
- Al momento non c'è nessuna impostazione per disattivare il controllo degli aggiornamenti. Se non lo vuoi, blocca `api.github.com` con un
  firewall; il programma non si rompe per questo.
- Il programma non è un prodotto di Google o di Zwift e non ha nessun legame con loro.
