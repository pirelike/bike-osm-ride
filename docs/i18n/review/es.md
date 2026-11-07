# Spanish (es) review: 1061 rows read, 65 rows flagged (66 findings)

| # | key | hu | current | category | problem | suggestion | confidence |
|---|---|---|---|---|---|---|---|
| 606 | ui_dialogs.settings.ki | Ki | Desactivado | length | Probably a short toggle value; "Desactivado" is much longer and may not fit a narrow control (no tight flag is set, so this is a risk only) | "No" or "Apagado" if the control is narrow | low |
| 303 | model.workout.konnyu_regeneralo_tekeres | Könnyű, regeneráló tekerés. | Pedaleo suave y regenerativo. | meaning | "regenerativo" means regenerative (as in regenerative braking), so the recovery purpose of the source is lost | "Pedaleo suave de recuperación." | medium |
| 740 | ui_hud.climb_summary_inside | ... +{remaining_gain} m hátra | ... {hero} km y +{remaining_gain} m por delante | meaning | "hátra" means "still to go", but "por delante" means "ahead", which reverses the sense of the pivot "to go" | "{hero} km y +{remaining_gain} m por recorrer" | medium |
| 751 | ui_hud.dash.fajlagos | Fajlagos | Relativa | meaning | "Relativa" drops the per-kg meaning that the pivot "Per kg" carries | "Por kg" (or "W/kg") | medium |
| 167 | app.updater.nyisd_meg_a_kiadas_oldalat_toltsd_le_a_deb_csomagot_ | no line break in hu or en | "...e instálalo" + line break + 13 spaces + "(sudo apt install ./bike-osm-ride-*.deb)." | spaces | Stray line break and indentation that neither the Hungarian nor the English contains | Join into one line: "Abre la página de la versión, descarga el paquete .deb e instálalo (sudo apt install ./bike-osm-ride-*.deb)." | medium |
| 168 | app.updater.nyisd_meg_a_kiadas_oldalat_toltsd_le_a_dmg_t_es_huzd | no line break in hu or en | "...arrastra bike-osm-ride.app" + line break + 13 spaces + "a la carpeta Aplicaciones." | spaces | Same stray line break and indentation | "...arrastra bike-osm-ride.app a la carpeta Aplicaciones." | medium |
| 586 | ui_dialogs.settings.ban_es_kapcsold_be_a_szamlazast_a_google_havi_ingyen | "-ban, és kapcsold be..." (no leading space) | " y activa la facturación..." (leading space) | spaces | Target starts with a space that the Hungarian source does not have; the English pivot has one, so the mismatch was copied from it | Drop the leading space unless the call site joins this text after a word and needs it | medium |
| 39 | app.controller.backup.elem_kimaradt_ismeretlen_vagy_serult | {n} elem kimaradt | Se omitieron {n} elementos | style | Plural not split: with n = 1 it reads "Se omitieron 1 elementos" | Add .one / .other keys | low |
| 41 | app.controller.backup.mentve_name_utvonal_tura_edzes | ({routes} útvonal, {rides} túra, ...) | ({routes} rutas, {rides} salidas, {workouts} entrenamientos) | style | Count nouns are fixed plurals, so one route reads "1 rutas" | Split into .one / .other keys, or rephrase without counts | low |
| 46 | app.controller.backup.visszaallitva_utvonal_tura_edzes | Visszaállítva: {routes} útvonal, ... | Restaurado: {routes} rutas, {rides} salidas, {workouts} entrenamientos. | style | Same plural problem as #41 | Split into .one / .other keys | low |
| 693 | ui_dialogs.welcome.recent_count_utvonal | {recent_count} útvonal | {recent_count} rutas | style | No .one / .other split, so one recent route reads "1 rutas" | Add .one ("1 ruta") and .other keys | low |
| 720 | ui_dialogs.workouts.imported_count_importalt | {imported_count} importált | {imported_count} importados | style | No plural split; one imported workout reads "1 importados" | Add .one / .other keys | low |
| 59 | app.controller.importalva_blokk_perc | „{name}” importálva | «{name}» importado | style | Guillemets «» where STYLE.tsv prescribes “ ” for es | “{name}” importado | medium |
| 89 | app.controller.review.importalva_1_km | „{name}” importálva | «{name}» importado | style | Same quotation mark rule | “{name}” importado | medium |
| 247 | ble.scan.a_kereses_nem_lat_egyetlen_bluetooth_eszkozt_sem_ell | „Közeli eszközök” | «Dispositivos cercanos» | style | Same quotation mark rule | “Dispositivos cercanos” | medium |
| 395 | route.bikerouter.a_linkben_ervenytelen_a_z_name_parameter | „{name}” | «{name}» | style | Same quotation mark rule | “{name}” | medium |
| 397 | route.bikerouter.a_linkben_nincs_utvonal_hianyzik_a_lonlats_resz_terv | „lonlats” | «lonlats» | style | Same quotation mark rule | “lonlats” | medium |
| 464 | ui_dialogs.about.a_szoftver_ahogy_van_all_rendelkezesre_garancia_nelk | „ahogy van” | «tal cual» | style | Same quotation mark rule | “tal cual” | medium |
| 529 | ui_dialogs.helpers.ehhez_az_utvonalhoz_meg_nincs_mentett_menet_a_gpx_sa | „Az útvonal ideje” | «Tiempo de la ruta» | style | Same quotation mark rule | “Tiempo de la ruta” | medium |
| 533 | ui_dialogs.helpers.ez_nem_bikerouter_de_link_a_bikerouter_de_megosztas_ | „Megosztás” | «Compartir» | style | Same quotation mark rule | “Compartir” | medium |
| 549 | ui_dialogs.helpers.rides_de_mind_szimulalt_a_legjobb_szellem_csak_valod | „Legjobb”, „Legutóbbi menet” | «Mejor», «Última salida» | style | Same quotation mark rule | “Mejor”, “Última salida” | medium |
| 555 | ui_dialogs.helpers.szokatlan_kulcs_a_google_api_kulcsok_altalaban_aiza_ | „AIza” | «AIza» | style | Same quotation mark rule | “AIza” | medium |
| 568 | ui_dialogs.settings.a_korabbi_ugyanezen_az_utvonalon_tekert_menet_vissza | „Legjobb”, „Az útvonal ideje” | «Mejor», «Tiempo de la ruta» | style | Same quotation mark rule | “Mejor”, “Tiempo de la ruta” | medium |
| 583 | ui_dialogs.settings.az_automatikus_a_keszulek_nyelvet_hasznalja_ha_az_ta | Automatikus (no quotes in hu) | «Automático» | style | Same quotation mark rule; the quotes are added by the translator | “Automático” | medium |
| 675 | ui_dialogs.welcome.a_bikerouter_de_megosztas_linkje_tartalmazza_az_utvo | „Megosztás” | «Compartir» | style | Same quotation mark rule | “Compartir” | medium |
| 710 | ui_dialogs.workouts.ebben_a_kategoriaban_nincs_a_cimkeknek_megfelelo_edz | „Mind” nézetet | «Todos» | style | Same quotation mark rule | “Todos” | medium |
| 147 | app.shell.speed_ellenallas_offset_1 | {speed} · ellenállás {offset} % | {speed} · resistencia {offset} % | style | Space before % (style: no space before %) | "{offset}%" | medium |
| 739 | ui_hud.climb_summary_ahead | {grade} % átlagos meredekség | {grade} % de pendiente media | style | Space before % | "{grade}% de pendiente media" | medium |
| 740 | ui_hud.climb_summary_inside | {grade} % átlagos meredekség; {pct} % kész | {grade} % de pendiente media; {pct} % completado | style | Space before % in both placeholders | "{grade}% ... {pct}% completado" | medium |
| 21 | app.controller.az_edzes_celwattjat_nincs_mire_kuldeni_nincs_trainer | Trainer szerepben | en el papel de rodillo (twice) | terminology | "papel" is a literal calque of szerep; the same role concept is "rol" in ui_dialogs.devices (Conectar como otro rol) | "en el rol de rodillo" | medium |
| 66 | app.controller.menet_elmentve_a_tekeres_naploba_szimulalt_menet_nem | PR | no será un récord personal | terminology | Glossary says PR is never translated; the target renders it as "récord personal" | "no será un PR" | high |
| 124 | app.shell.edzes_elore | Edzés · előre | Entrenamiento · vista previa | terminology | The look-ahead concept is "Vista anticipada" in settings and profile, but "vista previa" here | "Entrenamiento · vista anticipada" (or one term everywhere) | low |
| 149 | app.shell.szint_elore | Szint · előre | Desnivel · vista previa | terminology | Glossary maps elevation to "altitud"; "Desnivel" is elevation gain. This label is the elevation profile view | "Altitud · vista anticipada" (as ui_profile.magassag) | low |
| 150 | app.shell.szint_teljes | Szint · teljes | Desnivel · completo | terminology | Same as #149 | "Altitud · completo" | low |
| 151 | app.shell.tekeres | Tekerés | En marcha | terminology | Glossary gives tekerés = pedaleo; the HUD state label for the same word is "Pedaleando" (ui_hud.state_label) | "Pedaleando" | medium |
| 306 | model.workout.kuszobintervallumok_es_ftp_tesztek | Küszöbintervallumok és FTP-tesztek. | Intervalos de umbral y tests de FTP. | terminology | Glossary: teszt = prueba; "tests" is an English loan | "pruebas de FTP" | medium |
| 307 | model.workout.kutatason_alapul | Kutatáson alapul | Basado en investigación | terminology | Glossary gives "basado en estudios" | "Basado en estudios" | low |
| 310 | model.workout.lepcsos_es_idofutam_tesztek | Lépcsős és időfutam-tesztek. | Tests escalonados y contrarreloj. | terminology | Glossary teszt = prueba | "Pruebas escalonadas y contrarreloj." | medium |
| 312 | model.workout.levezetes | Levezetés | Vuelta a la calma | terminology | Glossary cool-down = enfriamiento | "Enfriamiento" | medium |
| 324 | model.workout.regeneracio | Regeneráció | Regeneración | terminology | Glossary recovery = recuperación | "Recuperación" | medium |
| 339 | model.workout.tempo | Tempó | Tempo | terminology | Glossary gives tempó = ritmo | "Ritmo" | medium |
| 340 | model.workout.teszt | Teszt | Test | terminology | Glossary teszt = prueba | "Prueba" | medium |
| 346 | model.workout.verseny | Verseny | Competición | terminology | Glossary race = carrera; versenydinamika in the same catalog uses "carrera" | "Carrera" | medium |
| 456 | sim.workout_import.levezetes | Levezetés | Vuelta a la calma | terminology | Same as #312 | "Enfriamiento" | medium |
| 741 | ui_hud.csucs | Csúcs | Máximo | terminology | Glossary csúcs = pico; "Máximo" differs | "Pico" | medium |
| 742 | ui_hud.csucspulzus | Csúcspulzus | Pulso máximo | terminology | Same as #741 | "Pulso pico" | medium |
| 743 | ui_hud.csucssebesseg | Csúcssebesség | Velocidad máxima | terminology | Same as #741 | "Velocidad pico" | medium |
| 744 | ui_hud.csucsteljesitmeny | Csúcsteljesítmény | Potencia máxima | terminology | Same as #741 | "Potencia pico" | medium |
| 967 | ui_review.power.csucsteljesitmeny | Csúcsteljesítmény | Potencia máxima | terminology | Same as #741 | "Potencia pico" | medium |
| 638 | ui_dialogs.settings.torles | Törlés | Borrar | terminology | Glossary törlés = eliminar; the other delete messages (#80) use eliminar | "Eliminar" | medium |
| 929 | ui_review.edzes_torlese | Edzés törlése | Borrar el entrenamiento | terminology | Same as #638 | "Eliminar el entrenamiento" | medium |
| 993 | ui_review.torles | Törlés | Borrar | terminology | Same as #638 | "Eliminar" | medium |
| 734 | ui_hud.befejezes | Befejezés | Finalizar | terminology | Glossary befejezés = terminar | "Terminar" | low |
| 827 | ui_kit.palette.aktiv_regeneracio | Aktív regeneráció | Regeneración activa | terminology | Glossary recovery = recuperación | "Recuperación activa" | medium |
| 840 | ui_kit.palette.tempo | Tempó | Tempo | terminology | Glossary tempó = ritmo | "Ritmo" | medium |
| 965 | ui_review.power.az_utolso_buckets_len_period_name_total_rides_edzes_ | szimulált edzések | las salidas simuladas | terminology | The same sentence calls the other rides "entrenamientos"; the source uses edzés for simulated rides too | "los entrenamientos simulados" | medium |
| 979 | ui_review.power.meg_nincs_20_perces_mert_teljesitmeny_az_edzesek_koz | „FTP-teszt 20 perc” | «Test de FTP de 20 minutos» | terminology | Glossary teszt = prueba; quotes also break the rule. The name must match the workout library title, which this review could not see | "«Prueba de FTP de 20 minutos»", after checking the library title | low |
| 15 | app.controller.a_menetet_nem_sikerult_a_tekeres_naploba_menteni | tekerés-naplóba | No se pudo guardar la salida en el registro de ciclismo. | unnatural | "registro de ciclismo" is not how a rider names a ride log; elsewhere the same log is "registro" (#65) | "registro de salidas" | low |
| 224 | ble.multi.indoor_bike_data_ertelmezhetetlen_e | értelmezhetetlen | Indoor Bike Data no interpretable ({e}) | unnatural | Calque; Spanish says "no se puede interpretar" | "Indoor Bike Data no se puede interpretar ({e})" | low |
| 320 | model.workout.osszeallas | Összeállás | Consolidación | unnatural | "Consolidación" does not mean combined; it reads as "consolidation" | "Combinado" | medium |
| 738 | ui_hud.blokk_vege | Blokk vége | Fin bloque | unnatural | Missing article | "Fin de bloque" | low |
| 800 | ui_hud.nincs_eloirt_watt | nincs előírt watt | sin vatios prescritos | unnatural | "prescritos" is a medical word; the rest of the UI says "objetivo" for target watts | "sin vatios objetivo" | medium |
| 894 | ui_profile.workout.blokk_vege | Blokk vége | Fin bloque | unnatural | Missing article, as #738 | "Fin de bloque" | low |
| 949 | ui_review.kinek_az_adataival_szamoljunk | Kinek az adataival számoljunk | Con los datos de quién calculamos | unnatural | Ungrammatical as a label; a Spanish label would name the action | "Calcular con los datos de" | medium |
| 1002 | ui_streetview.download.offline_street_view_kesz_kep.other | Offline Street View kész: {count} kép | Street View sin conexión listo: {count} imágenes | unnatural | "listo" placed after the noun reads as a calque | "Street View sin conexión: listo, {count} imágenes" | low |
| 262 | i18n.test.only_hu | Csak magyar | (empty) | untranslated | Target is empty; the en pivot is empty too, so this is probably a deliberate fallback fixture | Confirm it is a fixture; if not, add the Spanish text | low |

## Patterns

- Glossary drift on core cycling terms: tempó (ritmo), regeneráció (recuperación), verseny (carrera), csúcs (pico), teszt (prueba), törlés (eliminar) and levezetés (enfriamiento) appear as Tempo, Regeneración, Competición, Máximo or Potencia máxima, Test, Borrar and Vuelta a la calma in about 25 rows. Decide whether the glossary or the catalog wins, then fix in one pass.
- Quotation marks: 14 rows use « » where STYLE.tsv prescribes “ ” for es. RAE prefers «», so the style rule itself needs a decision before the rows are changed.
- Plurals: count strings with a fixed noun (rutas, importados, entrenamientos, elementos) have no .one / .other split, so a count of one reads "1 rutas". The style file asks for one / other keys.
- Extraction or copy artifacts: #167 and #168 carry a stray line break with indentation, and #586 has a leading space the source lacks. The `tight` column is empty for all 1061 rows, so length was judged heuristically (only #606 flagged).
- Mechanical checks passed for the rest: placeholders match the source in every row, no Hungarian or English words remain, no usted forms, no triple dots, decimal commas are used and brand names are unchanged. The percent-space rule is broken only in #147, #739 and #740.
