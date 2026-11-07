# Guía del usuario

Cómo se usa bike-osm-ride desde el primer arranque hasta el resumen de la salida. La instalación y la
visión general están en el [README](../../../README.md); los detalles para desarrolladores, en
[`ARCHITECTURE.md`](../../ARCHITECTURE.md) y compañía. Las capturas son de la versión 0.3.0 y muestran la
interfaz en su idioma de origen (húngaro); la aplicación también habla español. La ruta que aparece en
este documento es una ruta real de BRouter que está en el repositorio como dato de prueba (©
colaboradores de OpenStreetMap, ODbL).

**Contenido:** [Primer arranque](#primer-arranque) · [Cargar una ruta](#cargar-una-ruta) ·
[Conectar dispositivos](#conectar-dispositivos) · [Pedalear](#pedalear) · [La pantalla y el diseño](#la-pantalla-y-el-diseño) ·
[Entrenamientos](#entrenamientos) · [Cambio virtual](#cambio-virtual) · [Fantasma y segmentos](#fantasma-y-segmentos) ·
[Resumen y TCX](#resumen-y-tcx) · [Revisión y comparación](#revisión-y-comparación) ·
[Street View](#street-view-opcional) · [Teléfono](#teléfono-y-tableta) · [Ajustes](#ajustes) ·
[Teclas](#atajos-de-teclado) · [Solución de problemas](#solución-de-problemas)

## Primer arranque

Al arrancar se abre el diálogo **Ruta** sobre la pantalla de salida vacía. Hay tres formas de empezar: un
archivo GPX, un enlace de bikerouter o la ruta de demostración. Sin hardware también puedes pedalear con el
**ciclista simulado** (ver [Dispositivos](#conectar-dispositivos)).

![El diálogo Ruta](../../screenshots/route-dialog.png)

## Cargar una ruta

- **Archivo GPX.** Con el explorador de archivos (`Abrir GPX`, `Ctrl+O`) o arrastrándolo a la ventana.
  Cualquier GPX sirve: el programa usa los puntos `<trk>`/`<trkseg>` (varios tramos se encadenan) y, si no
  los hay, los puntos `<rte>`; también entiende un archivo con acentos, con BOM y lleno de espacios de
  nombres; con un archivo dañado (truncado) da un mensaje de error comprensible en lugar de una ruta
  recortada. El límite de tamaño es de 64 MB.
- **Enlace de bikerouter.** Planifica una ruta en [bikerouter.de](https://bikerouter.de), pulsa el enlace
  *Compartir* y pégalo en el campo (`Pegar` y luego `Cargar ruta`). Del enlace se traen también los puntos,
  el perfil, la alternativa y los evitados; la ruta la calcula el servidor de BRouter. Las direcciones
  BRouter en bruto y los parámetros `lonlats=…` también valen.
- **Ruta de demostración.** Un circuito corto y con desniveles por Budapest para probar (sus coordenadas
  van *cerca* de las calles, no por ellas).
- **Rutas recientes.** Las últimas 20, con datos (distancia, desnivel, cuándo la abriste, origen); la
  papelera la quita de la lista (el registro de salidas se conserva).

Si el GPX no trae datos de altitud (utilizables), el programa los pide al servicio Open-Meteo (salen las
coordenadas); si eso tampoco funciona, la ruta se carga con un perfil plano y avisa. Del archivo:

- si junto a la pista principal hay una pista *más corta* que discurre por la ruta (por ejemplo, un
  segmento de Strava metido dentro), o una pista marcada con `<type>segment</type>`, esa será un
  **segmento**: se ve como una franja en el perfil y durante la salida aparece «Segmento: … X km
  restantes» (la propia exportación GPX de Strava no incluye segmentos);
- si *todos* los puntos llevan marca de tiempo (exportación de Strava o de Garmin), la ruta es **también
  su propio fantasma**: ver el capítulo [Fantasma](#fantasma-y-segmentos).

El programa busca **subidas** en la ruta: tramos de al menos 500 m de longitud, con una media de al menos
el 3 % y que ganen al menos 25 m de desnivel, con categoría de 4.ª a HC (según longitud × pendiente media).

## Conectar dispositivos

El botón **Dispositivos** de la barra superior (al lado, tres puntos indican cuántos papeles se han
conectado). Hay cinco papeles: **Rodillo** (rodillo inteligente, FTMS), **Pulso**, **Potenciómetro** (CPS),
**Sensor de cadencia** (CSC) y **Cambio** (Zwift Click/Play).

![El diálogo Dispositivos](../../screenshots/devices.png)

1. `Iniciar búsqueda`: el programa busca sin filtrar y clasifica los dispositivos según los servicios que
   anuncian (la búsqueda se detiene sola a los 30 segundos).
2. Junto a cada dispositivo encontrado están los botones de los papeles: te conectas al papel que
   necesites. Un mismo dispositivo físico puede hacer **varios papeles a la vez**: si tu rodillo emite
   también potencia, cadencia o pulso, una sola conexión rellena todas las tarjetas (en las tarjetas se ve
   la marca *dispositivo compartido*). De los datos de los sensores gana la fuente más precisa (potencia:
   el potenciómetro antes que el rodillo; cadencia: CSC, luego el potenciómetro, luego el rodillo; pulso: la
   banda de pecho antes que el rodillo).
3. Lo que no se ha actualizado en 3 segundos pasa a «—»; si la conexión se corta, el programa lo reintenta
   tres veces.
4. **Conecta el rodillo en el papel Rodillo**: el papel de potenciómetro solo lee, el control (subidas,
   ERG) lo da el papel Rodillo.
5. **Ciclista simulado**: el primer elemento de la lista, siempre está ahí, incluso sin Bluetooth. En el
   panel de la izquierda puedes fijar los vatios con un deslizador (o con `↑`/`↓`, ±10 W); botones rápidos:
   Z2 150 W, Z4 250 W, Sprint 700 W, Libre 0 W. La cadencia y el pulso le siguen de forma creíble.

**¿Cómo controla el rodillo?** En una subida el rodillo recibe la pendiente del tramo que tienes delante
(aproximadamente 5–45 m por delante, suavizada, hasta ±20 %, escalada de 0 a 150 % con el ajuste
*Intensidad de la pendiente*). En pausa y al final el rodillo pasa a una resistencia plana y neutra. En un
entrenamiento (ERG) mantiene los vatios objetivo. El rodillo recibe las órdenes a través de un *filtro de
seguridad*: solo salen las operaciones necesarias y los objetivos se recortan al rango propio del rodillo.

> **Importante, y con sinceridad.** El control ERG lo hemos probado con un rodillo real (VE200); la
> simulación de subidas no la hemos confirmado todavía con un rodillo real, y el mando Zwift Click/Play, el
> pulsómetro y el potenciómetro independientes no los hemos probado con hardware real. Los detalles, en
> [`TESTING.md`](../../TESTING.md).

## Pedalear

`Inicio` (o `Espacio`) arranca la salida, `Pausa` la detiene (el tiempo también se detiene) y `Finalizar`
la cierra. Al final de la ruta la salida **termina sola**. Antes de empezar puedes saltar a cualquier punto
de la ruta haciendo clic en el perfil; durante la salida eso no funciona. El programa mantiene el equipo
despierto mientras pedaleas (Windows, Linux, macOS; en Android no).

La velocidad sale de los **vatios** medidos, con el modelo de Martin validado con mediciones en carretera
(gravedad, rodadura, fricción de rodamientos, resistencia del aire con radios, inercia), a partir de tu peso,
el peso de la bicicleta, el CdA y el Crr; con el interruptor *Usar la velocidad del rodillo* puedes usar
también la velocidad propia del rodillo. Detalles: [`SIMULATION.md`](../../SIMULATION.md).

![Salida, tema oscuro](../../screenshots/ride-dark.png)

## La pantalla y el diseño

La pantalla de salida se compone de **paneles**: *Mapa*, *Potencia* (el número grande, media de 3 s, W/kg,
zona), *Próxima subida*, *Estadísticas*, *Perfil de altitud*, *Control*, *Gráficos*, *Entrenamiento*,
*Ciclista simulado*, *Cambio*, *Navegador*. Funcionan como en un gestor de ventanas de mosaico: **no hay
solapamientos ni huecos.**

- **Redimensionar:** arrastrando el separador (la línea salta a celdas de una rejilla de 96 × 64,
  respetando el tamaño mínimo de los paneles; solo cambian los dos paneles vecinos).
- **Mover:** agarrando la cabecera de un panel y soltándolo en el *borde* de otro panel lo divide en dos; si
  lo sueltas en el *centro*, intercambian sitio (un marco fantasma muestra lo que va a pasar).
- **Cerrar:** con la × del panel; el vecino ocupa su lugar y el panel pasa a la barra superior como *chip*.
  Al pulsar el chip vuelve donde estaba; arrastrándolo a la superficie, va donde lo sueltes.
- **Todo se guarda.** *Ajustes → Diseño → Restablecer el diseño* devuelve la disposición inicial.
- El diseño (y la vista del perfil) se guarda **por separado con entrenamiento y sin él**: el
  entrenamiento elegido tiene su propio diseño, que sigue al libre hasta que lo cambies ahí.

![Tema claro](../../screenshots/ride-light.png)

**Mapa.** El mapa muestra las teselas estándar de OpenStreetMap; los estilos *Oscuro* y *Claro* las filtran
en local y *OSM* es el original. Con la rueda acercas alrededor del ratón y arrastrando lo desplazas; con los
botones de abajo a la izquierda activas el modo seguimiento y lo centras (`M`). El color de la ruta indica
la pendiente (frío: descenso, gris: llano, amarillo–naranja–rojo–púrpura: cada vez más empinado) y el
ciclista es el disco marcado con una flecha. Abajo, la atribución de OSM y la escala.

**Perfil de altitud.** `Completo` (toda la ruta) o `Vista anticipada` (los próximos ~3 km; la longitud se
ajusta en los ajustes, `P`); las subidas aparecen como banderas con su categoría y la escala de km abajo; al
hacer clic saltas (antes de la salida) y al pasar el ratón lees los datos. La escala de la vista anticipada
solo cambia cuando no hay más remedio. Si hay un entrenamiento elegido, el conmutador **Altitud |
Entrenamiento** muestra los bloques del entrenamiento.

**Tamaño del texto.** *Ajustes → Apariencia → Tamaño del texto* (80–160 %).

## Entrenamientos

El botón **Entrenamiento** de la barra superior (o `W`). La biblioteca integrada tiene **93 entrenamientos**
en 12 categorías (recuperación, resistencia, tempo, sweet spot, umbral, VO2max, anaerobio, sprint, escalada,
competición, test, mixto), entre ellos el test de FTP de 20 y de 8 minutos, tests ramp y de CP, y muchas
sesiones que siguen protocolos de investigación publicados (Rønnestad 30/15, Seiler 4 × 8, Helgerud 4 × 4,
Billat 30/30, los intervalos de sprint de Gibala y otros; las fuentes están, entrenamiento por entrenamiento,
en el campo `sources` de `workouts.json`).

![El selector de entrenamientos](../../screenshots/workout-picker.png)

- A la izquierda, categorías y **etiquetas** (para qué sirve, nivel, carácter, periodo; filtran con AND y
  al lado muestran el número de resultados); en el centro, la lista; a la derecha, la descripción (qué, por
  qué, cómo), el **esquema de bloques coloreado por zona** y la lista bloque a bloque con tiempo, objetivo e
  instrucción del entrenador.
- Los objetivos se calculan a partir del **%FTP**: indica tu FTP en *Ajustes → Ciclista*; si lo cambias, la
  parte que queda se recalcula al instante.
- Durante la salida el rodillo mantiene los vatios objetivo en **modo ERG**; la tarjeta *Entrenamiento* y la
  vista *Entrenamiento* del panel del perfil muestran el bloque, el tiempo restante, el bloque siguiente y
  el **mensaje del entrenador** (en el momento adecuado del entrenamiento, durante 12 segundos, solo ahí y
  no como aviso emergente). La pausa y la salida también aquí ponen el rodillo en neutro.
- **Entrenamiento propio:** *Importar entrenamiento…* lee también un archivo de MyWhoosh o de Zwift
  (`.xml`, `.zwo`) y la exportación JSON de MyWhoosh; se guarda en la carpeta `workouts/` y desde entonces se
  puede elegir como los demás.
- Durante la salida no se puede cambiar el entrenamiento.

![Entrenamiento en modo ERG](../../screenshots/workout.png)

## Cambio virtual

*Ajustes → Rodillo → Cambio virtual.* Transmisión 2 × 12 (50/34 y 11–34), **23 marchas**. Cambias con las
teclas `←`/`→`, desde el panel Cambio, con los botones de cambio en el teléfono o con un mando Zwift
Click/Play. Como un rodillo inteligente no tiene cambio, la marcha modifica la pendiente enviada al rodillo
(la más fácil, aproximadamente −5 %; la más dura, aproximadamente +5 %); el panel muestra el desarrollo de la
marcha y la velocidad que sale de la cadencia. **Durante un entrenamiento (ERG) el cambio no se ve y no
actúa** (manda el objetivo de vatios). Con un mando Zwift Click/Play, el primer toque activa el cambio; el
botón `+` del Click va hacia más duro y el `−` hacia más fácil; en el mando derecho del Play es el botón
`Shift`/`Y` y en el izquierdo el `Shift`/`B` (esto todavía no se ha probado con un mando real).

## Fantasma y segmentos

Cada salida terminada **se guarda automáticamente** con su ruta (registro por segundo, resumen, tiempos de
segmento), y en esa misma ruta una salida anterior se puede reproducir como **fantasma**: ya espera en la
línea de salida antes de empezar; durante la salida, un marcador translúcido en el mapa y en el perfil
muestra por dónde iba cuando tú estás aquí; la ventaja o desventaja se ve en segundos. *Ajustes → Mapa y
perfil → Fantasma*:

| Modo | Qué reproduce |
|---|---|
| Desactivado | nada |
| Tiempo de la ruta | las marcas de tiempo del propio GPX cargado (con una exportación de Strava o Garmin puedes competir ya antes de la primera salida; las rutas de BRouter no llevan tiempo) |
| Última salida | la última salida de la ruta |
| Mejor (PR) | la salida **completa** con el menor tiempo; si aún no ha llegado ninguna a meta, la que llegó más lejos (parcial), que se detiene donde acaba |

La línea bajo el selector dice por qué no hay fantasma en ese momento. Las subidas reconocidas son
**segmentos locales**: antes de la salida se ve el mejor tiempo (PR), pedaleando dentro, la diferencia medida
al segundo y, en el resumen, el «PR anterior» y la insignia «¡PR nuevo!». Una salida hecha con el
**ciclista simulado** se guarda, pero no será PR ni fantasma «Mejor».

## Resumen y TCX

Al final de la ruta (o después de `Finalizar`) se abre el **resumen**: distancia, tiempo, desnivel, potencia
media y normalizada, IF, TSS, energía, velocidad media, pulso, la tabla de segmentos y el reparto de las
zonas de potencia y de pulso.

![El resumen](../../screenshots/summary.png)

Abajo, el botón `Guardar como TCX` (esquema de Garmin; el nombre de archivo propuesto se compone de la
fecha y del nombre de la ruta) y `Ruta nueva`.

**Subir a Strava, Intervals.icu o TrainingPeaks:** no hay subida integrada. En la propia web de Strava,
dentro del menú *Subir archivo* (Upload), puedes subir a mano el TCX guardado; Intervals.icu y TrainingPeaks
también lo aceptan. Hasta la 0.3.1 había un subidor en segundo plano, pero en nuestro caso el inicio de
sesión tenía que repetirse después de cada salida, así que lo quitamos (0.3.2).

## Revisión y comparación

El botón **`Revisión`** (barra superior; en el teléfono, al final del diálogo `Ruta`, *Revisión de
entrenamientos*) abre una página a pantalla completa. `Esc` o `Atrás` la cierran; debajo, la pantalla de
salida sigue igual.

![La página de revisión](../../screenshots/review.png)

**Lista.** A la izquierda, la ruta (en el selector, todas las rutas con alguna salida guardada y la que está
cargada) y sus entrenamientos, el más reciente primero. Marca como máximo cuatro: cada uno recibe una letra
(A–D) y un color, y en el gráfico también cambia el patrón de la línea (la A es continua). **La A es el
entrenamiento de referencia**: el gráfico *Diferencia de tiempo* y la columna *Diferencia* de la tabla se
refieren a él (positivo = más lento). La papelera pide confirmación antes de borrar. Por defecto están
marcados los dos entrenamientos más recientes de la ruta.

**Gráfico.** *Potencia*, *Pulso*, *Cadencia*, *Velocidad* o *Diferencia de tiempo* en función de la
distancia. Si mueves el ratón (o el dedo) por el perfil o por el gráfico, en el cursor se lee el valor de
cada entrenamiento en la leyenda y en el mapa un punto indica el lugar. Si un entrenamiento no tiene ese
sensor (por ejemplo, sin pulsómetro), no tiene línea.

**Acercarse a un tramo.** Arrastra el ratón (o el dedo en una pantalla táctil) por el perfil o por el
gráfico, o pulsa los botones de *subida*; `Ruta completa` lo devuelve todo. El mapa se acerca al tramo y lo
destaca, y el gráfico y la tabla se limitan a ese mismo tramo. Un arrastre de menos de 100 m no cuenta como
tramo.

**Tabla del tramo.** Por entrenamiento: tiempo, velocidad media, potencia media y normalizada (NP, a partir
de al menos 30 segundos de datos), pulso medio y máximo, cadencia, desnivel, VAM y la diferencia con el
entrenamiento de referencia. Solo hay datos de un tramo que el entrenamiento haya recorrido **entero**; una
salida parcial queda vacía en los demás tramos.

### Potencia: máximos, FTP, carga

![La página de potencia](../../screenshots/review-power.png)

Arriba de la revisión, el conmutador **Comparación | Potencia** lleva a la otra página. Aquí están los
números de **todos** tus entrenamientos, independientemente de la ruta:

- **Potencia máxima.** La mejor media de vatios para 5 segundos y para 1, 5, 20 y 60 minutos, con la fecha y
  la ruta del entrenamiento. Si en la Comparación has marcado entrenamientos, sus valores aparecen columna a
  columna al lado (A–D). Un entrenamiento no tiene valor para un tiempo mayor que su propia duración.
- **FTP.** Propone como FTP el 95 % de tu mejor potencia de 20 minutos y muestra al lado la que está
  configurada ahora. El botón **Poner el FTP en N W** lo escribe en los Ajustes; la aplicación nunca lo
  cambia por su cuenta. El TSS de los entrenamientos guardados se queda calculado con el FTP de su momento.
  Para una propuesta exacta hace falta un esfuerzo real de 20 minutos a tope: el entrenamiento *Test de FTP
  20 minutos* (*Entrenamientos*, `W`) da justo eso.
- **Carga.** Las últimas 12 semanas o meses (*Semanal* | *Mensual*): las columnas muestran el TSS, encima el
  valor y, al pasar el ratón por la columna, el número y el tiempo de los entrenamientos.

**Solo cuentan los vatios medidos.** El ciclista simulado y los entrenamientos con vatios estimados (`~`) no
entran en los máximos ni en la propuesta de FTP (sí en el tiempo de la tendencia, no en su TSS). Un
entrenamiento importado en el que hubo potencia real sí cuenta. Los vatios propios del rodillo no son un
potenciómetro, así que la propuesta de FTP es tan exacta como ellos. Los números de los entrenamientos más
antiguos se calculan y se guardan la primera vez que se abre la página, lo que puede tardar unos segundos si
hay muchos entrenamientos.

### Importar un entrenamiento de otra aplicación

Con el botón **`Importar entrenamiento`** (o arrastrando el archivo a la página) cargas un GPX o un TCX, por
ejemplo uno exportado de Strava, Garmin o Wahoo. Se traen el pulso, la cadencia y, si los hay, los vatios.
Cosas que conviene saber:

- **La ruta la pones tú**: el entrenamiento se ajusta a la ruta que está marcada en la página (cárgala o
  elígela antes). Los puntos del trazo se proyectan sobre la ruta; lo que queda a más de 30 m de ella (un
  desvío, un error de GPS) se descarta, y la página dice cuántos se han descartado. Si el entrenamiento solo
  cubre una parte de la ruta, avisa.
- La **pausa automática** (una parada larga) no cuenta en el tiempo. Un archivo sin marcas de tiempo (por
  ejemplo, una ruta planificada) no se puede importar como entrenamiento.
- El mismo entrenamiento (la misma hora de inicio) no se puede importar dos veces.
- El entrenamiento importado **no será PR ni fantasma**: una salida de fuera, con paradas y rodando, no se
  puede comparar con tus tiempos en el rodillo. En la lista lo indica la etiqueta `Importado`.
- En el teléfono la importación aún no está disponible. El formato FIT no lo leemos; Strava exporta también
  GPX y TCX.

### Vatios estimados

Si el archivo **no trae datos de potencia**, el programa ejecuta la física de la simulación al revés: a partir
de la velocidad y de la pendiente calcula cuántos vatios hicieron falta (el cambio de energía cinética y el
trabajo contra la gravedad, la rodadura, los rodamientos y el aire en una ventana de 9 segundos). En un
entrenamiento así los números llevan el signo **`~`** y se ve la etiqueta `Vatios estimados`.

Con el icono del deslizador (*Los ajustes de la estimación*) puedes indicar con los datos de quién debe
calcular:

| Opción | Qué usa |
|---|---|
| **Mis datos** | el peso, la bicicleta y el Crr de los Ajustes |
| **Valores propios** | el *peso*, la *bicicleta* y la *resistencia a la rodadura (Crr)* indicados aquí, si estás mirando el entrenamiento de otra persona |

El CdA, el rendimiento de la transmisión y la densidad del aire se toman siempre de los Ajustes. El Crr en
asfalto bueno es de unos 0,004; **para la grava no hay un modelo aparte**, ahí solo modifica el Crr que
indiques. `Recalcular` calcula con los valores nuevos y guarda el entrenamiento; los vatios medidos no los
sobrescribe nunca una estimación.

**¿Cuánto de exacto es?** Es una aproximación. No conoce el viento ni la estela (el viento de cara parece
menos vatios), la pendiente la da la ruta y no la altitud ruidosa del GPS del archivo, y en curvas cerradas
la proyección puede dar una velocidad ruidosa durante un momento. Calculando al revés con la física del
propio programa, la vuelta sale con un error de aproximadamente el 3 %; no lo hemos comparado todavía con un
potenciómetro real, así que conviene contar con desviaciones del 10–15 %.

## Street View (opcional)

Google solo lo permite a través de su API, así que hace falta **tu propia clave de API de Google Maps**
(*Ajustes → Street View*; los pasos están explicados allí: proyecto de Google Cloud, facturación, habilitar
*Maps Embed API* y *Street View Static API*). Sin clave no aparece nada. Con `V` o con el botón `Street
View` del panel del mapa, una ventana pequeña aparece en la esquina inferior derecha del mapa; con `X` (o con
el botón de cambio) se intercambian las dos vistas. Cuando hay un error llega un mensaje comprensible (clave
no válida, cuota agotada, en ese lugar no hay panorama). El botón **Descargar** guarda una imagen cada
75–250 m para la ruta (como máximo unas 900; valen 30 días y se pueden borrar en los Ajustes), así la próxima
vez recorres el mismo camino sin clave y sin red. La descarga consume tu propia cuota de Google. Detalles y
las reglas de Google: [`STREETVIEW.md`](../../STREETVIEW.md).

## Teléfono y tableta

En una ventana más estrecha de 1000 px aparece la vista con pestañas: abajo **En marcha · Mapa · Gráficos ·
Datos** y, encima, la fila grande de botones `Inicio`/`Pausa`/`Finalizar`. En la pestaña **En marcha** está
en una sola pantalla todo lo que hace falta mientras pedaleas: el bloque del entrenamiento (tiempo restante,
bloque siguiente, el mensaje del entrenador), la potencia medida contra el objetivo (*en el objetivo / corto
/ largo*), la franja de zonas, cadencia, pulso, velocidad y pendiente, y debajo el perfil (deslizando el dedo
cambias entre sus vistas). Los diálogos ocupan toda la pantalla en el teléfono.

<p align="center">
  <img src="../../screenshots/phone-ride.png" width="280" alt="Diseño de teléfono, pestaña En marcha con entrenamiento">
  &nbsp;&nbsp;
  <img src="../../screenshots/phone-map.png" width="280" alt="Diseño de teléfono, pestaña Mapa">
</p>

La aplicación de Android es este mismo programa como APK. La ruta la cargas con el selector de archivos del
sistema, con un enlace de bikerouter, de entre las recientes o con la ruta de demostración; en el primer
arranque pide los permisos de Bluetooth; durante la salida la pantalla se queda encendida (en pausa y al
final de la salida vuelve a mandar el límite de tiempo propio del sistema; en el teléfono todavía no lo hemos
probado, [`ANDROID.md`](../../ANDROID.md)). Si la aplicación recibe un error, una franja roja muestra su
primera línea y el programa sigue siendo utilizable.

## Ajustes

Cada cambio se aplica al instante y se guarda.

![El diálogo Ajustes](../../screenshots/settings.png)

| Grupo | Qué ajustas |
|---|---|
| **Ciclista** | peso (75 kg), peso de la bicicleta (9 kg), FTP (200 W), pulso máximo (190), CdA (0,32 m²), Crr (0,004) |
| **Rodillo** | intensidad de la pendiente (0–150 %), usar la velocidad del rodillo, cambio virtual |
| **Apariencia** | tamaño del texto (80–160 %), tema (oscuro/claro), estilo del mapa (oscuro/claro/OSM) |
| **Mapa y perfil** | que el mapa siga al ciclista, vista del perfil (vista anticipada/completo), longitud de la vista anticipada, fantasma |
| **Diseño** | restablecer la disposición inicial |
| **Copia de seguridad** | *Guardar en archivo* y *Restaurar desde archivo* (ver abajo) |
| **Street View** | mostrar, la clave de Google, guardar los identificadores de panorama, descargar las imágenes, borrar los datos guardados |

### Copia de seguridad

*Guardar en archivo* crea un único **archivo zip** con tus rutas, tus salidas guardadas, tus entrenamientos
importados y tus ajustes. El archivo lo eliges tú (en Android, con el diálogo «Guardar como» del sistema) y
nada sale a la red. **No** lleva dentro la clave de Street View, las teselas del mapa, las imágenes de Street
View descargadas ni los registros: o son secretos o se pueden volver a descargar en cualquier momento.

*Restaurar desde archivo* **combina**: las rutas, salidas y entrenamientos de la copia se añaden a los
existentes, **no se borra nada y una salida existente no se sobrescribe**. Los ajustes de la copia, en
cambio, pasan a ser los tuyos (tu clave de Street View sigue siendo la anterior), y por eso la aplicación
pregunta antes. Restaurar el mismo archivo dos veces no hace nada. Al mudarte a un equipo nuevo, basta con
restaurar el archivo. Durante una salida no se puede hacer ni una copia ni una restauración; si el archivo
está dañado o es de una versión más nueva, la aplicación lo dice y no escribe nada. En Android esta parte
todavía no se ha probado en el teléfono.

## Atajos de teclado

| Tecla | Acción |
|---|---|
| `Espacio` | iniciar / pausar / continuar (al final, traer de vuelta el resumen) |
| `M` | modo seguimiento del mapa on/off |
| `P` | perfil de altitud: completo ↔ vista anticipada |
| `T` | tema oscuro/claro |
| `V` | panel de Street View on/off |
| `X` | intercambiar mapa y Street View |
| `W` | selector de entrenamientos |
| `←` / `→` | cambio virtual: más fácil / más duro |
| `↑` / `↓` | potencia del ciclista simulado ±10 W |
| `F11` | pantalla completa |
| `Esc` | cerrar la ventana, salir de pantalla completa |
| `Ctrl+O` | abrir un GPX |

Mientras escribes en un campo de texto las teclas no ejecutan ningún atajo (salvo `Esc`, `F11` y `Ctrl+O`);
con un diálogo abierto solo funcionan `Esc`, `F11` y, en el diálogo Ruta, `Ctrl+O`.

## Panel Navegador (solo Windows)

El panel **Navegador** integra en la ventana el **Firefox** instalado en el equipo, para que puedas ver una
serie o lo que quieras mientras pedaleas. Firefox usa su propio perfil (tus inicios de sesión se mantienen y
tu Firefox normal no se toca). En Mac y en Linux el panel no aparece. Al abrir un diálogo o al arrastrar un
panel, Firefox se oculta un momento (la interfaz no puede dibujar encima de una ventana nativa); cerrar el
panel cierra también Firefox; conviene que tenga al menos unos 500 px de ancho.

## Solución de problemas

| Síntoma | Qué hacer |
|---|---|
| El rodillo no reacciona a las subidas ni al entrenamiento | En el diálogo **Dispositivos** debe estar conectado en el papel *Rodillo* (el papel de potenciómetro solo lee). Mira el registro BLE (`ble.log`, según el §11 de [`BLUETOOTH.md`](../../BLUETOOTH.md)); que Zwift o una aplicación del teléfono no estén ocupando el rodillo. |
| No encuentra el dispositivo | Enciende el Bluetooth; en Windows la búsqueda no filtra, así que se ven muchos dispositivos: mira los papeles de cada uno. En Android concede los permisos de Bluetooth (*Iniciar búsqueda* vuelve a preguntar); en macOS el sistema pide permiso en la primera búsqueda. |
| El mapa está vacío o gris | No hay red y todavía no hay teselas en la caché: la ruta y los números funcionan igual. |
| La ruta no se carga | El mensaje de error dice el motivo (archivo dañado, enlace no admitido, sin red para BRouter). El enlace debe contener la parte `lonlats=`. |
| El perfil está plano | El archivo no trae datos de altitud y Open-Meteo tampoco estaba disponible. Carga un GPX con altitud o un enlace de bikerouter. |
| En Linux la ventana está vacía | El programa ajusta solo la variable `WEBKIT_DISABLE_DMABUF_RENDERER=1`; si aun así está vacía, prueba con `WEBKIT_DISABLE_COMPOSITING_MODE=1`. |
| macOS: «no se puede abrir» | Clic derecho → Abrir → Abrir (la aplicación no está notarizada); o `xattr -dr com.apple.quarantine /Applications/bike-osm-ride.app`. |
| Windows: si SmartScreen avisa | El programa no está firmado digitalmente, así que Windows puede detenerlo; se inicia por *Más información → Ejecutar de todas formas*. |
| Cuelgue / franja roja | La primera línea está en la franja y el registro completo en `crash.log` (en Android, `adb logcat -s bike-osm-ride`). Por favor, informa del fallo con el registro. |

**¿Dónde están mis datos?** Todo está en una sola carpeta, junto con los ajustes, el registro de salidas, las teselas y
los registros (si quieres una copia, usa el botón *Ajustes → Copia de seguridad*): Windows
`%LOCALAPPDATA%\bike-osm-ride`, macOS `~/Library/Application Support/bike-osm-ride`, Linux
`~/.local/share/bike-osm-ride` y, en Android, la carpeta privada de la aplicación. La estructura exacta la
describe [`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); lo que sale del equipo, [`PRIVACY.md`](../../PRIVACY.md).
