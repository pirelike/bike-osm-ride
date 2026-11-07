# Manuale dell'utente

> **Traduzione.** La versione autorevole di questo documento è il [manuale](../../USER_GUIDE.md) ungherese;
> è disponibile anche in [inglese](../en/USER_GUIDE.md), [spagnolo](../es/USER_GUIDE.md),
> [portoghese](../pt/USER_GUIDE.md), [tedesco](../de/USER_GUIDE.md) e [cinese](../zh/USER_GUIDE.md), e questa
> è la versione italiana.
> La lingua dell'interfaccia si cambia in *Impostazioni → Aspetto → Lingua*.

Come usare bike-osm-ride dal primo avvio al riepilogo della pedalata. L'installazione e la panoramica breve sono nel
[README](../../../README.md); i dettagli per sviluppatori sono in [`ARCHITECTURE.md`](../../ARCHITECTURE.md) e nei
documenti che lo accompagnano. Gli screenshot sono stati realizzati con la versione 0.3.0 e mostrano l'interfaccia
nella sua lingua originale, l'ungherese; l'app parla anche inglese, spagnolo, portoghese, tedesco, italiano e cinese. Il percorso
mostrato nel documento è un percorso reale di BRouter che è nel repository come dato di test
(© collaboratori di OpenStreetMap, ODbL).

**Contenuto:** [Primo avvio](#primo-avvio) · [Caricare un percorso](#caricare-un-percorso) ·
[Collegare i dispositivi](#collegare-i-dispositivi) · [La pedalata](#la-pedalata) ·
[Lo schermo e il layout](#lo-schermo-e-il-layout) · [Allenamenti](#allenamenti) ·
[Cambio virtuale](#cambio-virtuale) · [Fantasma e segmenti](#fantasma-e-segmenti) ·
[Riepilogo e TCX](#riepilogo-e-tcx) · [Street View](#street-view-opzionale) ·
[Telefono e tablet](#telefono-e-tablet) · [Impostazioni](#impostazioni) ·
[Scorciatoie da tastiera](#scorciatoie-da-tastiera) · [Risoluzione dei problemi](#risoluzione-dei-problemi)

## Primo avvio

All'avvio si apre la finestra **Percorso** sopra lo schermo della pedalata vuoto. Ci sono tre modi per iniziare: un
file GPX, un link di bikerouter o il percorso dimostrativo. Anche senza hardware puoi pedalare con il
**Ciclista simulato** (vedi [Collegare i dispositivi](#collegare-i-dispositivi)).

![La finestra Percorso](../../screenshots/route-dialog.png)

## Caricare un percorso

- **File GPX.** Scegliendolo dal disco (`Apri un GPX`, `Ctrl+O`) o trascinandolo sulla finestra. Va bene qualsiasi GPX:
  il programma usa i punti `<trk>`/`<trkseg>` (più tratti vengono concatenati uno dopo l'altro), e se non ci sono, i
  punti `<rte>`; capisce anche un file con accenti, BOM e namespace; per un file danneggiato (troncato) dà un messaggio
  di errore comprensibile, non un percorso accorciato. Il limite di dimensione è 64 MB.
- **Link di bikerouter.** Pianifica un percorso su [bikerouter.de](https://bikerouter.de), clicca sul link *Condividi* e
  incollalo nel campo (`Incolla`, poi `Carica un percorso`). Dal link arrivano i punti, il profilo, l'alternativa e le
  zone da evitare; il percorso lo calcola il server di BRouter. Funzionano anche gli indirizzi BRouter grezzi e i
  parametri `lonlats=…`.
- **Percorso dimostrativo.** Un breve giro collinare di Budapest per provarlo (le sue coordinate corrono *vicino* alle
  strade, non sopra).
- **Percorsi recenti.** Gli ultimi 20, con i dati (distanza, dislivello, quando l'hai aperto, origine); il cestino lo
  elimina dall'elenco (il registro delle uscite resta).

Se nel GPX non ci sono dati di quota utilizzabili, il programma li chiede al servizio Open-Meteo (le coordinate
vengono inviate); se anche questo non riesce, il percorso si carica con un profilo piatto e un avviso. Dal file:

- se accanto alla traccia principale c'è una traccia *più corta* che giace sul percorso (per esempio un segmento di
  Strava incorporato) oppure una traccia contrassegnata con `<type>segment</type>`, quella diventa un **segmento**: si
  vede come una fascia sul profilo e durante la pedalata la scritta “Segmento: … X km mancanti” lo indica
  (l'esportazione GPX di Strava non contiene segmenti);
- se *tutti* i punti hanno una marca temporale (un'esportazione di Strava o di Garmin), il percorso è
  **anche il suo fantasma**: vedi il capitolo [Fantasma](#fantasma-e-segmenti).

Il programma cerca **salite** nel percorso: tratti lunghi almeno 500 m, con una pendenza media di almeno 3% e che
guadagnano almeno 25 m di dislivello, con categoria dalla 4ª alla HC (in base a lunghezza × pendenza media).

## Collegare i dispositivi

Il pulsante **Dispositivi** nella barra superiore (i tre puntini accanto mostrano quante funzioni sono collegate). Ci
sono cinque funzioni: **Rullo** (rullo intelligente, FTMS), **Frequenza cardiaca**, **Misuratore di potenza** (CPS),
**Sensore di cadenza** (CSC) e **Cambio** (Zwift Click/Play).

![La finestra Dispositivi](../../screenshots/devices.png)

1. `Avvia la ricerca`: il programma cerca senza filtri e classifica i dispositivi in base ai servizi annunciati (la
   ricerca si ferma da sola dopo 30 secondi).
2. Accanto a ogni dispositivo trovato ci sono i pulsanti delle funzioni: lo colleghi alla funzione che ti serve. Un
   dispositivo fisico può coprire **più funzioni contemporaneamente**: se il tuo rullo trasmette anche la potenza, la
   cadenza o la frequenza cardiaca, un'unica connessione riempie tutte le schede (le schede mostrano l'indicazione
   *dispositivo condiviso*). Tra i dati dei sensori vince la fonte più precisa (potenza: il misuratore di potenza prima
   del rullo; cadenza: CSC, poi il misuratore di potenza, poi il rullo; frequenza cardiaca: la fascia toracica prima del
   rullo).
3. Ciò che non si è aggiornato da 3 secondi passa a “—”; se la connessione cade, il programma riprova tre volte.
4. **Collega il rullo nella funzione Rullo**: la funzione misuratore di potenza legge soltanto, il controllo (salite,
   ERG) arriva dalla funzione Rullo.
5. **Ciclista simulato**: il primo elemento dell'elenco, sempre presente, anche senza Bluetooth. Nel pannello a sinistra
   puoi impostare i watt con un cursore (oppure `↑`/`↓`, ±10 W); pulsanti rapidi: Z2 150 W, Z4 250 W, Sprint 700 W,
   Libero 0 W. La cadenza e la frequenza cardiaca lo seguono in modo credibile.

**Come controlla il rullo?** In salita il rullo riceve la pendenza del tratto davanti a te (circa 5–45 m avanti,
smussata, fino a ±20%, scalata a 0–150% con l'impostazione *Intensità della pendenza*). In pausa e alla fine il rullo
passa a una resistenza piatta e neutra. Durante un allenamento (ERG) mantiene i watt obiettivo. Il rullo riceve i
comandi attraverso un *filtro di sicurezza*: escono solo le operazioni necessarie e gli obiettivi sono limitati
all'intervallo del rullo stesso.

> **Importante, con onestà.** Il controllo ERG l'abbiamo provato con un rullo vero (VE200); la simulazione delle salite
> con un rullo vero non l'abbiamo ancora verificata, e il controller Zwift Click/Play, il cardiofrequenzimetro separato
> e il misuratore di potenza non li abbiamo ancora provati con un dispositivo vero. I dettagli:
> [`TESTING.md`](../../TESTING.md).

## La pedalata

`Avvia` (o `Space`) fa partire la pedalata, `Pausa` la ferma (anche il tempo si ferma), `Termina` la chiude. Alla fine
del percorso la pedalata **finisce da sola**. Prima di iniziare, cliccando sul profilo puoi saltare a qualsiasi punto del
percorso; durante la pedalata non è possibile. Il programma tiene la macchina sveglia finché pedali (Windows, Linux,
macOS; su Android no).

La velocità viene dai **watt** misurati, con il modello di Martin validato con misurazioni su strada (gravità,
rotolamento, attrito dei cuscinetti, resistenza dell'aria con i raggi, inerzia), dal tuo peso corporeo, dal peso della
bici, dal CdA e dal Crr; con l'interruttore *Usare la velocità del rullo* puoi usare anche la velocità del rullo stesso.
Dettagli: [`SIMULATION.md`](../../SIMULATION.md).

![La pedalata, tema scuro](../../screenshots/ride-dark.png)

## Lo schermo e il layout

Lo schermo della pedalata è fatto di **pannelli**: *Mappa*, *Potenza* (il numero grande, media di 3 s, W/kg, zona),
*Prossima salita*, *Statistiche*, *Profilo altimetrico*, *Controlli*, *Grafici*, *Allenamento*, *Ciclista simulato*,
*Cambio*, *Browser*. Funzionano come in un window manager a tiling: **niente sovrapposizioni e niente buchi.**

- **Ridimensionamento:** trascinando il separatore (la linea si aggancia alle celle di una griglia di 96 × 64,
  rispettando la dimensione minima dei pannelli; cambiano solo i due pannelli vicini).
- **Spostamento:** afferrando un pannello dalla sua intestazione e lasciandolo sul *bordo* di un altro pannello lo
  divide in due; lasciandolo al *centro* si scambiano di posto (una cornice fantasma mostra cosa succederà).
- **Chiusura:** il pulsante × del pannello; il vicino occupa il suo posto e il pannello passa nella barra superiore
  come *chip*. Cliccando sul chip torna dove era; trascinandolo sull'interfaccia va dove lo lasci.
- **Tutto viene salvato.** *Impostazioni → Layout → Ripristina il layout* riporta alla posizione predefinita.
- Il programma **memorizza il layout (e la vista del profilo) separatamente con e senza allenamento**: all'allenamento
  selezionato appartiene un layout proprio, che segue quello della pedalata libera finché non lo modifichi lì.

![Tema chiaro](../../screenshots/ride-light.png)

**Mappa.** La mappa mostra le tile standard di OpenStreetMap; gli stili *Scuro* e *Chiaro* le filtrano in locale, *OSM*
è l'originale. Con la rotellina ingrandisci intorno al mouse, trascinando sposti la vista; i pulsanti in basso a
sinistra attivano la modalità inseguimento e centrano la mappa (`M`). Il colore del percorso indica la pendenza (freddo:
discesa, grigio: pianura, giallo–arancione–rosso–porpora: salita sempre più ripida), e il ciclista è il disco con la
freccia. In basso l'attribuzione OSM e la scala.

**Profilo altimetrico.** `Completo` (l'intero percorso) o `Anteprima` (i prossimi ~3 km; la lunghezza si imposta nelle
impostazioni, `P`); le salite come bandierine con la categoria, la scala dei km in basso; clic per saltare (prima della
pedalata), muovi il mouse per leggere i dati. La scala dell'anteprima cambia solo quando serve. Se c'è un allenamento
selezionato, l'interruttore **Altimetria | Allenamento** mostra i blocchi dell'allenamento.

**Dimensione del testo.** *Impostazioni → Aspetto → Dimensione del testo* (80–160%).

**Lingua.** *Impostazioni → Aspetto → Lingua*: “Automatico” (segue la lingua di sistema se il suo testo è completo),
“Ungherese”, “Inglese”, “Spagnolo”, “Portoghese”, “Tedesco”, “Italiano”, “简体中文”. La modifica ha effetto immediato; non rinomina i percorsi e
le uscite che hai nominato prima (restano come li hai chiamati).

## Allenamenti

Il pulsante **Allenamento** nella barra superiore (o `W`). La libreria integrata ha **93 allenamenti** in 12 categorie
(recupero, resistenza, ritmo, sweet spot, soglia, VO2max, anaerobico, sprint, salita, gara, test, misto), tra cui il test
FTP da 20 e 8 minuti, i test ramp e CP, e molte sedute che seguono i protocolli di ricerca pubblicata (Rønnestad 30/15,
Seiler 4 × 8, Helgerud 4 × 4, Billat 30/30, gli intervalli sprint di Gibala e altri; le fonti sono, per ogni allenamento,
nel campo `sources` di `workouts.json`).

![Il selettore degli allenamenti](../../screenshots/workout-picker.png)

- A sinistra le categorie e le **etichette** (a cosa serve, livello, carattere, periodo; filtrano con logica AND, con accanto il
  numero di risultati), al centro l'elenco, a destra la descrizione (cosa, perché, come), lo
  **schema dei blocchi colorato per zona** e l'elenco blocco per blocco con tempo, obiettivo e istruzione del coach.
- Gli obiettivi si calcolano da **%FTP**: l'FTP lo imposti in *Impostazioni → Ciclista*; se lo modifichi, la parte
  rimanente si ricalcola subito.
- Durante la pedalata il rullo mantiene i watt obiettivo in **modalità ERG**; la scheda *Allenamento* e la vista
  *Allenamento* del pannello del profilo mostrano il blocco, il tempo rimanente, il blocco successivo e il
  **messaggio del coach** (al momento giusto dell'allenamento, per 12 secondi, solo lì, non come popup). Anche qui la
  pausa e l'uscita dall'allenamento rimettono il rullo in neutro.
- **Il tuo allenamento:** *Importa un allenamento…* legge un file MyWhoosh o Zwift (`.xml`, `.zwo`) e anche l'esportazione
  JSON di MyWhoosh; finisce nella cartella `workouts/` e da lì è selezionabile come gli altri.
- Durante una pedalata non si può cambiare allenamento.

![Allenamento in modalità ERG](../../screenshots/workout.png)

## Cambio virtuale

*Impostazioni → Rullo → Cambio virtuale.* Trasmissione 2 × 12 (50/34 e 11–34), **23 marce**. Si cambia marcia con i
tasti `←`/`→`, dal pannello Cambio, con i pulsanti del cambio sul telefono o con un controller Zwift Click/Play. Poiché un
rullo intelligente non ha cambio, la marcia modifica la pendenza inviata al rullo (la più leggera circa −5%, la più dura
circa +5%); il pannello mostra il rapporto della marcia e la velocità che deriva dalla cadenza.
**Durante un allenamento (ERG) il cambio non si vede e non ha effetto** (decidono i watt obiettivo). Con un controller
Zwift Click/Play la prima cambiata accende il cambio; il pulsante `+` del Click va verso il più duro, `−` verso il più
leggero, sul controller destro del Play `Shift`/`Y`, sul controller sinistro il pulsante `Shift`/`B` (questo non è ancora
stato provato con un controller vero).

## Fantasma e segmenti

Ogni uscita conclusa **viene salvata automaticamente** con il suo percorso (registrazione secondo per secondo,
riepilogo, tempi dei segmenti), e sullo stesso percorso un'uscita precedente si può riprodurre come **fantasma**: aspetta
sulla linea di partenza già prima del via e, durante la pedalata, un indicatore traslucido sulla mappa e sul profilo
mostra dove si trovava nel tuo stesso momento; il vantaggio o lo svantaggio si legge in secondi.
*Impostazioni → Mappa e profilo → Fantasma*:

| Modalità | Cosa riproduce |
|---|---|
| Spento | nulla |
| Tempo del percorso | le marche temporali del GPX caricato (con un'esportazione di Strava o Garmin puoi gareggiare già prima della prima uscita; nei percorsi di BRouter non c'è tempo) |
| Ultima uscita | l'ultima uscita del percorso |
| Migliore (PR) | l'uscita **completa** con il tempo più basso; se nessuna è ancora arrivata, quella che è arrivata più lontano (parziale), che si ferma alla sua fine |

La riga sotto il selettore ti dice perché al momento non c'è un fantasma. Le salite riconosciute sono **segmenti locali**:
prima del via si vede il miglior tempo (PR), dentro il segmento la differenza misurata al secondo, e nel riepilogo il
badge “PR precedente” e “Nuovo PR!”. Un'uscita fatta con il **ciclista simulato** viene salvata, ma non sarà un PR e non
sarà un fantasma “Migliore”.

## Riepilogo e TCX

Alla fine del percorso (o dopo `Termina`) si apre il **riepilogo**: distanza, tempo, dislivello, potenza media e
normalizzata, IF, TSS, energia, velocità media, frequenza cardiaca, la tabella dei segmenti e la distribuzione delle zone
di potenza e di frequenza cardiaca.

![Il riepilogo](../../screenshots/summary.png)

In basso il pulsante `Salva come TCX` (schema Garmin; il nome di file suggerito è composto da data e nome del percorso)
e `Nuovo percorso`.

**Caricamento su Strava, Intervals.icu, TrainingPeaks:** un caricamento integrato non c'è. Sul sito di Strava, nel menu
*Carica file*, puoi caricare a mano il TCX salvato; anche Intervals.icu e TrainingPeaks lo accettano. Fino alla 0.3.1
c'era un caricatore in background, ma da noi il login andava rifatto dopo ogni uscita, per questo l'abbiamo tolto
(0.3.2).

## Analisi e confronto delle uscite

Il pulsante **`Analisi`** (barra superiore; sul telefono, in fondo alla finestra `Percorso`,
*Analisi delle uscite salvate*) apre una pagina a tutto schermo. `Esc` o `Indietro` la chiude; sotto, lo schermo della
pedalata resta invariato.

![La pagina Analisi](../../screenshots/review.png)

**Elenco.** A sinistra il percorso (nel selettore tutti i percorsi con un'uscita salvata e quello caricato ora) e le sue
uscite, dalla più recente. Selezionane al massimo quattro: ognuna riceve una lettera (A–D) e un colore, e anche il motivo
della linea nel grafico è diverso (la A è continua). **La A è l'uscita di riferimento**: il grafico *Differenza di tempo* e
la colonna *Differenza* della tabella si riferiscono a lei (positivo = più lenta). Il cestino chiede conferma
prima di eliminare. Per impostazione predefinita sono selezionate le due uscite più recenti del percorso.

**Grafico.** *Potenza*, *Frequenza cardiaca*, *Cadenza*, *Velocità* o *Differenza di tempo* in funzione della distanza.
Se muovi il mouse (o il dito) sul profilo o sul grafico, i valori delle uscite nel punto del cursore si leggono nella
legenda, e un punto sulla mappa indica il luogo. Se un'uscita non ha quel sensore (per esempio senza
cardiofrequenzimetro), non ha una linea.

**Zoom su un tratto.** Trascina il mouse (o il dito, su uno schermo tattile) lungo il profilo o il grafico, oppure clicca
sui pulsanti delle *salite*; `Percorso completo` ripristina. La mappa si avvicina al tratto e lo evidenzia, il grafico e
la tabella si restringono allo stesso tratto. Un trascinamento più corto di 100 m non conta come tratto.

**Tabella del tratto.** Per ogni uscita: tempo, velocità media, potenza media e normalizzata (NP, da almeno 30 secondi di
dati), frequenza cardiaca media e massima, cadenza, dislivello, VAM e la differenza rispetto all'uscita di riferimento.
Ci sono dati solo per un tratto che l'uscita ha **percorso per intero**; un'uscita parziale resta vuota sugli altri
tratti.

### Potenza: picchi, FTP, carico

![La pagina Potenza](../../screenshots/review-power.png)

In cima all'Analisi il selettore **Confronto | Potenza** porta all'altra pagina. Qui ci sono i numeri di **tutte** le tue
uscite, indipendentemente dal percorso:

- **Potenza di picco.** La miglior media di watt su 5 secondi, 1, 5, 20 e 60 minuti, con la data e il percorso
  dell'uscita. Se nel Confronto hai selezionato delle uscite, i loro valori stanno accanto, in colonne (A–D). Un'uscita
  non ha un valore per una durata più lunga della sua.
- **FTP.** Suggerisce come FTP il 95% della tua miglior potenza di 20 minuti e accanto mostra quello impostato ora. Il
  pulsante **Imposta l'FTP a N W** lo scrive nelle Impostazioni; l'app non lo modifica mai da sola. Il TSS delle uscite
  salvate resta calcolato con l'FTP in vigore in quel momento. Per un suggerimento esatto serve un vero sforzo massimale
  di 20 minuti: l'allenamento *Test FTP 20 minuti* (*Allenamenti*, `W`) dà esattamente questo.
- **Carico.** Le ultime 12 settimane o mesi (*Settimanale* | *Mensile*): le colonne mostrano il TSS, sopra il valore, e il
  tooltip (con il cursore sulla colonna) mostra il numero e il tempo delle uscite.

**Contano solo i watt misurati.** Il ciclista simulato e le uscite con watt stimati (`~`) non entrano nei picchi e nel
suggerimento FTP (nel tempo del trend sì, nel suo TSS no). Un'uscita importata in cui c'era potenza reale conta. I watt
del rullo stesso non sono un misuratore di potenza, perciò anche il suggerimento FTP è preciso solo quanto quello. I
numeri delle uscite più vecchie vengono calcolati e salvati alla prima apertura, e con molte uscite può richiedere
qualche secondo.

### Importare un'uscita da un'altra applicazione

Con il pulsante **`Importa un'uscita`** (oppure trascinando il file sulla pagina) carichi un file GPX o TCX, per esempio
uno esportato da Strava, Garmin o Wahoo. La frequenza cardiaca, la cadenza e (se ci sono) i watt passano. Da sapere:

- **Sei tu a fornire il percorso**: l'uscita viene allineata al percorso selezionato nella pagina (prima caricalo o
  selezionalo). I punti della traccia vengono proiettati sul percorso; quello che resta a più di 30 m da esso (una
  deviazione, un errore GPS) viene lasciato fuori, e la pagina indica quanti sono stati esclusi. Se l'uscita copre solo
  una parte del percorso, avvisa.
- La **pausa automatica** (una sosta lunga) non conta nel tempo. Un file senza marche temporali (per esempio un percorso
  pianificato) non si può importare come uscita.
- La stessa uscita (la stessa ora di inizio) non si può importare due volte.
- L'uscita importata **non sarà un PR e non sarà un fantasma**: un'uscita all'aperto con soste e ruota libera non è
  confrontabile con i tuoi tempi sul rullo. Nell'elenco l'etichetta `Importata` la segnala.
- Sul telefono l'importazione non è ancora disponibile. Il formato FIT non lo leggiamo; Strava esporta sia GPX sia TCX.

### Watt stimati

Se nel file **non ci sono dati di potenza**, il programma esegue la fisica della simulazione al contrario: da velocità e
pendenza calcola quanti watt sono serviti (la variazione di energia cinetica e il lavoro contro gravità, rotolamento,
cuscinetti e aria in una finestra di 9 secondi). In un'uscita così i numeri ricevono un segno **`~`** e compare
l'etichetta `Watt stimati`.

Con l'icona del cursore (*Le impostazioni della stima*) puoi indicare con i dati di chi calcolare:

| Scelta | Cosa usa |
|---|---|
| **I miei dati** | il peso corporeo, la bici e il Crr delle Impostazioni |
| **Valori personalizzati** | il *peso corporeo*, la *bici* e la *resistenza al rotolamento (Crr)* indicati qui, se stai guardando l'uscita di un'altra persona |

Il CdA, l'efficienza della trasmissione e la densità dell'aria vengono sempre dalle Impostazioni. Il Crr su asfalto buono
è circa 0,004; **per il gravel non c'è un modello separato**, lì cambia solo il Crr indicato. `Ricalcola` calcola con i
nuovi valori e salva l'uscita; i watt misurati il programma non li sostituisce mai con una stima.

**Quanto è preciso?** È un'approssimazione. Non conosce il vento né la scia (il vento contrario sembra meno watt), la
pendenza viene dal percorso, non dalla quota rumorosa del GPS del file, e nelle curve strette la proiezione può dare una
velocità rumorosa per un breve tratto. Calcolando al contrario con la fisica del programma stesso, il risultato torna
entro circa il 3%; non l'abbiamo ancora confrontato con un misuratore di potenza reale, perciò conviene contare su una
differenza del 10–15%.

## Street View (opzionale)

Google lo consente solo tramite la sua API, perciò serve **la tua chiave Google Maps API**
(*Impostazioni → Street View*; i passaggi sono descritti lì: progetto Google Cloud, fatturazione, attivazione di
*Maps Embed API* e *Street View Static API*). Senza chiave non appare nulla. `V` o il pulsante `Street View` del pannello
della mappa apre una piccola finestra nell'angolo in basso a destra della mappa; la `X` (o il pulsante di scambio)
scambia le due viste. In caso di errore arriva un messaggio comprensibile (chiave non valida, quota esaurita, nessun
panorama in questo luogo). Il pulsante **Scarica** salva un'immagine ogni 75–250 m lungo il percorso (al massimo circa
900; valide 30 giorni, cancellabili nelle Impostazioni), così la prossima volta puoi pedalare la stessa strada senza
chiave e senza rete. Il download usa la tua quota Google. Dettagli e le regole di Google:
[`STREETVIEW.md`](../../STREETVIEW.md).

## Telefono e tablet

In una finestra più stretta di 1000 px compare una vista a schede: in basso **Pedalata · Mappa · Grafici · Dati**, sopra
la grande fila di pulsanti `Avvia`/`Pausa`/`Termina`. Nella scheda **Pedalata** c'è tutto quello che serve durante la
pedalata su un solo schermo: il blocco dell'allenamento (tempo rimanente, blocco successivo, il messaggio del coach), la
potenza misurata rispetto all'obiettivo (*in target / bassa / alta*), la barra delle zone, cadenza, frequenza cardiaca,
velocità, pendenza e, sotto, il profilo (trascinando di lato passi tra le sue viste). Sul telefono le finestre di dialogo
sono a tutto schermo.

<p align="center">
  <img src="../../screenshots/phone-ride.png" width="280" alt="Layout sul telefono, scheda Pedalata con un allenamento">
  &nbsp;&nbsp;
  <img src="../../screenshots/phone-map.png" width="280" alt="Layout sul telefono, scheda Mappa">
</p>

L'app Android è lo stesso programma come APK. Carichi un percorso con il selettore di file di sistema, con un link di
bikerouter, dai recenti o con il percorso dimostrativo; al primo avvio chiede i permessi Bluetooth; durante la pedalata lo
schermo resta acceso (in pausa e alla fine dell'uscita torna attivo il limite di tempo del sistema; sul telefono non
l'abbiamo ancora provato, [`ANDROID.md`](../../ANDROID.md)). Se l'app riceve un errore, una barra rossa mostra la prima
riga e il programma resta utilizzabile.

## Impostazioni

Ogni modifica ha effetto immediato e viene salvata.

![La finestra Impostazioni](../../screenshots/settings.png)

| Gruppo | Cosa imposti |
|---|---|
| **Ciclista** | peso corporeo (75 kg), il peso della bici (9 kg), FTP (200 W), frequenza cardiaca max. (190), CdA (0,32 m²), Crr (0,004) |
| **Rullo** | intensità della pendenza (0–150%), uso della velocità del rullo, cambio virtuale |
| **Aspetto** | lingua (automatico/ungherese/inglese/spagnolo/portoghese/tedesco/italiano/cinese), dimensione del testo (80–160%), tema (scuro/chiaro), stile della mappa (scuro/chiaro/OSM) |
| **Mappa e profilo** | la mappa segue il ciclista, vista del profilo (anteprima/completo), lunghezza dell'anteprima, fantasma |
| **Layout** | ripristino del layout predefinito |
| **Backup** | *Salva su file* e *Ripristina da file* (vedi sotto) |
| **Street View** | visualizzazione, la chiave Google, salvataggio degli id dei panorama, download delle immagini, cancellazione dei dati salvati |

### Backup

*Salva su file* crea un unico **file zip** dei tuoi percorsi, delle tue uscite salvate, dei tuoi allenamenti importati e
delle tue impostazioni. Il file lo scegli tu (su Android con la finestra “Salva con nome” del sistema) e nulla esce verso
la rete. **Non contiene** la chiave di Street View, le tile della mappa, le immagini di Street View scaricate e i log:
o sono segreti o si possono riscaricare in qualsiasi momento.

*Ripristina da file* **unisce**: i percorsi, le uscite e gli allenamenti del backup si aggiungono a quelli esistenti,
**niente viene eliminato e nessuna uscita esistente viene sovrascritta**. Le impostazioni del backup però entrano in
vigore (la tua chiave di Street View resta quella vecchia), perciò l'app chiede prima. Ripristinare due volte lo stesso
file non fa nulla. Quando passi a una macchina nuova, basta ripristinare il file. Durante una pedalata non puoi creare né
ripristinare un backup; se il file è danneggiato o è di una versione più recente, l'app lo dice e non scrive nulla. Su
Android questa parte non l'abbiamo ancora provata su un telefono.

## Scorciatoie da tastiera

| Tasto | Azione |
|---|---|
| `Space` | avvia / pausa / riprende (alla fine riporta il riepilogo) |
| `M` | attiva/disattiva l'inseguimento sulla mappa |
| `P` | profilo altimetrico: completo ↔ anteprima |
| `T` | tema scuro/chiaro |
| `V` | attiva/disattiva il pannello Street View |
| `X` | scambia mappa e Street View |
| `W` | selettore degli allenamenti |
| `←` / `→` | cambio virtuale: più leggero / più duro |
| `↑` / `↓` | potenza del ciclista simulato ±10 W |
| `F11` | schermo intero |
| `Esc` | chiude la finestra, esce dallo schermo intero |
| `Ctrl+O` | apre un GPX |

Digitando in un campo di testo i tasti non attivano comandi (tranne `Esc`, `F11` e `Ctrl+O`); con una finestra aperta
funzionano solo `Esc`, `F11` e, nella finestra Percorso, `Ctrl+O`.

## Pannello del browser (solo Windows)

Il pannello **Browser** incorpora nella finestra il **Firefox** installato sulla macchina, per poter guardare una serie o
qualsiasi altra cosa durante la pedalata. Firefox usa un proprio profilo (i tuoi accessi restano, il tuo Firefox normale
non viene toccato). Su Mac e Linux il pannello non compare. All'apertura di una finestra o durante il trascinamento di un
pannello Firefox si nasconde temporaneamente (l'interfaccia non riesce a disegnare sopra una finestra nativa); chiudere il
pannello chiude anche Firefox; deve essere largo almeno 500 px circa.

## Risoluzione dei problemi

| Sintomo | Cosa fare |
|---|---|
| Il rullo non reagisce alle salite / all'allenamento | Nella finestra **Dispositivi** deve essere collegato nella funzione *Rullo* (la funzione misuratore di potenza legge soltanto). Guarda il log BLE (`ble.log`, come descritto nel §11 di [`BLUETOOTH.md`](../../BLUETOOTH.md)); Zwift o un'app del telefono non devono tenere occupato il rullo. |
| Non trova il dispositivo | Accendi il Bluetooth; su Windows la ricerca è senza filtri, quindi si vedono molti dispositivi: guarda le funzioni del dispositivo. Su Android concedi i permessi Bluetooth (*Avvia la ricerca* richiede di nuovo); su macOS il sistema chiede il permesso alla prima ricerca. |
| La mappa è vuota o grigia | Non c'è rete e non c'è ancora nessuna tile in cache: il percorso e i numeri funzionano lo stesso. |
| Il percorso non si carica | Il messaggio di errore dice il motivo (file danneggiato, link non supportato, niente rete per BRouter). Il link deve contenere la parte `lonlats=`. |
| Il profilo è piatto | Il file non ha dati di quota e nemmeno Open-Meteo era raggiungibile. Carica un GPX con i dati di quota o un link di bikerouter. |
| Su Linux la finestra è vuota | Il programma imposta da solo la variabile `WEBKIT_DISABLE_DMABUF_RENDERER=1`; se resta vuota, prova con `WEBKIT_DISABLE_COMPOSITING_MODE=1`. |
| macOS: “non è possibile aprire” | Clic destro → Apri → Apri (l'app non è notarizzata); oppure `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`. |
| Windows: se SmartScreen avvisa | Il programma non è firmato digitalmente, perciò Windows può bloccarlo; si avvia da *Ulteriori informazioni → Esegui comunque*. |
| Crash / barra rossa | La prima riga è nella barra, il log completo è in `crash.log` (su Android `adb logcat -s bike-osm-ride`). Segnala l'errore con il log, per favore. |

**Dove sono i miei dati?** In un'unica cartella, insieme alle impostazioni, al registro delle uscite, alle tile e ai log
(se vuoi una copia, usa il pulsante *Impostazioni → Backup*): Windows `%LOCALAPPDATA%\bike-osm-ride`, macOS
`~/Library/Application Support/bike-osm-ride`, Linux `~/.local/share/bike-osm-ride`, Android la cartella privata
dell'app. La struttura esatta è descritta in [`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); cosa esce dalla macchina,
in [`PRIVACY.md`](../../PRIVACY.md).
