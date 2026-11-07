# Benutzerhandbuch

> **Übersetzung.** Maßgeblich ist die ungarische Fassung dieses Dokuments: [Benutzerhandbuch](../../USER_GUIDE.md);
> es ist auch auf [Englisch](../en/USER_GUIDE.md) und [Spanisch](../es/USER_GUIDE.md) verfügbar. Die Sprache der Oberfläche
> änderst du unter *Einstellungen → Erscheinungsbild → Sprache*.

bike-osm-ride vom ersten Start bis zur Zusammenfassung der Fahrt. Installation und der kurze Überblick stehen im
[README](../../../README.md); die Entwicklerdetails in [`ARCHITECTURE.md`](../../ARCHITECTURE.md) und seinen Begleitern.
Die Screenshots entstanden mit Version 0.3.0 und zeigen die Oberfläche in ihrer Quellsprache Ungarisch; die App spricht
auch Englisch, Spanisch und Deutsch. Die im Dokument gezeigte Route ist eine echte BRouter-Route, die im Repository als
Testdatei liegt (© OpenStreetMap-Mitwirkende, ODbL).

**Inhalt:** [Erster Start](#erster-start) · [Route laden](#route-laden) · [Geräte verbinden](#geräte-verbinden) ·
[Fahren](#fahren) · [Der Bildschirm und das Layout](#der-bildschirm-und-das-layout) · [Workouts](#workouts) ·
[Virtuelle Schaltung](#virtuelle-schaltung) · [Geist und Abschnitte](#geist-und-abschnitte) ·
[Zusammenfassung und TCX](#zusammenfassung-und-tcx) · [Street View](#street-view-optional) ·
[Telefon und Tablet](#telefon-und-tablet) · [Einstellungen](#einstellungen) · [Tastenkürzel](#tastenkürzel) ·
[Fehlersuche](#fehlersuche)

## Erster Start

Beim Start öffnet sich das Fenster **Route** über dem leeren Fahrtbildschirm. Es gibt drei Wege zu beginnen: eine GPX-Datei, ein
bikerouter-Link oder die Demo-Route. Ohne Hardware kannst du auch mit dem **simulierten Fahrer** fahren (siehe
[Geräte verbinden](#geräte-verbinden)).

![Das Fenster Route](../../screenshots/route-dialog.png)

## Route laden

- **GPX-Datei.** Durch Auswählen (`GPX öffnen`, `Strg+O`) oder durch Ziehen auf das Fenster. Jede GPX funktioniert: Das Programm nutzt
  die Punkte aus `<trk>`/`<trkseg>` (mehrere Segmente werden aneinandergereiht), und wenn es keine gibt, die Punkte aus `<rte>`;
  es versteht auch eine Datei mit Akzenten, BOM und Namensräumen; bei einer beschädigten (abgeschnittenen) Datei gibt es eine
  verständliche Fehlermeldung, keine verkürzte Route. Das Größenlimit liegt bei 64 MB.
- **Bikerouter-Link.** Plane eine Route auf [bikerouter.de](https://bikerouter.de), klicke auf den *Teilen*-Link und füge ihn in
  das Feld ein (`Einfügen`, dann `Route laden`). Die Punkte, das Profil, die Alternative und die Vermeidungen kommen alle
  aus dem Link; die Route berechnet der Server von BRouter. Rohe BRouter-Adressen und `lonlats=…`-Parameter funktionieren ebenfalls.
- **Demo-Route.** Eine kurze, hügelige Runde um Budapest zum Ausprobieren (ihre Koordinaten laufen *neben* den Straßen, nicht auf ihnen).
- **Letzte Routen.** Die letzten 20, mit Daten (Distanz, Höhe, wann du sie geöffnet hast, Quelle); der Papierkorb entfernt sie aus der
  Liste (das Fahrtenbuch bleibt).

Hat die GPX keine (brauchbaren) Höhendaten, fragt das Programm den Dienst Open-Meteo (die Koordinaten gehen hinaus); schlägt
auch das fehl, lädt die Route mit flachem Profil und warnt. Aus der Datei:

- liegt neben der Hauptspur eine *kürzere* Spur, die auf der Route liegt (zum Beispiel ein eingebetteter Strava-Abschnitt),
  oder eine Spur mit `<type>segment</type>`, wird daraus ein **Abschnitt**: Er erscheint als Band im Profil, und
  während einer Fahrt zeigt „Abschnitt: … noch X km“ ihn an (Strava's eigener GPX-Export enthält keinen Abschnitt);
- haben *alle* Punkte einen Zeitstempel (ein Strava- oder Garmin-Export), ist die Route **auch ihr eigener Geist**: siehe das
  Kapitel [Geist](#geist-und-abschnitte).

Das Programm sucht **Anstiege** in der Route: Abschnitte von mindestens 500 m Länge, mit durchschnittlich mindestens 3 % und mindestens
25 m Höhengewinn, mit einer Kategorie von der 4. bis HC (nach Länge × durchschnittlicher Steigung).

## Geräte verbinden

Die Schaltfläche **Geräte** in der oberen Leiste (drei Punkte daneben zeigen, wie viele Rollen verbunden sind). Es gibt fünf Rollen:
**Trainer** (Smart-Trainer, FTMS), **Puls**, **Leistungsmesser** (CPS), **Trittfrequenzsensor** (CSC) und **Schaltung**
(Zwift Click/Play).

![Das Fenster Geräte](../../screenshots/devices.png)

1. `Scan starten`: Das Programm sucht ohne Filter und ordnet die Geräte nach den Diensten ein, die sie anbieten (der
   Scan endet nach 30 Sekunden von selbst).
2. Neben jedem gefundenen Gerät stehen die Schaltflächen für die Rollen: Du verbindest es in der Rolle, die du brauchst. Ein physisches Gerät kann
   **mehrere Rollen gleichzeitig** erfüllen: Wenn dein Trainer auch Leistung, Trittfrequenz oder Puls sendet, füllt eine einzige Verbindung
   jede Karte (die Karten zeigen eine *gemeinsames Gerät*-Markierung). Bei den Sensordaten gewinnt die genauere Quelle (Leistung:
   der Leistungsmesser vor dem Trainer; Trittfrequenz: CSC, dann der Leistungsmesser, dann der Trainer; Puls: der Brustgurt
   vor dem Trainer).
3. Was sich 3 Sekunden lang nicht aktualisiert hat, wechselt auf „—“; bei einer verlorenen Verbindung versucht das Programm es dreimal.
4. **Verbinde den Trainer in der Rolle Trainer**: Die Rolle Leistungsmesser liest nur; die Steuerung (Anstiege, ERG) kommt aus der
   Rolle Trainer.
5. **Simulierter Fahrer**: der erste Eintrag in der Liste, immer da, auch ohne Bluetooth. Im linken Panel stellst du
   die Watt mit einem Schieberegler ein (oder `↑`/`↓`, ±10 W); Schnellschaltflächen: Z2 150 W, Z4 250 W, Sprint 700 W, Frei 0 W. Trittfrequenz und
   Puls folgen ihm glaubwürdig.

**Wie steuert es den Trainer?** An einem Anstieg bekommt der Trainer die Steigung des vor dir liegenden Abschnitts (etwa 5–45 m
voraus, geglättet, bis ±20 %, skaliert auf 0–150 % mit der Einstellung *Stärke der Steigung*). Bei einer Pause und am Ende geht
der Trainer auf einen flachen, neutralen Widerstand. In einem Workout (ERG) hält er die Zielwatt. Der Trainer erhält die
Befehle durch einen *Sicherheitsfilter*: Nur die nötigen Operationen gehen hinaus, und die Ziele werden auf den Bereich des Trainers
selbst begrenzt.

> **Wichtig, ehrlich.** Wir haben die ERG-Steuerung mit einem echten Trainer versucht (VE200); die Anstiegssimulation
> haben wir mit einem echten Trainer noch nicht bestätigt, und den Zwift-Click/Play-Controller, den separaten Pulssensor
> und den Leistungsmesser haben wir noch nicht mit echter Hardware versucht. Die Details: [`TESTING.md`](../../TESTING.md).

## Fahren

`Start` (oder `Leertaste`) startet die Fahrt, `Pause` hält sie an (die Zeit stoppt mit), `Beenden` schließt sie ab. Am Ende der Route
**endet die Fahrt von selbst**. Vor dem Start kannst du per Klick auf das Profil an jede Stelle der Route springen; während einer
Fahrt geht das nicht. Das Programm hält den Rechner wach, während du fährst (Windows, Linux, macOS; nicht unter Android).

Das Tempo kommt aus den gemessenen **Watt**, mit dem an Straßenmessungen validierten Modell von Martin (Schwerkraft, Rollen,
Lagerreibung, Luftwiderstand mit Speichen, Trägheit), aus deinem Körpergewicht, dem Gewicht des Rades, der CdA und dem Crr;
mit dem Schalter *Die Geschwindigkeit des Trainers verwenden* kannst du auch die Geschwindigkeit des Trainers selbst nutzen. Details:
[`SIMULATION.md`](../../SIMULATION.md).

![Fahren, dunkles Design](../../screenshots/ride-dark.png)

## Der Bildschirm und das Layout

Der Fahrtbildschirm besteht aus **Panels**: *Karte*, *Leistung* (die große Zahl, 3-s-Schnitt, W/kg, Zone), *Nächster Anstieg*, *Statistik*,
*Höhenprofil*, *Steuerung*, *Diagramme*, *Workout*, *Simulierter Fahrer*, *Schaltung*, *Browser*. Sie arbeiten wie in einem Kachel-
Fenstermanager: **Es gibt keine Überlappung und keine Lücke.**

- **Größe ändern:** durch Ziehen des Trenners (die Linie rastet an den Zellen eines 96 × 64-Rasters ein und respektiert die Mindestgröße
  der Panels; nur die beiden benachbarten Panels ändern sich).
- **Verschieben:** ein Panel an seiner Kopfzeile greifen und auf dem *Rand* eines anderen Panels loslassen teilt dieses Panel in zwei;
  auf der *Mitte* losgelassen tauschen sie die Plätze (ein Geisterrahmen zeigt, was passieren wird).
- **Schließen:** die ×-Schaltfläche des Panels; der Nachbar füllt seinen Platz, und das Panel wandert als *Chip* in die obere Leiste.
  Ein Klick auf den Chip bringt es zurück; auf die Fläche gezogen landet es dort, wo du es loslässt.
- **Alles wird gespeichert.** *Einstellungen → Layout → Layout zurücksetzen* stellt den Standard wieder her.
- Das Programm **speichert das Layout (und die Ansicht des Profils) getrennt mit und ohne Workout**: Das ausgewählte Workout
  hat sein eigenes Layout, das dem freien folgt, bis du es dort änderst.

![Helles Design](../../screenshots/ride-light.png)

**Karte.** Die Karte zeigt die Standardkacheln von OpenStreetMap; die Stile *Dunkel* und *Hell* filtern sie lokal, *OSM* ist das
Original. Zoomen mit dem Rad um die Maus, Verschieben durch Ziehen; die Schaltflächen unten links schalten den Folgemodus ein und
zentrieren ihn (`M`). Die Farbe der Route zeigt die Steigung (kalt: Abfahrt, grau: flach, gelb–orange–rot–violett:
zunehmend steiler Anstieg), und der Fahrer ist die mit einem Pfeil markierte Scheibe. Unten die OSM-Quellenangabe und der
Maßstab.

**Höhenprofil.** `Gesamt` (die ganze Route) oder `Vorausschau` (die nächsten ~3 km; ihre Länge lässt sich in den Einstellungen
einstellen, `P`); die Anstiege als Fähnchen mit ihrer Kategorie, die km-Skala unten; Klick zum Springen (vor einer Fahrt), Maus bewegen
zum Datenlesen. Die Skala der Vorausschau wechselt nur, wenn sie muss. Ist ein Workout gewählt, zeigt der Schalter **Höhe | Workout**
die Blöcke des Workouts.

**Textgröße.** *Einstellungen → Erscheinungsbild → Textgröße* (80–160 %).

**Sprache.** *Einstellungen → Erscheinungsbild → Sprache*: *Automatisch* (folgt der Sprache des Geräts, wenn ihr Text
vollständig ist), *Magyar*, *English*, *Español*, *Deutsch*. Der Wechsel wirkt sofort; er benennt Routen und Fahrten nicht um,
die du früher benannt hast (die bleiben, wie du sie genannt hast).

## Workouts

Die Schaltfläche **Workout** in der oberen Leiste (oder `W`). Die eingebaute Bibliothek hat **93 Workouts** in 12 Kategorien (Regeneration,
Ausdauer, Tempo, Sweet Spot, Schwelle, VO2max, anaerob, Sprint, Berg, Rennen, Test, gemischt), darunter die FTP-Tests über 20 und
8 Minuten, Ramp- und CP-Tests und viele Einheiten, die den Protokollen veröffentlichter Forschung folgen (Rønnestad
30/15, Seiler 4 × 8, Helgerud 4 × 4, Billat 30/30, Gibalas Sprintintervalle und andere; die Quellen stehen bei jedem Workout im
Feld `sources` von `workouts.json`).

![Die Workout-Auswahl](../../screenshots/workout-picker.png)

- Links stehen Kategorien und **Tags** (wofür es gut ist, Stufe, Charakter, Phase; sie filtern mit UND, mit der
  Anzahl der Treffer daneben), in der Mitte die Liste, rechts die Beschreibung (was, warum, wie), das
  **zonenfarbene Blockdiagramm** und die Liste Block für Block mit Zeit, Ziel und Coach-Anweisung.
- Die Ziele werden aus **%FTP** berechnet: Stelle deine FTP unter *Einstellungen → Fahrer* ein; änderst du sie, wird der restliche Teil
  sofort neu berechnet.
- Während einer Fahrt hält der Trainer die Zielwatt im **ERG-Modus**; die Karte *Workout* und die Ansicht *Workout* des Profil-Panels
  zeigen den Block, die verbleibende Zeit, den nächsten Block und die **Coach-Nachricht** (im richtigen Moment des Workouts,
  für 12 Sekunden, nur dort, nicht als Pop-up). Pause/Beenden setzt auch hier den Trainer auf neutral.
- **Dein eigenes Workout:** *Workout importieren…* liest eine MyWhoosh- oder Zwift-Datei (`.xml`, `.zwo`) und auch den JSON-Export von
  MyWhoosh; es landet im Ordner `workouts/`, und von da an lässt es sich wie die anderen auswählen.
- Während einer Fahrt lässt sich das Workout nicht wechseln.

![Workout im ERG-Modus](../../screenshots/workout.png)

## Virtuelle Schaltung

*Einstellungen → Trainer → Virtuelle Schaltung.* 2 × 12 Antrieb (50/34 und 11–34), **23 Gänge**. Du schaltest mit den Tasten `←`/`→`,
  im Panel Schaltung, mit den Schaltflächen am Telefon oder mit einem Zwift-Click/Play-Controller. Da ein Smart-
  Trainer keine Schaltung hat, verändert der Gang die Steigung, die an den Trainer geht (der leichteste etwa −5 %, der schwerste etwa
  +5 %); das Panel zeigt das Übersetzungsverhältnis und das Tempo, das aus der Trittfrequenz folgt. **Während eines Workouts (ERG) wird die Schaltung
  nicht angezeigt und wirkt nicht** (die Zielwatt entscheiden). Mit einem Zwift-Click/Play-Controller schaltet der erste Schaltvorgang
  die Schaltung ein; die `+`-Taste des Click schaltet Richtung schwerer, `−` Richtung leichter; beim rechten Controller des Play die
  `Shift`/`Y`-, beim linken die `Shift`/`B`-Taste (mit einem echten Controller noch nicht versucht).

## Geist und Abschnitte

Jede beendete Fahrt **wird automatisch gespeichert**, mit ihrer Route (Aufzeichnung pro Sekunde, Zusammenfassung, Abschnittszeiten), und auf
derselben Route lässt sich eine frühere Fahrt als **Geist** abspielen: Er wartet vor dem Start an der Startlinie, während einer Fahrt
zeigt eine halbtransparente Markierung auf Karte und Profil, wo er war, als du hier warst; Vorsprung oder Rückstand werden in
Sekunden angezeigt. *Einstellungen → Karte und Profil → Geist*:

| Modus | Was er abspielt |
|---|---|
| Aus | nichts |
| Zeit der Route | die eigenen Zeitstempel der geladenen GPX (mit einem Strava-/Garmin-Export kannst du vor der ersten Fahrt Rennen fahren; BRouter-Routen haben keine Zeit) |
| Letzte Fahrt | die letzte Fahrt der Route |
| Bester (PR) | die **vollständige** Fahrt mit der kleinsten Zeit; ist noch keine Fahrt beendet, die, die am weitesten kam (teilweise), die an ihrem eigenen Ende hält |

Die Zeile unter dem Auswahlfeld sagt dir, warum es gerade keinen Geist gibt. Die erkannten Anstiege sind **lokale Abschnitte**:
Vor dem Start wird die Bestzeit (PR) gezeigt, beim Fahren darin die auf die Sekunde gemessene Differenz, und in der Zusammenfassung
der „frühere PR“ und die Markierung „Neuer PR!“. Eine Fahrt mit dem **simulierten Fahrer** wird gespeichert, aber sie wird kein PR und
kein „Bester“-Geist.

## Zusammenfassung und TCX

Am Ende der Route (oder nach `Beenden`) öffnet sich die **Zusammenfassung**: Distanz, Zeit, Höhe, Durchschnitts- und normalisierte
Leistung, IF, TSS, Energie, Durchschnittstempo, Puls, die Abschnittstabelle und die Verteilung der Leistungs- und Puls-
zonen.

![Die Zusammenfassung](../../screenshots/summary.png)

Unten die Schaltfläche `Als TCX speichern` (Garmin-Schema; der vorgeschlagene Dateiname entsteht aus Datum und Routennamen)
und `Neue Route`.

**Hochladen zu Strava, Intervals.icu, TrainingPeaks:** Einen eingebauten Upload gibt es nicht. Auf Strava selbst kannst du im Menü *Upload
file* die gespeicherte TCX von Hand hochladen; Intervals.icu und TrainingPeaks akzeptieren sie auch. Bis 0.3.1 gab es einen
Hintergrund-Uploader, aber in unserem Fall musste die Anmeldung nach jeder Fahrt wiederholt werden, deshalb haben wir ihn entfernt (0.3.2).

## Fahrten ansehen und vergleichen

Die Schaltfläche **`Übersicht`** (obere Leiste; am Telefon unten im Fenster `Route`, *Gespeicherte Fahrten ansehen*) öffnet eine
Vollbildseite. `Esc` oder `Zurück` schließt sie; darunter bleibt der Fahrtbildschirm unverändert.

![Die Übersichtsseite](../../screenshots/review.png)

**Liste.** Links die Route (im Auswahlfeld jede Route mit gespeicherter Fahrt und die gerade geladene) und ihre
Fahrten, neueste zuerst. Wähle höchstens vier: Jede bekommt einen Buchstaben (A–D) und eine Farbe, und auch das Linienmuster im Diagramm
unterscheidet sich (A ist durchgezogen). **A ist die Bezugsfahrt**: Das Diagramm *Zeitdifferenz* und die Spalte *Differenz* der Tabelle
beziehen sich darauf (positiv = langsamer). Der Papierkorb fragt vor dem Löschen nach. Standardmäßig sind die beiden
neuesten Fahrten der Route ausgewählt.

**Diagramm.** *Leistung*, *Puls*, *Trittfrequenz*, *Tempo* oder *Zeitdifferenz* in Abhängigkeit von der Distanz. Bewegst du die
Maus (oder den Finger) über das Profil oder das Diagramm, lassen sich die Werte der Fahrten am Cursor in der Legende ablesen, und ein
Punkt auf der Karte zeigt die Stelle. Hat eine Fahrt diesen Sensor nicht (zum Beispiel keinen Pulssensor), hat sie keine Linie.

**In einen Abschnitt zoomen.** Zieh die Maus (oder den Finger auf einem Touchscreen) entlang des Profils oder des Diagramms, oder klicke die
*Anstieg*-Schaltflächen; `Gesamte Route` stellt alles wieder her. Die Karte zoomt in den Abschnitt und hebt ihn hervor, und Diagramm und Tabelle
verengen sich auf denselben Abschnitt. Ein Zug kürzer als 100 m gilt nicht als Abschnitt.

**Abschnittstabelle.** Pro Fahrt: Zeit, Durchschnittstempo, Durchschnitts- und normalisierte Leistung (NP, aus mindestens 30 Sekunden Daten),
Durchschnitts- und Maximalpuls, Trittfrequenz, Anstieg, VAM und die Differenz zur Bezugsfahrt. Daten gibt es nur für einen
Abschnitt, den die Fahrt **vollständig** abgedeckt hat; eine teilweise Fahrt ist bei den anderen Abschnitten leer.

### Leistung: Spitzen, FTP, Belastung

![Die Leistungsseite](../../screenshots/review-power.png)

Oben in der Übersicht bringt dich der Schalter **Vergleich | Leistung** auf die andere Seite. Hier stehen die Zahlen **aller**
deiner Fahrten, unabhängig von der Route:

- **Spitzenleistung.** Die besten Durchschnittswatt über 5 Sekunden, 1, 5, 20 und 60 Minuten, mit Datum und Route der Fahrt. Hast du
  in der Übersicht Fahrten ausgewählt, stehen ihre Werte daneben in Spalten (A–D). Eine Fahrt hat für eine
  längere Dauer als ihre eigene keinen Wert.
- **FTP.** Er schlägt 95 % deiner besten 20-Minuten-Leistung als FTP vor und zeigt die aktuell eingestellte daneben. Die Schaltfläche **FTP
  auf N W setzen** schreibt sie in die Einstellungen; die App ändert sie nie von selbst. Die TSS gespeicherter Fahrten bleibt
  mit der FTP ihrer eigenen Zeit berechnet. Für einen exakten Vorschlag brauchst du eine echte 20-Minuten-Bestleistung: Das Workout *FTP-
  Test 20 Minuten* (*Workouts*, `W`) liefert genau das.
- **Belastung.** Die letzten 12 Wochen oder Monate (*Wöchentlich* | *Monatlich*): Ihre Balken zeigen die TSS, der Wert darüber und der
  Tooltip (auf dem Balken stehen) zeigt Anzahl und Zeit der Fahrten.

**Nur gemessene Watt zählen.** Der simulierte Fahrer und Fahrten mit geschätzten (`~`) Watt kommen nicht unter die Spitzen und den
FTP-Vorschlag (in der Zeit des Trends schon, in seiner TSS nicht). Eine importierte Fahrt, in der echte Leistung steckte,
zählt. Die Watt des Trainers stammen nicht von einem Leistungsmesser, deshalb ist der FTP-Vorschlag nur so genau wie diese. Die Zahlen
älterer Fahrten werden beim ersten Öffnen berechnet und gespeichert, was bei vielen Fahrten ein paar Sekunden dauern kann.

### Eine Fahrt aus einer anderen App importieren

Mit der Schaltfläche **`Fahrt importieren`** (oder indem du die Datei auf die Seite ziehst) lädst du eine GPX- oder TCX-Datei, zum Beispiel eine
aus Strava, Garmin oder Wahoo exportierte. Puls, Trittfrequenz und (falls vorhanden) Watt kommen mit. Wissenswertes:

- **Die Route lieferst du**: Die Fahrt wird auf die auf der Seite gewählte Route abgebildet (lade oder wähle sie zuerst). Die Punkte
  der Spur werden auf die Route projiziert; was mehr als 30 m davon entfernt liegt (ein Umweg, ein GPS-Fehler), bleibt weg, und
  die Seite nennt die Zahl der ausgelassenen Punkte. Deckt die Fahrt nur einen Teil der Route ab, warnt sie.
- Die **automatische Pause** (ein langer Stopp) zählt nicht zur Zeit. Eine Datei ohne Zeitstempel (zum Beispiel eine geplante
  Route) lässt sich nicht als Fahrt importieren.
- Dieselbe Fahrt (dieselbe Startzeit) lässt sich nicht zweimal importieren.
- Die importierte Fahrt **wird kein PR und kein Geist**: Eine Fahrt draußen mit Stopps und Freilauf lässt sich nicht
  mit deinen Trainerzeiten vergleichen. In der Liste markiert das Tag `Importiert` sie.
- Am Telefon ist der Import noch nicht verfügbar. Das FIT-Format lesen wir nicht; Strava exportiert sowohl GPX als auch TCX.

### Geschätzte Watt

Hat die Datei **keine Leistungsdaten**, lässt das Programm die Physik der Simulation rückwärts laufen: Aus Tempo und Steigung
berechnet es, wie viele Watt nötig waren (die Änderung der Bewegungsenergie und die Arbeit gegen Schwerkraft, Rollen, Lager
und die Luft in einem 9-Sekunden-Fenster). Bei so einer Fahrt bekommen die Zahlen ein **`~`**-Zeichen und es erscheint ein Tag `Geschätzte Watt`.

Mit dem Schieberegler-Symbol (*Einstellungen der Schätzung*) stellst du ein, mit wessen Daten gerechnet wird:

| Wahl | Was es nutzt |
|---|---|
| **Meine eigenen Daten** | Körpergewicht, Rad und Crr aus den Einstellungen |
| **Eigene Werte** | das hier angegebene *Körpergewicht*, *Rad* und *Rollwiderstand (Crr)*, wenn du die Fahrt von jemand anderem ansiehst |

CdA, Antriebswirkungsgrad und Luftdichte kommen immer aus den Einstellungen. Crr auf gutem Asphalt ist etwa 0,004; **für Schotter gibt es
kein eigenes Modell**, dort ändert nur der angegebene Crr etwas. `Neu berechnen` rechnet mit den neuen Werten und
speichert die Fahrt; das Programm überschreibt gemessene Watt nie mit einer Schätzung.

**Wie genau ist das?** Eine Näherung. Es kennt weder Wind noch Windschatten (Gegenwind sieht aus wie weniger Watt), die
Steigung kommt aus der Route, nicht aus der verrauschten GPS-Höhe der Datei, und in scharfen Kurven kann die Projektion kurzzeitig ein verrauschtes
Tempo ergeben. Rechnet man mit der Physik des Programms selbst rückwärts, kommt die Runde auf etwa 3 % genau zurück; mit einem echten Leistungsmesser
haben wir es noch nicht verglichen, deshalb lohnt es sich, mit 10–15 % Abweichung zu rechnen.

## Street View (optional)

Google erlaubt das nur über seine API, deshalb brauchst du **deinen eigenen Google-Maps-API-Schlüssel** (*Einstellungen → Street View*; die
Schritte sind dort beschrieben: Google-Cloud-Projekt, Abrechnung, Aktivieren der *Maps Embed API* und der *Street View Static
API*). Ohne Schlüssel erscheint nichts. `V` oder die Schaltfläche `Street View` des Kartenpanels bringt ein kleines Fenster in die untere
rechte Ecke der Karte; das `X` (oder die Tausch-Schaltfläche) tauscht die beiden Ansichten. Bei Fehlern kommt eine verständliche Meldung
(ungültiger Schlüssel, aufgebrauchtes Kontingent, kein Panorama an dieser Stelle). Die Schaltfläche **Herunterladen** speichert alle 75–250 m ein Bild für die
Route (höchstens etwa 900; 30 Tage gültig, in den Einstellungen löschbar), sodass du beim nächsten Mal dieselbe Straße ohne
Schlüssel und ohne Netz fahren kannst. Der Download nutzt dein eigenes Google-Kontingent. Details und Googles Regeln:
[`STREETVIEW.md`](../../STREETVIEW.md).

## Telefon und Tablet

In einem Fenster schmaler als 1000 px erscheint eine Ansicht mit Tabs: unten **Fahrt · Karte · Diagramme · Daten**, darüber die
große Reihe `Start`/`Pause`/`Beenden`. Im Tab **Fahrt** ist alles, was du während einer Fahrt brauchst, auf einem Bildschirm: der
Block des Workouts (verbleibende Zeit, nächster Block, die Nachricht des Coaches), die Leistung gegen das Ziel (*im Ziel / niedrig /
hoch*), der Zonenbalken, Trittfrequenz, Puls, Tempo, Steigung, darunter das Profil (seitliches Ziehen wechselt zwischen seinen
Ansichten). Die Dialoge sind am Telefon bildschirmfüllend.

<p align="center">
  <img src="../../screenshots/phone-ride.png" width="280" alt="Telefon-Layout, Tab Fahrt mit einem Workout">
  &nbsp;&nbsp;
  <img src="../../screenshots/phone-map.png" width="280" alt="Telefon-Layout, Tab Karte">
</p>

Die Android-Anwendung ist dasselbe Programm als APK. Du lädst eine Route mit dem System-Dateiauswähler, mit einem bikerouter-
Link, aus den letzten oder mit der Demo-Route; beim ersten Start fragt sie nach den Bluetooth-Berechtigungen; während einer Fahrt
bleibt der Bildschirm an (in einer Pause und am Ende der Fahrt gilt wieder das Zeitlimit des Systems; am Telefon haben wir es noch nicht
versucht, [`ANDROID.md`](../../ANDROID.md)). Bekommt die Anwendung einen Fehler, zeigt eine rote Leiste seine erste Zeile, und
das Programm bleibt benutzbar.

## Einstellungen

Jede Änderung wirkt sofort und wird gespeichert.

![Das Fenster Einstellungen](../../screenshots/settings.png)

| Gruppe | Was du einstellst |
|---|---|
| **Fahrer** | Körpergewicht (75 kg), Gewicht des Rades (9 kg), FTP (200 W), max. Herzfrequenz (190), CdA (0,32 m²), Crr (0,004) |
| **Trainer** | Stärke der Steigung (0–150 %), Geschwindigkeit des Trainers verwenden, virtuelle Schaltung |
| **Erscheinungsbild** | Sprache (automatisch/Ungarisch/Englisch/Spanisch/Deutsch), Textgröße (80–160 %), Design (dunkel/hell), Kartenstil (dunkel/hell/OSM) |
| **Karte und Profil** | Die Karte folgt dem Fahrer, Profilansicht (Vorausschau/gesamte), Länge der Vorausschau, Geist |
| **Layout** | Standard wiederherstellen |
| **Sicherung** | *In eine Datei speichern* und *Aus einer Datei wiederherstellen* (siehe unten) |
| **Street View** | Anzeigen, der Google-Schlüssel, Panorama-IDs speichern, Bilder herunterladen, die gespeicherten Daten löschen |

### Sicherung

*In eine Datei speichern* erzeugt eine einzige **Zip-Datei** aus deinen Routen, deinen gespeicherten Fahrten, deinen importierten Workouts und deinen Einstellungen.
Du wählst die Datei (unter Android über den „Speichern unter“-Dialog des Systems), und nichts geht ins Netz. **Sie enthält nicht**
den Street-View-Schlüssel, die Kartenkacheln, die heruntergeladenen Street-View-Bilder oder die Protokolle: Diese sind entweder Geheimnisse oder
lassen sich jederzeit neu herunterladen.

*Aus einer Datei wiederherstellen* **führt zusammen**: Die Routen, Fahrten und Workouts der Sicherung werden den vorhandenen hinzugefügt, **nichts wird
gelöscht und keine vorhandene Fahrt überschrieben**. Die Einstellungen der Sicherung gelten jedoch (dein Street-View-Schlüssel bleibt
der alte), deshalb fragt die App zuerst. Dieselbe Datei zweimal wiederherzustellen tut nichts. Beim Umzug auf einen neuen Rechner einfach die
Datei wiederherstellen. Während einer Fahrt lässt sich weder sichern noch wiederherstellen; ist die Datei beschädigt oder von einer neueren Version,
sagt die App das und schreibt nichts. Unter Android wurde dieser Teil noch nicht auf einem Telefon versucht.

## Tastenkürzel

| Taste | Aktion |
|---|---|
| `Leertaste` | Start / Pause / Fortsetzen (am Ende die Zusammenfassung zurückholen) |
| `M` | Folgemodus der Karte ein/aus |
| `P` | Höhenprofil: gesamt ↔ Vorausschau |
| `T` | dunkles/helles Design |
| `V` | Street-View-Panel ein/aus |
| `X` | Karte und Street View tauschen |
| `W` | Workout-Auswahl |
| `←` / `→` | virtuelle Schaltung: leichter / schwerer |
| `↑` / `↓` | Leistung des simulierten Fahrers ±10 W |
| `F11` | Vollbild |
| `Esc` | Fenster schließen, Vollbild verlassen |
| `Strg+O` | eine GPX öffnen |

Beim Tippen in einem Textfeld lösen die Tasten keinen Befehl aus (außer `Esc`, `F11`, `Strg+O`); bei geöffnetem Dialog funktionieren nur
`Esc`, `F11` und im Fenster Route `Strg+O`.

## Browser-Panel (nur Windows)

Das Panel **Browser** bettet das auf dem Rechner installierte **Firefox** in das Fenster ein, sodass du während einer Fahrt eine Serie oder
anderes ansehen kannst. Firefox nutzt ein eigenes Profil (deine Anmeldungen bleiben, dein normaler Firefox bleibt unberührt). Unter Mac
und Linux erscheint das Panel nicht. Öffnet sich ein Fenster oder wird ein Panel gezogen, versteckt sich Firefox vorübergehend (die Oberfläche
kann nicht über ein natives Fenster zeichnen); das Schließen des Panels schließt auch Firefox; es sollte mindestens etwa 500 px breit sein.

## Fehlersuche

| Symptom | Was zu tun ist |
|---|---|
| Der Trainer reagiert nicht auf die Anstiege / das Workout | Im Fenster **Geräte** muss er in der Rolle *Trainer* verbunden sein (die Rolle Leistungsmesser liest nur). Sieh ins BLE-Log (`ble.log`, wie in §11 von [`BLUETOOTH.md`](../../BLUETOOTH.md) beschrieben); Zwift oder eine Telefon-App sollte den Trainer nicht belegen. |
| Das Gerät wird nicht gefunden | Schalte Bluetooth ein; unter Windows sucht der Scan ungefiltert, es erscheinen also viele Geräte: Sieh dir die Rollen des Geräts an. Unter Android erteile die Bluetooth-Berechtigungen (*Scan starten* fragt erneut); unter macOS fragt das System beim ersten Scan. |
| Die Karte ist leer oder grau | Es gibt kein Netz und noch keine zwischengespeicherte Kachel: Route und Zahlen funktionieren trotzdem. |
| Die Route lädt nicht | Die Fehlermeldung sagt warum (beschädigte Datei, nicht unterstützter Link, kein Netz für BRouter). Der Link muss den Teil `lonlats=` enthalten. |
| Das Profil ist flach | Die Datei hat keine Höhendaten, und Open-Meteo war auch nicht verfügbar. Lade eine GPX mit Höhe oder einen bikerouter-Link. |
| Unter Linux ist das Fenster leer | Das Programm setzt die Variable `WEBKIT_DISABLE_DMABUF_RENDERER=1` selbst; ist es trotzdem leer, versuche `WEBKIT_DISABLE_COMPOSITING_MODE=1`. |
| macOS: „kann nicht geöffnet werden“ | Rechtsklick → Öffnen → Öffnen (die Anwendung ist nicht notarisiert); oder `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`. |
| Windows: SmartScreen warnt | Das Programm ist nicht digital signiert, deshalb kann Windows es stoppen; es lässt sich über *Weitere Informationen → Trotzdem ausführen* starten. |
| Absturz / rote Leiste | Die erste Zeile steht in der Leiste, das vollständige Log in `crash.log` (unter Android `adb logcat -s bike-osm-ride`). Bitte melde den Fehler mit dem Log. |

**Wo sind meine Daten?** In einem einzigen Ordner, zusammen mit den Einstellungen, dem Fahrtenbuch, den Kacheln und den Protokollen (willst du eine
Kopie, nutze die Schaltfläche *Einstellungen → Sicherung*): Windows `%LOCALAPPDATA%\bike-osm-ride`, macOS
`~/Library/Application Support/bike-osm-ride`, Linux `~/.local/share/bike-osm-ride`, Android der private Ordner
der Anwendung. Die genaue Struktur beschreibt [`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); was den Rechner verlässt,
[`PRIVACY.md`](../../PRIVACY.md).
