# bike-osm-ride — en español

> **Traducción.** La versión autoritativa de este documento es el [`README.md`](../../../README.md) húngaro.
> Las capturas de pantalla son de la interfaz original (húngara): la app habla húngaro y español, y el
> inglés está en camino. El manual completo y la política de privacidad también están en español:
> [manual del usuario](USER_GUIDE.md) · [privacidad](PRIVACY.md).

**Pedaleo virtual en tus propias rutas GPX, con un rodillo inteligente… o sin él.**
Mapa, perfil de altitud, entrenamientos, fantasma y exportación TCX. Rápido, moderno, 2D. Rust.

**Ir a:** [Descarga](#descarga) · [Inicio rápido](#inicio-rápido) · [Qué hace](#qué-hace) ·
[Dispositivos y estado](#dispositivos-y-estado) · [Documentación](#documentación) ·
[Desarrollo](#desarrollo) · [Licencia](#licencia)

## ¿Por qué este?

- **Tu ruta.** No es un mundo prefabricado: puedes pedalear cualquier GPX o enlace de bikerouter, con
  desniveles y subidas reales, y el rodillo sigue la pendiente de la ruta.
- **Todos los datos que importan.** Potencia (media de 3 s, W/kg, zona), cadencia, pulso, velocidad,
  pendiente, NP/IF/TSS, desnivel, ETA: en números grandes y legibles, sin 3D.
- **Entrenamiento y análisis.** 93 entrenamientos integrados y documentados con sus fuentes, en modo
  ERG y con mensajes del entrenador; fantasma y segmentos locales con PR; exportación TCX.
- **Sigue siendo tuyo.** Sin cuenta, sin telemetría, tus datos se quedan en tu equipo
  ([privacidad](../../PRIVACY.md)). Código abierto con licencia permisiva.
- **Funciona sin hardware.** Con el ciclista simulado puedes probarlo todo sin Bluetooth.

## Descarga

La última versión: **[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**.
En los nombres de archivo, `<versión>` es el número de la versión (por ejemplo `0.3.0`).

| Plataforma | Archivo | Instalación |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<versión>-windows-x64.exe` | no hay instalador: ejecútalo. Hace falta el [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (en Windows 11 ya viene). El programa no está firmado digitalmente, así que SmartScreen puede detenerlo. |
| **Linux** (x86-64, glibc 2.35+) | `bike-osm-ride-<versión>-linux-x86_64.AppImage` | `chmod +x` y ejecútalo; trae WebKitGTK y GTK consigo |
| | `bike-osm-ride-<versión>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<versión>-linux-amd64.deb` (pide `libwebkit2gtk-4.1-0`, `libgtk-3-0` y `bluez`) |
| **macOS 11+** (Intel y Apple Silicon) | `bike-osm-ride-<versión>-macos-universal.dmg` | arrástralo a *Aplicaciones*; en el primer arranque, **clic derecho → Abrir → Abrir** (no está notarizado); en la primera búsqueda Bluetooth, permite el Bluetooth |
| **Android 7+** | `bike-osm-ride-<versión>-android.apk` | cópialo al teléfono y ábrelo (hay que permitir la instalación desde orígenes desconocidos), o `adb install -r` |

El programa **avisa solo** cuando hay una versión nueva: en la barra superior aparece el botón
*Actualización* (en Windows y como AppImage se sustituye a sí mismo; en las demás plataformas abre la
página de la versión). El paquete de Android está firmado con una clave de depuración, así que entre dos
versiones puede hacer falta desinstalar la anterior.

## Inicio rápido

1. **Arranca.** Se abre la ventana *Ruta*: arrastra un GPX, pega un enlace de bikerouter o elige la
   *Ruta de demostración*.
2. **Conecta el rodillo** (*Dispositivos* → *Iniciar búsqueda* → en el rol *Rodillo inteligente*), o elige
   el **ciclista simulado** si no tienes hardware a mano. También puedes conectar un pulsómetro, un
   potenciómetro, un sensor de cadencia y un Zwift Click/Play.
3. *(Opcional)* **Entrenamiento**: el botón *Entrenamiento* de la barra superior (`W`), con 93
   entrenamientos para elegir o el tuyo importado.
4. **`Inicio`** (`Espacio`). El rodillo sigue las subidas (o mantiene los vatios objetivo del
   entrenamiento) y los números cobran vida.
5. Al terminar, guarda un TCX desde el **resumen** (lo aceptan Strava, Intervals.icu y TrainingPeaks).

El manual completo: [`docs/USER_GUIDE.md`](../../USER_GUIDE.md) (húngaro).

## Qué hace

**Rutas**
- Archivo GPX (selector, arrastrar y soltar, `Ctrl+O`), enlace de bikerouter/BRouter (también llegan el
  perfil, la alternativa y los rodeos), rutas recientes y una ruta de demostración integrada.
- Lector de GPX tolerante (BOM, espacios de nombres, varios tramos, `<rte>`, mensajes comprensibles ante
  un archivo roto); la altitud que falta la rellena Open-Meteo. Detección de subidas de 4.ª a HC; los
  segmentos metidos en el archivo (por ejemplo un segmento de Strava) se ven como franjas.

**Pedaleo**
- Física realista (Martin y col., 1998: gravedad, rodadura, rozamiento de rodamientos, resistencia
  aerodinámica con radios, inercia); el rodillo recibe la pendiente del tramo que tienes delante (hasta
  ±20%, con una intensidad del 0–150%).
- **Rodillo FTMS** (subidas y ERG), **potenciómetro** (CPS), **velocidad/cadencia** (CSC),
  **pulsómetro** (HR): un mismo dispositivo físico puede cumplir *varios roles* en una sola conexión;
  **Zwift Click/Play** como cambio virtual; **ciclista simulado**.
- **Cambio virtual**: transmisión 2 × 12, 23 marchas, modifica la resistencia del rodillo.

**Entrenamiento y análisis**
- 93 entrenamientos integrados en 12 categorías, con etiquetas, gráfico de bloques coloreado por zonas y
  943 mensajes del entrenador con temporización; importación de entrenamientos de MyWhoosh y Zwift
  (`.zwo`, XML, JSON).
- Zonas de Coggan, NP, IF, TSS, kcal; zonas de pulso; grabación segundo a segundo.
- **Fantasma** (la última salida, la mejor, o los tiempos propios del GPX) y **segmentos locales con PR**.
- **Revisión y comparación de entrenamientos**: las salidas hechas en la misma ruta (hasta cuatro) lado a
  lado por potencia, pulso, cadencia, velocidad y diferencia de tiempo; acercarse a un tramo (arrastrando
  o desde las subidas), tabla de tramos. **Importación de un entrenamiento GPX o TCX** exportado de Strava
  o Garmin, con pulso y cadencia; si no trae vatios, la física de la simulación los estima hacia atrás (con
  tu peso o el que indiques, la bici y la resistencia a la rodadura).
- **Página de potencia**: potencia máxima desde 5 s hasta 60 min, sugerencia de FTP a partir de tus
  mejores 20 minutos (se aplica con un botón), carga semanal y mensual. Solo cuentan los vatios medidos.
- **Copia de seguridad**: rutas, salidas, entrenamientos importados y ajustes en un único zip (sin la
  clave de Google), con una restauración que fusiona. Nada sale a la red.
- **Exportación TCX** (esquema de Garmin): Strava, Intervals.icu y TrainingPeaks lo aceptan subiéndolo a
  mano. No hay subida integrada: el inicio de sesión de Strava había que repetirlo tras cada salida, así
  que se quitó (0.3.2).

**Interfaz**
- **Diseño de mosaico**: los paneles funcionan como en un gestor de ventanas en mosaico (sin solapes, sin
  huecos); se redimensionan, se mueven y se cierran arrastrando; diseño distinto con entrenamiento y sin él.
- Tema oscuro y claro (los dos completos), tamaño de texto del 80 al 160%, **interfaz en húngaro y
  español**, fuentes integradas.
- **Teléfono y tableta**: vista con pestañas; en la pestaña «Pedaleando» está todo lo que hace falta
  mientras pedaleas.
- **Street View** opcional (con tu propia clave de Google y descarga sin conexión) y un **panel
  Navegador** (Windows, integra el Firefox instalado).
- Mantiene el equipo despierto mientras pedaleas (Windows, Linux, macOS).

## Dispositivos y estado

**No todo se ha probado con hardware o servicios reales.** Con honestidad:

| Elemento | Estado |
|---|---|
| Rodillo inteligente FTMS, **ERG** (entrenamiento) | probado, **también con un rodillo real** (un modelo: VE200) |
| **Simulación de subidas** FTMS | probada a fondo (con transporte simulado y los vectores de bytes de la especificación); **aún no confirmada con un rodillo real** |
| Pulsómetro, potenciómetro aparte, sensor CSC | decodificadores probados; los datos CPS/CSC del rodillo son reales; con un sensor aparte aún no se ha probado |
| Zwift Click / Play | implementado (no oficial), aún no probado con un mando real |
| Ciclista simulado | funciona sin hardware, en todas las plataformas |
| Windows | en uso diario |
| Linux, macOS | la CI los compila y hace una prueba de humo de 20 s en Linux; aún no hay verificación en una máquina real (se agradece experiencia) |
| Android | la interfaz se ha ejecutado (los fallos de la 0.3.0 se encontraron ejecutándola); el selector de archivos del sistema está implementado; **el Bluetooth aún no se ha probado en un dispositivo** |
| Street View | implementado y probado con un servidor de pruebas local; aún no probado con el servicio real |

Detalles: [`docs/TESTING.md`](../../TESTING.md) y [`docs/BLUETOOTH.md`](../../BLUETOOTH.md) (inglés). Se
agradecen informes de fallos, experiencias y dispositivos compatibles o incompatibles en Issues (conviene
adjuntar el `ble.log`).

## Documentación

| Para | Qué |
|---|---|
| **Usuarios** | [Manual del usuario](USER_GUIDE.md) · [Privacidad y tráfico de red](PRIVACY.md) · [Registro de cambios](../../../CHANGELOG.md) (húngaro) |
| **Desarrolladores** | [Arquitectura](../../ARCHITECTURE.md) · [Guía de desarrollo](../../DEVELOPMENT.md) · [Pruebas](../../TESTING.md) · [Publicación](../../RELEASING.md) · [Registro de decisiones](../../DECISIONS.md) · [Traducciones](../../I18N.md) (inglés) |
| **Temas** | [Bluetooth](../../BLUETOOTH.md) · [Simulación y física](../../SIMULATION.md) · [Rutas y formatos](../../ROUTES_AND_DATA.md) · [Interfaz](../../UI.md) · [Sistema de diseño](../../DESIGN.md) · [Android](../../ANDROID.md) · [Street View](../../STREETVIEW.md) · [Antecedentes y código reutilizado](../../PRIOR_ART.md) (inglés) |

El índice completo: [`docs/README.md`](../../README.md).

## Desarrollo

```powershell
cargo run -p ride-app                      # build de depuración y arranque (Windows, Linux, macOS)
cargo test --workspace                     # toda la batería de pruebas
node scripts/readme_shots.mjs              # regenerar las capturas
```

Espacio de trabajo de Rust con diez crates de biblioteca y la aplicación (`ride-app`):

```
crates/
  ride-model         modelo de datos común (Route, Settings, RideSnapshot, Workout, …) – el «contrato»
  ride-route         GPX/bikerouter → Route, subidas, persistencia, registro de salidas
  ride-sim           física, RideEngine, ejecutor de entrenamientos, fantasma, segmentos, TCX (sin I/O ni reloj)
  ride-ble           hub BLE (FTMS/CPS/CSC/HR/Zwift) + ciclista simulado
  ride-ui-kit        sistema de diseño: tokens, paleta, iconos, formato
  ride-ui-map        mapa de teselas · ride-ui-profile  perfil y gráficos
  ride-ui-hud        indicadores, tarjetas, controles · ride-ui-dialogs  ventanas
  ride-ui-streetview panel de Street View
  ride-app           el armazón: estado, controlador, mosaico, proxy de teselas, actualizador, Android
packaging/  scripts/  tests/fixtures/  docs/
```

El código se apoya en Dioxus 0.7 (interfaz sobre webview) y `btleplug`. En Windows el desarrollo usa la
cadena de herramientas GNU (el exe de la versión lo compila la CI con MSVC). Requisitos, interruptores
(`RIDE_*`), depuración y empaquetado: [`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md). La guía del proyecto
para Claude Code: [`CLAUDE.md`](../../../CLAUDE.md).

**Contribuir.** Se agradecen informes de fallos, registros de mediciones y parches. Las reglas, en corto:
solo se puede reutilizar código con licencia permisiva (ver abajo), el texto de la interfaz se escribe en
húngaro y se traduce, no se entra en pánico con datos externos, y una función nueva trae prueba y
documentación. El código que envíes entra con la licencia del proyecto (MIT o Apache-2.0), salvo que
indiques otra cosa.

## Licencia

### Código propio del proyecto

El código propio de `bike-osm-ride` tiene **doble licencia: [MIT](../../../LICENSE-MIT) O
[Apache-2.0](../../../LICENSE-APACHE)**, a tu elección (SPDX: `MIT OR Apache-2.0`). Si contribuyes, tu
código entra con estas dos mismas licencias, salvo que indiques otra cosa.

### Código abierto reutilizado

Usamos a propósito solo código con licencias permisivas. El origen y la licencia se conservan en los
archivos fuente y en [`NOTICE.md`](../../../NOTICE.md). La tabla completa, con cada proyecto y su licencia
(ftms, cycling-ble, OsmolControl, iron-heart, las fuentes Barlow con SIL OFL 1.1, virtual-cog, RustCrypto
y las dependencias de Rust), está en el [`README.md`](../../../README.md#felhasznált-nyílt-forrású-kód)
húngaro y en [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md).

No se ha utilizado código de proyectos con licencia GPL/AGPL (por ejemplo OpenCycling, suffice,
GoldenCheetah o QDomyos-Zwift), porque sometería todo el programa al copyleft; los detalles de la
investigación están en [`docs/PRIOR_ART.md`](../../PRIOR_ART.md).

**Auditoría de licencias (2026-11-04).** Entre las dependencias externas de crates.io no hay **ninguna
GPL, AGPL o LGPL, ni ninguna licencia desconocida**; las licencias permisivas son MIT, Apache-2.0, BSD,
ISC, Zlib, Unicode, CC0, Unlicense y similares. El único copyleft es **MPL-2.0 a nivel de archivo**
(`option-ext` en Linux y macOS, la cadena `cssparser`/`selectors` en Android, ninguna en Windows), sin
modificar y tomado de crates.io. Eso no afecta a la licencia propia del proyecto.

### Mapas y fuentes de datos (atribución)

La aplicación llama a servicios externos mientras funciona; sus datos se usan según sus propias
condiciones y la aplicación muestra las atribuciones obligatorias en el mapa (la lista completa de
llamadas: [`docs/PRIVACY.md`](../../PRIVACY.md)):

- **Datos de mapa:** © colaboradores de [OpenStreetMap](https://www.openstreetmap.org/copyright),
  [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Teselas:** las teselas estándar de [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles)
  (`tile.openstreetmap.org`). Los estilos *Oscuro* y *Claro* filtran esas mismas teselas en local (filtro
  CSS), así que no hace falta clave de API ni un servidor de estilos de terceros. Las teselas descargadas
  se guardan en caché local para no cargar los servidores ([política de uso de teselas de
  OSM](https://operations.osmfoundation.org/policies/tiles/)); no hay precarga masiva.
- **Street View (opcional):** servicios de [Google Maps Platform](https://mapsplatform.google.com): el
  endpoint de *metadata* de Street View Static API (solo JSON: estado, id de panorama, posición), el
  endpoint de imagen de Static API (solo en la descarga sin conexión que pidas) y la vista Street View de
  Maps Embed API. El programa usa **tu** clave de API (tu cuenta de Google Cloud, tu cuota y tu
  facturación); la clave solo está en tu equipo, en `settings.json`, nunca va a un registro y solo se
  envía a `maps.googleapis.com` / `www.google.com`. Se aplican las [condiciones de Google Maps
  Platform](https://cloud.google.com/maps-platform/terms) y las [políticas de Street
  View](https://developers.google.com/maps/documentation/streetview/policies). El programa **no descarga
  imágenes por defecto**: solo si pulsas *Descargar* en el panel guarda imágenes de la ruta en tu equipo
  durante un máximo de 30 días; las muestra solo sin clave o si falla la red, y por lo demás se ve la
  vista en vivo a plena resolución. El almacenamiento permanente de imágenes de Street View está limitado
  por las condiciones de Google, así que es decisión y responsabilidad tuya; la descarga consume la cuota
  de tu propia cuenta de Google. Las *políticas* permiten guardar los identificadores de panorama (pueden
  cambiar con el tiempo, así que junto a ellos se conservan la coordenada y la marca de tiempo, y la vista
  incrustada recibe también la coordenada). Este programa no es un producto de Google ni tiene relación
  con Google.
- **Descripciones de entrenamientos:** la biblioteca integrada es texto propio, redactado a partir de
  protocolos de entrenamiento públicos (British Cycling, TrainingPeaks, CTS, FasCat, TrainerRoad y otros)
  y de estudios de ciencia del entrenamiento (Rønnestad, Seiler, Helgerud, Billat, Gibala, Bossi, Karsten
  y otros); las fuentes usadas aparecen por entrenamiento en el campo `sources` de
  `crates/ride-model/assets/workouts.json`.
- **Planificación de rutas:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de)
  (servicio público; solo se llama si pegas un enlace de bikerouter, y la ruta la calcula su servidor).
  Los `tests/fixtures/brouter_*.gpx` son respuestas de BRouter, con datos de OSM (ODbL), y esa misma ruta
  aparece en las capturas del README.
- **Relleno de altitud que falta (opcional):** [Open-Meteo](https://open-meteo.com) Elevation API,
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), para uso no comercial.

Nuestra licencia no cubre estos datos ni estos servicios.

### Aviso legal

El software se ofrece «tal cual», sin garantía; los detalles están en [LICENSE-MIT](../../../LICENSE-MIT)
y [LICENSE-APACHE](../../../LICENSE-APACHE). El control del dispositivo (el ajuste automático de la
resistencia del rodillo) corre de tu cuenta y riesgo.
