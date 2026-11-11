# Datenschutz und Netzwerkverkehr

> **Übersetzung.** Maßgeblich ist die ungarische Fassung dieses Dokuments: [Datenschutzseite](../../PRIVACY.md);
> sie ist auch auf [Englisch](../en/PRIVACY.md) und [Spanisch](../es/PRIVACY.md) verfügbar.

bike-osm-ride **hat kein Konto, keine Telemetrie, keine Analyse und keine Werbung**. Deine Daten (Fahrten, Einstellungen, Sensordaten)
bleiben auf deinem eigenen Rechner; das Programm ruft über das Netz nur ab, was du anforderst. Diese Seite listet genau auf, was
hinausgehen kann und was auf deinem Rechner liegt. Die Aussagen beruhen auf dem Quellcode (2026-11-04, Version 0.3.0).

## Was deinen Rechner verlässt, und wann

| Adresse | Wann | Was sie erhält | Hinweis |
|---|---|---|---|
| `tile.openstreetmap.org` | wenn die Karte Kacheln anfordert | die Koordinaten der Kachel (Zoom, x, y) und der Programmname im User-Agent (`bike-osm-ride/0.1 (desktop app)`) | Kacheln sind 30 Tage aktuell; ältere Offline-Kopien bleiben bis zu 90 Tage / insgesamt 512 MiB auf der Platte. Bereinigung beim Start und stündlich während der Nutzung. |
| `brouter.de` | wenn du einen bikerouter-Link einfügst | die aus dem Link gelesenen Punkte (Längengrad, Breitengrad), das Profil und die Vermeidungen | die Route berechnet ihr Server |
| `api.open-meteo.com` | nur wenn die GPX keine brauchbaren Höhendaten hat | die Koordinaten der Stützpunkte der Route (höchstens 100 pro Anfrage) | CC BY 4.0, für nichtkommerzielle Nutzung |
| `api.github.com` | beim Start, fünf Sekunden später | die Abfrage der neuesten Version, `User-Agent: bike-osm-ride/<version>` | schlägt sie fehl, wird es still; **in den Einstellungen lässt sie sich nicht abschalten** |
| `github.com` (Download) | nur wenn du die Schaltfläche *Update* drückst | der Download des neuen Pakets | |
| `maps.googleapis.com`, `www.google.com` | nur wenn du Street View eingeschaltet und einen API-Schlüssel angegeben hast | die Position des Fahrers (Koordinate) und **deinen eigenen** Google-Schlüssel | es gelten Googles Bedingungen; siehe [`STREETVIEW.md`](../../STREETVIEW.md) |
| die in Firefox geöffneten Seiten | nur wenn du das Browser-Panel nutzt (Windows) | was du darin ansiehst | ein eigenes Firefox-Profil, dein normaler Browser bleibt unberührt |

Mehr gibt es nicht. Es gibt keine Anmeldung am Programm, kein Senden von Absturzberichten, keine Nutzungsstatistik. Bluetooth-Verkehr
läuft nur zwischen Trainer, Sensoren und dem Rechner.

## Was auf deinem Rechner liegt

Alles liegt in einem einzigen Ordner (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride`, unter Android im privaten Ordner der Anwendung). Die Struktur beschreibt
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); kurz:

- **Einstellungen** (`settings.json`): Körpergewicht, FTP, max. Herzfrequenz, Erscheinungsbild, Layout, dein Google-API-Schlüssel;
- **Routen und Fahrtenbuch**: die verarbeitete Fassung der geöffneten Routen, die vollständige Aufzeichnung jeder
  beendeten Fahrt pro Sekunde (Position, Watt, Puls, Trittfrequenz);
- **Kacheln**, die heruntergeladenen **Street-View-Bilder** (höchstens 30 Tage) und Panorama-IDs;
- **Protokolle**: `ble.log` (die Rohdatenpakete von Trainer und Sensoren: Watt, Puls, Steuerbefehle; sie
  enthalten keinen Gerätenamen und keine Adresse) und `crash.log` (abgefangene Fehler); beide rotieren und bleiben auf dem Rechner;
- das Firefox-Profil des Browser-Panels;
- die Reste des Strava-Uploads von Version **0.3.1 und älter**: die Ordner `strava` und `strava-webview` (letzterer
  enthält die Cookies deiner Strava-Anmeldung). Das Programm nutzt sie nicht mehr und löscht sie nicht: Du kannst sie
  gern löschen.

**Sicherung.** *Einstellungen → Sicherung* erzeugt eine einzige Zip-Datei aus deinen Routen, deinen Fahrten, deinen importierten Workouts und deinen
Einstellungen. Die Datei landet dort, wo du sie speicherst (unter Android über den „Speichern unter“-Dialog des Systems); das Programm sendet nichts
irgendwohin und lädt sie auch nicht hoch. Der Google-API-Schlüssel **ist nicht darin**, und beim Wiederherstellen akzeptiert es auch keinen Schlüssel
aus der Datei; die Kacheln, die Street-View-Bilder und die Protokolle sind ebenfalls nicht darin. Die Datei enthält die
genauen Routen deiner Fahrten und deine Gesundheitsdaten (Puls, Watt), behandle sie also wie deine anderen persönlichen Dateien.

**Geheimnisse.** Das einzige Geheimnis ist der Google-API-Schlüssel. Er steht in `settings.json` *im Klartext* (nicht verschlüsselt), schütze
den Ordner also wie deine anderen persönlichen Dateien. Das Programm protokolliert ihn nie, er landet nicht in einer Fehlermeldung und
erscheint nicht in der Debug-Ausgabe, und er geht nur an Google (oder während der Entwicklung an einen lokalen Testserver unter `127.0.0.1`).

**Löschen.** Löschst du den Ordner, ist alles verschwunden. Die Street-View-Bilder und -IDs lassen sich auch
getrennt unter *Einstellungen → Street View* löschen; das Fahrtenbuch einer aus der Liste der letzten Routen entfernten Route bleibt.

### Importierte Fahrtdateien

**Fahrt importieren** liest eine GPX- oder TCX-Datei, die du auswählst (oder hineinziehst). Das Programm verarbeitet die Datei auf deinem eigenen Rechner,
und speichert nur das Ergebnis (die Aufzeichnung pro Sekunde: Position auf der Route, Höhe, Tempo, Puls, Trittfrequenz, Watt)
im Ordner `rides`, genau wie deine eigenen Fahrten. Der ursprüngliche Inhalt der Datei (zum Beispiel der von Strava geschriebene Name und die Zeitstempel)
geht nirgendwo anders hin, und nichts geht ins Netz. Auch die Berechnung der geschätzten Watt
geschieht lokal.

## Wissenswertes

- OpenStreetMap, BRouter, Open-Meteo, GitHub und Google sehen deine eigenen Anfragen (IP-Adresse, Zeit); so ist es bei
  jedem Netzwerkclient.
- Es gibt derzeit keine Einstellung, um die Update-Prüfung abzuschalten. Wenn du sie nicht willst, blockiere `api.github.com` mit einer
  Firewall; das Programm geht daran nicht kaputt.
- Das Programm ist kein Google- oder Zwift-Produkt und hat keine Verbindung mit ihnen.
