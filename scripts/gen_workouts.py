# Generates crates/ride-model/assets/workouts.json (the embedded workout library).
# Usage: python -I scripts/gen_workouts.py crates/ride-model/assets/workouts.json
#
# Every workout carries a one-line goal, a description (what, why, how), tags from the fixed
# vocabulary of `ride_model::WorkoutTag` and the public sources it is based on. The checks at the
# end mirror the Rust tests in `ride-model/src/workout.rs`.
import json
import sys

# ───────────────────────────── sources ─────────────────────────────
COGGAN = "https://cycling.favero.com/en/blog/power-training-zones-cycling"
FTP_ALLEN = "https://cycling.favero.com/blog/ftp-test-what-it-is-and-how-to-do-it/"
CTS_8MIN = "https://trainright.com/cts-field-test-why-eight-minutes-not-20/"
RONN_SI = "https://trainright.com/short-intervals-work-improve-performance/"
RONN_2021 = "https://e-space.mmu.ac.uk/630338/1/Ronnestad%202021%20-%20Superior%20Physiological%20Adaptations%20After%20a%20Microcycle%20of%20Short%20Intervals%20Versus%20Long%20Intervals%20in%20Cyclists.pdf"
ODDEN_2024 = "https://pmc.ncbi.nlm.nih.gov/articles/PMC11534653"
SEILER = "https://knowledgeiswatt.substack.com/p/42-hit-long-intervals-4x8-better"
BOSSI = "https://napier-repository.worktribe.com/output/2896921/optimizing-interval-training-through-power-output-variation-within-the-work-intervals"
GIBALA = "https://coachsci.sdsu.edu/csa/vol191/gibala.htm"
HELGERUD = "https://pubmed.ncbi.nlm.nih.gov/17414804/"
BILLAT = "https://www.triathlete.com/training/workouts/one-hour-workout-billats-30-30/"
POLARIZED = "https://www.frontiersin.org/journals/physiology/articles/10.3389/fphys.2014.00033/full"
DURABILITY = "https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12243304/"
DURABILITY_AMATEUR = "https://www.frontiersin.org/journals/sports-and-active-living/articles/10.3389/fspor.2025.1530162/text"
NORWEGIAN = "https://rouvy.com/blog/how-to-train-like-a-norwegian"
NORWEGIAN_TP = "https://www.trainingpeaks.com/blog/the-norwegian-training-method-simplified-how-to-use-it-at-any-level/"
LOW_CADENCE_RCT = "https://www.ncbi.nlm.nih.gov/pmc/articles/PMC3907705/"
LOW_CADENCE_CTS = "https://trainright.com/cycle-uphill-faster-with-low-cadence-high-torque-workout/"
CP_FIELD = "https://research.aber.ac.uk/en/publications/validity-and-reliability-of-critical-power-field-testing/"
POWER_PROFILE = "https://roadmancycling.com/glossary/power-profile"
SPRINT_ROUVY = "https://rouvy.com/en/blog/cycling-sprints-workouts"
SPRINT_ROADMAN = "https://roadmancycling.com/answers/how-to-improve-cycling-sprint-power"
SWEETSPOT_FASCAT = "https://fascatcoaching.com/blogs/training-tips/how-much-sweet-spot-training/"
SWEETSPOT_BC = "https://www.britishcycling.org.uk/zuvvi/media/pdfs/2024/insight_zone/indoor_training_plans/INDOOR_TRAINER_SESSIONS_-_Sweet-spot_intervals.pdf"
SWEETSPOT_POL = "https://fascatcoaching.com/blogs/training-tips/sweet-spot-polarized/"
OVER_UNDER = "https://join.cc/cycling-tips/over-unders-cycling"
ZONE2_EF = "https://www.efprocycling.com/tips-recipes/james-shaw-zone-2-workouts/"
BASE_BC = "https://www.britishcycling.org.uk/zuvvi/media/bc_files/sportivetrainingplans/outdoor_sessions/IZ_Rollers_Sessions_Base_Endurance_Ride.pdf"
BASE_JOIN = "https://join.cc/cycling-tips/cycling-base-training"
RECOVERY_BC = "https://img.britishcycling.org.uk/zuvvi/media/bc_files/sportivetrainingplans/outdoor_sessions/IZ_Rollers_Sessions_Recovery_Ride.pdf"
TEMPO = "https://personalbestpace.com/tempo-training-for-cyclists-the-zone-that-builds-real-world-riding-fitness/"
CADENCE_ROUVY = "https://rouvy.com/blog/cycling-cadence-optimal-rpm-guide"
RAMP_ROUVY = "https://rouvy.com/blog/ramp-test-guide"
RAMP_ZI = "https://zwiftinsider.com/new-ramp-test/"
CLIMB_STRAVA = "https://stories.strava.com/articles/workout-week-climbing-repeats-developing-power-bike"
CLIMB_BC = "https://www.britishcycling.org.uk/sportives/article/sp20130419--Sportive-Training-Plans-Week-5--Short-steep-climbing-0"
OPENERS = "https://source-e.net/rider-resources/finding-the-perfect-openers-what-science-says-about-pre-race-cycling-workouts/"

# ───────────────────────────── tags ─────────────────────────────
GOAL_TAGS = {
    "aerobic_base", "threshold", "vo2max", "anaerobic", "sprint", "durability", "climbing",
    "time_trial", "race_dynamics", "technique", "strength", "recovery", "testing", "opener",
}
OTHER_TAGS = {
    "beginner", "advanced", "key_session", "evidence_based", "time_crunched", "long_ride",
    "base", "build", "peak",
}
ALL_TAGS = GOAL_TAGS | OTHER_TAGS

# ───────────────────────────── step helpers ─────────────────────────────
WARM_NOTE = "Fokozatos ráhangolódás; a végén két-három rövid, laza gyorsítás 100 fölötti fordulattal."
COOL_NOTE = "Könnyű pörgetés, hagyd a pulzust lassan visszaállni."


def target(t):
    if t == "free":
        return {"type": "free"}
    if isinstance(t, tuple):
        return {"type": "ramp_pct", "from": t[0], "to": t[1]}
    if isinstance(t, dict):
        return t
    return {"type": "pct_ftp", "value": t}


def s(kind, name, sec, t, cad=None, note=None):
    return {
        "kind": kind,
        "name": name,
        "duration_s": int(sec),
        "target": target(t),
        "cadence": cad,
        "note": note,
    }


def wu(sec, a=45, b=75, cad=85, note=WARM_NOTE):
    return s("warmup", "Bemelegítés", sec, (a, b), cad, note)


def cd(sec, a=55, b=35, cad=85, note=COOL_NOTE):
    return s("cooldown", "Levezetés", sec, (a, b), cad, note)


def rest(sec, name="Pihenő", pct=50, cad=85, note=None):
    return s("recovery", name, sec, pct, cad, note)


def first(i, note):
    """Notes go on the first repetition only; the rest repeat the same instruction."""
    return note if i == 0 else None


def intervals(n, sec, pct, rest_s, rest_pct=50, label="Intervallum", kind="interval",
              cad=None, note=None, rest_note=None, rest_cad=85):
    out = []
    for i in range(n):
        out.append(s(kind, f"{label} {i + 1}", sec, pct, cad, first(i, note)))
        if i < n - 1:
            out.append(rest(rest_s, f"Pihenő {i + 1}", rest_pct, rest_cad, first(i, rest_note)))
    return out


def micro(n, on_s, on_pct, off_s, off_pct, label, set_no=None, cad=None, on_note=None,
          off_note=None, kind="interval"):
    """`n` on/off pairs; the last off is left out so a set rest can follow directly."""
    out = []
    prefix = f"{set_no}/" if set_no is not None else ""
    for i in range(n):
        out.append(s(kind, f"{label} {prefix}{i + 1}", on_s, on_pct, cad,
                     on_note if (i == 0 and set_no in (None, 1)) else None))
        if i < n - 1:
            out.append(s("recovery", "Laza", off_s, off_pct, 85,
                         off_note if (i == 0 and set_no in (None, 1)) else None))
    return out


# ───────────────────────────── library ─────────────────────────────
W = []


def wk(id, name, category, ftp_pct, goal, description, tags, sources, steps):
    W.append(dict(id=id, name=name, category=category, goal=goal, description=description,
                  ftp_pct=ftp_pct, tags=tags, sources=sources, steps=steps))


# ── Regeneráció ──
wk("a-recovery-spin-30", "Regeneráló tekerés 30 perc", "recovery", 45,
   "Aktív pihenőnap: átmozgatja a lábakat, fáradtság nélkül",
   "Fél óra nagyon könnyű tekerés az 1-es zónában (55% FTP alatt), kemény edzés vagy verseny utáni napra. "
   "Az aktív regeneráció serkenti a keringést, de nem ad új terhelést, ezért itt a kevesebb a több: ha "
   "kísértést érzel, hogy gyorsíts, inkább szállj le korábban. Beszélgetős tempó, magasabb kadencia, nulla erőlködés.",
   ["recovery", "beginner", "time_crunched", "base", "build", "peak"], [RECOVERY_BC, COGGAN],
   [wu(300, 35, 48, 85, "Indulj szinte terhelés nélkül."),
    s("steady", "Könnyű pörgetés", 1200, 45, 90, "Laza, kerek pedálozás; ha beszélni nem tudnál, túl erős."),
    cd(300, 45, 30)])

wk("a-recovery-spin-45", "Regeneráló tekerés pörgetésekkel 45 perc", "recovery", 47,
   "Regeneráció és laza pedáltechnika egyszerre",
   "Háromnegyed óra az 1-es zónában, benne négy egyperces, magas fordulatú (105–110/perc) pörgetéssel, ugyanazon "
   "a könnyű wattszámon. A pörgetések az idegrendszert ébren tartják és a kerek pedálozást csiszolják, a terhelés "
   "viszont végig regeneráló marad. Ideális két kulcsedzés közötti napra.",
   ["recovery", "technique", "beginner", "time_crunched", "base", "build", "peak"], [RECOVERY_BC, CADENCE_ROUVY],
   [wu(420, 35, 50)]
   + [x for i in range(4) for x in (
       s("steady", f"Könnyű {i + 1}", 300, 46, 88, first(i, "Laza tekerés, nyugodt légzés.")),
       s("steady", f"Pörgetés {i + 1}", 60, 48, 108,
         first(i, "Ugyanaz a könnyű watt, csak gyorsabb láb; a nyereg ne ugráljon alattad.")))]
   + [s("steady", "Könnyű 5", 480, 45, 88), cd(360, 45, 30)])

wk("a-recovery-easy-60", "Könnyű kitekerés 60 perc", "recovery", 52,
   "Hosszabb aktív pihenés verseny vagy nehéz blokk után",
   "Egy óra az 1-es zóna tetején és a 2-es alján (50–55% FTP), amikor a test már nem kér teljes pihenőt, de "
   "még nem áll készen minőségi munkára. A célja a vérkeringés és az anyagcsere enyhe serkentése; a végén "
   "frissebbnek kell érezned magad, mint az elején. Tapasztaltabbaknak a heti terhelés könnyítő napjára.",
   ["recovery", "aerobic_base", "beginner", "base", "build", "peak"], [RECOVERY_BC, COGGAN],
   [wu(420, 38, 52),
    s("steady", "Könnyű 1", 1200, 52, 90, "Egyenletes, laza; ne emeld a wattot akkor sem, ha könnyűnek tűnik."),
    s("steady", "Könnyű 2", 1200, 54, 92, "Kicsit gyorsabb láb, ugyanaz a könnyed érzés."),
    s("steady", "Könnyű 3", 600, 50, 88),
    cd(180, 45, 30)])

# ── Állóképesség ──
wk("a-endurance-beginner-45", "Kezdő állóképesség 45 perc", "endurance", 56,
   "Az első rendszeres aerob edzések, a folyamatos tekerés megszokása",
   "Rövid, egyenletes 2-es zónás edzés kezdőknek vagy hosszú kihagyás után. A 2-es zóna (56–75% FTP) a szív és "
   "az izmok aerob kapacitását fejleszti, miközben gyorsan regenerálódsz belőle, ezért ez az edzésfelépítés "
   "alapja. Ha ez könnyen megy, a következő lépés a 60 perces változat.",
   ["aerobic_base", "beginner", "time_crunched", "base"], [BASE_BC, COGGAN],
   [wu(480, 40, 56),
    s("steady", "Egyenletes 1", 900, 56, 88, "Nyugodt tempó, folyamatos orrlégzés még épp menjen."),
    s("steady", "Egyenletes 2", 600, 60, 90, "Egy hajszállal erősebb, de még mindig beszélgetős tempó."),
    s("steady", "Egyenletes 3", 360, 56, 88),
    cd(360, 50, 35)])

wk("a-endurance-beginner-60", "Kezdő állóképesség 60 perc", "endurance", 59,
   "Az egyórás folyamatos 2-es zónás tekerés felépítése",
   "Egy óra egyenletes aerob munka, amelyben a terhelés kétszer finoman megemelkedik, majd visszaáll. Kezdőknek "
   "ez a heti alapedzés: javítja a zsír- és szénhidrát-anyagcserét, a hajszálerek sűrűségét és a "
   "mitokondriumok számát. A tempó végig legyen beszélgetős; a cél az idő a nyeregben, nem a watt.",
   ["aerobic_base", "beginner", "base"], [BASE_BC, ZONE2_EF],
   [wu(600, 40, 58),
    s("steady", "Egyenletes 1", 900, 58, 90, "Laza, egyenletes tempó."),
    s("steady", "Emelt 1", 300, 64, 92, "Kicsit határozottabb, a légzés még nyugodt."),
    s("steady", "Egyenletes 2", 900, 58, 90),
    s("steady", "Emelt 2", 300, 64, 92),
    cd(600, 50, 35)])

wk("a-endurance-base-75", "Alapállóképesség 75 perc", "endurance", 64,
   "Folyamatos 2-es zónás munka, aerob hatékonyság",
   "A klasszikus alapozó edzés: hosszú, egyenletes blokkok a 2-es zóna közepén. A profik és a kutatások "
   "(polarizált edzésmodell) is azt mutatják, hogy a heti terhelés nagy része ilyen könnyű-közepes tempóban "
   "telik, mert ez építi a legtöbb aerob alapot a legkevesebb fáradással. Figyelj a pulzus-sodródásra: ha "
   "egyenletes watt mellett is felkúszik, igyál és ne emelj.",
   ["aerobic_base", "base", "build"], [ZONE2_EF, POLARIZED, BASE_JOIN],
   [wu(600, 45, 62),
    s("steady", "Zóna 2 – 1", 1500, 64, 90, "Egyenletes watt, laza felsőtest."),
    s("steady", "Zóna 2 – 2", 900, 68, 92, "A zóna felső része, még mindig kontrollált."),
    s("steady", "Zóna 2 – 3", 1200, 64, 88),
    cd(300, 50, 35)])

wk("a-endurance-base-90", "Alapállóképesség 90 perc", "endurance", 66,
   "Az aerob bázis építése hosszabb, stabil terheléssel",
   "Másfél óra 2-es zóna, a közepén egy negyedórás szakasszal a zóna tetején. A hosszabb idő miatt már a "
   "glikogénraktárak és a táplálkozás is szerepet kap: 30–45 percenként egyél és igyál, ahogy versenyen is "
   "tennéd. Az egyik legfontosabb alapozó edzés, heti egy-két alkalommal érdemes beiktatni.",
   ["aerobic_base", "base", "build"], [ZONE2_EF, BASE_BC],
   [wu(600, 45, 64),
    s("steady", "Zóna 2 – 1", 1800, 65, 90, "Nyugodt, egyenletes munka; egyél valamit a blokk közepén."),
    s("steady", "Zóna 2 felső", 900, 70, 92, "A zóna teteje, de még nem tempó."),
    s("steady", "Zóna 2 – 2", 1800, 65, 88),
    cd(300, 50, 35)])

wk("a-endurance-120", "Alapállóképesség 2 óra", "endurance", 66,
   "Hosszú aerob munka beltéren, kadenciaváltással a monotónia ellen",
   "Két óra a 2-es zónában, húszperces szakaszokban váltakozó kadenciával (85–95), hogy a hosszú beltéri edzés "
   "ne legyen egyhangú, és a pedálozás is fejlődjön. A hosszú, könnyű munka a mitokondriális alkalmazkodás "
   "és a zsírhasznosítás legfontosabb ingere. Óránként 60–80 g szénhidrát és rendszeres ivás kell hozzá.",
   ["aerobic_base", "durability", "long_ride", "base"], [ZONE2_EF, POLARIZED],
   [wu(600, 45, 64)]
   + [s("steady", f"Zóna 2 – {i + 1}", 1200, 64 + (i % 2) * 4, 85 + (i % 3) * 5,
        first(i, "Kadenciát váltunk szakaszonként; a watt végig a 2-es zónában marad.")) for i in range(5)]
   + [cd(600, 55, 35)])

wk("a-endurance-long-150", "Hosszú alapozó 150 perc", "endurance", 66,
   "Hosszú távú aerob terhelés és tartós állóképesség",
   "Két és fél óra egyenletes 2-es zóna, rövid emelt szakaszokkal. A hosszú alapedzés nem csak aerob kapacitást "
   "épít, hanem tartósságot is: azt, hogy mennyire esik vissza a teljesítményed órák után. A mai kutatások "
   "szerint ez az egyik legjobb előrejelzője a versenyeredménynek amatőröknél is. Táplálkozz rendszeresen.",
   ["aerobic_base", "durability", "long_ride", "base"], [DURABILITY_AMATEUR, ZONE2_EF, BASE_JOIN],
   [wu(600, 45, 65),
    s("steady", "Zóna 2 – 1", 2400, 65, 90, "Egyenletes; 30 percenként egyél és igyál."),
    s("steady", "Zóna 2 felső", 1800, 69, 92, "A zóna felső része."),
    s("steady", "Zóna 2 – 2", 2400, 65, 88),
    s("steady", "Zóna 2 felső 2", 1200, 70, 92, "A végén is tartsd a minőséget: kerek pedálozás, laza váll."),
    cd(600, 55, 35)])

wk("a-endurance-long-180", "Hosszú alapozó 180 perc", "endurance", 66,
   "A leghosszabb beltéri alapozó edzés, maximális aerob volumen",
   "Három óra a 2-es zónában: a hosszú hétvégi tekerés beltéri változata. A szakaszok finoman hullámoznak a "
   "zónán belül, a végén pedig a zóna tetején kell kitartani, amikor már fáradt vagy; ez a tartósság edzése. "
   "Csak megfelelő alapok mellett, bőséges táplálkozással és folyadékkal érdemes nekivágni.",
   ["aerobic_base", "durability", "long_ride", "base"], [DURABILITY, ZONE2_EF],
   [wu(600, 45, 65),
    s("steady", "Zóna 2 – 1", 2400, 65, 90, "Egyenletes; 30 percenként egyél és igyál."),
    s("steady", "Zóna 2 – 2", 2400, 67, 88),
    s("steady", "Zóna 2 – 3", 2400, 65, 92),
    s("steady", "Fáradt felső 2-es zóna", 2400, 70, 90,
      "Már fáradt vagy: ugyanaz a minőség, egyenletes watt, nyugodt felsőtest."),
    cd(600, 55, 35)])

wk("a-endurance-cadence-60", "Fordulatszám-gyakorlat 60 perc", "endurance", 60,
   "Pedáltechnika és széles fordulatszám-tartomány",
   "Könnyű 2-es zónás edzés, amelyben magas (110 körüli) és alacsony (60 körüli) kadenciás szakaszok "
   "váltakoznak azonos, könnyű wattszám mellett. A cél nem az erő, hanem a koordináció: kerek, rángatás nélküli "
   "pedálozás minden fordulatszámon. Kezdőknek és alapozó időszakban is hasznos technikai edzés.",
   ["technique", "aerobic_base", "beginner", "base"], [CADENCE_ROUVY, BASE_BC],
   [wu(600, 42, 60),
    s("steady", "Egyenletes 1", 300, 60, 90, "Alap kadencia, laza tekerés."),
    s("steady", "Magas kadencia 1", 240, 58, 110, "Gyors láb, a csípő maradjon nyugodt a nyeregben."),
    s("steady", "Egyenletes 2", 300, 62, 88),
    s("steady", "Magas kadencia 2", 240, 58, 112),
    s("steady", "Egyenletes 3", 300, 62, 88),
    s("steady", "Alacsony kadencia 1", 240, 64, 60, "Nyomd végig a teljes kört, ne csak lefelé taposs."),
    s("steady", "Egyenletes 4", 300, 62, 88),
    s("steady", "Alacsony kadencia 2", 240, 64, 60),
    s("steady", "Egyenletes 5", 300, 62, 90),
    cd(540, 50, 35)])

wk("a-endurance-sprints-90", "Alapozó tekerés sprintekkel 90 perc", "endurance", 66,
   "Aerob alap építése úgy, hogy a sprintképesség se kopjon el",
   "Másfél óra 2-es zóna, benne hat rövid, 10 másodperces sprinttel, mindegyik után bő öt perc könnyű "
   "tekeréssel. A rövid, maximális sprintek alig fárasztanak, viszont az alapozó hetekben is ébren tartják az "
   "idegrendszert és a gyors izomrostokat. A sprinteknél a trainer szabad módban van: annyi watt, amennyit "
   "teljes erőből kinyomsz.",
   ["aerobic_base", "sprint", "base"], [SPRINT_ROADMAN, ZONE2_EF],
   [wu(600, 45, 65),
    s("steady", "Zóna 2 – nyitó", 900, 65, 90, "Egyenletes 2-es zóna.")]
   + [x for i in range(6) for x in (
       s("sprint", f"Sprint {i + 1}", 10, "free", 110,
         first(i, "Tíz másodperc teljes erőből; állva indulj, majd ülj vissza.")),
       s("steady", f"Zóna 2 – {i + 1}", 350, 64, 88,
         first(i, "Az első percben pörgess nagyon lazán, aztán vissza a 2-es zónába.")))]
   + [s("steady", "Zóna 2 – záró", 1200, 66, 90), cd(540, 55, 35)])

wk("a-endurance-lt1-3x20", "Felső 2-es zóna 3 × 20 perc", "endurance", 74,
   "Az aerob küszöb (LT1) körüli munka, magasabb aerob teljesítmény",
   "Három húszperces blokk a 2-es zóna tetején (72–75% FTP), közvetlenül az aerob küszöb alatt. A norvég "
   "módszer is nagy mennyiségű, kontrollált munkát végez ezen a határon: elég erős ahhoz, hogy jelentős aerob "
   "ingert adjon, de még nem halmoz fel tejsavat, így másnap is lehet edzeni. A légzés legyen végig "
   "nyugodt; ha beszélni sem tudsz, túl erős.",
   ["aerobic_base", "durability", "base", "build"], [NORWEGIAN_TP, ZONE2_EF, COGGAN],
   [wu(600, 45, 68)]
   + intervals(3, 1200, 74, 300, 55, label="Felső 2-es zóna", kind="steady", cad=90,
               note="72–75% FTP, a határ alatt; a végén is beszélgetős tempó.",
               rest_note="Öt perc lazán; igyál.")
   + [cd(600, 55, 35)])

wk("a-endurance-tempo-finish-120", "Alapozó tempó-befejezéssel 2 óra", "endurance", 70,
   "Tartósság: tempó tartása fáradt lábakkal a hosszú edzés végén",
   "Másfél óra 2-es zóna után húsz perc tempó (85% FTP), majd levezetés. A versenyek a végén dőlnek el, amikor "
   "már órák vannak a lábadban; a friss és a fáradt teljesítmény közötti különbség (a tartósság) külön "
   "fejleszthető, és a kutatások szerint jól elválasztja a sikeres amatőröket a többiektől. A tempóblokkban a "
   "watt ne essen vissza.",
   ["durability", "aerobic_base", "long_ride", "key_session", "base", "build"], [DURABILITY, DURABILITY_AMATEUR],
   [wu(600, 45, 65),
    s("steady", "Zóna 2 – 1", 2400, 65, 90, "Egyenletes; egyél rendszeresen, a végén kell az energia."),
    s("steady", "Zóna 2 – 2", 2400, 67, 88),
    s("interval", "Fáradt tempó", 1200, 85, 90, "Húsz perc tempó a hosszú munka végén: egyenletes, nem több."),
    cd(600, 55, 35)])

# ── Tempo ──
wk("b-tempo-3x15", "Tempó 3 × 15 perc", "tempo", 80,
   "Izom-állóképesség és aerob erő a 3-as zónában",
   "Három negyedórás tempóblokk (78–82% FTP) öt perc pihenővel. A tempó a sík terepi tempózás, a hosszú "
   "emelkedők és a csoportos tekerés intenzitása: elég kemény ahhoz, hogy erősítse az izom-állóképességet, de "
   "bőven küszöb alatti. Jó belépő a strukturált intervallumokba és az alapozás második felébe.",
   ["aerobic_base", "beginner", "base"], [TEMPO, COGGAN],
   [wu(600, 45, 70)]
   + intervals(3, 900, 80, 300, 55, label="Tempó", cad=88,
               note="Határozott, de kontrollált; csak rövid mondatokban tudnál beszélni.")
   + [cd(600, 55, 35)])

wk("b-tempo-2x40", "Tempó 2 × 40 perc", "tempo", 79,
   "Hosszú, egyenletes tempó-kitartás",
   "Két negyvenperces tempóblokk: a hosszú sportive-ok és a gran fondók tempója beltéren. A hosszú, egyenletes "
   "3-as zónás munka az izom-állóképességet és a tartósságot fejleszti, és megtanít a táplálkozásra terhelés "
   "közben. Haladóknak alapozás végén vagy hosszú versenyre készülve.",
   ["aerobic_base", "durability", "long_ride", "base"], [TEMPO, DURABILITY_AMATEUR],
   [wu(900, 45, 70),
    s("interval", "Tempó 1", 2400, 78, 88, "Egyenletes; 15 percenként igyál."),
    rest(600, "Pihenő", 60, 85),
    s("interval", "Tempó 2", 2400, 80, 86, "Ugyanaz a minőség fáradtabban; ez a lényeg."),
    cd(900, 55, 35)])

wk("b-tempo-piramis", "Tempó piramis", "tempo", 80,
   "Fokozatos tempóterhelés váltakozó blokkhosszal",
   "Öt, egyre hosszabb, majd rövidülő tempóblokk (5-10-15-10-5 perc), a zóna aljáról a tetejéig emelkedő "
   "intenzitással. A változó hosszúság tempóérzéket nevel, és a monotónia nélkül halmoz fel közel 45 perc "
   "3-as zónás munkát. Alapozó időszak második felére.",
   ["aerobic_base", "base"], [TEMPO, COGGAN],
   [wu(600, 45, 70),
    s("interval", "Tempó 5′", 300, 76, 90, "A zóna alja."),
    rest(180, "Pihenő 1", 58),
    s("interval", "Tempó 10′", 600, 78, 88),
    rest(180, "Pihenő 2", 58),
    s("interval", "Tempó 15′", 900, 80, 88, "A piramis csúcsa: egyenletes, fegyelmezett."),
    rest(240, "Pihenő 3", 58),
    s("interval", "Tempó 10′ (2)", 600, 82, 86),
    rest(180, "Pihenő 4", 58),
    s("interval", "Tempó 5′ (2)", 300, 84, 86, "Az utolsó, legerősebb tempó."),
    cd(720, 55, 35)])

wk("b-tempo-60", "Tempó 1 × 60 perc", "tempo", 80,
   "Egy óra folyamatos tempó: aerob erő és mentális kitartás",
   "Egyetlen órás tempóblokk 78–82% FTP-n, időfutam-pozícióban is végezhető. A hosszú, megszakítás nélküli munka "
   "a koncentrációt, a pozíció megtartását és az egyenletes adagolást is edzi, nem csak az aerob rendszert. "
   "Haladó alapozó edzés, hosszú versenyek és időfutamok előtt.",
   ["aerobic_base", "durability", "time_trial", "advanced", "base"], [TEMPO, COGGAN],
   [wu(900, 45, 72),
    s("interval", "Tempó 60′", 3600, 80, 88,
      "Egyenletes watt, nyugodt felsőtest; 15 percenként igyál, de ne állj ki a pozícióból."),
    cd(600, 55, 35)])

wk("b-tempo-bursts-3x15", "Tempó 3 × 15 perc gyorsításokkal", "tempo", 84,
   "Tempó tartása ismétlődő ritmusváltások mellett",
   "Három tempóblokk, amelyekben hárompercenként egy 15 másodperces, 140%-os gyorsítás jön, majd azonnal vissza "
   "a tempóba. A mezőnyben ritkán lehet egyenletesen tekerni: a gyorsítások után a tempó újbóli felvétele és a "
   "tejsav menet közbeni lebontása edzhető. Az építő időszak első heteibe való, a küszöbmunka előszobája.",
   ["race_dynamics", "aerobic_base", "build"], [TEMPO, OVER_UNDER],
   [wu(720, 45, 72)]
   + [x for b in range(3) for x in (
       [y for i in range(5) for y in (
           s("interval", f"Tempó {b + 1}/{i + 1}", 165, 80, 88,
             "Tempó, a gyorsítások között vissza ugyanarra a wattra." if (b, i) == (0, 0) else None),
           s("surge", f"Gyorsítás {b + 1}/{i + 1}", 15, 140, 105,
             "Tizenöt másodperc élesen, ülve vagy állva." if (b, i) == (0, 0) else None))]
       + ([rest(300, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(600, 55, 35)])

# ── Sweet spot ──
wk("b-sweetspot-4x8-intro", "Sweet spot belépő 4 × 8 perc", "sweetspot", 87,
   "Első lépés a küszöb alatti munkába, rövid blokkokkal",
   "Négy nyolcperces blokk 85–88% FTP-n, négy perc pihenővel. A sweet spot (88–94% FTP) a tempó és a küszöb "
   "között van: sok edzéshatás viszonylag kevés fáradással. Ez a belépő változat rövid blokkokkal mutatja meg, "
   "milyen érzés a zóna, mielőtt a hosszabb 12–20 perces ismétlésekre váltanál.",
   ["threshold", "beginner", "base"], [SWEETSPOT_BC, SWEETSPOT_FASCAT],
   [wu(600, 45, 72)]
   + intervals(4, 480, 87, 240, 55, label="Sweet spot", cad=90,
               note="Erős, de fenntartható; a végén még éreznéd, hogy lenne benned egy ötödik.")
   + [cd(600, 55, 35)])

wk("b-sweetspot-2x12-quick", "Sweet spot gyors 2 × 12 perc", "sweetspot", 89,
   "Hatékony küszöbfejlesztés, ha csak 45 perced van",
   "Két tizenkét perces sweet spot blokk egy rövid bemelegítés után. A sweet spot attól népszerű, hogy egységnyi "
   "időre vetítve nagy edzésingert ad, ezért szűkös időkeretben is értelmes, minőségi munka. Hétköznapi "
   "edzésnek ideális az alapozás és az építés idején.",
   ["threshold", "time_crunched", "base", "build"], [SWEETSPOT_FASCAT, SWEETSPOT_BC],
   [wu(540, 45, 75)]
   + intervals(2, 720, 89, 240, 55, label="Sweet spot", cad=90,
               note="88–90% FTP, egyenletesen; a második blokk is ugyanilyen legyen.")
   + [cd(420, 55, 35)])

wk("b-sweetspot-3x12", "Sweet spot 3 × 12 perc", "sweetspot", 88,
   "FTP-emelés mérsékelt fáradással",
   "Három tizenkét perces blokk a sweet spot alján, négy perc pihenővel: a legtöbb edzésterv első sweet spot "
   "edzése. Összesen 36 perc munka a küszöb alatt, amiből a következő napon is jól lehet regenerálódni. Ha "
   "ez már könnyű, lépj tovább a 2 × 20 vagy a 3 × 20 perces változatra.",
   ["threshold", "beginner", "base"], [SWEETSPOT_BC, SWEETSPOT_FASCAT],
   [wu(600, 45, 72)]
   + intervals(3, 720, 88, 240, 55, label="Sweet spot", cad=90,
               note="Erős, egyenletes tempó; a légzés mély, de ritmusos.")
   + [cd(360, 55, 35)])

wk("b-sweetspot-2x20", "Sweet spot 2 × 20 perc", "sweetspot", 91,
   "Hosszú, összefüggő sweet spot munka",
   "Két húszperces blokk a sweet spot közepén (90–92% FTP). A húszperces ismétlés a küszöbfejlesztés egyik "
   "alapformája: elég hosszú ahhoz, hogy az izmok tartós terhelés alatt dolgozzanak, de még nem olyan "
   "megterhelő, mint a valódi küszöbmunka. Alapozás végén és építő időszakban heti egyszer-kétszer.",
   ["threshold", "base", "build"], [SWEETSPOT_FASCAT, SWEETSPOT_BC],
   [wu(600, 45, 72),
    s("interval", "Sweet spot 1", 1200, 91, 90, "Egyenletes; az első öt percben ne told túl."),
    rest(300, "Pihenő", 55),
    s("interval", "Sweet spot 2", 1200, 92, 88, "Ugyanaz a fegyelem; a második blokk adja az edzés értékét."),
    cd(600, 55, 35)])

wk("b-sweetspot-3x20", "Sweet spot 3 × 20 perc", "sweetspot", 90,
   "Nagy volumenű sweet spot, versenykitartás",
   "Egy óra sweet spot három blokkban: az egyik leghatékonyabb módja a küszöbteljesítmény és a tartós erő "
   "növelésének, ha már a 2 × 20 könnyen megy. A harmadik blokk már fáradtan kezdődik, így a tartósságot is "
   "fejleszti. Igyál a pihenőkben, és a blokkok alatt is vegyél magadhoz szénhidrátot.",
   ["threshold", "durability", "advanced", "base", "build"], [SWEETSPOT_FASCAT, SWEETSPOT_POL],
   [wu(900, 45, 72)]
   + intervals(3, 1200, 90, 300, 55, label="Sweet spot", cad=88,
               note="90% FTP, egyenletesen; az utolsó blokk is ugyanezen a wattszámon menjen.")
   + [cd(900, 55, 35)])

wk("b-sweetspot-1x45", "Sweet spot 1 × 45 perc", "sweetspot", 88,
   "Folyamatos erő-kitartás sweet spotban",
   "Háromnegyed óra megszakítás nélkül a sweet spot alján: a hosszú időfutamok és emelkedők mentális és "
   "fizikai főpróbája. A folyamatos munka a pozíció megtartását és az adagolást is gyakoroltatja. Haladóknak, "
   "miután a 3 × 15 és a 2 × 20 perc magabiztosan megy.",
   ["threshold", "durability", "time_trial", "advanced", "base", "build"], [SWEETSPOT_FASCAT, SWEETSPOT_BC],
   [wu(900, 45, 72),
    s("interval", "Sweet spot 45′", 2700, 88, 88,
      "Egyenletes watt, nyugodt felsőtest; az első tíz perc érződjön könnyűnek."),
    cd(900, 55, 35)])

wk("b-sweetspot-progressziv", "Progresszív sweet spot 25 + 24 perc", "sweetspot", 89,
   "Fokozatos terhelésnövelés a zónán belül, tempóérzék",
   "Két hosszú blokk, amelyekben a watt lassan emelkedik: az első 84-ről 89%-ra, a második 88-ról 94%-ra. A "
   "fokozatos emelés miatt az elején könnyű tartani a fegyelmet, a végén viszont a zóna tetején dolgozol, már "
   "fáradtan. Jó választás, ha hajlamos vagy túl erősen kezdeni az intervallumokat.",
   ["threshold", "durability", "base", "build"], [SWEETSPOT_FASCAT, SWEETSPOT_BC],
   [wu(900, 45, 70),
    s("interval", "Progresszív blokk 1", 1500, (84, 89), 88, "A trainer lassan emel; te csak kövesd."),
    rest(240, "Pihenő", 60),
    s("interval", "Progresszív blokk 2", 1440, (88, 94), 86, "A blokk vége a zóna teteje; tartsd a kadenciát."),
    cd(720, 55, 35)])

wk("b-sweetspot-4x10-kadencia", "Sweet spot 4 × 10 perc kadenciaváltással", "sweetspot", 89,
   "Küszöb alatti munka és kadencia-rugalmasság",
   "Négy tízperces sweet spot blokk, mindegyik más kadenciával (90, 72, 100, 85). Ugyanazt a wattszámot "
   "különböző fordulattal tartani más-más izommunkát és koordinációt kíván, ahogy a változó terepen is. A "
   "küszöbfejlesztés mellé technikai ingert ad, egyhangúság nélkül.",
   ["threshold", "technique", "base", "build"], [SWEETSPOT_BC, CADENCE_ROUVY],
   [wu(720, 45, 72),
    s("interval", "Sweet spot 1 (90)", 600, 90, 90, "Természetes kadencia."),
    rest(180, "Pihenő 1", 55),
    s("interval", "Sweet spot 2 (72)", 600, 90, 72, "Alacsonyabb fordulat, több nyomaték; a felsőtest maradjon laza."),
    rest(180, "Pihenő 2", 55),
    s("interval", "Sweet spot 3 (100)", 600, 89, 100, "Gyors láb, a csípő ne billegjen."),
    rest(180, "Pihenő 3", 55),
    s("interval", "Sweet spot 4 (85)", 600, 89, 85),
    cd(540, 55, 35)])

wk("b-sweetspot-over-under", "Sweet spot over-under", "sweetspot", 91,
   "Tejsav-feldolgozás és az intenzitásváltás kezelése a küszöb alatt",
   "Két tizennyolc perces blokk, amelyben három perc 88% és egy perc 95–97% váltakozik. A rövid „over” "
   "szakaszok kicsit tejsavat termelnek, az „under” szakaszokban pedig menet közben kell feldolgozni; ez a "
   "képesség kell, amikor a mezőny tempója hullámzik. Szelídebb előszobája a küszöb körüli over-undereknek.",
   ["threshold", "race_dynamics", "build"], [OVER_UNDER, SWEETSPOT_FASCAT],
   [wu(720, 45, 72)]
   + [x for b in range(2) for x in (
       [y for i in range(6) for y in (
           s("interval", f"Under {b + 1}/{i + 1}", 180, 88 + 2 * b, 88,
             "Under: a sweet spot alja, itt dolgozod fel a tejsavat." if (b, i) == (0, 0) else None),
           s("interval", f"Over {b + 1}/{i + 1}", 60, 95 + 2 * b, 95,
             "Over: épp az FTP alatt; érezhetően erősebb, de nem kapkodós." if (b, i) == (0, 0) else None))]
       + ([rest(300, "Pihenő", 55)] if b == 0 else []))]
   + [cd(900, 55, 35)])

wk("b-sweetspot-sorozatok", "Sweet spot rövid rákapcsolásokkal", "sweetspot", 90,
   "Sweet spot terhelés éles, rövid gyorsításokkal",
   "Három sorozat, amelyekben három perc sweet spot után 20 másodperces, 115–118%-os gyorsítás jön, majd "
   "azonnal vissza a sweet spotba. A gyorsítások a mezőnyben előforduló kilövéseket utánozzák; a lényeg, hogy "
   "utánuk a watt ne essen le. Az építő időszakban a sima sweet spot versenyszerűbb változata.",
   ["threshold", "race_dynamics", "build"], [SWEETSPOT_FASCAT, OVER_UNDER],
   [wu(720, 45, 72)]
   + [x for b in range(3) for x in (
       [y for i in range(4) for y in (
           s("interval", f"Sweet spot {b + 1}/{i + 1}", 180, 88 + (b == 2) * 2, 90,
             "Sweet spot, egyenletesen." if (b, i) == (0, 0) else None),
           s("surge", f"Rákapcsolás {b + 1}/{i + 1}", 20, 115 + b, 100,
             "Húsz másodperc élesen, majd azonnal vissza a sweet spotba." if (b, i) == (0, 0) else None))]
       + ([rest(240, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(900, 55, 35)])

# ── Küszöb ──
wk("c-threshold-2x20", "Küszöb 2 × 20 perc", "threshold", 97,
   "FTP közeli terhelés, a laktátküszöb fejlesztése",
   "A klasszikus küszöbedzés: két húszperces blokk 95–98% FTP-n, öt perc pihenővel. A cél, hogy a "
   "laktátküszöböt hosszú, egyenletes terhelés alatt told feljebb, és megszokd a kellemetlenséget, amit egy "
   "időfutam vagy hosszú emelkedő jelent. Ha a watt tartósan 5%-nál többet esik, fejezd be a blokkot.",
   ["threshold", "time_trial", "key_session", "build"], [FTP_ALLEN, COGGAN],
   [wu(900, 45, 75),
    s("interval", "Küszöb 1", 1200, 97, 90, "Egyenletes tempó, kontrollált légzés."),
    rest(300, "Pihenő", 50),
    s("interval", "Küszöb 2", 1200, 97, 88, "Ugyanaz a watt; a második blokk a fontosabb."),
    cd(600, 55, 35)])

wk("c-threshold-2x30", "Küszöb 2 × 30 perc", "threshold", 93,
   "Hosszú, egyenletes küszöbterhelés az FTP fenntarthatóságáért",
   "Két félórás blokk a küszöb alatt (93% FTP): a küszöbidő meghosszabbítása, vagyis hogy minél tovább bírd "
   "az FTP közeli tempót. Időfutamra és hosszú emelkedőkre készülőknek az egyik legspecifikusabb edzés. Csak "
   "akkor, ha a 2 × 20 perc 95%-on már magabiztosan megy.",
   ["threshold", "time_trial", "durability", "advanced", "key_session", "build"], [FTP_ALLEN, SWEETSPOT_FASCAT],
   [wu(900, 45, 78),
    s("interval", "Küszöb 1", 1800, 93, 88, "Az első tíz perc érződjön könnyebbnek, mint kellene."),
    rest(300, "Pihenő", 55),
    s("interval", "Küszöb 2", 1800, 93, 86, "Pozíció, légzés, egyenletes watt."),
    cd(600, 55, 35)])

wk("c-threshold-3x15", "Küszöb 3 × 15 perc", "threshold", 98,
   "45 perc küszöbmunka, az FTP fenntarthatóságának növelése",
   "Három negyedórás blokk 97–100% FTP-n: összesen 45 perc a küszöbön, ami már a 2 × 20 percnél is több "
   "munka. Az utolsó blokk enyhén erősebb, hogy a végére is maradjon kihívás. Építő időszakban, időfutamra "
   "vagy hosszú emelkedőkre készülve.",
   ["threshold", "time_trial", "key_session", "build"], [FTP_ALLEN, COGGAN],
   [wu(900, 45, 78)]
   + intervals(2, 900, 97, 300, 55, label="Küszöb", cad=90, note="97% FTP, egyenletesen.")
   + [rest(300, "Pihenő 2", 55),
      s("interval", "Küszöb 3", 900, 100, 88, "Az utolsó blokk az FTP-n."),
      cd(600, 55, 35)])

wk("c-threshold-4x10", "Küszöb 4 × 10 perc", "threshold", 100,
   "Rövidebb, ismételt küszöbblokkok a magas watt tartásához",
   "Négy tízperces blokk az FTP-n, négy perc pihenővel. A rövidebb blokkok miatt magasabb intenzitáson "
   "gyűlik össze 40 perc küszöbmunka, mint egy 2 × 20-ban. Jó átmenet a sweet spot és a hosszú küszöbblokkok "
   "között, és kiváló versenyidőszaki fenntartó edzés is.",
   ["threshold", "key_session", "build", "peak"], [FTP_ALLEN, COGGAN],
   [wu(900, 45, 80)]
   + intervals(3, 600, 100, 240, 55, label="Küszöb", cad=90, note="Az FTP-n; az első percben ne lőj túl.")
   + [rest(240, "Pihenő 3", 55),
      s("interval", "Küszöb 4", 600, 102, 88, "Az utolsó blokk egy hajszállal az FTP fölött."),
      cd(480, 55, 35)])

wk("c-threshold-ladder-3x12", "Küszöb lépcső 3 × 12 perc", "threshold", 98,
   "Fokozatosan emelkedő küszöbterhelés a blokkokon belül",
   "Három tizenkét perces blokk, amelyekben a watt 92%-ról 100–104%-ra emelkedik. Az elején a sweet spotban "
   "kezdesz, a végén az FTP fölött fejezel be, így fáradtan kell a legnagyobb terhelést tartani, mint egy "
   "emelkedő tetején. A blokkok egyre magasabbról indulnak.",
   ["threshold", "climbing", "key_session", "build"], [FTP_ALLEN, SWEETSPOT_FASCAT],
   [wu(900, 45, 78),
    s("interval", "Lépcső 1", 720, (92, 100), 90, "A trainer lassan emel; az utolsó percek a lényeg."),
    rest(240, "Pihenő 1", 55),
    s("interval", "Lépcső 2", 720, (94, 102), 88),
    rest(240, "Pihenő 2", 55),
    s("interval", "Lépcső 3", 720, (96, 104), 86, "Az utolsó lépcső az FTP fölé visz."),
    cd(360, 55, 35)])

wk("c-threshold-overunder-3x5", "Over-under 3 × 5 kör (112% / 90%)", "threshold", 97,
   "A tejsav menet közbeni lebontásának fejlesztése a küszöb körül",
   "Három sorozat, mindegyikben öt kör: egy perc 112% (over), majd két perc 90% (under). Az over szakasz "
   "tejsavat halmoz fel, az under szakaszban pedig a küszöb alatt, de még terhelés alatt kell feldolgozni. Ez a "
   "képesség kell támadás után, vagy amikor egy emelkedő meredekebb részei után sem lehet pihenni.",
   ["threshold", "race_dynamics", "key_session", "build", "peak"], [OVER_UNDER, COGGAN],
   [wu(900, 45, 80)]
   + [x for b in range(3) for x in (
       [y for i in range(5) for y in (
           s("interval", f"Over {b + 1}/{i + 1}", 60, 112, 95,
             "Over: egy perc az FTP fölött, lendületesen." if (b, i) == (0, 0) else None),
           s("interval", f"Under {b + 1}/{i + 1}", 120, 90, 88,
             "Under: ne pihenj, a küszöb alatt dolgozd fel a tejsavat." if (b, i) == (0, 0) else None))]
       + ([rest(240, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(420, 55, 35)])

wk("c-threshold-overunder-2x20", "Hosszú over-under 2 × 20 perc", "threshold", 98,
   "Húszperces over-under blokkok, versenyszerű küszöbterhelés",
   "Két húszperces blokk, amelyekben 90 másodperc 110% és egy perc 92% váltakozik. Átlagban az FTP közelében "
   "vagy, de a folyamatos ritmusváltás sokkal versenyszerűbb, mint egy egyenletes 2 × 20. Haladóknak, az "
   "építő időszak második felében.",
   ["threshold", "race_dynamics", "advanced", "key_session", "build", "peak"], [OVER_UNDER, COGGAN],
   [wu(900, 45, 80)]
   + [x for b in range(2) for x in (
       [y for i in range(8) for y in (
           s("interval", f"Over {b + 1}/{i + 1}", 90, 110, 92,
             "Over: 110%, lendületesen, de nem sprintként." if (b, i) == (0, 0) else None),
           s("interval", f"Under {b + 1}/{i + 1}", 60, 92, 88,
             "Under: 92%, itt sem pihensz." if (b, i) == (0, 0) else None))]
       + ([rest(300, "Pihenő", 55)] if b == 0 else []))]
   + [cd(600, 55, 35)])

wk("c-threshold-surges-3x12", "Küszöb 3 × 12 perc gyorsításokkal", "threshold", 100,
   "Küszöbmunka húszmásodperces gyorsításokkal, versenyszerű ritmusváltás",
   "Három tizenkét perces küszöbblokk, amelyben 160 másodpercenként egy 20 másodperces, 120%-os gyorsítás "
   "jön. A gyorsítások után vissza kell állni a küszöbre anélkül, hogy a watt leesne; ez a szökésben vagy "
   "egy emelkedőn végrehajtott támadás után kell. Kemény kulcsedzés, versenyidőszak előtt.",
   ["threshold", "race_dynamics", "advanced", "key_session", "build", "peak"], [OVER_UNDER, COGGAN],
   [wu(900, 45, 80)]
   + [x for b in range(3) for x in (
       [y for i in range(4) for y in (
           s("interval", f"Küszöb {b + 1}/{i + 1}", 160, 100 - 2 * (b == 2), 90,
             "Az FTP-n, egyenletesen." if (b, i) == (0, 0) else None),
           s("surge", f"Gyorsítás {b + 1}/{i + 1}", 20, 120, 105,
             "Húsz másodperc élesen, majd vissza a küszöbre." if (b, i) == (0, 0) else None))]
       + ([rest(240, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(360, 55, 35)])

wk("c-threshold-fatigued-blocks", "Fáradt küszöb, előfárasztva", "threshold", 96,
   "Küszöbteljesítmény fenntartása előfárasztás után",
   "Tíz perc az FTP fölött, majd három tizenöt perces blokk csökkenő, de még mindig magas intenzitással "
   "(96–94%). A szervezet fáradt állapotban tanul meg wattot tartani, ami a verseny hajrájában döntő; a "
   "tartósság kutatások szerint külön edzhető képesség. Haladóknak, versenyre hangolódva.",
   ["threshold", "durability", "advanced", "key_session", "build", "peak"], [DURABILITY, FTP_ALLEN],
   [wu(900, 45, 78),
    s("interval", "Előfárasztás", 600, 104, 92, "Tíz perc a küszöb felett; itt még friss vagy."),
    rest(180, "Pihenő 1", 65),
    s("interval", "Fáradt küszöb 1", 900, 96, 88, "Tartsd a wattot, még ha nehéznek is tűnik."),
    rest(180, "Pihenő 2", 60),
    s("interval", "Fáradt küszöb 2", 900, 96, 86),
    rest(180, "Pihenő 3", 60),
    s("interval", "Fáradt küszöb 3", 900, 94, 85, "Az utolsó blokk a fejben dől el."),
    cd(360, 55, 35)])

wk("c-threshold-seiler-4x16", "Seiler 4 × 16 perc", "threshold", 95,
   "Hosszú küszöbintervallumok a Seiler-kísérlet protokollja szerint",
   "Négy tizenhat perces blokk két perc pihenővel, a legnagyobb még egyenletesen tartható intenzitáson "
   "(a kísérletben átlagosan ~95% FTP). Seiler és mtsai. (2013) ezt a 4 × 8 és a 4 × 4 perces változattal "
   "hasonlították össze: mindhárom javított, a hosszú blokkok főleg a küszöbteljesítményt. Nagy mennyiségű, "
   "kontrollált küszöbmunka, ha a 3 × 15 perc már rutin.",
   ["threshold", "time_trial", "evidence_based", "advanced", "key_session", "build"], [SEILER, COGGAN],
   [wu(900, 45, 78)]
   + intervals(4, 960, 95, 120, 50, label="Küszöb", cad=90,
               note="A legnagyobb, még egyenletesen tartható tempó; ne az első blokkban dőljön el.",
               rest_note="Két perc pihenő: rövid, de elég egy korty italra.")
   + [cd(600, 55, 35)])

wk("c-threshold-norwegian-5x8", "Norvég küszöb 5 × 8 perc", "threshold", 93,
   "Kontrollált küszöbmunka, sok minőségi idő kis fáradással",
   "Öt nyolcperces blokk 92–94% FTP-n, mindössze egy perc pihenővel. A norvég módszer lényege, hogy a "
   "küszöbmunkát szándékosan a küszöb alatt, rövid pihenőkkel szakaszolva végzik, így nagy mennyiséget lehet "
   "felhalmozni anélkül, hogy a regeneráció napokig tartana. Az edzés végén érezned kell, hogy maradt benned.",
   ["threshold", "aerobic_base", "key_session", "base", "build"], [NORWEGIAN, NORWEGIAN_TP],
   [wu(900, 45, 75)]
   + intervals(5, 480, 93, 60, 55, label="Küszöb", cad=90,
               note="Épp a küszöb alatt; ha egyre nehezebb, az túl erős, nem jól adagolt.",
               rest_note="Egy perc lazán, nem több.")
   + [cd(600, 55, 35)])

wk("c-threshold-norwegian-10x3", "Norvég küszöb 10 × 3 perc", "threshold", 97,
   "Rövidebb, kicsit erősebb küszöbszakaszok rövid pihenővel",
   "Tíz háromperces szakasz 96–98% FTP-n, egy perc pihenővel. A norvég dupla küszöbnap délutáni, rövidebb és "
   "kicsit gyorsabb adagjának mintájára: a rövid szakaszok miatt a küszöbön tölthető idő nő, a tejsav mégsem "
   "szalad el. 30 perc munka az FTP közelében, egy hagyományos 2 × 15-nél kisebb terheléssel.",
   ["threshold", "key_session", "build"], [NORWEGIAN, NORWEGIAN_TP],
   [wu(900, 45, 78)]
   + intervals(10, 180, 97, 60, 55, label="Küszöb", cad=92,
               note="Az FTP alatt egy hajszállal; a tizedik se legyen küzdelem.",
               rest_note="Egy perc laza pörgetés.")
   + [cd(600, 55, 35)])

wk("c-threshold-quick-3x8", "Küszöb gyors 3 × 8 perc", "threshold", 100,
   "Minőségi küszöbinger 45 perc alatt",
   "Három nyolcperces blokk az FTP-n, három perc pihenővel, egy rövid, de alapos bemelegítés után. Ha kevés az "
   "idő, a küszöbmunka akkor is hatásos: 24 perc az FTP-n fenntartja és fejleszti a küszöböt. Hétköznap esti "
   "kulcsedzésnek.",
   ["threshold", "time_crunched", "key_session", "build", "peak"], [FTP_ALLEN, COGGAN],
   [wu(600, 45, 80)]
   + intervals(3, 480, 100, 180, 50, label="Küszöb", cad=90, note="Az FTP-n, egyenletesen.")
   + [cd(300, 55, 35)])

wk("c-threshold-1x40", "Küszöbidő-hosszabbítás 1 × 40 perc", "threshold", 94,
   "Az FTP-közeli tempó minél hosszabb fenntartása",
   "Egyetlen negyvenperces blokk 94% FTP-n: az időfutamok és a hosszú emelkedők legspecifikusabb edzése. A "
   "„time to exhaustion” növelése, vagyis hogy ne csak magasabb, hanem tovább is tartható legyen a "
   "küszöbteljesítmény, külön edzhető. Csak haladóknak, akiknek a 2 × 20 perc már magabiztos.",
   ["threshold", "time_trial", "durability", "advanced", "key_session", "build", "peak"], [FTP_ALLEN, SEILER],
   [wu(900, 45, 80),
    s("interval", "Küszöb 40′", 2400, 94, 88,
      "Az első húsz perc legyen visszafogott; a második felében derül ki, mennyit ér a tempóérzéked."),
    cd(600, 55, 35)])

# ── VO2max ──
wk("d-vo2-4x4", "VO2max 4 × 4 perc (Helgerud)", "vo2max", 113,
   "A VO2max és a szív pumpakapacitásának növelése",
   "Négy négyperces intervallum 90–95% maximális pulzuson (kb. 110–115% FTP), három perc aktív pihenővel. "
   "Helgerud és mtsai. (2007) vizsgálatában ez a formátum növelte legjobban a VO2max-ot, a verőtérfogat kb. "
   "10%-os emelkedésével. Egyszerű, jól ismételhető, nagyon hatékony aerob kulcsedzés.",
   ["vo2max", "evidence_based", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(900, 45, 75)]
   + intervals(4, 240, 113, 180, 50, label="VO2max", cad=100,
               note="Az első percben kússz fel, utána tartsd; a végére a légzés legyen maximális.",
               rest_note="Három perc aktív pihenő, könnyű tekerés.")
   + [cd(780, 55, 35)])

wk("d-vo2-5x5", "VO2max 5 × 5 perc", "vo2max", 110,
   "Maximális oxigénfelvétel, hosszabb VO2max ismétlésekkel",
   "Öt ötperces intervallum 108–112% FTP-n, öt perc pihenővel. Az ötperces ismétlés elég hosszú ahhoz, hogy "
   "minden blokkban több perc teljen a VO2max közelében. Akkor jó választás, ha a 4 × 4 már megy, és több "
   "munkát akarsz a magas aerob tartományban.",
   ["vo2max", "advanced", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(900, 45, 75)]
   + intervals(5, 300, 110, 300, 50, label="VO2max", cad=100,
               note="Kemény, de egyenletes; a negyedik percben legyen igazán nehéz.")
   + [cd(600, 55, 35)])

wk("d-vo2-6x3", "VO2max 6 × 3 perc", "vo2max", 118,
   "VO2max inger rövidebb, ismételhető szakaszokkal",
   "Hat háromperces ismétlés 118% FTP-n, négy perc pihenővel. A rövidebb ismétlés magasabb intenzitást enged, "
   "így gyorsabban éred el a VO2max-ot, és a pihenő is elég a minőség megtartásához. Jó középút a 4 × 4 és "
   "a mikro-intervallumok között.",
   ["vo2max", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(1020, 45, 75)]
   + intervals(6, 180, 118, 240, 55, label="VO2max", cad=102,
               note="Erős indulás, aztán tartsd; az utolsó perc a lényeg.")
   + [cd(660, 55, 35)])

wk("d-vo2-8x2", "VO2max 8 × 2 perc", "vo2max", 120,
   "VO2max és a gyors légzés-felfutás gyakorlása",
   "Nyolc kétperces ismétlés 120% FTP-n, három perc pihenővel. A rövid ismétlések miatt kezdők és "
   "versenyidőszakban lévők számára is jól kezelhető, mégis összesen 16 percnyi munka gyűlik össze a magas "
   "aerob tartományban. Rövid emelkedőkre és szökésekre is felkészít.",
   ["vo2max", "climbing", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(900, 45, 73)]
   + intervals(8, 120, 120, 180, 55, label="VO2max", cad=104,
               note="Két perc keményen; a légzés gyorsan felfut, ez a cél.")
   + [cd(900, 55, 35)])

wk("d-vo2-10x1", "VO2max 10 × 1 perc", "vo2max", 125,
   "Rövid, ismételhető magas intenzitás tolerálása",
   "Tíz egyperces ismétlés 125% FTP-n, két perc pihenővel. Az egyperces szakaszok a VO2max és az anaerob "
   "rendszer határán dolgoznak, ezért kritériumokra, rövid emelkedőkre és támadásokra is jól felkészítenek. "
   "A második felében már minden ismétlés a VO2max közelébe visz.",
   ["vo2max", "race_dynamics", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(900, 45, 70)]
   + intervals(10, 60, 125, 120, 55, label="VO2max", cad=105,
               note="Egy perc keményen, ülve; egyenletes, nem sprint.")
   + [cd(900, 55, 35)])

wk("d-vo2-30-30", "Billat 30/30 – 3 × 10", "vo2max", 125,
   "Hosszú idő a VO2max közelében mikro-intervallumokkal",
   "Harminc másodperc kemény munka (kb. a maximális aerob teljesítmény, ~120–125% FTP), harminc másodperc "
   "könnyű pihenő, három tízes sorozatban. Billat vizsgálatai szerint a 30/30 váltakozással több idő tölthető "
   "a VO2max közelében és kevesebb tejsav keletkezik, mint folyamatos, nagyon kemény terheléssel. A sorozat "
   "végére kell igazán nehéznek lennie.",
   ["vo2max", "evidence_based", "key_session", "build", "peak"], [BILLAT, COGGAN],
   [wu(900, 45, 72)]
   + [x for b in range(3) for x in (
       micro(10, 30, 125, 30, 50, "30/30", b + 1, 100,
             "Harminc másodperc élesen, de nem maximálisan; ugyanazt a tempót tartsd végig.",
             "Harminc másodperc lazán, ne állj meg.")
       + ([rest(300, f"Sorozatpihenő {b + 1}", 45)] if b < 2 else []))]
   + [cd(600, 55, 35)])

wk("d-vo2-40-20", "VO2max 40/20 – 3 × 8", "vo2max", 120,
   "VO2max és anaerob kapacitás együttes terhelése",
   "Negyven másodperc munka 120% FTP-n, húsz másodperc pihenő, három nyolcas sorozatban. A rövid pihenő miatt "
   "az oxigénfelvétel nem esik vissza, így a sorozat második felében már végig a VO2max közelében dolgozol. "
   "Keményebb, mint a 30/30; haladóknak, versenyidőszak előtt.",
   ["vo2max", "anaerobic", "advanced", "key_session", "build", "peak"], [BILLAT, RONN_SI],
   [wu(900, 45, 72)]
   + [x for b in range(3) for x in (
       micro(8, 40, 120, 20, 45, "40/20", b + 1, 102,
             "Negyven másodperc keményen, egyenletesen.",
             "Húsz másodperc: lazíts, de forogjon a láb.")
       + ([rest(300, f"Sorozatpihenő {b + 1}", 45)] if b < 2 else []))]
   + [cd(720, 55, 35)])

wk("d-vo2-ronnestad-30-15", "Rønnestad 30/15 – 3 × 13", "vo2max", 120,
   "Teljesítmény a VO2max és a küszöb között, kutatással igazolt formátum",
   "Három sorozat, mindegyikben tizenhárom kör: 30 másodperc munka (kb. 118–125% FTP), 15 másodperc könnyű "
   "tekerés; a sorozatok között három perc pihenő. Rønnestad és mtsai. vizsgálataiban ez a rövid intervallumos "
   "formátum azonos megterhelés mellett nagyobb javulást hozott a csúcsteljesítményben és a 20 perces tesztben, "
   "mint a 4 × 5 perces hosszú intervallumok. A cél a legnagyobb, mindhárom sorozatban tartható watt.",
   ["vo2max", "threshold", "evidence_based", "advanced", "key_session", "build", "peak"], [RONN_SI, RONN_2021],
   [wu(900, 45, 72)]
   + [x for b in range(3) for x in (
       micro(13, 30, 120, 15, 50, "30/15", b + 1, 100,
             "Harminc másodperc erősen, de úgy, hogy a harmadik sorozat is menjen.",
             "Tizenöt másodperc lazán.")
       + ([rest(180, f"Sorozatpihenő {b + 1}", 45)] if b < 2 else []))]
   + [cd(600, 55, 35)])

wk("d-vo2-ronnestad-intro", "Rønnestad 30/15 belépő – 2 × 10", "vo2max", 118,
   "A 30/15 formátum megismerése rövidebb sorozatokkal",
   "Két tízes 30/15 sorozat (30 másodperc ~118% FTP, 15 másodperc könnyű), három perc pihenővel. A kutatásokban "
   "használt 3 × 13-as protokoll könnyebb, rövidebb belépője: megtanít az adagolásra, mielőtt a teljes "
   "változatot csinálnád. Rövid idő alatt is komoly aerob inger.",
   ["vo2max", "evidence_based", "time_crunched", "build"], [RONN_SI, RONN_2021],
   [wu(720, 45, 72)]
   + [x for b in range(2) for x in (
       micro(10, 30, 118, 15, 50, "30/15", b + 1, 100,
             "Erős, egyenletes 30 másodperc; az első körök érződjenek könnyűnek.",
             "Tizenöt másodperc lazán.")
       + ([rest(180, "Sorozatpihenő", 45)] if b == 0 else []))]
   + [cd(660, 55, 35)])

wk("d-vo2-seiler-4x8", "Seiler 4 × 8 perc", "vo2max", 105,
   "Nagy mennyiségű munka közvetlenül a küszöb fölött – a kísérlet nyertes protokollja",
   "Négy nyolcperces intervallum két perc pihenővel, a legnagyobb még tartható intenzitáson (a kísérletben "
   "átlagosan ~105–106% FTP). Seiler és mtsai. (2013) hét hetes vizsgálatában ez a csoport javult a legtöbbet "
   "VO2peak-ben és a 4 mmol-os laktátküszöbnél mért teljesítményben, többet, mint a 4 × 4 és a 4 × 16 perces "
   "csoport. A titka a sok, kemény, de nem maximális munka.",
   ["vo2max", "threshold", "evidence_based", "key_session", "build", "peak"], [SEILER, COGGAN],
   [wu(900, 45, 78)]
   + intervals(4, 480, 105, 120, 50, label="Intervallum", cad=95,
               note="Kemény, de egyenletes; a negyediket is ugyanazon a wattszámon kell befejezni.",
               rest_note="Két perc könnyű pörgetés.")
   + [cd(780, 55, 35)])

wk("d-vo2-odden-5x8", "Változó intenzitású 5 × 8 perc (Rønnestad 2024)", "vo2max", 105,
   "Több idő a VO2max közelében változó tempójú hosszú intervallumokkal",
   "Öt nyolcperces intervallum, mindegyik 30/15-ös résszel kezd (118% / 60%), majd egyperces 110% és 90%-os "
   "szakaszok váltják egymást; két perc pihenővel. Odden, Rønnestad és mtsai. (2024) 22 kerékpárossal "
   "kimutatták, hogy minél nagyobb oxigénfelvétel-hányadon zajlik az intervallum, annál nagyobb a teljesítmény "
   "javulása; a változó tempó ezt segíti. Haladó kulcsedzés.",
   ["vo2max", "threshold", "evidence_based", "advanced", "key_session", "build", "peak"], [ODDEN_2024, BOSSI],
   [wu(900, 45, 78)]
   + [x for i in range(5) for x in (
       [s("interval", f"{i + 1}. – 30″ erős", 30, 118, 100,
          first(i, "Kemény kezdés: gyorsan felfuttatja az oxigénfelvételt.")),
        s("recovery", f"{i + 1}. – 15″ laza", 15, 60, 90),
        s("interval", f"{i + 1}. – 30″ erős (2)", 30, 118, 100),
        s("recovery", f"{i + 1}. – 15″ laza (2)", 15, 60, 90),
        s("interval", f"{i + 1}. – 30″ erős (3)", 30, 118, 100),
        s("recovery", f"{i + 1}. – 15″ laza (3)", 15, 60, 90),
        s("interval", f"{i + 1}. – 1′ fent", 60, 110, 95,
          first(i, "Innen egyperces váltások: fent 110%, lent 90%, ne pihenj.")),
        s("interval", f"{i + 1}. – 1′ lent", 60, 90, 90),
        s("interval", f"{i + 1}. – 1′ fent (2)", 60, 110, 95),
        s("interval", f"{i + 1}. – 1′ lent (2)", 60, 90, 90),
        s("interval", f"{i + 1}. – 1′ fent (3)", 60, 110, 95),
        s("interval", f"{i + 1}. – 1′ lent (3)", 60, 90, 90),
        s("interval", f"{i + 1}. – 1′15″ záró", 75, 105, 95)]
       + ([rest(120, f"Pihenő {i + 1}", 50)] if i < 4 else []))]
   + [cd(600, 55, 35)])

wk("d-vo2-varied-6x5", "Változó intenzitású VO2 6 × 5 perc", "vo2max", 108,
   "VO2max ingert adó intervallum, kevesebb szenvedéssel",
   "Hat ötperces intervallum, amelyekben 30 másodperc 125% és 60 másodperc 100% váltakozik, két és fél perc "
   "pihenővel. Bossi és mtsai. vizsgálatában a változó intenzitású munkaszakaszok mellett a kerékpárosok "
   "több időt töltöttek a VO2max 90%-a fölött, mint azonos átlagú egyenletes intervallumokkal, miközben a "
   "szubjektív megterhelés nem volt nagyobb.",
   ["vo2max", "evidence_based", "key_session", "build", "peak"], [BOSSI, ODDEN_2024],
   [wu(900, 45, 75)]
   + [x for i in range(6) for x in (
       [s("interval", f"{i + 1}. – csúcs", 30, 125, 102, first(i, "30 másodperc 125%: emeld meg a légzést.")),
        s("interval", f"{i + 1}. – tartás", 60, 100, 92, first(i, "60 másodperc az FTP-n: ne pihenj, tartsd.")),
        s("interval", f"{i + 1}. – csúcs (2)", 30, 125, 102),
        s("interval", f"{i + 1}. – tartás (2)", 60, 100, 92),
        s("interval", f"{i + 1}. – csúcs (3)", 30, 125, 102),
        s("interval", f"{i + 1}. – tartás (3)", 60, 100, 92),
        s("interval", f"{i + 1}. – záró csúcs", 30, 125, 102)]
       + ([rest(150, f"Pihenő {i + 1}", 50)] if i < 5 else []))]
   + [cd(690, 55, 35)])

wk("d-vo2-hardstart-5x4", "Kemény kezdésű VO2max 5 × 4 perc", "vo2max", 112,
   "Gyorsabb oxigénfelvétel-felfutás, több idő a VO2max közelében",
   "Öt négyperces intervallum: az első 45 másodperc 135%, utána 3 perc 15 másodperc 108%; három perc "
   "pihenővel. A kemény kezdés gyorsabban felfuttatja az oxigénfelvételt, így az intervallum nagyobb része "
   "telik a VO2max közelében; a kutatások szerint épp ez az idő függ össze a fejlődéssel. A kezdés után ne "
   "akarj tovább gyorsítani.",
   ["vo2max", "evidence_based", "advanced", "key_session", "build", "peak"], [ODDEN_2024, BOSSI],
   [wu(900, 45, 75)]
   + [x for i in range(5) for x in (
       [s("interval", f"{i + 1}. – kemény kezdés", 45, 135, 105,
          first(i, "45 másodperc nagyon erősen, ülve; ez nem sprint.")),
        s("interval", f"{i + 1}. – tartás", 195, 108, 98,
          first(i, "Vissza 108%-ra, és tartsd; a légzés maradjon a csúcson."))]
       + ([rest(180, f"Pihenő {i + 1}", 50)] if i < 4 else []))]
   + [cd(780, 55, 35)])

wk("d-vo2-15-15", "15/15 mikro-intervallumok 2 × 16", "vo2max", 125,
   "Magas aerob terhelés rövid, könnyen adagolható szakaszokkal",
   "Két sorozat, mindegyikben tizenhat kör 15 másodperc 125% és 15 másodperc 55% váltakozásával. Helgerud és "
   "mtsai. (2007) a 15/15 formátummal is jelentős VO2max-javulást értek el, közel a 4 × 4 percéhez. A "
   "rövid szakaszok kevésbé ijesztőek, ezért a magas intenzitással ismerkedőknek is jó választás.",
   ["vo2max", "evidence_based", "build"], [HELGERUD, BILLAT],
   [wu(900, 45, 72)]
   + [x for b in range(2) for x in (
       micro(16, 15, 125, 15, 55, "15/15", b + 1, 102,
             "Tizenöt másodperc élénken, egyenletes, nem sprint.",
             "Tizenöt másodperc lazán, forogjon a láb.")
       + ([rest(300, "Sorozatpihenő", 45)] if b == 0 else []))]
   + [cd(645, 55, 35)])

wk("d-vo2-pyramid", "VO2max piramis 1-2-3-4-3-2-1", "vo2max", 115,
   "Változatos VO2max munka, egyre hosszabb, majd rövidülő szakaszokkal",
   "Egy-, két-, három-, négy-, majd három-, két- és egyperces szakaszok 110–125% FTP-n (a rövidebbek "
   "erősebbek), a munkával azonos hosszú pihenővel. A piramis összesen 16 perc munkát ad a magas aerob "
   "tartományban, és a változó hossz fejben is könnyebbé teszi. Az építő időszak közepére.",
   ["vo2max", "key_session", "build"], [HELGERUD, COGGAN],
   [wu(900, 45, 75)]
   + [x for i, (m, p) in enumerate([(1, 125), (2, 120), (3, 115), (4, 110), (3, 115), (2, 120), (1, 125)])
      for x in ([s("interval", f"{m} perc ({i + 1}.)", m * 60, p, 100,
                   first(i, "A rövidebb szakaszok erősebbek; a négyperces a piramis csúcsa."))]
                + ([rest(m * 60, f"Pihenő {i + 1}", 50)] if i < 6 else []))]
   + [cd(720, 55, 35)])

wk("d-vo2-quick-5x3", "VO2max gyors 5 × 3 perc", "vo2max", 115,
   "VO2max inger 45 percben",
   "Öt háromperces ismétlés 115% FTP-n, három perc pihenővel, egy tömör bemelegítés után. Kevés idő mellett "
   "is teljes értékű VO2max edzés: 15 perc munka a magas aerob tartományban. Hétköznapi kulcsedzésnek az "
   "építő és a versenyidőszakban.",
   ["vo2max", "time_crunched", "key_session", "build", "peak"], [HELGERUD, COGGAN],
   [wu(600, 45, 78)]
   + intervals(5, 180, 115, 180, 50, label="VO2max", cad=100,
               note="Három perc keményen; a harmadik percre légy a csúcson.")
   + [cd(240, 55, 35)])

wk("d-vo2-intro-6x2", "VO2max belépő 6 × 2 perc", "vo2max", 110,
   "Első ismerkedés a VO2max intervallumokkal",
   "Hat kétperces ismétlés 110% FTP-n, három perc pihenővel. A rövid szakasz és a bő pihenő miatt ez a "
   "legbarátságosabb belépő a magas intenzitású munkába, mégis valódi aerob ingert ad. Ha könnyen megy, "
   "jöhet a 8 × 2 vagy a Helgerud-féle 4 × 4.",
   ["vo2max", "beginner", "build"], [HELGERUD, COGGAN],
   [wu(900, 45, 72)]
   + intervals(6, 120, 110, 180, 50, label="VO2max", cad=98,
               note="Erős, de kontrollált két perc; nem kell kifulladni.")
   + [cd(780, 55, 35)])

# ── Anaerob ──
wk("d-ana-3x45", "Anaerob kapacitás 2 × 3 × 45 másodperc", "anaerobic", 150,
   "Anaerob kapacitás és tejsavtűrés fejlesztése",
   "Két sorozat, mindegyikben három 45 másodperces, közel maximális (kb. 150% FTP) szakasszal, négy perc "
   "pihenővel, a sorozatok között nyolccal. A 30–60 másodperces erőfeszítésekhez hosszú pihenő kell, hogy a "
   "minőség végig megmaradjon; ez nem állóképességi, hanem kapacitásedzés. Támadásokhoz, rövid emelkedőkhöz.",
   ["anaerobic", "race_dynamics", "advanced", "key_session", "build", "peak"], [CLIMB_BC, COGGAN],
   [wu(900, 45, 75)]
   + [x for b in range(2) for x in (
       [y for i in range(3) for y in (
           [s("interval", f"Anaerob {b + 1}/{i + 1}", 45, 150, 100,
              "Közel maximális 45 másodperc; az utolsó tíz másodpercben se ess szét." if (b, i) == (0, 0) else None)]
           + ([rest(240, f"Pihenő {b + 1}/{i + 1}", 45)] if i < 2 else []))]
       + ([rest(480, "Sorozatpihenő", 45, 80, "Nyolc perc könnyű tekerés, teljes visszaállás.")] if b == 0 else []))]
   + [cd(900, 55, 35)])

wk("d-ana-gibala-sit", "Sprintintervallum 6 × 30 mp (Gibala)", "anaerobic", 170,
   "Nagy edzéshatás nagyon kevés idő alatt",
   "Hat 30 másodperces, teljes erőből végzett (Wingate-szerű) sprint, négy perc könnyű pihenővel. Gibala és "
   "mtsai. kísérleteiben a heti háromszor elvégzett 4–6 × 30 mp-es edzés hasonló izom-oxidatív "
   "alkalmazkodást hozott, mint a többszörös idejű állóképességi tekerés. A sprintekben a trainer szabad "
   "módban van: kezdj nagy fordulatról és told ki a végéig.",
   ["anaerobic", "vo2max", "evidence_based", "time_crunched", "key_session", "build"], [GIBALA, COGGAN],
   [wu(600, 45, 72)]
   + intervals(6, 30, "free", 240, 45, label="Sprint", kind="sprint", cad=110,
               note="30 másodperc teljes erőből; a második felében esni fog a watt, ez természetes.",
               rest_note="Négy perc nagyon könnyű tekerés.")
   + [cd(360, 55, 35)])

wk("d-ana-6x1", "Anaerob kapacitás 6 × 1 perc", "anaerobic", 135,
   "Egyperces erőfeszítések: támadások, rövid emelkedők, hajrák",
   "Hat egyperces szakasz 135% FTP-n, négy perc pihenővel. Az egyperces munka az anaerob kapacitást és a "
   "tejsavtűrést terheli, a bő pihenő miatt pedig minden ismétlés minőségi marad. Kritériumokra, "
   "cyclocrossra és rövid, meredek emelkedőkre.",
   ["anaerobic", "race_dynamics", "climbing", "key_session", "build", "peak"], [CLIMB_BC, COGGAN],
   [wu(900, 45, 75)]
   + intervals(6, 60, 135, 240, 45, label="Anaerob", cad=100,
               note="Egy perc nagyon keményen, egyenletesen; ne az első 15 másodpercben égesd el.")
   + [cd(600, 55, 35)])

# ── Sprint ──
wk("d-sprint-10x10", "Sprint 8 × 10 mp teljes pihenővel", "sprint", 160,
   "Csúcsteljesítmény és idegrendszeri sprintkapacitás",
   "Nyolc tízmásodperces, maximális sprint, mindegyik után öt perc könnyű tekerés. A csúcsteljesítmény "
   "fejlesztéséhez teljes visszaállás kell: ha túl rövid a pihenő, a sprint lassul, és az edzés átcsúszik "
   "anaerob állóképességibe. A sprintekben a trainer szabad módban van. Hetente legfeljebb kétszer.",
   ["sprint", "build", "peak"], [SPRINT_ROADMAN, SPRINT_ROUVY],
   [wu(720, 45, 72)]
   + intervals(8, 10, "free", 290, 45, label="Sprint", kind="sprint", cad=115,
               note="Tíz másodperc teljes erőből; az első négyet állva, a többit ülve.",
               rest_note="Öt perc nagyon könnyű pörgetés; teljes visszaállás kell.")
   + [cd(220, 50, 35)])

wk("d-sprint-leadout", "Lead-out sprintek 8 × (30 mp + 12 mp)", "sprint", 135,
   "Sprint kihozása már magas tempóból, a valódi befutó helyzetéhez hasonlóan",
   "Nyolc ismétlés: 30 másodperc 120%-os „lead-out” tempó, majd 12 másodperc teljes erőből kihozott sprint, "
   "utána öt perc könnyű tekerés. Versenyen ritkán sprintelsz friss lábakkal: a befutó előtt a mezőny tempója "
   "már magas. Az edzés ezt a helyzetet utánozza, a sprintnél a trainer szabad módban van.",
   ["sprint", "race_dynamics", "advanced", "peak"], [SPRINT_ROADMAN, SPRINT_ROUVY],
   [wu(900, 45, 72)]
   + [x for i in range(8) for x in (
       [s("work", f"Lead-out {i + 1}", 30, 120, 100, first(i, "Harminc másodperc magas tempó a kerék mögött.")),
        s("sprint", f"Sprint {i + 1}", 12, "free", 115, first(i, "Indítsd teljes erőből, állva, majd told ki."))]
       + ([rest(300, f"Pihenő {i + 1}", 45)] if i < 7 else []))]
   + [cd(564, 55, 35)])

wk("d-sprint-standing", "Álló rajtok 8 × 12 mp", "sprint", 160,
   "Robbanékonyság és nyomaték alacsony sebességről indulva",
   "Nyolc 12 másodperces sprint alacsony kadenciáról (kb. 50-ről), nehéz áttétellel indítva, állva; mindegyik "
   "után öt perc könnyű tekerés. Az álló helyzetből induló sprint a legnagyobb nyomatékot kívánja, ezért a "
   "csúcsteljesítmény mellett a kanyarkigyorsításokat és a lassú emelkedős támadásokat is fejleszti. A trainer "
   "szabad módban van.",
   ["sprint", "strength", "build", "peak"], [SPRINT_ROUVY, SPRINT_ROADMAN],
   [wu(900, 45, 72)]
   + intervals(8, 12, "free", 288, 45, label="Álló rajt", kind="sprint", cad=50,
               note="Lassítsd le a lábat kb. 50-re, nehéz áttétel, majd állva robbants.",
               rest_note="Öt perc könnyű tekerés.")
   + [cd(600, 55, 35)])

wk("d-sprint-spinups", "Pörgetett sprintek 6 × 15 mp", "sprint", 150,
   "Ülő sprint és gyors láb: kadencia és idegrendszeri koordináció",
   "Hat 15 másodperces, ülve végzett sprint könnyű áttétellel, a lehető legmagasabb (120+) fordulatra "
   "pörgetve, öt perc pihenővel. Az ülő, pörgetett sprint a gyors izomműködést és a koordinációt edzi; a "
   "kutatók és edzők szerint az álló sprint mellett ezt is érdemes gyakorolni. A trainer szabad módban van.",
   ["sprint", "technique", "time_crunched", "base", "build"], [SPRINT_ROUVY, CADENCE_ROUVY],
   [wu(600, 45, 70)]
   + intervals(6, 15, "free", 285, 45, label="Pörgetés", kind="sprint", cad=125,
               note="Ülve maradj, könnyű áttétel; a fordulat legyen a cél, nem a watt.",
               rest_note="Öt perc könnyű tekerés.")
   + [cd(450, 55, 35)])

# ── Hegymászás ──
wk("e-climb-alpe-60", "Hosszú emelkedő szimulációja 60 perc", "climb", 84,
   "Folyamatos, hosszú hegyi mászás versenytempóban",
   "Egy óra egyfolytában, négy negyedórás szakaszban 78-ról 88%-ra emelkedő terheléssel, majd egy öt perces "
   "„hajrá” a küszöb közelében. Egy alpesi hágó beltéri változata: a hosszú, egyenletes munka a tempó- és "
   "sweet spot tartományban az izom-állóképességet és az adagolást fejleszti. Szakaszonként állj ki egy-két "
   "percre, ahogy a valódi emelkedőn is.",
   ["climbing", "threshold", "durability", "advanced", "build"], [CLIMB_STRAVA, SWEETSPOT_FASCAT],
   [wu(600, 45, 70),
    s("climb", "Emelkedő 1", 900, 78, 80, "Az emelkedő alja: visszafogottan, ülve."),
    s("climb", "Emelkedő 2", 900, 82, 78, "A szakasz közepén állj ki egy percre."),
    s("climb", "Emelkedő 3", 900, 85, 76),
    s("climb", "Emelkedő 4", 900, 88, 75, "Fáradtan is tartsd a tempót és a kadenciát."),
    s("climb", "Hajrá a csúcsig", 300, 95, 80, "Az utolsó öt perc a küszöb közelében."),
    cd(900, 55, 35)])

wk("e-climb-repeats-5x3", "Hegyi ismétlések 5 × 3 perc", "climb", 108,
   "VO2max közeli hegyi ismétlések",
   "Öt háromperces, 108%-os szakasz egy képzeletbeli 5–7%-os emelkedőn, négy perc pihenővel. A rövid, "
   "kemény emelkedők a VO2max-ot és az erőt egyszerre terhelik; az ismétlésenként kicsit alacsonyabb (75 "
   "körüli) kadencia az emelkedős pedálozást utánozza. Egy-egy ismétlést végezz állva.",
   ["climbing", "vo2max", "key_session", "build", "peak"], [CLIMB_STRAVA, CLIMB_BC],
   [wu(600, 45, 75)]
   + intervals(5, 180, 108, 240, 55, label="Emelkedő", kind="climb", cad=75,
               note="Három perc erősen, kb. 75-ös kadenciával; az utolsó percben állj ki.")
   + [cd(600, 55, 35)])

wk("e-climb-torque-4x8", "Nyomatékos emelkedő 4 × 8 perc (55 rpm)", "climb", 85,
   "Erő-állóképesség és nyomaték alacsony kadencián",
   "Négy nyolcperces blokk 85% FTP-n, kb. 55-ös kadenciával, ülve. Az alacsony kadenciás munka a meredek "
   "emelkedők nagy nyomatékú pedálozását gyakoroltatja és technikai változatosságot ad. Őszintén: a "
   "kutatások (pl. Kristoffersen és mtsai., 2014) szerint az alacsony kadencia önmagában nem javít többet, "
   "mint ugyanez természetes fordulattal, ezért kiegészítésnek, ne fő edzésnek használd. Térd- vagy "
   "derékpanasz esetén hagyd ki.",
   ["climbing", "strength", "technique", "base"], [LOW_CADENCE_CTS, LOW_CADENCE_RCT],
   [wu(720, 45, 70)]
   + intervals(4, 480, 85, 180, 60, label="Nyomaték", kind="climb", cad=55,
               note="55 körüli kadencia, ülve; nyugodt felsőtest, a térd egy vonalban.",
               rest_note="Pihenő magas, 95 körüli kadenciával: lazítsd ki a lábat.", rest_cad=95)
   + [cd(840, 55, 35)])

wk("e-climb-sit-stand-3x12", "Ülve-állva hegymenet 3 × 12 perc", "climb", 92,
   "Pozícióváltás terhelés alatt, a watt megtartásával",
   "Három tizenkét perces sweet spot blokk (92% FTP), amelyekben két perc ülés és egy perc állás váltakozik. "
   "A hosszú emelkedőkön a pozícióváltás pihenteti az izmokat, de csak akkor hatékony, ha közben a watt nem "
   "esik le. Az edzés ezt a technikát gyakoroltatja, miközben értékes küszöb alatti munkát ad.",
   ["climbing", "threshold", "technique", "build"], [CLIMB_STRAVA, SWEETSPOT_FASCAT],
   [wu(720, 45, 72)]
   + [x for b in range(3) for x in (
       [y for i in range(4) for y in (
           s("climb", f"Ülve {b + 1}/{i + 1}", 120, 92, 80,
             "Két perc ülve, 80 körüli kadenciával." if (b, i) == (0, 0) else None),
           s("climb", f"Állva {b + 1}/{i + 1}", 60, 92, 68,
             "Egy perc állva: emelj egyet az áttételen, a watt maradjon ugyanannyi." if (b, i) == (0, 0) else None))]
       + ([rest(300, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(600, 55, 35)])

wk("e-climb-steep-8x90", "Rövid, meredek emelkedők 8 × 90 mp", "climb", 120,
   "Erő és anaerob kapacitás rövid, meredek falakon",
   "Nyolc másfél perces szakasz 120% FTP-n, 70 körüli kadenciával, három perc pihenővel. A rövid, meredek "
   "„falak” (klasszikusok, dombos sportive-ok) a VO2max és az anaerob rendszer határán dolgoznak, nagy "
   "nyomatékkal. Egyet ülve, egyet állva végezz felváltva.",
   ["climbing", "anaerobic", "vo2max", "key_session", "build", "peak"], [CLIMB_BC, CLIMB_STRAVA],
   [wu(900, 45, 75)]
   + intervals(8, 90, 120, 180, 50, label="Fal", kind="climb", cad=70,
               note="Másfél perc keményen, 70 körüli kadenciával; felváltva ülve és állva.")
   + [cd(630, 55, 35)])

wk("e-climb-attack-4x6", "Hegyi támadások 4 × 6 perc", "climb", 102,
   "Küszöbtempó után támadás az emelkedő tetején",
   "Négy hatperces blokk: öt perc 95% az emelkedőn, majd egy perc 125%-os támadás a „csúcs” előtt, négy perc "
   "pihenővel. A hegyi versenyek gyakran ott dőlnek el, ahol már fáradtan kell még egyszer gyorsítani. Az "
   "edzés ezt a helyzetet gyakoroltatja.",
   ["climbing", "race_dynamics", "threshold", "key_session", "build", "peak"], [CLIMB_STRAVA, OVER_UNDER],
   [wu(900, 45, 78)]
   + [x for i in range(4) for x in (
       [s("climb", f"Emelkedő {i + 1}", 300, 95, 82, first(i, "Öt perc a küszöb alatt, egyenletesen.")),
        s("attack", f"Támadás {i + 1}", 60, 125, 95, first(i, "Egy perc támadás, állva indítva, ülve befejezve."))]
       + ([rest(240, f"Pihenő {i + 1}", 50)] if i < 3 else []))]
   + [cd(780, 55, 35)])

# ── Verseny ──
wk("e-race-crit", "Kritérium szimuláció", "race", 100,
   "Rövid, robbanékony körök és sprintbefejezés",
   "Hat kör, mindegyikben egy 120%-os, egyperces kanyarkigyorsítás után négy perc 88%-os mezőnytempó, a végén "
   "pedig egy 30 másodperces hajrá. A kritériumok ismétlődő gyorsításait utánozza; a lényeg, hogy a "
   "kilövések után azonnal vissza kell állni egy erős tempóra.",
   ["race_dynamics", "anaerobic", "key_session", "peak"], [OVER_UNDER, SPRINT_ROADMAN],
   [wu(600, 45, 75)]
   + [x for i in range(6) for x in (
       s("attack", f"{i + 1}. kör – kigyorsítás", 60, 120, 95,
         first(i, "Kanyar utáni kilövés: állva indulj, majd ülj vissza.")),
       s("work", f"{i + 1}. kör – mezőny", 240, 88, 88,
         first(i, "Négy perc erős mezőnytempó; a pulzus még magasan van.")))]
   + [s("sprint", "Hajrá", 30, "free", 110, "Az utolsó 30 másodperc teljes erőből."), cd(570, 55, 35)])

wk("e-race-surges", "Támadások és szökés szimuláció", "race", 100,
   "Ismételt támadások, üldözések és visszazárkózás kezelése",
   "Három támadás–üldözés–összeállás blokk, négy rövid gyorsítás tempóval, három csapatmunka-blokk és egy "
   "egyperces hajrá. Egy országúti verseny forgatókönyve: a különböző erőfeszítések sorrendje és "
   "ismétlődése edzi azt, amit egyetlen intervallumtípus nem tud.",
   ["race_dynamics", "vo2max", "anaerobic", "advanced", "key_session", "peak"], [OVER_UNDER, SPRINT_ROADMAN],
   [wu(600, 45, 75)]
   + [x for i in range(3) for x in (
       s("attack", f"{i + 1}. támadás", 45, 130, 100, first(i, "Teljes lendülettel, állva indítva.")),
       s("chase", f"{i + 1}. üldözés", 135, 105, 90, first(i, "Üldözés időfutam-tempóban.")),
       s("regroup", f"{i + 1}. összeállás", 180, 65, 85, first(i, "A csoport összeáll: könnyebb, de nem pihenő.")))]
   + [x for i in range(4) for x in (
       s("surge", f"{i + 1}. gyorsítás", 30, 125, 95, first(i, "Rövid gyorsítás, majd vissza a tempóba.")),
       s("work", f"{i + 1}. tempó", 150, 90, 85))]
   + [x for i in range(3) for x in (
       s("work", f"{i + 1}. váltás az élen", 60, 110, 90, first(i, "Váltás az élen, erősen.")),
       s("work", f"{i + 1}. a kerékben", 120, 80, 85))]
   + [s("sprint", "Befutó", 60, 150, 110, "Hosszú hajrá, az utolsó 15 másodperc teljes erőből."),
      cd(600, 55, 35)])

wk("d-race-opener", "Verseny előtti aktiválás (opener)", "race", 105,
   "Az idegrendszer ébresztése a verseny előtti napon, fáradtság nélkül",
   "Rövid, éles nyitó edzés a verseny előtti napra: három félperces gyorsítás, egy rövid FTP-blokk és két "
   "tízmásodperces sprint, sok könnyű tekeréssel. A cél a lábak felébresztése és a versenytempó "
   "felidézése, nem a fárasztás; az edzés végén frissnek kell érezned magad.",
   ["opener", "peak"], [OPENERS, COGGAN],
   [wu(900, 45, 72, 85, "Nyugodt ráhangolás; nem kell sietni.")]
   + intervals(3, 30, "free", 180, 50, label="Gyorsítás", kind="sprint", cad=110,
               note="Kb. 90%-os erőfeszítés, nem teljes erő; a cél az ébresztés.")
   + [rest(180, "Pihenő 3", 50),
      s("interval", "FTP-blokk", 150, 100, 90, "Két és fél perc az FTP-n, a versenytempó felidézése."),
      rest(180, "Pihenő 4", 50)]
   + intervals(2, 10, "free", 110, 50, label="Sprint", kind="sprint", cad=115, note="Tíz másodperc élesen.")
   + [cd(900, 55, 35)])

wk("e-race-opener-30", "Gyors aktiválás 30 perc", "race", 100,
   "Rövid nyitó edzés a verseny napján vagy előtte, kevés idővel",
   "Fél óra: bemelegítés, három egyperces emelés a küszöb közelében és három rövid sprint. A versenyek előtti "
   "bemelegítés mintájára felkészíti a szív-keringési rendszert és az izmokat a rajtra, anélkül, hogy "
   "fárasztana. Rövid versenyek (krit, időfutam) előtt bemelegítésként is használható.",
   ["opener", "time_crunched", "peak"], [OPENERS, COGGAN],
   [wu(480, 45, 72)]
   + intervals(3, 60, 100, 120, 50, label="Emelés", cad=95, note="Egy perc az FTP-n, kontrolláltan.")
   + [rest(120, "Pihenő 3", 50)]
   + intervals(3, 8, "free", 112, 50, label="Sprint", kind="sprint", cad=115, note="Nyolc másodperc élesen.")
   + [cd(528, 55, 35)])

wk("e-race-tt-40", "Időfutam-szimuláció 40 perc", "race", 98,
   "Időfutam-adagolás: visszafogott kezdés, erős befejezés",
   "Negyven perc időfutam-tempó három szakaszban: tíz perc 95%, húsz perc 98%, majd tíz perc 100–103% az "
   "utolsó két perc szabad hajrájával. Az időfutamokat a túl gyors kezdés rontja el leggyakrabban; az edzés a "
   "negatív split adagolását tanítja. Ha van időfutam-pozíciód, abban tekerj.",
   ["time_trial", "threshold", "advanced", "key_session", "peak"], [FTP_ALLEN, CTS_8MIN],
   [wu(900, 45, 80)]
   + intervals(2, 30, 110, 90, 55, label="Nyitó", cad=100, note="Két rövid emelés a rajt előtt.")
   + [rest(180, "Rajt előtt", 50),
      s("interval", "Rajt – visszafogott", 600, 95, 90, "Az első tíz perc érződjön túl könnyűnek. Jó így."),
      s("interval", "Középszakasz", 1200, 98, 90, "Egyenletes, pozícióban; csak a következő percre figyelj."),
      s("interval", "Erős befejezés", 480, 102, 92, "Emelj; innen már nem kell tartalékolni."),
      s("test", "Hajrá", 120, "free", 95, "Az utolsó két perc szabad: told ki, amennyi maradt."),
      cd(600, 55, 35)])

wk("e-race-breakaway", "Szökés-szimuláció 3 × (1 + 8 perc)", "race", 100,
   "Támadás után a szökés megtartása a küszöb közelében",
   "Három blokk: egy perc 130%-os támadás, majd azonnal nyolc perc 95%-os tartás, öt perc pihenővel. A szökés "
   "nem a támadáson, hanem az utána következő percek tartásán múlik, amikor a tejsav még magas. Ez az edzés "
   "pontosan ezt a fájdalmas átmenetet gyakoroltatja.",
   ["race_dynamics", "threshold", "anaerobic", "advanced", "key_session", "peak"], [OVER_UNDER, COGGAN],
   [wu(900, 45, 78)]
   + [x for i in range(3) for x in (
       [s("attack", f"Támadás {i + 1}", 60, 130, 100, first(i, "Egy perc teljes lendülettel, hogy elszakadj.")),
        s("chase", f"Tartás {i + 1}", 480, 95, 90,
          first(i, "Azonnal vissza 95%-ra: az első két perc a legnehezebb, ne engedj."))]
       + ([rest(300, f"Pihenő {i + 1}", 50)] if i < 2 else []))]
   + [cd(780, 55, 35)])

wk("e-race-gravel", "Gravel és MTB: változó terhelés", "race", 95,
   "Folyamatosan változó terhelés, ahogy a terepen",
   "Három blokk, mindegyikben rövid, meredek rámpák (30 mp 140%), tempós egyenesek és küszöb körüli "
   "emelkedők váltják egymást pihenő nélkül. A gravel és az MTB terhelése ritkán egyenletes: sok rövid "
   "gyorsítás, közte alig pihenés. Az edzés a VO2max-ot, az anaerob kapacitást és a tempó újrafelvételét "
   "egyszerre terheli.",
   ["race_dynamics", "vo2max", "anaerobic", "key_session", "build", "peak"], [OVER_UNDER, CLIMB_BC],
   [wu(720, 45, 75)]
   + [x for b in range(3) for x in (
       [s("climb", f"{b + 1}. emelkedő", 180, 100, 80, first(b, "Három perc a küszöbön, mint egy hosszabb domb.")),
        s("surge", f"{b + 1}. rámpa", 30, 140, 90, first(b, "Rövid, meredek rámpa: állva, erősen.")),
        s("work", f"{b + 1}. egyenes", 150, 80, 90, first(b, "Tempó a kavicson; nem pihenő.")),
        s("surge", f"{b + 1}. rámpa (2)", 30, 140, 90),
        s("work", f"{b + 1}. hullámzó", 120, 90, 88),
        s("surge", f"{b + 1}. rámpa (3)", 30, 140, 90),
        s("work", f"{b + 1}. egyenes (2)", 180, 78, 90)]
       + ([rest(300, f"Pihenő {b + 1}", 55)] if b < 2 else []))]
   + [cd(780, 55, 35)])

wk("e-race-group-ride", "Csoportos tekerés szimuláció 90 perc", "race", 78,
   "Mezőnyszerű, hullámzó terhelés: aerob alap ritmusváltásokkal",
   "Másfél óra 2-es és 3-as zóna, közben szabálytalanul érkező gyorsításokkal (15 mp–1 perc, 105–150%), "
   "ahogy egy csoportos tekerésen vagy egy amatőr verseny elején. Hasznos, ha nincs csoportod: aerob alapot "
   "ad, miközben a ritmusváltásokat is gyakoroltatja. Nem kulcsedzés, inkább minőségi „hosszú”.",
   ["race_dynamics", "aerobic_base", "durability", "base", "build"], [TEMPO, POLARIZED],
   [wu(600, 45, 68)]
   + [x for i, (on_s, on_p, off_s, off_p) in enumerate([
       (60, 105, 540, 68), (15, 150, 465, 72), (30, 120, 570, 70), (60, 110, 480, 75),
       (15, 150, 345, 68), (30, 125, 570, 72), (60, 105, 420, 78), (15, 150, 525, 70)])
      for x in (s("surge", f"Gyorsítás {i + 1}", on_s, on_p, 98,
                  first(i, "Valaki meglódult az élen: zárj rá, aztán vissza a tempóba.")),
                s("steady", f"Mezőny {i + 1}", off_s, off_p, 90,
                  first(i, "Mezőnytempó a 2-es és 3-as zóna határán.")))]
   + [cd(600, 55, 35)])

# ── Teszt ──
wk("c-test-ftp-20min", "FTP-teszt 20 perc (5 perces nyitással)", "test", 105,
   "Az FTP meghatározása Hunter Allen húszperces protokolljával",
   "Alapos bemelegítés, három pörgetés, egy öt perces maximális erőfeszítés, majd tíz perc pihenő után a "
   "húszperces teszt. Az FTP a 20 perces átlagteljesítmény 95%-a; az előtte végzett ötperces erőfeszítés "
   "levesz a frissességből, így a 20 perces szám közelebb esik az egyórás teljesítményhez. A tesztszakaszokban "
   "a trainer szabad módban van: te adagolsz.",
   ["testing", "threshold", "build"], [FTP_ALLEN, POWER_PROFILE],
   [wu(1200, 45, 75)]
   + intervals(3, 60, 95, 60, 55, label="Pörgetés", cad=110, note="Gyors láb, 110 fölött.")
   + [rest(300, "Pihenő 3", 55),
      s("test", "5 perc maximális", 300, "free", 95, "Erősen, de ne a teljes kimerülésig; ez nyitás."),
      rest(600, "Pihenő 4", 50),
      s("test", "20 perces teszt", 1200, "free", 90,
        "Kezdj visszafogottan, a második tíz percben emelj; az utolsó két perc teljes erőből."),
      cd(600, 55, 35)])

wk("c-test-ftp-8min", "FTP-teszt 2 × 8 perc (CTS)", "test", 105,
   "Az FTP becslése két nyolcperces erőfeszítéssel",
   "Két nyolcperces maximális erőfeszítés tíz perc pihenővel. A rövidebb szakaszokat könnyebb jól adagolni, "
   "ezért kezdőknek is megbízhatóbb, mint a húszperces teszt; az FTP becslése a két 8 perces átlag "
   "jobbikának kb. 90%-a. A tesztszakaszokban a trainer szabad módban van.",
   ["testing", "threshold", "beginner", "base", "build"], [CTS_8MIN, FTP_ALLEN],
   [wu(1200, 45, 78)]
   + intervals(2, 60, 95, 60, 55, label="Pörgetés", cad=110, note="Gyors láb, 110 fölött.")
   + [rest(300, "Pihenő 2", 55),
      s("test", "1. nyolc perc", 480, "free", 92, "Erős, egyenletes kezdés; az utolsó percben adj bele mindent."),
      rest(600, "Pihenő 3", 50),
      s("test", "2. nyolc perc", 480, "free", 90, "Ugyanaz a stratégia; jó, ha az elsőnél nem gyengébb."),
      cd(600, 55, 35)])

wk("e-ramp-20w", "Ramp teszt 20 W/perc", "test", 100,
   "A maximális aerob teljesítmény és a becsült FTP gyors meghatározása",
   "Rövid bemelegítés után percenként 20 wattal emelkedő terhelés a teljes kimerülésig, ERG módban. Az utolsó "
   "teljes perc átlagának kb. 75%-a adja az FTP becslését. Rövid, könnyen ismételhető, kevés adagolási "
   "hibalehetőséggel; az átlagos tekerőnek ez az alapértelmezett teszt. Akkor állj le, ha már nem tudod tartani "
   "a kadenciát.",
   ["testing", "vo2max", "beginner", "time_crunched", "base", "build", "peak"], [RAMP_ROUVY, RAMP_ZI],
   [wu(420, 40, 65),
    s("warmup", "Ráhangolás", 180, 70, 95, "Három perc egyenletesen, majd indul a lépcső."),
    s("test", "Ramp", 1200, {"type": "step_watts", "start": 100, "step": 20, "step_s": 60, "max": 600}, 90,
      "Minden percben +20 W. Tartsd a kadenciát, amíg tudod; ha elfogy, a teszt véget ért."),
    cd(300, 50, 30)])

wk("e-ramp-15w", "Ramp teszt 15 W/perc (finom lépések)", "test", 100,
   "Pontosabb MAP-becslés kisebb lépésközzel, könnyebb bringásoknak",
   "Percenként 15 wattal emelkedő lépcső a kimerülésig. A kisebb lépésköz alacsonyabb FTP mellett (kb. 200 W "
   "alatt) pontosabb eredményt ad, mert több lépcsőn át tart a teszt. Az FTP becslése itt is az utolsó teljes "
   "perc kb. 75%-a.",
   ["testing", "vo2max", "beginner", "time_crunched", "base", "build", "peak"], [RAMP_ROUVY, RAMP_ZI],
   [wu(480, 40, 66),
    s("warmup", "Ráhangolás", 120, 75, 95, "Két perc egyenletesen."),
    s("test", "Ramp", 1500, {"type": "step_watts", "start": 90, "step": 15, "step_s": 60, "max": 550}, 90,
      "Minden percben +15 W, a kimerülésig."),
    cd(300, 50, 30)])

wk("e-ramp-25w", "Ramp teszt 25 W/perc", "test", 105,
   "Gyorsabb ramp teszt erősebb bringásoknak",
   "Percenként 25 wattal emelkedő lépcső a kimerülésig, magasabb FTP mellett (kb. 300 W fölött), hogy a "
   "teszt ne nyúljon túl hosszúra. A rövidebb teszt kevésbé fárasztó, így a maximális aerob teljesítmény "
   "pontosabban mérhető. Az FTP becslése az utolsó teljes perc kb. 75%-a.",
   ["testing", "vo2max", "advanced", "time_crunched", "base", "build", "peak"], [RAMP_ROUVY, RAMP_ZI],
   [wu(480, 40, 68),
    s("warmup", "Ráhangolás", 180, 72, 95, "Három perc egyenletesen."),
    s("test", "Ramp", 1200, {"type": "step_watts", "start": 100, "step": 25, "step_s": 60, "max": 650}, 90,
      "Minden percben +25 W, a kimerülésig."),
    cd(300, 50, 30)])

wk("e-ramp-3min-steps", "MAP teszt 3 perces lépésekkel", "test", 100,
   "Stabil állapotú maximális aerob teljesítmény hosszabb lépésekkel",
   "Háromperces, 30 wattos lépcsők a kimerülésig. A hosszabb lépésekben az oxigénfelvétel és a pulzus be tud "
   "állni, ezért a laborokban is használt, klasszikus MAP-protokollhoz áll közelebb, és pulzuszónák "
   "meghatározására is alkalmas. Hosszabb és keményebb, mint a percenkénti ramp.",
   ["testing", "vo2max", "advanced", "time_crunched", "base", "build"], [RAMP_ROUVY, COGGAN],
   [wu(540, 40, 70),
    s("warmup", "Ráhangolás", 180, 75, 95, "Három perc egyenletesen."),
    s("test", "Lépcsős teszt", 1380, {"type": "step_watts", "start": 150, "step": 30, "step_s": 180, "max": 600}, 90,
      "Háromperces lépcsők, +30 W; minden lépcső végén figyeld a pulzust."),
    cd(600, 50, 30)])

wk("c-test-cp-3-12", "Kritikus teljesítmény teszt 3 + 12 perc", "test", 105,
   "A kritikus teljesítmény (CP) és az anaerob tartalék (W′) becslése",
   "Egy háromperces és egy tizenkét perces maximális erőfeszítés, közöttük harminc perc könnyű tekeréssel. A "
   "két átlagteljesítményből kiszámítható a kritikus teljesítmény (a küszöb jó közelítése) és a W′, vagyis "
   "hogy mennyi munkát tudsz a küszöb fölött elvégezni. Karsten és mtsai. szerint a terepen végzett "
   "többpontos CP-tesztek érvényesek és megbízhatók. Mindkét szakasz szabad módban, egyenletes adagolással.",
   ["testing", "threshold", "anaerobic", "evidence_based", "advanced", "base", "build"], [CP_FIELD, POWER_PROFILE],
   [wu(1200, 45, 78)]
   + intervals(2, 60, 100, 120, 55, label="Nyitó", cad=100, note="Egy perc az FTP-n, ébresztés.")
   + [rest(300, "Rajt előtt", 50),
      s("test", "3 perc maximális", 180, "free", 100, "Egyenletesen, az utolsó percben mindent bele."),
      rest(1800, "Pihenő", 50, 85, "Harminc perc könnyű tekerés; ne rövidítsd le."),
      s("test", "12 perc maximális", 720, "free", 92, "Visszafogott első három perc, aztán emelj."),
      cd(600, 55, 35)])

wk("c-test-power-profile", "Teljesítményprofil-teszt (15 mp, 1 perc, 5 perc)", "test", 120,
   "Erősségek és gyengeségek feltérképezése a teljesítményprofilhoz",
   "Egy 15 másodperces sprint (a legjobb 5 másodperc számít), egy egyperces és egy ötperces maximális "
   "erőfeszítés, bőséges pihenőkkel. Allen és Coggan teljesítményprofilja e számokat (a 20 perces FTP-teszttel "
   "együtt) veti össze, hogy kiderüljön, sprinter, versenyző vagy időfutamos alkat vagy, és hol van a "
   "legnagyobb tartalék. Minden tesztszakasz szabad módban.",
   ["testing", "sprint", "anaerobic", "vo2max", "build"], [POWER_PROFILE, FTP_ALLEN],
   [wu(1200, 45, 75)]
   + intervals(2, 10, 120, 110, 50, label="Ébresztő", cad=105, note="Rövid, könnyed gyorsítás.")
   + [rest(240, "Rajt előtt", 50),
      s("test", "15 mp sprint", 15, "free", 110, "Teljes erőből, állva indítva."),
      rest(600, "Pihenő 1", 50),
      s("test", "1 perc maximális", 60, "free", 100, "Erős kezdés, de tartsd ki végig."),
      rest(900, "Pihenő 2", 50),
      s("test", "5 perc maximális", 300, "free", 95, "Egyenletes adagolás, az utolsó percben mindent bele."),
      cd(585, 55, 35)])

wk("c-test-durability", "Tartóssági teszt: 5 perc 90 perc után", "test", 76,
   "Mennyit esik a teljesítményed hosszú terhelés után",
   "Kilencven perc 75% FTP után egy ötperces maximális erőfeszítés. Hasonlítsd össze egy friss, pihent "
   "állapotban mért ötperces eredménnyel: a különbség a tartósságod. A kutatások szerint a fáradt állapotú "
   "teljesítmény amatőröknél is külön számít, és a sikeresebb versenyzőknél kisebb a visszaesés.",
   ["testing", "durability", "advanced", "long_ride", "build"], [DURABILITY, DURABILITY_AMATEUR],
   [wu(600, 45, 70),
    s("steady", "Előterhelés 1", 2700, 75, 90, "Egyenletes 75%, rendszeres evés-ivás."),
    s("steady", "Előterhelés 2", 2700, 75, 88, "Ugyanaz; ne gyorsíts a teszt előtt."),
    rest(180, "Rajt előtt", 55),
    s("test", "5 perc maximális fáradtan", 300, "free", 95, "Egyenletes adagolás, mintha friss lennél."),
    cd(720, 55, 35)])

# ── Vegyes ──
wk("f-mixed-threshold-vo2", "Küszöb és VO2max kombó", "mixed", 105,
   "Két kulcsinger egy edzésben: küszöb, majd VO2max",
   "Két tizenkét perces küszöbblokk (97%), majd öt kétperces VO2max ismétlés (118%). A küszöbmunka után már "
   "fáradtan kell a magas aerob tartományban dolgozni, ami a versenyek második felére jellemző. Haladóknak, "
   "ha heti kevés kulcsedzésbe kell sok minőséget sűríteni.",
   ["threshold", "vo2max", "durability", "advanced", "key_session", "build", "peak"], [FTP_ALLEN, HELGERUD],
   [wu(900, 45, 78)]
   + intervals(2, 720, 97, 300, 55, label="Küszöb", cad=90, note="Az FTP alatt egy hajszállal, egyenletesen.")
   + [rest(300, "Pihenő", 50)]
   + intervals(5, 120, 118, 180, 50, label="VO2max", cad=102, note="Két perc keményen, fáradt lábbal is.")
   + [cd(600, 55, 35)])

wk("f-mixed-sweetspot-sprints", "Sweet spot és fáradt sprintek", "mixed", 95,
   "Sweet spot munka, majd sprintek fáradt lábakkal, mint a verseny végén",
   "Három tizenkét perces sweet spot blokk, utána négy 10 másodperces sprint öt perc pihenővel. A verseny "
   "végén friss láb nélkül kell sprintelni: a fáradt sprint külön képesség. A sprinteknél a trainer szabad "
   "módban van.",
   ["threshold", "sprint", "durability", "build", "peak"], [SWEETSPOT_FASCAT, SPRINT_ROADMAN],
   [wu(720, 45, 72)]
   + intervals(3, 720, 90, 240, 55, label="Sweet spot", cad=90, note="90%, egyenletesen.")
   + [rest(300, "Pihenő", 50)]
   + intervals(4, 10, "free", 290, 45, label="Fáradt sprint", kind="sprint", cad=110,
               note="Teljes erőből, állva indítva.")
   + [cd(530, 55, 35)])

wk("f-mixed-long-vo2-finish", "Hosszú alap VO2max befejezéssel 2 óra", "mixed", 80,
   "Tartósság: VO2max munka órányi terhelés után",
   "Hetvenöt perc 2-es zóna után négy háromperces VO2max ismétlés (112%), majd levezetés. A hosszú "
   "előterhelés után végzett magas intenzitás egyszerre fejleszti az aerob kapacitást és a tartósságot; a "
   "versenyek döntő pillanatai is így érkeznek. Csak jó alapokkal, és bőséges szénhidrát-bevitellel.",
   ["durability", "vo2max", "aerobic_base", "advanced", "long_ride", "key_session", "build"],
   [DURABILITY, DURABILITY_AMATEUR],
   [wu(600, 45, 65),
    s("steady", "Zóna 2 – 1", 2400, 66, 90, "Egyenletes; egyél rendszeresen."),
    s("steady", "Zóna 2 – 2", 2100, 68, 88),
    rest(180, "Átállás", 55)]
   + intervals(4, 180, 112, 180, 50, label="Fáradt VO2max", cad=100,
               note="A fáradt lábaknak ugyanazt a wattszámot kell tartaniuk.")
   + [cd(660, 55, 35)])


# ───────────────────────────── finish ─────────────────────────────
def finish(w):
    steps = w["steps"]
    total = sum(st["duration_s"] for st in steps)
    if total % 60:
        steps[-1]["duration_s"] += 60 - total % 60
        print(f"  padded {w['id']} cooldown by {60 - total % 60}s")
        total = sum(st["duration_s"] for st in steps)
    w["duration_min"] = total // 60
    errs = []
    if steps[0]["kind"] != "warmup" or steps[-1]["kind"] != "cooldown":
        errs.append("warmup/cooldown")
    if len(steps) > 80:
        errs.append(f"{len(steps)} steps")
    if any(st["duration_s"] < 5 for st in steps):
        errs.append("short step")
    unknown = set(w["tags"]) - ALL_TAGS
    if unknown:
        errs.append(f"unknown tags {unknown}")
    if not set(w["tags"]) & GOAL_TAGS:
        errs.append("no goal tag")
    if ("time_crunched" in w["tags"]) != (w["duration_min"] <= 45):
        errs.append(f"time_crunched vs {w['duration_min']} min")
    if ("long_ride" in w["tags"]) != (w["duration_min"] >= 120):
        errs.append(f"long_ride vs {w['duration_min']} min")
    if len(w["tags"]) != len(set(w["tags"])):
        errs.append("duplicate tag")
    if not w["sources"] or len(w["description"]) < 120 or not w["goal"]:
        errs.append("text/sources")
    if errs:
        raise SystemExit(f"{w['id']}: {errs}")
    order = ["id", "name", "category", "goal", "description", "ftp_pct", "duration_min", "tags", "sources", "steps"]
    return {k: w[k] for k in order}


out = [finish(w) for w in W]
ids = [w["id"] for w in out]
assert len(ids) == len(set(ids)), "duplicate id"
path = sys.argv[1]
with open(path, "w", encoding="utf-8", newline="\n") as f:
    json.dump({"schema_version": 1, "workouts": out}, f, ensure_ascii=False, indent=1)
    f.write("\n")
from collections import Counter
print(len(out), "workouts")
print(Counter(w["category"] for w in out))
print(Counter(t for w in out for t in w["tags"]))
