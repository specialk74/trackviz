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
| **NSD** | **No Stop Divert**: il dispositivo che sposta il carrier, pieno o vuoto, dalla Main-Lane alla Pit-Lane **senza fermarlo**. Il termine è solo interno: **nei video non compare** (il riquadro non ha etichetta e le didascalie dicono "diverted without stopping"). Lo verifica un test su tutte le scene; nei file di scena restano `kind=nsd` e l'id `nsd`. |
| **Pit-Lane** | Corsia secondaria, parallela alla Main-Lane, in cui il carrier viene fermato per farlo campionare dall'analizzatore. Dopo il punto di campionamento rientra nella Main-Lane, così i carrier processati (pieni o vuoti) tornano nella corsia principale. |
| **Punto di campionamento** | Posizione sulla Pit-Lane in cui il carrier si ferma e l'analizzatore aspira (pipetta) il campione. |
| **Turnstile** | Ruota con **3 tasche** a 120° alla fine della Pit-Lane. Posizioni: **ingresso** (all'altezza della Pit-Lane), **sotto l'ago** dell'analizzatore (in alto: il campionamento avviene qui), **uscita** (all'altezza della Pit-Lane, dal lato opposto, verso la Main-Lane). Ogni rotazione porta ingresso → ago → uscita. Quando il carrier con il tubo arriva sotto l'ago, l'Interfaccia manda Sample Acquisition. Dopo ogni Sample Complete la ruota gira: il carrier in ingresso va sotto l'ago e quello campionato va verso l'uscita. A ruota vuota, appena il primo carrier è in ingresso la ruota gira subito. |
| **Gate + antenna** | Davanti alla ruota. Il carrier si ferma **sempre** all'antenna, che lo legge: l'Interfaccia manda **di nuovo Prepare** (in futuro con un'informazione in più; per ora è disegnato in viola). Se sotto l'ago non c'è un campionamento in corso il gate si apre e il carrier entra nella tasca d'ingresso; altrimenti aspetta il Sample Complete. Solo dopo che il carrier in attesa è entrato la ruota gira (in ingresso → ago, campionato → uscita). |
| **Spur** | Ramo a U **perpendicolare** alla Main-Lane (come nell'NCCLS). L'NSD devia il carrier, senza fermarlo, in un breve tratto di pit-lane che gira di 90° nel ramo d'andata; sul ramo d'andata c'è il punto di campionamento con l'**analizzatore alla sua destra**; in cima il carrier attraversa e scende dal ramo di ritorno fino alla Main-Lane. |
| **Deviazione standard** | Deviazione usata dentro lo spur con più punti di campionamento: a differenza dell'NSD **ogni** carrier si ferma e viene letto, poi viene deviato nella pit-lane di un ago o prosegue dritto, mentre il successivo si ferma dietro. Vale anche per chi rientra da un ago e incontra la deviazione successiva. |
| **Clear All** | Messaggio dell'Interfaccia senza parametri: vale per tutto lo strumento e **cancella tutti i Prepare**. Non interrompe mai un campionamento in corso. |
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
8. Quando il carrier **esce dal modulo dell'Interfaccia**, l'**Interfaccia manda Release** all'Analizzatore.
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
| **Poche dipendenze: `resvg`, `dejavu`** | All'inizio crates.io era bloccato e il tool usava solo `std`. La lettura delle scene resta scritta a mano; per la rasterizzazione si usa `resvg`, che rende il tool un unico eseguibile portabile (anche Windows). |
| **Scene in Mermaid** | Le scene sono sequence diagram Mermaid: i messaggi si leggono come diagramma, il lato fisico sta nei commenti `%%`, i dati (punti, lane, device, attori, messaggi, video) in righe `%% @<sezione> chiave=valore`. Nessun altro formato di file. |
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
├── doc/IMPROVEMENTS.md   # miglioramenti proposti e punti da confermare
├── build.rs              # scarica Inter (SHA-256 verificato) in OUT_DIR
├── fonts/                # README e licenze dei font (i .ttf non sono in git)
├── PROJECT.md            # questo documento
├── scenes/
│   ├── 01_prepare_sample_release.mmd
│   ├── 02_turnstile_three_carriers.mmd
│   ├── 03_clear_missing_carrier.mmd
│   ├── 04_clear_missing_carrier_turnstile.mmd
│   ├── 05_analyzer_error_recovery.mmd
│   ├── 06_analyzer_error_recovery_turnstile.mmd
│   ├── 07_carrier_runaway.mmd
│   ├── 08_carrier_runaway_turnstile.mmd
│   ├── 09_spur_happy_path.mmd
│   ├── 10_spur_clear_missing_carrier.mmd
│   ├── 11_spur_analyzer_error_recovery.mmd
│   ├── 12_spur_carrier_runaway.mmd
│   ├── 13_spur2_happy_path.mmd
│   ├── 14_spur2_clear_missing_carrier.mmd
│   ├── 15_spur2_analyzer_error_recovery.mmd
│   ├── 16_spur2_carrier_runaway.mmd
│   ├── 17_mermaid_spur_happy_path.mmd
│   └── common/           # parti comuni .mmd: video, attori, messaggi, geometrie, direttive
├── out/                  # video generati
└── src/
    ├── main.rs           # CLI, rasterizzazione resvg → ffmpeg (RGBA grezzo), thread worker
    ├── doc.rs            # dati della scena: tabelle, valori, lettura delle righe @
    ├── physics.rs        # fisica dei carrier: code a contatto, incroci, precedenze
    ├── mermaid.rs        # lettura delle scene Mermaid
    ├── lint.rs           # avvisi su scene valide (carrier sovrapposti, didascalie tagliate, ...)
    ├── scene.rs          # modello della scena, validazione, calcolo timeline
    ├── scene_tests.rs    # unit test di scene.rs (scene di prova minime + tutte le scene in scenes/)
    └── render.rs         # generazione dell'SVG di un fotogramma (con i suoi test)
```

### CLI

```
trackviz <scene.mmd>... [-o out.mp4] [-j jobs]   # video completo (senza -o: out/<nome della scena>.mp4)
trackviz "scenes/*.mmd"                          # più scene in parallelo (* e ? nel nome del file)
trackviz <scene.mmd> --still <sec> [-o f.png]    # singolo fotogramma PNG
trackviz <scene.mmd> --still <sec> --svg -o f.svg
trackviz <scene.mmd>... --layout                 # solo out/<nome>_layout.png: punti e device con i nomi
trackviz <scene.mmd>... --check                  # valida e stampa la timeline
trackviz <scene.mmd> --preview [--from S] [--to S] [--watch]   # anteprima veloce
trackviz <flusso.mmd>                             # le scene possono essere diagrammi Mermaid
```

**Avvisi** (`lint.rs`, stampati con `--check` e in generazione, non bloccano e il codice d'uscita resta 0): carrier fisici sovrapposti, didascalie tagliate e Query prima dell'OK al suo Prepare o della risposta alla precedente. Un test verifica che nessuna scena ne abbia.

La timeline viene stampata su stderr (inizio/fine di ogni step) e serve come **log degli eventi** per regolare i tempi; con più scene in rendering si stampa solo una riga per scena.

**Scene in Mermaid:** una scena è un sequence diagram Mermaid (`.mmd`) e ogni comando lo accetta (`trackviz scena.mmd`, `--preview`, `--watch`, `--check`, caratteri jolly). `src/mermaid.rs` legge il diagramma e costruisce direttamente i dati della scena (`src/doc.rs`), che passano poi al modello (`scene.rs`). Mermaid descrive solo i messaggi: geometria, carrier, movimenti, didascalie e tempi stanno nei commenti `%%`. Una direttiva sconosciuta è un errore, `%% # testo` è un commento, gli errori indicano file e riga. Gli step sono in sequenza, a meno di `when`, `with`, `at`, `delay` prima di essi. I messaggi diventano messaggi con nome se esiste il tipo, il secondo Prepare di un campione è quello viola, `alt`/`loop`/`opt`/`par` sono rifiutati perché una scena è un solo percorso.

**Righe `@`:** `%% @<sezione> chiave=valore ...` descrive una tabella di dati (`point`, `lane`, `device`, `actor`, `message`, `carrier`, `step`, `video`). Valori: `true`/`false`, numeri, liste separate da virgole (`points=a,b,c`), testo tra virgolette se ha spazi (con `\"` e `\\`), oppure una parola. Le righe `@video` aggiungono chiavi all'unica tabella video (vince l'ultima); le altre sezioni sono liste nell'ordine del file. Le direttive producono le stesse tabelle.

**Quando parte uno step:** ogni step parte quando finisce il precedente, a meno di una direttiva subito prima: `%% on <antenna> [<carrier>]` quando il carrier è sopra l'antenna (senza carrier quello del `SampleId` del messaggio, o quello che lo step muove o mostra; per didascalie, highlight, pipette, note il carrier va scritto); `%% after <secondi>` quei secondi dopo la fine dello step precedente (un messaggio finisce quando arriva); `%% after <secondi> <messaggio> [<carrier>]` quei secondi dopo l'arrivo dell'ultimo messaggio con quel testo (per quel carrier); `%% after <secondi> <antenna> <carrier>` quei secondi dopo che il carrier è sopra l'antenna; `%% after <secondi> pipette|rotate|needle <device>` quei secondi dopo la fine di quell'azione; `%% at <secondi>` a un istante del video. Una sola tra `on`, `after`, `at` per step. `%% color #RRGGBB` colora il messaggio successivo (il Prepare viola è `#9333EA`). Il testo del messaggio resta quello scritto. Le direttive `when`, `with`, `delay`, `after_start`, `after_end` non esistono più (errore che indica le nuove); tutte le scene usano `on`/`after`/`at`. Nel motore `on` diventa `when <carrier> at <punto>`, `after` con un riferimento diventa `when ...` + `delay`, `after` semplice `after_end`.

**Conversione delle scene 01–16:** fatta con uno script dalla timeline precedente. Uno step che partiva alla *partenza* di un messaggio ora parte al suo *arrivo* (44 casi, tra cui i carrier che si muovono dopo l'OK: è la regola A3); le soste tarate a mano nelle code (`w1`, `queue`, `q1`, ...) sono sparite: due move consecutivi dello stesso carrier attraverso un punto senza antenna sono diventati un solo move, e la fisica lo fa fermare dietro chi ha davanti. Le cause nei punti di coda senza antenna delle scene 03, 07, 10, 12, 16 sono state riscritte a mano (run-away: insieme al move del run-away; pipettatura: dopo l'arrivo del carrier all'antenna del punto di campionamento). Durate: 02 81,9 s, 03 62,2 s, 04 65,9 s, 09 43,7 s, 10 63,7 s, 13 66,8 s, 14 64,2 s, 16 57,9 s (prima 78,1, 61,1, 63,3, 38,8, 57,4, 60,1, 59,6, 54,3).

**Antenne:** ogni geometria ha antenne numerate nel verso dei carrier (`@device kind=antenna`): sampling e spur 1 NSD, 2 punto di campionamento, 3 fine della pit-lane (Release); turnstile 1 NSD, 2 davanti al gate, 3 fine della pit-lane; spur2 1 NSD, 2 Divert 1, 3 punto di campionamento 1, 4 Divert 2, 5 punto di campionamento 2, 6 fine dello spur. Le antenne sono lettori piatti sulla corsia, sotto i carrier, senza scritte; le onde si allargano attorno al carrier mentre è sopra il punto, fermo o in transito. Il nastro sotto i carrier non si ferma mai: dove il carrier deve fermarsi sull'antenna (punti di campionamento, divert dello spur2) `gate=true` genera un gate subito davanti al carrier (`<antenna>.gate`, orientato secondo la corsia); davanti al tornello il gate c'era già. All'NSD il carrier è letto e deviato al volo. Un device nsd, sampling o divert può portare la sua antenna e il suo gate sulla stessa riga (`antenna=antenna2 gate=true`): il programma crea l'antenna sullo stesso punto e il gate davanti; nelle scene si scrive sempre `on antenna2`, nel layout il nome è `sp.antenna2.gate`. L'ultima antenna (`pit_out`, `po` nello spur2) ha `visible=false`: è alla fine della pit-lane, prima che il carrier torni sulla Main-Lane, e serve solo a far partire il Release. Il gate fa passare un carrier alla volta: si apre in 0,22 s (`GATE_TIME`) quando il carrier che trattiene riparte, o poco prima che un carrier che non si ferma lo raggiunga, e si richiude in 0,22 s appena il carrier è passato.

**`pass <device>`:** il carrier fermo sul punto del device in quel momento (fantasmi e carrier nella ruota esclusi) riparte e il nastro lo porta alla prossima fermata della sua route (antenna con gate, ingresso di un tornello) o alla fine; nessun carrier fermo lì è un errore. È un `move` calcolato (`Scene::pass_move`), quindi vale la fisica. Le scene lo usano ogni volta che un carrier riparte da un gate; i `move` restano per le partenze, i run-away, i fantasmi e l'uscita dalla ruota. Nello spur2 i due move "fino all'NSD" e "dall'NSD al divert" sono diventati uno solo (l'NSD non ferma i carrier).

**Route:** una route elenca tappe, punti o id di corsie. Un id di corsia vale tutti i suoi punti, compresi quelli che cadono sui suoi segmenti (code, punti di campionamento: `lane_paths`); tra due tappe il programma completa il percorso più breve lungo le corsie, nel loro verso, attraversando un tornello da `<id>.in` a `<id>.out` (`expand_route`). Due punti che nessuna corsia collega sono uniti in linea retta (come prima). Le scene usano la forma corta: `main_in,pit,main_out`, `spawn,pit_entry,pit_exit,gone`, `spawn,pit1,gone`, `spawn,gone`.

**Capienza della pit-lane:** `capacity=N` sulla corsia, nella geometria (nessun valore = nessun limite). Un carrier conta da quando lascia la Main-Lane a quando ci rientra; i fantasmi non contano. Dopo aver calcolato la timeline, `Scene::overflow` cerca il primo carrier che troverebbe la corsia piena; `Scene::with_straight` rifà la scena con quel carrier dritto sulla Main-Lane fino alla fine della sua route e senza i suoi step dopo la deviazione (move oltre quel punto, messaggi, pipette del suo `sid`, step legati al suo arrivo alla deviazione o a punti dove non passa più, e a cascata quelli che partivano da questi; un `pass` che non trova nessuno è saltato). Si ripete finché nessuna corsia supera la sua capienza; `--check` scrive in testa "C3 goes straight on: its pit-lane is full". Attenzione: anche step generali (per esempio l'interfaccia che torna grigia quando l'ultimo carrier esce) possono essere saltati se erano legati a quel carrier.

**Nastro:** 17,5 cm/s; un carrier largo 4 cm è disegnato largo 44 px (11 px/cm), quindi 192,5 px/s (`BELT_SPEED`). Tutti i move sono lineari a quella velocità; nelle scene non ci sono più `speed=`, `ease=` né `duration=` sui move. Le scene sono state riscritte per questa velocità: didascalie unite dove gli eventi erano troppo vicini, partenze dei carrier riposizionate, step riordinati per tempo. Durate: 01 29 s, 02 69 s, 03 45 s, 04 57 s, 05 34 s, 06 42 s, 07 26 s, 08 42 s, 09 30 s, 10 47 s, 11 36 s, 12 29 s, 13 49 s, 14 50 s, 15 43 s, 16 38 s, 17 29 s.

**Immagine di layout:** ogni video completo scrive anche `out/<nome>_layout.png` (`render::layout`): il tracciato senza carrier con i nomi di tutti i punti e device (le antenne invisibili come cerchio tratteggiato) e l'elenco dei device con tipo e posizione. `--layout` scrive solo l'immagine.

**Avvisi in più di `--check`:** didascalie visibili meno di 2,5 s o scritte in un ordine diverso da quello in cui compaiono; messaggi scritti in un ordine diverso da quello di invio (il video segue il tempo, il diagramma Mermaid l'ordine del file). Il test sulle scene li vieta.

**Parti comuni:** `common/` è cercata accanto al diagramma e poi in `scenes/`. Ogni scena legge prima `common/video.mmd`, `common/actors.mmd` e `common/messages.mmd`; `%% geometry <nome>` legge `common/<nome>.mmd`. `%% include <file>` inserisce altre direttive (percorso relativo, annidabile, niente inclusioni circolari). Un file incluso può contenere solo righe `%%`: un messaggio incluso mancherebbe dal diagramma mostrato da Mermaid. `--watch` controlla anche i file inclusi e quelli comuni.

**Route di default:** le route di default per geometria (carrier senza `route=`) sono in `mermaid.rs`: se cambiano i punti di una geometria vanno aggiornate (lo verifica un test).

**Anteprima:** `--preview` genera a metà dimensione e a 15 fps, `--from`/`--to` solo quel tratto (l'orologio mostra il tempo reale della scena). Scrivono `out/<nome>_preview.mp4`, così il video completo non viene toccato. Su una scena da 55 s: 11,1 s il video completo, 2,2 s con `--preview`, 0,4 s con `--preview` su 10 s. `--watch` ripete il comando quando cambia la scena o un file incluso (controllo ogni 300 ms), su una sola scena; un file con errori stampa l'errore e il controllo continua.

**Più scene:** i caratteri jolly `*` e `?` sono espansi dal programma (solo nel nome del file), così funzionano anche tra virgolette e su Windows. I thread di `-j` sono divisi tra le scene: `min(scene, jobs)` in contemporanea, `jobs / scene` thread ciascuna, almeno 1. Una scena che fallisce non ferma le altre: gli errori sono elencati alla fine, con codice d'uscita 1. Due scene che scriverebbero lo stesso video sono un errore; `-o` e `--still` accettano una sola scena. L'help mostra versione e repository, letti dal manifesto del pacchetto Rust.

---

## 7. Moduli in dettaglio

### 7.1 `doc.rs` e `mermaid.rs` — lettura delle scene

- `doc.rs`: i dati di una scena sono tabelle (`Table`) di chiavi e valori (`Value`: testo, numero, booleano, lista), raccolte in sezioni (`Doc`). `parse_line` legge una riga `@`; chiavi duplicate e valori malformati sono errori.
- `Table` offre accessor tipizzati (`str_opt/str_req`, `num_opt/num_req`, `bool_opt`, `str_list`) e `check_keys(allowed)`, che **rifiuta le chiavi sconosciute** (protegge dagli errori di battitura). Ogni tabella ricorda file e riga da cui viene, per gli errori.
- `mermaid.rs`: legge il diagramma con i suoi `%% include`, i file comuni e la geometria, e trasforma direttive, messaggi e righe `@` in tabelle.

### 7.2 `scene.rs` — modello e timeline

**Modello:** `Video`, `Point`, `Lane` (Main/Pit/Link), `Device` (Nsd/Sampling/Analyzer/Zone), `Actor`, `Carrier`, `Step` con `Action`.

**Validazioni:** riferimenti a point/actor/device/carrier esistenti, punto `start` sulla route, target del `move` **più avanti** sulla route del carrier, `pipette` solo su device `sampling` o `turnstile` e solo se esiste un `analyzer`, route che attraversano un turnstile solo come `<id>.in`, `<id>.out` consecutivi, regole della ruota (vedi sotto), `wait` con `duration` obbligatoria, niente messaggi di un attore verso sé stesso.

**Calcolo dei tempi degli step** (in ordine di file; sono le chiavi delle tabelle `@step` che le direttive producono):
- `start=after` (default): inizia alla fine dello step precedente.
- `start=with`: inizia insieme allo step precedente.
- `start=after_all`: inizia quando tutti gli step precedenti sono finiti.
- A questi si somma `delay`. Il primo step parte da 0 (+ delay).
- Senza nessuna chiave di tempo (`start`, `at`, `after_start`, `after_end`) vale `after`: gli step sono in sequenza, non contemporanei. Per farli partire insieme serve `start=with` (o `after_start=0`). `caption` e `status` durano 0 s, quindi lo step che li segue parte nello stesso istante.

**Durate di default:** caption 0 (resta visibile fino alla caption successiva), message 1,5 s (tempo di volo della freccia), note 1,5 s, pipette 3 s, highlight 1,5 s, rotate 2 s, move calcolata (vedi sotto).

**Turnstile:** il device genera i punti `<id>.in`, `<id>.needle`, `<id>.out` (posizioni fisse delle tre posizioni). Un `move` che termina in `<id>.in` aggancia il carrier alla tasca in ingresso; da lì la sua posizione segue l'angolo della ruota (`Timeline::carrier_pos`); un `move` successivo lo sgancia dall'uscita. Le tasche vengono assegnate e controllate **in ordine di tempo** dopo aver disposto tutti gli step (`simulate_wheels`), perché l'ordine nel file non coincide sempre con l'ordine temporale. Errori: ingresso occupato, uscita da una posizione diversa dall'uscita, rotazione con un carrier ancora in uscita, arrivi o uscite durante una rotazione, rotazione mentre un carrier fuori dalle tasche (appena uscito o in arrivo) è ancora dentro il cerchio della ruota (distanza dal centro < r + bordo 32 px + raggio del carrier 22 px). In caso di errore `--check` stampa anche la timeline calcolata fino a quel punto. Un carrier che entra nella ruota (dal gate alla tasca d'ingresso) mentre è in corso un `pipette` su quel turnstile è un errore: deve aspettare al gate.

**Gate e antenna** (`gate`, `antenna`, con `at`): nessuno step dedicato. Il gate è aperto negli intervalli in cui un carrier lo attraversa (calcolati in `Timeline::gates` proiettando il punto del gate sulla route di ogni carrier: aperto quando il davanti del carrier lo raggiunge, ma non prima che riparta se era fermo; chiuso quando il retro lo ha superato; apertura e chiusura in 0,22 s).

**`when` (chiave di `@step`) e catena delle cause:** `when` fa partire uno step da ciò che lo causa, più `delay`: `"C1 at merge"` (il carrier raggiunge il punto, anche a metà di un move), `"query C1 sent"` / `"query C1 arrived"` (un messaggio con nome parte / arriva; il carrier è opzionale), `"pipette sp done"` (fine di `pipette`, `rotate` o `needle` su quel device). Si riferisce all'ultimo step corrispondente **prima** di esso nel file: gli step delle scene sono scritti in ordine di tempo. Se la causa manca è un errore.

Le scene sono una catena: ogni risposta segue la sua richiesta, la `pipette` la Sample Acquisition, la Sample Complete la fine della `pipette` (`after 0.3 pipette sp`), un `move` l'arrivo del carrier a un'antenna o la sua causa. Restano assoluti solo le prime partenze dei carrier e le didascalie iniziali. Cambiando una velocità o una durata la scena si sposta a catena; se nasce un conflitto fisico (carrier che si toccano, ruota che gira con un carrier nel cerchio) `--check` lo segnala. Un test lo verifica sulla scena 09.

**`at`, `after_start`, `after_end`:** uno step con `at=<secondi>` parte a un istante assoluto; con `after_start=<secondi>` parte quei secondi dopo l'**inizio** dello step precedente nel file (equivale a `start=with` + `delay`), con `after_end=<secondi>` dopo la sua **fine** (equivale a `start=after` + `delay`). A tutti si somma `delay`. Si può usare una sola tra `at`, `after_start`, `after_end`, `when` e `start`; valori negativi di `after_*` sono rifiutati.

**Movimento dei carrier:**
- La route è una polilinea di punti con le distanze cumulative.
- Ogni `move` genera un segmento `(t0, t1, s0, s1, ease)` sull'ascissa curvilinea *s*, se nessuno lo ostacola.
- **Fisica (`physics.rs`):** i `move` chiesti dagli step sono giocati tutti insieme, a passi di 1/120 s. Un carrier che andrebbe a meno di due raggi da un altro carrier visibile, avvicinandosi, si ferma a contatto e riparte quando c'è spazio; un move chiesto mentre il carrier si muove ancora parte quando si ferma. Due carrier in movimento le cui strade (150 px avanti) si incrociano: passa quello già sulla strada dell'altro, altrimenti il più vicino all'incrocio, a parità il primo dichiarato; la decisione resta finché l'incrocio non è risolto e l'altro si tiene lontano dalla strada del primo. Carrier nello stesso punto (spawn): parte il primo dichiarato. Fantasmi, carrier nascosti o nella ruota non occupano spazio; fuori dal pannello (`TRACK_X`) nessuno si ferma. Un move ostacolato diventa una serie di segmenti lineari.
- **Catena:** gli arrivi decidono gli step (`on`, `when`) e gli step decidono i move, quindi `Timeline::build` ripete la disposizione degli step con i move reali finché i tempi non cambiano più (al massimo 40 volte); le verifiche su ruota, ago e gate si fanno sull'ultima. `--check` scrive `(waits N s behind C1)` o `(stops behind C1 and never gets there)`.
- Easing: `linear`, `in` = 1−cos(πx/2), `out` = sin(πx/2), `inout` = (1−cos πx)/2.
- Durata del move = lunghezza / speed × fattore, con fattore 1 per linear e **π/2 per gli altri**. In questo modo la **velocità di picco è uguale a `speed`**, e un move `in` seguito da un move `out` ha velocità continua nel punto di giunzione. È così che si rappresenta il passaggio **senza fermata** dall'NSD.

**Carrier fantasma** (`ghost = true`): posizione logica di un carrier (dove l'Interfaccia crede che sia) mentre fisicamente è altrove. Non apre i gate e non viene letto dalle antenne. `visible = false` lo nasconde all'inizio; `show`/`hide` lo fanno comparire o sparire in dissolvenza (`CarrierPath::opacity`).

**Stato dell'analizzatore e delle zone:** `status` vale anche per le zone (es. "Interface module": rossa in errore, verde in ready, grigia in normal). `status` colora l'analizzatore (error: rosso con "ERROR"; ready: verde con "READY"; pulsa per 1,5 s al cambio). `needle` porta giù o su l'ago e lo lascia lì; con l'ago giù e lo strumento non in errore compare "Aspirating…", in errore l'ago diventa rosso con "Needle down". `--check` rifiuta una rotazione della ruota, o un carrier che lascia il punto di campionamento, tra l'inizio di un `needle down` e la fine del `needle up` successivo.

**Operatore** (`operator`): operatore di laboratorio stilizzato (camice bianco lungo, guanti blu sottili, capelli scuri, senza attrezzi) che entra a piedi dal bordo destro del pannello, si ferma a destra dell'analizzatore, lavora (la mano si muove sullo strumento) e se ne va. Ogni camminata dura min(2 s, durata/3).

**Righe del sequence diagram:** ogni `message` e ogni `note` occupa una riga, assegnata in ordine di tempo (a parità di tempo, nell'ordine del file).

**Segnaposto:** `{SampleId}` e `{CarrierId}` in `text`/`params` vengono sostituiti con i valori del carrier indicato in `carrier`, o del primo carrier se non è indicato; `{Needle}` con la chiave `needle` dello step (obbligatoria in quel caso).

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

Le sezioni delle righe `@` e le loro chiavi; le direttive producono le stesse tabelle. Sono errori gli id duplicati (point, lane, device, actor, carrier, message) e le inclusioni circolari. Le parti comuni stanno in `scenes/common/`, così `"scenes/*.mmd"` prende solo le scene. Le geometrie delle scene 03, 07 e 08 stanno in `sampling_queue.mmd`, `sampling_queue2.mmd` e `turnstile_queue.mmd`.

**Messaggi con nome:** i tipi di messaggio sono definiti una sola volta in `scenes/common/messages.mmd` (`@message`); un messaggio del diagramma che corrisponde a un tipo diventa quello, con `carrier` e, se il tipo contiene `{Needle}`, `needle`. Uno step con `type` non può ridefinire il messaggio. **Nota automatica:** `pipette` aggiunge da solo, con gli stessi tempi e sull'attore con `analyzer=true`, la nota "Pipetting Analyzer - SampleId=…" ("· Needle n" con più punti di campionamento; il numero dell'analizzatore non compare perché ce n'è uno solo). Il SampleId è quello scritto sulla direttiva, `%% pipette sp duration=4.5 sid=SID-000123`; senza, la nota non lo riporta. Il programma non lo ricava da solo. Una `Note over AN: ...` scritta subito dopo il `pipette` nel diagramma ne sostituisce il testo (chiave `note` dello step). Nel diagramma le note occupano lo stesso spazio della scritta di un messaggio, sopra la riga, così la riga successiva non le copre.

| Sezione | Chiavi |
|---|---|
| `@video` | `title`, `subtitle`, `width` (1920), `height` (1080), `fps` (30), `tail` (2), `show_clock` (true), `show_times` (true) |
| `@point` | `id`, `x`, `y` |
| `@lane` | `id`, `label`, `kind` = main\|pit\|link, `points` = lista di id, `label_x`, `label_y`, `capacity` |
| `@device` | `id`, `kind` = nsd\|sampling\|divert\|antenna (richiedono `at`; `label_x`, `label_y` opzionali per spostare l'etichetta o, per un'antenna, il lettore; ogni `sampling` è un ago dell'analizzatore) \| analyzer\|zone (richiedono `x`,`y`,`w`,`h`) \| turnstile (`x`,`y` centro, `r` (70), `entry` = right\|left (right)) \| gate\|antenna (richiedono `at`), `label` |
| `@actor` | `id`, `label`, `sub`, `color`, `analyzer` (false) |
| `@message` | `id`, `from`, `to`, `text`, `params`, `reply`, `color` (messaggio con nome) |
| `@carrier` | `id`, `tube` (true), `sample_id`, `route` = punti e id di corsie (vedi sotto), `start` (primo punto della route), `speed` (100 px/s), `cap_color`, `ghost` (false), `visible` (true), `tag` (SampleId) |
| `@step` | `action`, uno tra `start` = after\|with\|after_all, `at` (secondi assoluti), `after_start` / `after_end` (secondi dopo l'inizio / la fine dello step precedente), `when` = `<carrier> at <punto>` e le altre cause, `delay`, `duration` + le chiavi specifiche dell'azione |

| action | Chiavi specifiche |
|---|---|
| `caption` | `title`, `text` |
| `move` | `carrier`, `to`, `ease` = linear\|in\|out\|inout (inout), `speed`, `runaway` (false: carrier che scappa, anello rosso pulsante e cartellino "… · RUN-AWAY" dal primo all'ultimo movimento di fuga, soste comprese) |
| `message` | `type`, `carrier`, `needle`; oppure, senza `type`: `from`, `to`, `text`, `params`, `reply` (false), `carrier`, `needle`, `color` (colore del mittente) |
| `note` | `actor`, `text` |
| `pipette` | `device` sampling o turnstile, `sample_id`, `note` (aggiunge la nota "Pipetting Analyzer - SampleId=…") |
| `highlight` | `device` |
| `rotate` | `device` (turnstile) |
| `show` / `hide` | `carrier` (dissolvenza, default 0,5 s) |
| `status` | `device` (analyzer, zone o sampling = un solo ago), `state` = error\|ready\|normal (vale fino al `status` successivo) |
| `needle` | `device` (sampling o turnstile), `to` = down\|up (default 0,9 s) |
| `operator` | `device` (analyzer); default 7 s, minimo 2 s |
| `wait` | (solo `duration`) |

---

## 9. Scena 01 — `01_prepare_sample_release.mmd`

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

## 9b. Scena 02 — `02_turnstile_three_carriers.mmd`

Turnstile (`ts`, centro 440,505, r 70, ingresso a destra) al posto del punto di campionamento, con **antenna** (`pit_wait`, x 640) e **gate** (x 596) davanti. Tre carrier pieni (SID-000123/124/125, tappi viola, azzurro, rosa). C2 e C3 partono da un punto fuori dal pannello (`spawn`, x 1010) e tutti escono verso `gone` (x −40). Per ogni carrier: Prepare/OK all'NSD, Query/Tests to perform con DMX, **secondo Prepare (viola) all'antenna**, Sample Acquisition sotto l'ago, Sample Complete, Release sul rientro. C1 trova la ruota libera: sosta all'antenna, entra, la ruota gira. C2 e C3 aspettano al gate chiuso durante il pipettaggio del carrier precedente; dopo il Sample Complete entrano e solo allora la ruota gira. L'ultima rotazione avviene senza carrier in attesa. Messaggi, didascalie e rotazioni usano `at`. Durata ≈ 78 s, sequence diagram a scorrimento (scorre).

## 9c. Scene 03 e 04 — carrier atteso che non arriva: Clear

Stessa situazione in due geometrie: 03 con il punto di campionamento semplice (geometria della scena 01, con un punto di coda `queue` 60 px prima del campionamento), 04 con turnstile, gate e antenna (geometria della scena 02).

- C1 e C3 vengono deviati nella Pit-Lane: Prepare/OK, Query/Tests to perform.
- C2 si presume deviato (Prepare/OK, Query/Tests to perform) ma **fisicamente va dritto** sulla Main-Lane ed esce. La sua posizione logica è un **fantasma** (`C2L`) che compare all'NSD e segue la Pit-Lane.
- C3 si ferma **sopra** il fantasma di C2 (coda nella 03, antenna nella 04).
- **03:** dopo il Sample Complete di C1 fantasma e C3 avanzano insieme al punto di campionamento; l'Interfaccia manda **Clear** (SampleId di C2), l'Analizzatore risponde **OK** (il fantasma sparisce), poi Sample Acquisition di C3.
- **04:** appena l'antenna legge C3 (C1 è ancora in pipettaggio) l'Interfaccia manda **Clear** di C2 → **OK** (il fantasma sparisce) → **Prepare viola** di C3. C3 aspetta al gate il Sample Complete di C1, entra, la ruota gira, Sample Acquisition di C3.
- Per C2 non c'è Release.
- Durate: 03 ≈ 61 s, 04 ≈ 63 s.

## 9d. Scene 05 e 06 — errore dello strumento con l'ago giù, recovery

Un solo carrier; 05 con il punto di campionamento semplice (geometria della scena 01), 06 con turnstile, gate e antenna (geometria della scena 02).

- Flusso normale fino alla Sample Acquisition (nella 06 anche il Prepare viola all'antenna, l'ingresso e la rotazione).
- Durante l'aspirazione l'analizzatore va in errore lasciando l'ago nel tubo: diventa rosso e manda subito **Status `State=Error`** e **Sample Complete** (con SampleId); l'Interfaccia risponde **OK** allo Status (senza parametri). Il Sample Complete **non** basta per rilasciare: con lo strumento in errore il carrier resta fermo (nella 06 la ruota non gira).
- L'operatore arriva allo strumento (azione `operator`, 7 s, insieme alla nota "Operator recovery") ed esegue la recovery: mentre lavora l'ago risale.
- Lo strumento torna verde e manda **Status `State=Ready`**; l'Interfaccia risponde **OK** e solo ora rilascia il carrier (nella 06 prima la ruota lo porta all'uscita).
- Sulla Main-Lane l'Interfaccia manda **Release**.
- Lo strumento resta verde 3 s dopo lo Status Ready, poi torna grigio.
- I messaggi Status e i relativi OK non hanno SampleId (è lo stato dello strumento).
- Durate: 05 ≈ 50 s, 06 ≈ 53 s.

## 9e. Scene 07 e 08 — carrier run-away durante il campionamento

Tre carrier deviati (Prepare/OK, Query/Tests to perform per ciascuno); 07 con il punto di campionamento semplice (geometria della scena 03 con due posizioni di coda, `queue` e `queue2`), 08 con turnstile, gate e antenna (geometria della scena 02 con una posizione di coda `pit_q` dietro l'antenna).

- **07:** Sample Acquisition per C1; **prima che l'ago scenda** C1 scappa (`runaway = true`) ed esce fino alla Main-Lane. C2 scivola sotto l'ago, C3 avanza in coda. L'analizzatore aspira C2 credendo che sia C1.
- **08:** C2 aspetta al gate (Prepare viola all'antenna), C3 dietro di lui. Dopo la Sample Acquisition per C1 la ruota **gira da sola**: C1 va all'uscita e scappa verso la Main-Lane. L'ago scende nella tasca vuota e non aspira nulla.
- In entrambe l'analizzatore manda **Sample Complete** con il SampleId di C1; l'Interfaccia manda **Cancel** (SampleId di C1) → **OK**, poi manda via tutti i carrier senza campionarli (nella 08 C2 e C3 passano dalla ruota: entrano, rotazioni, escono; C3 si ferma all'antenna ma non riceve Prepare). Subito dopo l'OK al Cancel parte **Clear All** (senza parametri) → **OK**, mentre i carrier stanno uscendo. Nessun Release.
- L'**interfaccia** (zona "Interface module") diventa **rossa** al momento della fuga (07: quando C1 lascia il punto di campionamento; 08: quando la ruota gira da sola) e torna **grigia** quando l'ultimo carrier è uscito completamente dal riquadro.
- Nella 08 l'ultima rotazione parte solo quando C2 è uscito completamente dal cerchio della ruota.
- Durate: 07 ≈ 48 s, 08 ≈ 58 s.

## 9f. Scene 09–12 — spur

Geometria comune: Main-Lane a y 760 (NSD a x 800, rientro a x 290); deviazione diagonale e curva di 90° nel ramo d'andata (x 660), con due posizioni di coda (`queue2` y 560, `queue` y 500) sotto il punto di campionamento (y 440); analizzatore a destra (x 730–930, y 330–550); traversa in cima (y 250) e ramo di ritorno (x 380). Velocità dei carrier 110 px/s (lo spur è più lungo); l'uscita dal punto di campionamento è un move `in` fino alla cima, `linear` fino al rientro e `out` fino all'uscita, senza fermate.

- **09** happy path (≈ 39 s): come la 01.
- **10** carrier mancante → Clear (≈ 57 s): come la 03.
- **11** errore dello strumento con l'ago giù e recovery (≈ 47 s): come la 05; l'operatore lavora sotto l'analizzatore.
- **12** carrier run-away → Cancel, Clear All (≈ 48 s): come la 07, con l'interfaccia rossa dalla fuga finché tutti i carrier ne sono usciti.

## 9g. Scene 13–16 — spur con un analizzatore e due punti di campionamento

Un solo strumento con due aghi: un solo attore Analyzer e un solo device `analyzer` a destra dei due punti di campionamento. Ramo d'andata a x 440 con due deviazioni standard (`div1` a y 650, `div2` a y 410). Da ognuna parte verso destra una pit-lane che sale passando dal punto di campionamento (x 600) e rientra nel ramo **sopra** la deviazione; chi esce dall'ago 1 si ferma quindi anche a `div2`. Velocità 120 px/s.

- **Messaggi:** Prepare all'NSD e di nuovo (viola) a ogni deviazione che devia il carrier; alle altre il carrier è solo letto. Una Sample Acquisition per ogni ago. `Needle=n` compare in Sample Acquisition, Sample Complete e Status. Un solo Release per carrier. Clear, Cancel e Clear All non hanno `Needle`.
- **Quali aghi** dipende dai test del campione: uno solo o entrambi.
- Le deviazioni standard sono etichettate "Divert 1" e "Divert 2". Un carrier può entrare in una pit-lane mentre l'ago sta campionando il carrier precedente. Soste ai divert brevi: 0,6 s con il Prepare viola, 0,4 s se il carrier è solo letto (il carrier non aspetta l'arrivo della freccia).
- **13** happy path (≈ 60 s): C1 usa entrambi gli aghi, C2 solo l'ago 2; C1 entra nella seconda pit-lane mentre C2 è in campionamento.
- **14** Clear (≈ 60 s): il fantasma di C2 si ferma a `div1`, C3 ci si ferma sopra; alla lettura di C3 parte il Clear di C2 → OK → Prepare viola di C3, mentre C3 è già stato deviato: la sosta al divert non aspetta la risposta.
- **15** errore (≈ 56 s): errore **dell'ago 1** (`status` sul device `sampling`: diventa rossa solo la sua fascia dell'analizzatore), l'ago 2 continua durante la recovery.
- **16** run-away (≈ 54 s): C1 scappa dall'ago 1 e si ferma comunque a `div2`; il carrier dopo finisce sotto l'ago. Via solo i carrier della pit-lane dell'ago 1, che è rossa dalla fuga finché non si è svuotata. L'ago 2 continua.

---

## 10. Assunzioni da confermare (scelte non specificate dall'utente)

| # | Assunzione | Stato |
|---|---|---|
| A1 | Nomi dei messaggi verso DMX: "Query", "Tests to perform", "Results" | Confermata |
| A2 | L'OK al Prepare arriva **prima** della Query a DMX | Confermata, verificata da `--check` |
| A3 | Sample Acquisition, Sample Complete e Release **hanno una risposta OK senza parametri**. L'ago scende dopo l'OK alla Sample Acquisition, il carrier riparte dopo l'OK alla Sample Complete | Confermata |
| A4 | Release parte quando il carrier è **uscito dal modulo dell'Interfaccia** | Confermata |
| A5 | Results a DMX in tutte le scene, alla fine, uno dopo l'altro, per i campioni campionati regolarmente. Nessun Results per il campione cancellato nel run-away, per quello dell'errore e per quello del Clear | Confermata; sul campione del Clear è una deduzione |
| A6 | SampleId `SID-000123` e durata della pipettatura 4–5 s | Confermati come valori di esempio |
| A7 | Messaggi di stato dell'analizzatore: oggi solo Error e Ready | Scena dedicata in Roadmap |
| A8 | Il secondo Prepare (all'antenna o al divert) ha una risposta OK senza parametri; l'informazione in più che porterà è ancora da definire | Confermata |
| A9 | La risposta dell'Analizzatore al Clear si chiama "OK" | Scelta dell'utente |
| A10 | L'Analizzatore manda una Query a DMX alla volta: la successiva solo dopo aver ricevuto la risposta alla precedente. La Sample Acquisition può arrivare allo strumento anche prima dei test da DMX | Confermata dall'utente |
| A9 | La risposta dell'Analizzatore al Clear si chiama "OK" (scene 03/04) | Scelta dell'utente |

---

## 11. Roadmap (evoluzioni concordate o previste)

Miglioramenti proposti e punti da confermare: vedi [`IMPROVEMENTS.md`](IMPROVEMENTS.md).

1. **Risposta negativa al Prepare** (campione da non processare): ramo alternativo, con il carrier che attraversa la Pit-Lane senza campionamento.
2. **Messaggi di stato**: entrambe le modalità, su richiesta dell'Interfaccia o spontanea al cambio di stato.
3. **Più carrier**, pieni e vuoti, con code nella Pit-Lane. Servirà la gestione delle collisioni o delle distanze minime. (La scena 02 ha tre carrier pieni con il turnstile; code e carrier vuoti restano da fare.)
4. ~~Scroll del sequence diagram~~ — fatto.
5. **Versione realistica** del video, se lo schematico non convince.

---

## 12. Limiti noti e note per chi riparte

- **Test:** `cargo test` esegue gli unit test della lettura delle righe `@` (`doc.rs`) e dei diagrammi (`mermaid.rs`), della timeline (`scene_tests.rs`: tempi di partenza, movimenti, messaggi, fantasmi, turnstile, ago, stato, operatore e la validità di tutte le scene in `scenes/`) e del disegno (`render.rs`). Vanno aggiornati quando si aggiunge un'azione o una regola.

- Le righe `@` non hanno testo su più righe: un valore lungo va su una riga sola.
- La larghezza dei testi è **stimata** dal numero di caratteri (font proporzionale), quindi le etichette molto lunghe possono uscire un po' dai box.
- Con `resvg` la scena 01 (1432 fotogrammi) si genera in ~9 s su un Mac a 10 core.
- La pipetta usa sempre il primo device `analyzer` della scena.
- `ffmpeg -f svg_pipe` è stato provato e **non funziona** (errori di decodifica con più SVG concatenati): non riprovarlo.
- In Rust attenzione alle raw string: gli SVG contengono `"#RRGGBB"`, quindi vanno usati delimitatori `r##"…"##`.
