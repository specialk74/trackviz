# trackviz — Specifica completa del progetto

Questo documento contiene tutto quello che serve per ricostruire il progetto da zero: obiettivo, dominio, requisiti, decisioni prese, architettura, formato delle scene, stile grafico, la prima scena realizzata e i punti ancora aperti.

---

## 1. Obiettivo

Uno strumento che genera **video esplicativi** (MP4) dell'interazione tra l'automazione di laboratorio Inpeco e un analizzatore di terze parti.

- **Destinatari dei video:** i produttori di analizzatori.
- **Scopo:** far capire **come Inpeco vuole scambiare i messaggi** e **in quali momenti** l'automazione si aspetta di riceverli o di mandarli, in relazione al movimento fisico del carrier sul track.
- **Requisito chiave:** non deve servire per un solo video. È un **tool riutilizzabile, guidato da file di scenario**: una nuova scena si ottiene scrivendo un nuovo file, senza modificare il codice.

---

## 2. Glossario di dominio

| Termine | Significato |
|---|---|
| **Track** | Il sistema di nastri trasportatori dell'automazione Inpeco. |
| **Carrier** | Supporto ("puck") che trasporta **una sola** provetta. Può viaggiare **pieno** (con tubo) o **vuoto**. |
| **Tubo / provetta** | Contiene il campione biologico. Ha un'etichetta con il **SampleId**. |
| **SampleId** | Identificativo del campione letto dall'etichetta. **Deve comparire in tutti i messaggi.** |
| **Main-Lane** | Corsia principale del track. |
| **NSD** | **No Stop Divert**: il dispositivo che sposta il carrier, pieno o vuoto, dalla Main-Lane alla Pit-Lane **senza fermarlo**. Il termine è solo interno: **nei video non compare** (il riquadro non ha etichetta e le didascalie dicono "diverted without stopping"). Lo verifica un test su tutte le scene; nei file di scena restano `kind = "nsd"` e l'id `nsd`. |
| **Pit-Lane** | Corsia secondaria, parallela alla Main-Lane, in cui il carrier viene fermato per farlo campionare dall'analizzatore. Dopo il punto di campionamento rientra nella Main-Lane, così i carrier processati (pieni o vuoti) tornano nella corsia principale. |
| **Punto di campionamento** | Posizione sulla Pit-Lane in cui il carrier si ferma e l'analizzatore aspira (pipetta) il campione. |
| **Turnstile** | Ruota con **3 tasche** a 120° alla fine della Pit-Lane. Posizioni: **ingresso** (all'altezza della Pit-Lane), **sotto l'ago** dell'analizzatore (in alto: il campionamento avviene qui), **uscita** (all'altezza della Pit-Lane, dal lato opposto, verso la Main-Lane). Ogni rotazione porta ingresso → ago → uscita. Quando il carrier con il tubo arriva sotto l'ago, l'Interfaccia manda Sample Acquisition. Dopo ogni Sample Complete la ruota gira: il carrier in ingresso va sotto l'ago e quello campionato va verso l'uscita. A ruota vuota, appena il primo carrier è in ingresso la ruota gira subito. |
| **Gate + antenna** | Davanti alla ruota. Il carrier si ferma **sempre** all'antenna, che lo legge: l'Interfaccia manda **di nuovo Prepare** (in futuro con un'informazione in più; per ora è disegnato in viola). Se sotto l'ago non c'è un campionamento in corso il gate si apre e il carrier entra nella tasca d'ingresso; altrimenti aspetta il Sample Complete. Solo dopo che il carrier in attesa è entrato la ruota gira (in ingresso → ago, campionato → uscita). |
| **Spur** | Ramo a U **perpendicolare** alla Main-Lane (come nell'NCCLS). L'NSD devia il carrier, senza fermarlo, in un breve tratto di pit-lane che gira di 90° nel ramo d'andata; sul ramo d'andata c'è il punto di campionamento con l'**analizzatore alla sua destra**; in cima il carrier attraversa e scende dal ramo di ritorno fino alla Main-Lane. |
| **Deviazione standard** | Deviazione usata dentro lo spur con più punti di campionamento: a differenza dell'NSD **ogni** carrier si ferma e viene letto, poi viene deviato nella pit-lane di un ago o prosegue dritto, mentre il successivo si ferma dietro. Vale anche per chi rientra da un ago e incontra la deviazione successiva. |
| **Interfaccia** | Il modulo di automazione Inpeco davanti all'analizzatore (NSD + Pit-Lane + rientro). È anche l'attore che scambia i messaggi con l'analizzatore. |
| **Analizzatore / strumento** | Lo strumento di terze parti che esegue i test. |
| **DMX** | Il nome Inpeco del **LIS**. Comunica con lo strumento: gli dice quali test eseguire e ne riceve i risultati. |

---

## 3. Flusso di riferimento (scena 01 — happy path, un carrier)

Comportamento descritto dall'utente:

1. Il carrier con il tubo viaggia sulla **Main-Lane**.
2. Quando arriva all'**NSD** viene deviato nella Pit-Lane senza fermarsi. In quel momento l'**Interfaccia manda Prepare** all'Analizzatore.
3. Dopo il Prepare l'Analizzatore può rispondere **"OK"** oppure con un valore che indica che il campione **non deve essere processato** (il caso negativo è da specializzare in seguito).
4. Dopo aver ricevuto il Prepare, l'**Analizzatore interroga DMX** per sapere quali test eseguire.
5. Quando il carrier arriva al **punto di campionamento**, l'**Interfaccia manda Sample Acquisition** all'Analizzatore.
6. L'Analizzatore pipetta il materiale biologico dalla provetta.
7. Quando arriva **Sample Complete** dall'Analizzatore all'Interfaccia, il carrier si muove e viene **rilasciato** dall'Interfaccia.
8. Quando il carrier esce dall'Interfaccia e **arriva sulla Main-Lane**, l'**Interfaccia manda Release** all'Analizzatore.
9. Dopo l'analisi l'**Analizzatore manda i risultati a DMX**.

**Messaggi di stato** (non inclusi nella scena 01): lo strumento comunica il proprio stato o **su richiesta dell'Interfaccia** oppure **quando lo stato cambia**. Quale dei due dipende da come è stato sviluppato l'analizzatore.

---

## 4. Requisiti del video (scelte dell'utente)

- **Stile:** schematico 2D. Se non convince, in seguito si proverà una versione realistica.
- **Durata:** massimo **1 minuto**.
- **Testo:** didascalie esplicative, **nessuna voce**.
- **Lingua:** tutto in **inglese** (etichette, didascalie, messaggi).
- **Composizione:** a sinistra la simulazione del track (carrier in movimento), a destra il **sequence diagram** sincronizzato.
- **Partecipanti del sequence diagram:** Interface (Inpeco automation), Analyzer, DMX (LIS).
- **Messaggi:** nome più parametri, con almeno `SampleId=...`.
- **Carrier:** si parte da **uno solo**. In seguito ce ne saranno di più, pieni o vuoti.

---

## 5. Decisioni tecniche

| Decisione | Motivazione |
|---|---|
| **Linguaggio: Rust** | Richiesta dell'utente: tool riutilizzabile e da far evolvere. |
| **Poche dipendenze: `resvg`, `dejavu`** | All'inizio crates.io era bloccato e il tool usava solo `std`. Il parser TOML resta scritto a mano; per la rasterizzazione si usa `resvg`, che rende il tool un unico eseguibile portabile (anche Windows). |
| **Parser TOML scritto a mano** (sottoinsieme) | Al posto di `serde`/`toml`. |
| **Rendering: un SVG per fotogramma** | L'SVG viene generato come stringa, così la grafica vettoriale è semplice da scrivere e scalabile. |
| **Rasterizzazione in-process con `resvg`** | Il decoder SVG integrato in ffmpeg (`svg_pipe`) **non funzionava** con più fotogrammi concatenati. In origine si usava `rsvg-convert` (un processo per fotogramma); `resvg` elimina la dipendenza di sistema ed è molto più veloce. I fotogrammi vanno a ffmpeg come RGBA grezzo (`-f rawvideo`), senza passare dal PNG. |
| **Encoding con `ffmpeg`** | I PNG vengono passati via `image2pipe` e codificati in H.264 (`libx264`, `-preset veryfast -crf 18 -pix_fmt yuv420p -movflags +faststart`). |
| **Rendering parallelo** | N thread worker (default = numero di core): ognuno genera l'SVG, lo rasterizza e invia il PNG. Il thread principale li riordina (BTreeMap) e li scrive in sequenza a ffmpeg. |
| **Spazio di coordinate fisso 1920×1080** | Lo scenario usa sempre queste coordinate. `width`/`height` del video scalano solo l'output tramite `viewBox`. |
| **Timeline deterministica calcolata in anticipo** | Ogni step ha un inizio e una fine. Il fotogramma al tempo *t* è una funzione pura di *t*, quindi è possibile renderizzare qualunque istante (`--still`). |

**Dipendenze di sistema:** solo `ffmpeg` (e solo per l'MP4). I font vengono incorporati nell'eseguibile, così il risultato è identico su ogni macchina, ma i `.ttf` non sono nel repository: **Inter** (font variabile, OFL 1.1) lo scarica `build.rs` alla prima compilazione con `curl`, da un URL del repository Google Fonts fissato a un commit, e ne verifica lo SHA-256; **DejaVu Sans Mono** arriva dal crate `dejavu`. Per compilare offline basta mettere a mano `fonts/Inter.ttf` (vedi `fonts/README.md`). I font di sistema servono solo per i glifi mancanti.

---

## 6. Struttura del progetto

```
trackviz/
├── Cargo.toml            # edition 2021, dipendenze: resvg, dejavu (build: sha2)
├── README.md             # guida rapida e riferimento del formato
├── build.rs              # scarica Inter (SHA-256 verificato) in OUT_DIR
├── fonts/                # README e licenze dei font (i .ttf non sono in git)
├── PROJECT.md            # questo documento
├── scenes/
│   ├── 01_prepare_sample_release.toml
│   ├── 02_turnstile_three_carriers.toml
│   ├── 03_clear_missing_carrier.toml
│   ├── 04_clear_missing_carrier_turnstile.toml
│   ├── 05_analyzer_error_recovery.toml
│   ├── 06_analyzer_error_recovery_turnstile.toml
│   ├── 07_carrier_runaway.toml
│   ├── 08_carrier_runaway_turnstile.toml
│   ├── 09_spur_happy_path.toml
│   ├── 10_spur_clear_missing_carrier.toml
│   ├── 11_spur_analyzer_error_recovery.toml
│   ├── 12_spur_carrier_runaway.toml
│   ├── 13_spur2_happy_path.toml
│   ├── 14_spur2_clear_missing_carrier.toml
│   ├── 15_spur2_analyzer_error_recovery.toml
│   ├── 16_spur2_carrier_runaway.toml
│   └── common/           # parti comuni incluse dalle scene: video, attori, geometrie
├── out/                  # video generati
└── src/
    ├── main.rs           # CLI, rasterizzazione resvg → ffmpeg (RGBA grezzo), thread worker
    ├── toml.rs           # parser del sottoinsieme TOML
    ├── scene.rs          # modello della scena, validazione, calcolo timeline
    ├── scene_tests.rs    # unit test di scene.rs (scene di prova minime + tutte le scene in scenes/)
    └── render.rs         # generazione dell'SVG di un fotogramma (con i suoi test)
```

### CLI

```
trackviz <scene.toml>... [-o out.mp4] [-j jobs]   # video completo (senza -o: out/<nome del .toml>.mp4)
trackviz "scenes/*.toml"                          # più scene in parallelo (* e ? nel nome del file)
trackviz <scene.toml> --still <sec> [-o f.png]    # singolo fotogramma PNG
trackviz <scene.toml> --still <sec> --svg -o f.svg
trackviz <scene.toml>... --check                  # valida e stampa la timeline
```

La timeline viene stampata su stderr (inizio/fine di ogni step) e serve come **log degli eventi** per regolare i tempi; con più scene in rendering si stampa solo una riga per scena.

**Più scene:** i caratteri jolly `*` e `?` sono espansi dal programma (solo nel nome del file), così funzionano anche tra virgolette e su Windows. I thread di `-j` sono divisi tra le scene: `min(scene, jobs)` in contemporanea, `jobs / scene` thread ciascuna, almeno 1. Una scena che fallisce non ferma le altre: gli errori sono elencati alla fine, con codice d'uscita 1. Due scene che scriverebbero lo stesso video sono un errore; `-o` e `--still` accettano una sola scena. L'help mostra versione e repository, letti da `Cargo.toml`.

---

## 7. Moduli in dettaglio

### 7.1 `toml.rs` — parser

- Supporta: commenti `#` (ignorati dentro le stringhe), `[tabella]`, `[[array_di_tabelle]]`, `chiave = valore`.
- Valori: stringhe tra doppi apici (escape `\" \\ \n \t`), numeri (int/float, `_` ammesso), `true`/`false`, array, anche su più righe (letti finché le parentesi non si bilanciano).
- Errori con **numero di riga**: chiavi duplicate, tabelle duplicate, valori malformati.
- `Table` offre accessor tipizzati (`str_opt/str_req`, `num_opt/num_req`, `bool_opt`, `str_list`) e `check_keys(allowed)`, che **rifiuta le chiavi sconosciute** (protegge dagli errori di battitura).

### 7.2 `scene.rs` — modello e timeline

**Modello:** `Video`, `Point`, `Lane` (Main/Pit/Link), `Device` (Nsd/Sampling/Analyzer/Zone), `Actor`, `Carrier`, `Step` con `Action`.

**Validazioni:** riferimenti a point/actor/device/carrier esistenti, punto `start` sulla route, target del `move` **più avanti** sulla route del carrier, carrier che non può ricevere un nuovo move mentre è ancora in movimento, `pipette` solo su device `sampling` o `turnstile` e solo se esiste un `analyzer`, route che attraversano un turnstile solo come `<id>.in`, `<id>.out` consecutivi, regole della ruota (vedi sotto), `wait` con `duration` obbligatoria, niente messaggi di un attore verso sé stesso.

**Calcolo dei tempi degli step** (in ordine di file):
- `start = "after"` (default): inizia alla fine dello step precedente.
- `start = "with"`: inizia insieme allo step precedente.
- `start = "after_all"`: inizia quando tutti gli step precedenti sono finiti.
- A questi si somma `delay`. Il primo step parte da 0 (+ delay).
- Senza nessuna chiave di tempo (`start`, `at`, `after_start`, `after_end`) vale `after`: gli step sono in sequenza, non contemporanei. Per farli partire insieme serve `start = "with"` (o `after_start = 0`). `caption` e `status` durano 0 s, quindi lo step che li segue parte nello stesso istante.

**Durate di default:** caption 0 (resta visibile fino alla caption successiva), message 1,5 s (tempo di volo della freccia), note 1,5 s, pipette 3 s, highlight 1,5 s, rotate 2 s, move calcolata (vedi sotto).

**Turnstile:** il device genera i punti `<id>.in`, `<id>.needle`, `<id>.out` (posizioni fisse delle tre posizioni). Un `move` che termina in `<id>.in` aggancia il carrier alla tasca in ingresso; da lì la sua posizione segue l'angolo della ruota (`Timeline::carrier_pos`); un `move` successivo lo sgancia dall'uscita. Le tasche vengono assegnate e controllate **in ordine di tempo** dopo aver disposto tutti gli step (`simulate_wheels`), perché l'ordine nel file non coincide sempre con l'ordine temporale. Errori: ingresso occupato, uscita da una posizione diversa dall'uscita, rotazione con un carrier ancora in uscita, arrivi o uscite durante una rotazione, rotazione mentre un carrier fuori dalle tasche (appena uscito o in arrivo) è ancora dentro il cerchio della ruota (distanza dal centro < r + bordo 32 px + raggio del carrier 22 px). In caso di errore `--check` stampa anche la timeline calcolata fino a quel punto. Un carrier che entra nella ruota (dal gate alla tasca d'ingresso) mentre è in corso un `pipette` su quel turnstile è un errore: deve aspettare al gate.

**Gate e antenna** (`gate`, `antenna`, con `at`): nessuno step dedicato. Il gate è aperto negli intervalli in cui un carrier lo attraversa (calcolati in `Timeline::gates` proiettando il punto del gate sulla route di ogni carrier: da 60 px prima a 45 px dopo, con apertura/chiusura di 0,4 s). L'antenna mostra la lettura quando un carrier è fermo sul suo punto.

**`at`, `after_start`, `after_end`:** uno step con `at = <secondi>` parte a un istante assoluto; con `after_start = <secondi>` parte quei secondi dopo l'**inizio** dello step precedente nel file (equivale a `start = "with"` + `delay`), con `after_end = <secondi>` dopo la sua **fine** (equivale a `start = "after"` + `delay`). A tutti si somma `delay`. Si può usare una sola tra `at`, `after_start`, `after_end` e `start`; valori negativi di `after_*` sono rifiutati.

**Movimento dei carrier:**
- La route è una polilinea di punti con le distanze cumulative.
- Ogni `move` genera un segmento `(t0, t1, s0, s1, ease)` sull'ascissa curvilinea *s*.
- Easing: `linear`, `in` = 1−cos(πx/2), `out` = sin(πx/2), `inout` = (1−cos πx)/2.
- Durata del move = lunghezza / speed × fattore, con fattore 1 per linear e **π/2 per gli altri**. In questo modo la **velocità di picco è uguale a `speed`**, e un move `in` seguito da un move `out` ha velocità continua nel punto di giunzione. È così che si rappresenta il passaggio **senza fermata** dall'NSD.

**Carrier fantasma** (`ghost = true`): posizione logica di un carrier (dove l'Interfaccia crede che sia) mentre fisicamente è altrove. Non apre i gate e non viene letto dalle antenne. `visible = false` lo nasconde all'inizio; `show`/`hide` lo fanno comparire o sparire in dissolvenza (`CarrierPath::opacity`).

**Stato dell'analizzatore e delle zone:** `status` vale anche per le zone (es. "Interface module": rossa in errore, verde in ready, grigia in normal). `status` colora l'analizzatore (error: rosso con "ERROR"; ready: verde con "READY"; pulsa per 1,5 s al cambio). `needle` porta giù o su l'ago e lo lascia lì; con l'ago giù e lo strumento non in errore compare "Aspirating…", in errore l'ago diventa rosso con "Needle down". `--check` rifiuta una rotazione della ruota, o un carrier che lascia il punto di campionamento, tra l'inizio di un `needle down` e la fine del `needle up` successivo.

**Operatore** (`operator`): operatore di laboratorio stilizzato (camice bianco lungo, guanti blu sottili, capelli scuri, chiave inglese) che entra a piedi dal bordo destro del pannello, si ferma a destra dell'analizzatore, lavora (braccio che oscilla e scintille ambra sul bordo) e se ne va. Ogni camminata dura min(2 s, durata/3).

**Righe del sequence diagram:** ogni `message` e ogni `note` occupa una riga, assegnata nell'ordine del file.

**Segnaposto:** `{SampleId}` e `{CarrierId}` in `text`/`params` vengono sostituiti con i valori del carrier indicato in `carrier`, o del primo carrier se non è indicato.

**Durata totale** = fine dell'ultimo step + `tail`.

### 7.3 `render.rs` — fotogramma SVG

**Layout fisso** (coordinate 1920×1080):

| Zona | Rettangolo |
|---|---|
| Header (titolo + sottotitolo + orologio) | y 0–110 |
| Pannello TRACK · TOP VIEW | x 30, y 120, w 920, h 765 (con clipPath) |
| Pannello MESSAGE SEQUENCE | x 970, y 120, w 920, h 765 |
| Banda delle didascalie | x 30, y 905, w 1860, h 150 |

**Elementi del track** (in ordine di disegno):
1. Zone (`zone`): rettangolo tratteggiato con etichetta, per esempio "Interface module".
2. Corsie: prima i link, poi la Pit-Lane, poi la Main-Lane. Doppia polilinea (bordo `#AEB8C5` largo 40, interno `#DDE3EA` largo 34) e chevron di direzione ogni ~70 px.
3. Analizzatore: rettangolo grigio con etichetta.
4. NSD: riquadro 72×64 sul punto, con etichetta sotto. Durante un `highlight` diventa ambra e pulsa.
5. Punto di campionamento: cerchio rosso tratteggiato con etichetta sotto.
   Gate: paletto sopra la corsia e sbarra rossa/bianca che attraversa la corsia; aprendosi si accorcia (vista dall'alto della sbarra che si alza). Antenna: lettore sopra la corsia con due archi per lato, che pulsano in blu con una linea tratteggiata verso il carrier durante la lettura.
   Turnstile: disco grigio con tre tasche e razzi che ruotano (ease in-out), mozzo centrale, posizione sotto l'ago segnata da un cerchio rosso tratteggiato fisso, etichetta sotto la ruota. Con la pipetta sul turnstile, "Aspirating…" va a sinistra del braccio.
6. Carrier: prima i fantasmi (cerchio tratteggiato r 26 semitrasparente, tappo tratteggiato, cartellino tratteggiato in corsivo), poi i carrier reali, poi tutti i cartellini. Carrier reale: disco scuro (r 22) con ombra. Se ha il tubo, cerchio bianco con tappo colorato (`cap_color`); se è vuoto, alloggiamento scuro. Accanto c'è il **cartellino con il SampleId** (o `tag`, o "C1 (empty)"): a destra se c'è spazio, altrimenti a sinistra, sotto o sopra, scegliendo la prima posizione che non copre altri carrier o cartellini. Nessun cartellino per i carrier fuori dal pannello.
7. Pipetta (durante `pipette`): braccio che esce dal lato dell'analizzatore più vicino al punto di campionamento (il fondo se l'analizzatore è sopra la corsia, un fianco nello spur) fino al tubo (20% discesa, 60% aspirazione, 20% risalita). Durante l'aspirazione compaiono onde blu e la scritta "Aspirating…"; con l'ago di lato "Aspirating…" e "Needle down" sono scritti dentro l'analizzatore. I cartellini dei carrier non coprono mai l'analizzatore. Se a destra dell'analizzatore non c'è spazio, l'operatore lavora da sotto (braccio alzato).

**Sequence diagram:**
- Riquadri degli attori distribuiti uniformemente, con la banda superiore del colore dell'attore. Linee della vita tratteggiate.
- Messaggio: la freccia cresce dalla sorgente alla destinazione durante la sua durata (ease-out). Il colore è quello del mittente. Le risposte (`reply`) sono tratteggiate con punta aperta.
- Etichetta su una riga sopra la freccia: **nome** in grassetto più `params` in monospazio grigio, su un rettangolo bianco che copre le linee della vita. Si usa il rettangolo invece di `paint-order` (non supportato da librsvg, il primo rasterizzatore usato).
- Un messaggio "fresco" (fino a 1,5 s dopo l'arrivo) ha la freccia più spessa e un alone ambra.
- Note: box giallo centrato sulla linea della vita.
- A sinistra di ogni riga, in grigio, l'istante di inizio (`show_times`).
- Altezza delle righe = spazio disponibile / numero di righe, massimo 70 px. Se verrebbe sotto 38 px, le righe sono alte 46 px e il diagramma **scorre** verso l'alto: ogni nuova riga è visibile quando compare (scroll di 0,6 s che termina all'inizio della riga). Le righe sono ritagliate sotto i riquadri degli attori.

**Didascalie:** l'ultima caption iniziata resta visibile, con fade-in di 0,4 s. Il titolo è maiuscolo in ambra, il testo bianco 27 px va a capo a ~118 caratteri, massimo 2 righe con titolo e 3 senza.

**Palette:** sfondo `#F1F4F8`, testo `#0F172A`, grigio `#64748B`, accento `#F59E0B`. Attori: Interface `#2563EB`, Analyzer `#EA580C`, DMX `#059669`. Tappo del tubo `#7C3AED`.

---

## 8. Formato delle scene (riferimento)

Una scena può iniziare con `include = ["common/video.toml", ...]`: percorsi relativi al file che include, inclusioni annidate ammesse. Le sezioni `[[...]]` incluse vengono prima di quelle della scena; le chiavi di `[video]` si fondono e vince la scena. Sono errori gli id duplicati (point, lane, device, actor, carrier) e le inclusioni circolari; gli errori in un file incluso ne riportano il nome. Le parti comuni stanno in `scenes/common/`, così `"scenes/*.toml"` prende solo le scene. Le scene 03, 07 e 08 hanno geometrie uniche e le tengono nel proprio file.

| Sezione | Chiavi |
|---|---|
| `[video]` | `title`, `subtitle`, `width` (1920), `height` (1080), `fps` (30), `tail` (2), `show_clock` (true), `show_times` (true) |
| `[[point]]` | `id`, `x`, `y` |
| `[[lane]]` | `id`, `label`, `kind` = main\|pit\|link, `points` = [id…], `label_x`, `label_y` |
| `[[device]]` | `id`, `kind` = nsd\|sampling\|divert (richiedono `at`; `label_x`, `label_y` opzionali per spostare l'etichetta; ogni `sampling` è un ago dell'analizzatore) \| analyzer\|zone (richiedono `x`,`y`,`w`,`h`) \| turnstile (`x`,`y` centro, `r` (70), `entry` = right\|left (right)) \| gate\|antenna (richiedono `at`), `label` |
| `[[actor]]` | `id`, `label`, `sub`, `color` |
| `[[carrier]]` | `id`, `tube` (true), `sample_id`, `route` = [id…], `start` (primo punto della route), `speed` (100 px/s), `cap_color`, `ghost` (false), `visible` (true), `tag` (SampleId) |
| `[[step]]` | `action`, uno tra `start` = after\|with\|after_all, `at` (secondi assoluti), `after_start` / `after_end` (secondi dopo l'inizio / la fine dello step precedente), `delay`, `duration` + le chiavi specifiche dell'azione |

| action | Chiavi specifiche |
|---|---|
| `caption` | `title`, `text` |
| `move` | `carrier`, `to`, `ease` = linear\|in\|out\|inout (inout), `speed`, `runaway` (false: carrier che scappa, anello rosso pulsante e cartellino "… · RUN-AWAY" durante il movimento) |
| `message` | `from`, `to`, `text`, `params`, `reply` (false), `carrier`, `color` (colore del mittente) |
| `note` | `actor`, `text` |
| `pipette` | `device` sampling o turnstile |
| `highlight` | `device` |
| `rotate` | `device` (turnstile) |
| `show` / `hide` | `carrier` (dissolvenza, default 0,5 s) |
| `status` | `device` (analyzer, zone o sampling = un solo ago), `state` = error\|ready\|normal (vale fino al `status` successivo) |
| `needle` | `device` (sampling o turnstile), `to` = down\|up (default 0,9 s) |
| `operator` | `device` (analyzer); default 7 s, minimo 2 s |
| `wait` | (solo `duration`) |

---

## 9. Scena 01 — `01_prepare_sample_release.toml`

**Geometria:** flusso dei carrier da destra verso sinistra, Pit-Lane in alto (sopra la Main-Lane).

| Punto | x | y |
|---|---|---|
| main_in | 910 | 740 |
| nsd | 730 | 740 |
| pit_in | 620 | 540 |
| sampling | 440 | 540 |
| pit_out | 290 | 540 |
| merge | 180 | 740 |
| main_out | 65 | 740 |

- **Corsie:** Main-Lane [main_in, nsd, merge, main_out], con etichetta in (830, 800). Pit-Lane [pit_in, sampling, pit_out], con etichetta in (250, 500). Link "divert" [nsd, pit_in] e link "return" [pit_out, merge].
- **Device:** zona "Interface module" (90, 425, 720×420), Analyzer (200, 185, 480×175), NSD su `nsd`, "Sampling point" su `sampling`.
- **Carrier:** C1 con tubo, SampleId `SID-000123` (segnaposto), route completa, speed 75.
- **Video:** 1920×1080, 30 fps, tail 3,5 s. **Durata totale ≈ 47,7 s.**

**Timeline risultante:**

| t (s) | Evento |
|---|---|
| 0,0 | Caption 1 · Main-Lane |
| 1,0 – 4,8 | C1 → nsd (ease in) |
| 4,8 | Caption 2 · NSD — Prepare. Highlight NSD (2 s) |
| 4,8 – ~13 | C1 → sampling (ease out, senza fermarsi all'NSD) |
| 4,8 | **IF → AN Prepare** (SampleId) |
| 6,8 | AN → IF OK (reply) |
| ~10,1 | Caption 3 · Test query |
| 10,4 | **AN → DMX Query** |
| 12,4 | DMX → AN Tests to perform (reply) |
| ~14,5 | Caption 4 · Sampling point — Sample Acquisition |
| 14,8 | **IF → AN Sample Acquisition** |
| ~18,3 | Caption 5 · Pipetting. Pipetta 4,5 s + nota "Pipetting" su AN |
| ~23,1 | Caption 6 · Sample Complete |
| 23,4 | **AN → IF Sample Complete** |
| ~27,7 | Caption 7 · Carrier released. C1 → merge (ease in) |
| ~36,0 | Caption 8 · Main-Lane — Release. C1 → main_out (ease out) |
| 36,0 | **IF → AN Release** |
| ~41 | Caption 9 · Results. Nota "Analysis" su AN |
| 42,7 | **AN → DMX Results** |

*(I valori esatti si ottengono con `--check`.)*

**Testi delle didascalie (in inglese):**
1. *Main-Lane* — A carrier holding a sample tube travels along the Main-Lane towards the analyzer interface.
2. *Diverted — Prepare* — The carrier is diverted into the Pit-Lane without stopping. At this moment the interface sends Prepare to the analyzer.
3. *Test query* — After Prepare, the analyzer queries DMX (LIS) to know which tests must be performed on this sample.
4. *Sampling point — Sample Acquisition* — The carrier stops at the sampling point. The interface sends Sample Acquisition: the tube is in position and can be aspirated.
5. *Pipetting* — The analyzer aspirates the biological material from the tube. Meanwhile the carrier is held at the sampling point.
6. *Sample Complete* — When aspiration is finished the analyzer sends Sample Complete. Only after receiving it does the interface release the carrier.
7. *Carrier released* — The interface releases the carrier: it leaves the Pit-Lane and heads back to the Main-Lane.
8. *Main-Lane — Release* — As soon as the carrier with the tube is back on the Main-Lane, the interface sends Release: the sample has left the analyzer interface.
9. *Results* — Later, when the analysis is complete, the analyzer sends the results to DMX.

---

## 9b. Scena 02 — `02_turnstile_three_carriers.toml`

Turnstile (`ts`, centro 440,505, r 70, ingresso a destra) al posto del punto di campionamento, con **antenna** (`pit_wait`, x 640) e **gate** (x 596) davanti. Tre carrier pieni (SID-000123/124/125, tappi viola, azzurro, rosa). C2 e C3 partono da un punto fuori dal pannello (`spawn`, x 1010) e tutti escono verso `gone` (x −40). Per ogni carrier: Prepare/OK all'NSD, Query/Tests to perform con DMX, **secondo Prepare (viola) all'antenna**, Sample Acquisition sotto l'ago, Sample Complete, Release sul rientro. C1 trova la ruota libera: sosta all'antenna, entra, la ruota gira. C2 e C3 aspettano al gate chiuso durante il pipettaggio del carrier precedente; dopo il Sample Complete entrano e solo allora la ruota gira. L'ultima rotazione avviene senza carrier in attesa. Messaggi, didascalie e rotazioni usano `at`. Durata ≈ 68 s, 27 righe nel sequence diagram (scorre).

## 9c. Scene 03 e 04 — carrier atteso che non arriva: Clear

Stessa situazione in due geometrie: 03 con il punto di campionamento semplice (geometria della scena 01, con un punto di coda `queue` 60 px prima del campionamento), 04 con turnstile, gate e antenna (geometria della scena 02).

- C1 e C3 vengono deviati nella Pit-Lane: Prepare/OK, Query/Tests to perform.
- C2 si presume deviato (Prepare/OK, Query/Tests to perform) ma **fisicamente va dritto** sulla Main-Lane ed esce. La sua posizione logica è un **fantasma** (`C2L`) che compare all'NSD e segue la Pit-Lane.
- C3 si ferma **sopra** il fantasma di C2 (coda nella 03, antenna nella 04).
- **03:** dopo il Sample Complete di C1 fantasma e C3 avanzano insieme al punto di campionamento; l'Interfaccia manda **Clear** (SampleId di C2), l'Analizzatore risponde **OK** (il fantasma sparisce), poi Sample Acquisition di C3.
- **04:** appena l'antenna legge C3 (C1 è ancora in pipettaggio) l'Interfaccia manda **Clear** di C2 → **OK** (il fantasma sparisce) → **Prepare viola** di C3. C3 aspetta al gate il Sample Complete di C1, entra, la ruota gira, Sample Acquisition di C3.
- Per C2 non c'è Release.
- Durate: 03 ≈ 54 s, 04 ≈ 56 s.

## 9d. Scene 05 e 06 — errore dello strumento con l'ago giù, recovery

Un solo carrier; 05 con il punto di campionamento semplice (geometria della scena 01), 06 con turnstile, gate e antenna (geometria della scena 02).

- Flusso normale fino alla Sample Acquisition (nella 06 anche il Prepare viola all'antenna, l'ingresso e la rotazione).
- Durante l'aspirazione l'analizzatore va in errore lasciando l'ago nel tubo: diventa rosso e manda subito **Status `State=Error`** e **Sample Complete** (con SampleId); l'Interfaccia risponde **OK** allo Status (senza parametri). Il Sample Complete **non** basta per rilasciare: con lo strumento in errore il carrier resta fermo (nella 06 la ruota non gira).
- L'operatore arriva allo strumento (azione `operator`, 7 s, insieme alla nota "Operator recovery") ed esegue la recovery: mentre lavora l'ago risale.
- Lo strumento torna verde e manda **Status `State=Ready`**; l'Interfaccia risponde **OK** e solo ora rilascia il carrier (nella 06 prima la ruota lo porta all'uscita).
- Sulla Main-Lane l'Interfaccia manda **Release**.
- I messaggi Status e i relativi OK non hanno SampleId (è lo stato dello strumento).
- Durate: 05 ≈ 45 s, 06 ≈ 51 s.

## 9e. Scene 07 e 08 — carrier run-away durante il campionamento

Tre carrier deviati (Prepare/OK, Query/Tests to perform per ciascuno); 07 con il punto di campionamento semplice (geometria della scena 03 con due posizioni di coda, `queue` e `queue2`), 08 con turnstile, gate e antenna (geometria della scena 02 con una posizione di coda `pit_q` dietro l'antenna).

- **07:** Sample Acquisition per C1; **prima che l'ago scenda** C1 scappa (`runaway = true`) ed esce fino alla Main-Lane. C2 scivola sotto l'ago, C3 avanza in coda. L'analizzatore aspira C2 credendo che sia C1.
- **08:** C2 aspetta al gate (Prepare viola all'antenna), C3 dietro di lui. Dopo la Sample Acquisition per C1 la ruota **gira da sola**: C1 va all'uscita e scappa verso la Main-Lane. L'ago scende nella tasca vuota e non aspira nulla.
- In entrambe l'analizzatore manda **Sample Complete** con il SampleId di C1; l'Interfaccia manda **Cancel** (SampleId di C1) → **OK**, poi manda via tutti i carrier senza campionarli (nella 08 C2 e C3 passano dalla ruota: entrano, rotazioni, escono; C3 si ferma all'antenna ma non riceve Prepare). Quando l'ultimo è sulla Main-Lane: **Clear All** (senza parametri) → **OK**. Nessun Release.
- L'**interfaccia** (zona "Interface module") diventa **rossa** al momento della fuga (07: quando C1 lascia il punto di campionamento; 08: quando la ruota gira da sola) e torna **grigia** quando l'ultimo carrier è uscito completamente dal riquadro.
- Nella 08 l'ultima rotazione parte solo quando C2 è uscito completamente dal cerchio della ruota.
- Durate: 07 ≈ 48 s, 08 ≈ 60 s.

## 9f. Scene 09–12 — spur

Geometria comune: Main-Lane a y 760 (NSD a x 800, rientro a x 290); deviazione diagonale e curva di 90° nel ramo d'andata (x 660), con due posizioni di coda (`queue2` y 560, `queue` y 500) sotto il punto di campionamento (y 440); analizzatore a destra (x 730–930, y 330–550); traversa in cima (y 250) e ramo di ritorno (x 380). Velocità dei carrier 110 px/s (lo spur è più lungo); l'uscita dal punto di campionamento è un move `in` fino alla cima, `linear` fino al rientro e `out` fino all'uscita, senza fermate.

- **09** happy path (≈ 36 s): come la 01.
- **10** carrier mancante → Clear (≈ 51 s): come la 03.
- **11** errore dello strumento con l'ago giù e recovery (≈ 46 s): come la 05; l'operatore lavora sotto l'analizzatore.
- **12** carrier run-away → Cancel, Clear All (≈ 48 s): come la 07, con l'interfaccia rossa dalla fuga finché tutti i carrier ne sono usciti.

## 9g. Scene 13–16 — spur con un analizzatore e due punti di campionamento

Un solo strumento con due aghi: un solo attore Analyzer e un solo device `analyzer` a destra dei due punti di campionamento. Ramo d'andata a x 440 con due deviazioni standard (`div1` a y 650, `div2` a y 410). Da ognuna parte verso destra una pit-lane che sale passando dal punto di campionamento (x 600) e rientra nel ramo **sopra** la deviazione; chi esce dall'ago 1 si ferma quindi anche a `div2`. Velocità 120 px/s.

- **Messaggi:** Prepare all'NSD e di nuovo (viola) a ogni deviazione che devia il carrier; alle altre il carrier è solo letto. Una Sample Acquisition per ogni ago. `Needle=n` compare in Sample Acquisition, Sample Complete e Status. Un solo Release per carrier. Clear, Cancel e Clear All non hanno `Needle`.
- **Quali aghi** dipende dai test del campione: uno solo o entrambi.
- Le deviazioni standard sono etichettate "Divert 1" e "Divert 2". Un carrier può entrare in una pit-lane mentre l'ago sta campionando il carrier precedente. Soste ai divert brevi: 0,6 s con il Prepare viola, 0,4 s se il carrier è solo letto (il carrier non aspetta l'arrivo della freccia).
- **13** happy path (≈ 51 s): C1 usa entrambi gli aghi, C2 solo l'ago 2; C1 entra nella seconda pit-lane mentre C2 è in campionamento.
- **14** Clear (≈ 51 s): il fantasma di C2 si ferma a `div1`, C3 ci si ferma sopra; alla lettura di C3 Clear di C2 → OK → Prepare viola di C3.
- **15** errore (≈ 50 s): errore **dell'ago 1** (`status` sul device `sampling`: diventa rossa solo la sua fascia dell'analizzatore), l'ago 2 continua durante la recovery.
- **16** run-away (≈ 55 s): C1 scappa dall'ago 1 e si ferma comunque a `div2`; il carrier dopo finisce sotto l'ago. Via solo i carrier della pit-lane dell'ago 1, che è rossa dalla fuga finché non si è svuotata. L'ago 2 continua.

---

## 10. Assunzioni da confermare (scelte non specificate dall'utente)

| # | Assunzione | Stato |
|---|---|---|
| A1 | Nomi dei messaggi verso DMX: "Query", "Tests to perform", "Results" | Da sostituire con i nomi reali |
| A2 | L'OK al Prepare arriva **prima** della query a DMX | Da confermare |
| A3 | Sample Acquisition, Sample Complete e Release **non hanno risposta** | Da confermare |
| A4 | Release parte quando il carrier raggiunge il punto di rientro (`merge`) | Coerente con la descrizione, da confermare |
| A5 | I risultati a DMX sono mostrati alla fine, con la didascalia "Later…" | Scelta narrativa |
| A6 | SampleId `SID-000123` e durata della pipettatura 4,5 s | Segnaposto |
| A7 | Messaggi di stato dell'analizzatore omessi | Da aggiungere |
| A8 | Il secondo Prepare (all'antenna) non ha risposta OK; l'informazione in più che porterà è ancora da definire | Da confermare |
| A10 | L'Analizzatore manda una Query a DMX alla volta: la successiva solo dopo aver ricevuto la risposta alla precedente (regola dell'utente, verificata da un test su tutte le scene). La Sample Acquisition di un campione parte dopo che i suoi test sono noti | Confermata la prima parte |
| A9 | La risposta dell'Analizzatore al Clear si chiama "OK" (scene 03/04) | Scelta dell'utente |

---

## 11. Roadmap (evoluzioni concordate o previste)

1. **Risposta negativa al Prepare** (campione da non processare): ramo alternativo, con il carrier che attraversa la Pit-Lane senza campionamento.
2. **Messaggi di stato**: entrambe le modalità, su richiesta dell'Interfaccia o spontanea al cambio di stato.
3. **Più carrier**, pieni e vuoti, con code nella Pit-Lane. Servirà la gestione delle collisioni o delle distanze minime. (La scena 02 ha tre carrier pieni con il turnstile; code e carrier vuoti restano da fare.)
4. ~~Scroll del sequence diagram~~ — fatto.
5. **Versione realistica** del video, se lo schematico non convince.

---

## 12. Limiti noti e note per chi riparte

- **Test:** `cargo test` esegue gli unit test del parser (`toml.rs`), della timeline (`scene_tests.rs`: tempi di partenza, movimenti, messaggi, fantasmi, turnstile, ago, stato, operatore e la validità di tutte le scene in `scenes/`) e del disegno (`render.rs`). Vanno aggiornati quando si aggiunge un'azione o una regola.

- Il parser TOML è un sottoinsieme: niente tabelle inline `{}`, chiavi puntate, date o stringhe multilinea `"""`. Se la rete lo permette, si possono adottare `serde` + `toml`.
- La larghezza dei testi è **stimata** dal numero di caratteri (font proporzionale), quindi le etichette molto lunghe possono uscire un po' dai box.
- Con `resvg` la scena 01 (1432 fotogrammi) si genera in ~9 s su un Mac a 10 core.
- La pipetta usa sempre il primo device `analyzer` della scena.
- `ffmpeg -f svg_pipe` è stato provato e **non funziona** (errori di decodifica con più SVG concatenati): non riprovarlo.
- In Rust attenzione alle raw string: gli SVG contengono `"#RRGGBB"`, quindi vanno usati delimitatori `r##"…"##`.
