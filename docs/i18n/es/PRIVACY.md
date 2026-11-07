# Privacidad y tráfico de red

bike-osm-ride **no tiene cuenta, ni telemetría, ni analítica, ni publicidad**. Tus datos (salidas, ajustes,
datos de los sensores) se quedan en tu propio equipo; el programa solo llama por la red a lo que tú pides.
Esta página enumera con exactitud qué puede salir y qué hay en tu equipo. Las afirmaciones están hechas a
partir del código fuente (2026-11-04, versión 0.3.0).

## Qué sale de tu equipo, y cuándo

| Dirección | Cuándo | Qué recibe | Nota |
|---|---|---|---|
| `tile.openstreetmap.org` | cuando el mapa pide teselas | las coordenadas de la tesela (zoom, x, y) y el nombre del programa en el User-Agent (`bike-osm-ride/0.1 (desktop app)`) | se guarda en caché localmente 30 días; no hay precarga masiva (por la política de teselas de OSM) |
| `brouter.de` | si pegas un enlace de bikerouter | los puntos leídos del enlace (longitud, latitud), el perfil y los evitados | la ruta la calcula su servidor |
| `api.open-meteo.com` | solo si el GPX no tiene datos de altitud utilizables | las coordenadas de los puntos de muestra de la ruta (como máximo 100 por petición) | CC BY 4.0, para uso no comercial |
| `api.github.com` | al arrancar, cinco segundos después | la consulta de la última versión, `User-Agent: bike-osm-ride/<versión>` | se calla si no funciona; **no se puede desactivar en los ajustes** |
| `github.com` (descarga) | solo si pulsas el botón *Actualizar* | la descarga del paquete nuevo | |
| `maps.googleapis.com`, `www.google.com` | solo si has activado Street View y has dado una clave de API | la posición del ciclista (coordenada) y **tu propia** clave de Google | se le aplican las condiciones de Google; ver [`STREETVIEW.md`](../../STREETVIEW.md) |
| las páginas abiertas en Firefox | solo si usas el panel Navegador (Windows) | lo que tú mires en él | perfil de Firefox aparte, tu navegador normal no se toca |

No hay nada más. No hay inicio de sesión en el programa, ni envío de informes de fallos, ni estadísticas de
uso. El tráfico Bluetooth solo va entre el rodillo, los sensores y el equipo.

## Qué hay en tu equipo

Todo está en una sola carpeta (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride` y, en Android, la carpeta privada de la aplicación). La estructura la describe
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); en resumen:

- **ajustes** (`settings.json`): peso, FTP, pulso máximo, apariencia, diseño y tu clave de API de Google;
- **rutas y registro de salidas**: la versión procesada de las rutas abiertas y el registro completo segundo a
  segundo de cada salida terminada (posición, vatios, pulso, cadencia);
- **teselas**, las **imágenes de Street View** descargadas (como máximo 30 días) e identificadores de panorama;
- **registros**: `ble.log` (los paquetes de datos en bruto del rodillo y de los sensores: vatios, pulso,
  órdenes de control; no llevan nombre ni dirección de dispositivo) y `crash.log` (los errores capturados);
  ambos rotan y se quedan en el equipo;
- el perfil de Firefox del panel Navegador;
- los restos de la subida a Strava de la versión **0.3.1 y anteriores**: las carpetas `strava` y
  `strava-webview` (en la segunda están las cookies de tu inicio de sesión de Strava). El programa ya no las
  usa ni las borra: puedes borrarlas con tranquilidad.

**Copia de seguridad.** *Ajustes → Copia de seguridad* crea un único archivo zip con tus rutas, tus salidas,
tus entrenamientos importados y tus ajustes. El archivo va donde tú lo guardes (en Android, con el diálogo
«Guardar como» del sistema); el programa no envía nada a ningún sitio ni lo sube. La clave de API de Google
**no va dentro**, y al restaurar tampoco acepta la clave que venga del archivo; las teselas, las imágenes de
Street View y los registros tampoco están dentro. El archivo contiene las rutas exactas de tus salidas y tus
datos de salud (pulso, vatios), así que trátalo como el resto de tus archivos personales.

**Secretos.** El único secreto es la clave de API de Google. En `settings.json` está *en texto plano* (sin
cifrar), así que protege la carpeta como el resto de tus archivos personales. El programa nunca la registra
en los logs, no acaba en un mensaje de error, no aparece en la salida de depuración y solo sale hacia Google
(o hacia un servidor de pruebas local en `127.0.0.1` durante el desarrollo).

**Borrar.** Borrando la carpeta desaparece todo. Las imágenes e identificadores de Street View se pueden
borrar aparte en *Ajustes → Street View*; si quitas una ruta de la lista de rutas recientes, su registro de
salidas se conserva.

### Archivos de entrenamiento importados

**Importar entrenamiento** lee un archivo GPX o TCX que tú eliges (o arrastras). El programa lo procesa en tu
propio equipo y solo guarda el resultado (el registro segundo a segundo: posición en la ruta, altitud,
velocidad, pulso, cadencia, vatios) en la carpeta `rides`, igual que tus propias salidas. El contenido
original del archivo (por ejemplo, el nombre y las marcas de tiempo que escribió Strava) no va a ningún otro
sitio, y nada sale a la red. El cálculo de los vatios estimados también se hace en local.

## Cosas que conviene saber

- OpenStreetMap, BRouter, Open-Meteo, GitHub y Google ven tus propias peticiones (dirección IP, momento); es
  lo que pasa con cualquier cliente de red.
- Actualmente no hay ningún ajuste para desactivar la comprobación de actualizaciones. Si no la quieres,
  bloquea `api.github.com` con el cortafuegos; el programa no se estropea por ello.
- El programa no es un producto de Google ni de Zwift, y no tiene relación con ellos.
