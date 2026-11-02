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
| **NSD** | **No Stop Divert**: il dispositivo che sposta il carrier, pieno o vuoto, dalla Main-Lane alla Pit-Lane **senza fermarlo**. |
| **Pit-Lane** | Corsia secondaria, parallela alla Main-Lane, in cui il carrier viene fermato per farlo campionare dall'analizzatore. Dopo il punto di campionamento rientra nella Main-Lane, così i carrier processati (pieni o vuoti) tornano nella corsia principale. |
| **Punto di campionamento** | Posizione sulla Pit-Lane in cui il carrier si ferma e l'analizzatore aspira (pipetta) il campione. |
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
| **Zero dipendenze esterne (solo `std`)** | Nell'ambiente di sviluppo crates.io era bloccato. Questo rende anche il tool facile da compilare ovunque. Si possono introdurre crate in futuro (vedi §12). |
| **Parser TOML scritto a mano** (sottoinsieme) | Al posto di `serde`/`toml`. |
| **Rendering: un SVG per fotogramma** | L'SVG viene generato come stringa, così la grafica vettoriale è semplice da scrivere e scalabile. |
| **Rasterizzazione con `rsvg-convert`** (librsvg) | Il decoder SVG integrato in ffmpeg (`svg_pipe`) **non funzionava** con più fotogrammi concatenati. |
| **Encoding con `ffmpeg`** | I PNG vengono passati via `image2pipe` e codificati in H.264 (`libx264`, `-preset veryfast -crf 18 -pix_fmt yuv420p -movflags +faststart`). |
| **Rendering parallelo** | N thread worker (default = numero di core): ognuno genera l'SVG, lo rasterizza e invia il PNG. Il thread principale li riordina (BTreeMap) e li scrive in sequenza a ffmpeg. |
| **Spazio di coordinate fisso 1920×1080** | Lo scenario usa sempre queste coordinate. `width`/`height` del video scalano solo l'output tramite `viewBox`. |
| **Timeline deterministica calcolata in anticipo** | Ogni step ha un inizio e una fine. Il fotogramma al tempo *t* è una funzione pura di *t*, quindi è possibile renderizzare qualunque istante (`--still`). |

**Dipendenze di sistema:** `rsvg-convert`, `ffmpeg`, font **Inter** (fallback DejaVu Sans) e **DejaVu Sans Mono** per i testi monospazio.

---

## 6. Struttura del progetto

```
trackviz/
├── Cargo.toml            # edition 2021, nessuna dipendenza
├── README.md             # guida rapida e riferimento del formato
├── PROJECT.md            # questo documento
├── scenes/
│   └── 01_prepare_sample_release.toml
├── out/                  # video generati
└── src/
    ├── main.rs           # CLI, pipeline rsvg → ffmpeg, thread worker
    ├── toml.rs           # parser del sottoinsieme TOML
    ├── scene.rs          # modello della scena, validazione, calcolo timeline
    └── render.rs         # generazione dell'SVG di un fotogramma
```

### CLI

```
trackviz <scene.toml> [-o out.mp4] [-j jobs]      # video completo
trackviz <scene.toml> --still <sec> [-o f.png]    # singolo fotogramma PNG
trackviz <scene.toml> --still <sec> --svg -o f.svg
trackviz <scene.toml> --check                     # valida e stampa la timeline
```

La timeline viene sempre stampata su stderr (inizio/fine di ogni step) e serve come **log degli eventi** per regolare i tempi.

---

## 7. Moduli in dettaglio

### 7.1 `toml.rs` — parser

- Supporta: commenti `#` (ignorati dentro le stringhe), `[tabella]`, `[[array_di_tabelle]]`, `chiave = valore`.
- Valori: stringhe tra doppi apici (escape `\" \\ \n \t`), numeri (int/float, `_` ammesso), `true`/`false`, array, anche su più righe (letti finché le parentesi non si bilanciano).
- Errori con **numero di riga**: chiavi duplicate, tabelle duplicate, valori malformati.
- `Table` offre accessor tipizzati (`str_opt/str_req`, `num_opt/num_req`, `bool_opt`, `str_list`) e `check_keys(allowed)`, che **rifiuta le chiavi sconosciute** (protegge dagli errori di battitura).

### 7.2 `scene.rs` — modello e timeline

**Modello:** `Video`, `Point`, `Lane` (Main/Pit/Link), `Device` (Nsd/Sampling/Analyzer/Zone), `Actor`, `Carrier`, `Step` con `Action`.

**Validazioni:** riferimenti a point/actor/device/carrier esistenti, punto `start` sulla route, target del `move` **più avanti** sulla route del carrier, carrier che non può ricevere un nuovo move mentre è ancora in movimento, `pipette` solo su device `sampling` e solo se esiste un `analyzer`, `wait` con `duration` obbligatoria, niente messaggi di un attore verso sé stesso.

**Calcolo dei tempi degli step** (in ordine di file):
- `start = "after"` (default): inizia alla fine dello step precedente.
- `start = "with"`: inizia insieme allo step precedente.
- `start = "after_all"`: inizia quando tutti gli step precedenti sono finiti.
- A questi si somma `delay`. Il primo step parte da 0 (+ delay).

**Durate di default:** caption 0 (resta visibile fino alla caption successiva), message 1,5 s (tempo di volo della freccia), note 1,5 s, pipette 3 s, highlight 1,5 s, move calcolata (vedi sotto).

**Movimento dei carrier:**
- La route è una polilinea di punti con le distanze cumulative.
- Ogni `move` genera un segmento `(t0, t1, s0, s1, ease)` sull'ascissa curvilinea *s*.
- Easing: `linear`, `in` = 1−cos(πx/2), `out` = sin(πx/2), `inout` = (1−cos πx)/2.
- Durata del move = lunghezza / speed × fattore, con fattore 1 per linear e **π/2 per gli altri**. In questo modo la **velocità di picco è uguale a `speed`**, e un move `in` seguito da un move `out` ha velocità continua nel punto di giunzione. È così che si rappresenta il passaggio **senza fermata** dall'NSD.

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
6. Carrier: disco scuro (r 22) con ombra. Se ha il tubo, cerchio bianco con tappo colorato (`cap_color`); se è vuoto, alloggiamento scuro. Accanto c'è il **cartellino con il SampleId** (o "C1 (empty)"), che si sposta a sinistra se uscirebbe dal pannello.
7. Pipetta (durante `pipette`): braccio che scende dal fondo dell'analizzatore fino al tubo (20% discesa, 60% aspirazione, 20% risalita). Durante l'aspirazione compaiono onde blu e la scritta "Aspirating…".

**Sequence diagram:**
- Riquadri degli attori distribuiti uniformemente, con la banda superiore del colore dell'attore. Linee della vita tratteggiate.
- Messaggio: la freccia cresce dalla sorgente alla destinazione durante la sua durata (ease-out). Il colore è quello del mittente. Le risposte (`reply`) sono tratteggiate con punta aperta.
- Etichetta su una riga sopra la freccia: **nome** in grassetto più `params` in monospazio grigio, su un rettangolo bianco che copre le linee della vita. Il `paint-order` non è supportato da librsvg, per questo si usa il rettangolo.
- Un messaggio "fresco" (fino a 1,5 s dopo l'arrivo) ha la freccia più spessa e un alone ambra.
- Note: box giallo centrato sulla linea della vita.
- A sinistra di ogni riga, in grigio, l'istante di inizio (`show_times`).
- Altezza delle righe = spazio disponibile / numero di righe, massimo 70 px.

**Didascalie:** l'ultima caption iniziata resta visibile, con fade-in di 0,4 s. Il titolo è maiuscolo in ambra, il testo bianco 27 px va a capo a ~118 caratteri, massimo 2 righe con titolo e 3 senza.

**Palette:** sfondo `#F1F4F8`, testo `#0F172A`, grigio `#64748B`, accento `#F59E0B`. Attori: Interface `#2563EB`, Analyzer `#EA580C`, DMX `#059669`. Tappo del tubo `#7C3AED`.

---

## 8. Formato delle scene (riferimento)

| Sezione | Chiavi |
|---|---|
| `[video]` | `title`, `subtitle`, `width` (1920), `height` (1080), `fps` (30), `tail` (2), `show_clock` (true), `show_times` (true) |
| `[[point]]` | `id`, `x`, `y` |
| `[[lane]]` | `id`, `label`, `kind` = main\|pit\|link, `points` = [id…], `label_x`, `label_y` |
| `[[device]]` | `id`, `kind` = nsd\|sampling (richiedono `at`) \| analyzer\|zone (richiedono `x`,`y`,`w`,`h`), `label` |
| `[[actor]]` | `id`, `label`, `sub`, `color` |
| `[[carrier]]` | `id`, `tube` (true), `sample_id`, `route` = [id…], `start` (primo punto della route), `speed` (100 px/s), `cap_color` |
| `[[step]]` | `action`, `start` = after\|with\|after_all, `delay`, `duration` + le chiavi specifiche dell'azione |

| action | Chiavi specifiche |
|---|---|
| `caption` | `title`, `text` |
| `move` | `carrier`, `to`, `ease` = linear\|in\|out\|inout (inout), `speed` |
| `message` | `from`, `to`, `text`, `params`, `reply` (false), `carrier` |
| `note` | `actor`, `text` |
| `pipette` | `device` (`carrier` è accettato ma non ancora usato) |
| `highlight` | `device` |
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
2. *NSD — Prepare* — At the NSD (No Stop Divert) the carrier is diverted into the Pit-Lane without stopping. At this moment the interface sends Prepare to the analyzer.
3. *Test query* — After Prepare, the analyzer queries DMX (LIS) to know which tests must be performed on this sample.
4. *Sampling point — Sample Acquisition* — The carrier stops at the sampling point. The interface sends Sample Acquisition: the tube is in position and can be aspirated.
5. *Pipetting* — The analyzer aspirates the biological material from the tube. Meanwhile the carrier is held at the sampling point.
6. *Sample Complete* — When aspiration is finished the analyzer sends Sample Complete. Only after receiving it does the interface release the carrier.
7. *Carrier released* — The interface releases the carrier: it leaves the Pit-Lane and heads back to the Main-Lane.
8. *Main-Lane — Release* — As soon as the carrier with the tube is back on the Main-Lane, the interface sends Release: the sample has left the analyzer interface.
9. *Results* — Later, when the analysis is complete, the analyzer sends the results to DMX.

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

---

## 11. Roadmap (evoluzioni concordate o previste)

1. **Risposta negativa al Prepare** (campione da non processare): ramo alternativo, con il carrier che attraversa la Pit-Lane senza campionamento.
2. **Messaggi di stato**: entrambe le modalità, su richiesta dell'Interfaccia o spontanea al cambio di stato.
3. **Più carrier**, pieni e vuoti, con code nella Pit-Lane. Servirà la gestione delle collisioni o delle distanze minime.
4. **Scroll del sequence diagram** quando le righe superano lo spazio disponibile.
5. **Versione realistica** del video, se lo schematico non convince.

---

## 12. Limiti noti e note per chi riparte

- Il parser TOML è un sottoinsieme: niente tabelle inline `{}`, chiavi puntate, date o stringhe multilinea `"""`. Se la rete lo permette, si possono adottare `serde` + `toml` (e magari `tiny-skia` + `ab_glyph` per eliminare rsvg-convert).
- La larghezza dei testi è **stimata** dal numero di caratteri (font proporzionale), quindi le etichette molto lunghe possono uscire un po' dai box.
- Il rendering costa ~70–150 ms per fotogramma in rsvg-convert: ~2 minuti per 48 s su 2 core.
- La pipetta usa sempre il primo device `analyzer` della scena.
- `ffmpeg -f svg_pipe` è stato provato e **non funziona** (errori di decodifica con più SVG concatenati): non riprovarlo.
- In Rust attenzione alle raw string: gli SVG contengono `"#RRGGBB"`, quindi vanno usati delimitatori `r##"…"##`.
