# trackviz — Miglioramenti e punti aperti

Promemoria di ciò che resta da fare o da decidere. Le funzioni già realizzate sono
descritte in `README.md` e `PROJECT.md`; qui sono solo riassunte in fondo.

## 0. In corso: tempi a posizione e fisica dei carrier

Deciso con l'utente, a tappe:

1. **Fatto:** antenne in tutte le geometrie; direttive `on <antenna> [carrier]`,
   `after <secondi> [<messaggio> <carrier>]`, `at`, `color`; tolte le parole chiave
   (`Prepare 1`, `OK`, ...) e il Prepare viola automatico.
2. **Fatto — fisica dei carrier:** due carrier non occupano mai lo stesso spazio.
   Chi raggiunge un carrier fermo si avvicina fino a toccarlo e si ferma; riparte quando
   l'altro si muove. I carrier vanno solo avanti. `move C2 to <punto>` vuol dire "parte
   verso il punto": soste in coda e ripartenze le calcola il programma.
3. **Fatto — conversione delle scene 01–16** a `on`/`after`/`at`; `when`, `with`,
   `delay`, `after_start`, `after_end` non esistono più come direttive. `after` accetta
   anche un'antenna (`after 0.15 antenna2 C1`) o la fine di un'azione (`after 0.2
   pipette sp`). Dettagli e durate in `PROJECT.md`.

4. **Fatto — nastro a 17,5 cm/s, antenne sulla corsia e gate:** vedi `PROJECT.md`.

## 1. Miglioramenti proposti, non ancora fatti

Le voci 1.2–1.4 servono a scrivere le scene più in fretta, le 1.5–1.7 a controllarle e
consegnarle (fatte: 1.1 highlight automatico, 1.2 `trackviz new`, 1.3 snippet e controllo in VS Code, 1.4 la ruota aspetta di essere libera). Nuovi scenari e contenuti sono nella Roadmap di `PROJECT.md`, sezione 11.

### 1.4 Fatto — la ruota aspetta di essere libera

`rotate` parte quando la ruota è libera: rotazione precedente finita, carrier in arrivo
già nella tasca d'ingresso, nessun carrier fuori dalle tasche dentro il cerchio della
ruota per tutta la rotazione. Nel log di `--check`: "(waits N s for the wheel to be
clear)". Nella scena 08 l'ultima rotazione non è più legata a `on antenna3 C2`.

### 1.5 Foglio riassuntivo di una scena

Un'immagine con un fotogramma per ogni didascalia, per controllare una scena in pochi
secondi senza guardare il video.

### 1.6 Indice dei video

Una pagina HTML o un README con l'elenco degli scenari, per consegnarli come pacchetto
unico.

### 1.7 Didascalie in più lingue

La stessa scena con didascalie in inglese e in italiano o altre lingue, se i destinatari
lo richiedono. Oggi tutti i testi dei video sono in inglese.

## 2. Da confermare

- **Results e Clear:** nessun Results per il campione del Clear (C2 nelle scene 03, 04, 10,
  14). È una deduzione: il campione non è mai arrivato al punto di campionamento.
- **Durata dei video:** con il nastro a 17,5 cm/s solo la scena 02 supera il minuto
  (69 s, tre carrier nel tornello). Il limite iniziale era 1 minuto.

Le assunzioni A1–A10 di `PROJECT.md`, sezione 10, sono confermate.

## 3. Limiti noti

- I file `.mmd` delle scene non sono stati provati in un visualizzatore Mermaid reale
  (GitHub, VS Code): sono verificati solo dal tool.
- Un messaggio scritto nel `.mmd` che non corrisponde a un tipo di `messages.mmd` resta testo
  libero, senza avvisi: rinominare un tipo non aggiorna i diagrammi.
- `after <secondi> <messaggio>` si riferisce all'ultimo messaggio con quel testo già
  scritto nel file: i messaggi vanno tenuti in ordine di tempo.
- La fisica non conosce gate e ruota: un carrier che deve aspettare al gate va mandato
  fino al punto dell'antenna, e l'ingresso nella ruota resta un `move` esplicito.
- Alcuni ritardi hanno ancora decimali poco "umani" (`after 0.972 OK C1`): il riordino
  per tempo esprime una causa lontana sommando le durate degli step in mezzo.
- Un tool per riordinare una scena per tempo (`trackviz sort`) sarebbe utile: oggi è
  uno script esterno usato durante la riscrittura.
- Una geometria diversa richiede un proprio file in `scenes/common/`.
- Con più scene il resto della divisione di `-j` resta inutilizzato (4 scene con `-j 6`:
  2 thread fermi).

## 4. Valutati e scartati

- **Blocchi di step riusabili:** circa 890 righe risparmiate con 47 blocchi diversi; le
  sequenze si ripetono poco e il blocco Prepare + OK non funzionava.
- **Didascalie riusate:** solo 21 ripetizioni esatte su 124.
- **Step su una riga sola:** meno righe, ma scene più difficili da leggere e da confrontare
  in git.

## 5. Già realizzati

Scene in Mermaid con parti comuni (`include`, geometrie), righe `@`, messaggi con nome e
`{Needle}`, tempi con `on`/`after`/`at` legati alle antenne e ai messaggi, fisica dei
carrier (code, incroci, nastro a 17,5 cm/s), antenne sulla corsia con gate, `pass`,
route con nomi di corsia, capienza della pit-lane, nota di pipettatura con `sid=`,
accensione automatica dei device letti dalle antenne, rotazione che aspetta la ruota libera, immagine di layout (`--layout`), snippet e controllo delle scene in VS Code (`.vscode`), scheletro di una scena nuova (`trackviz new`),
avvisi in `--check` (sovrapposizioni, didascalie corte o fuori ordine, messaggi fuori
ordine), più scene in parallelo con i caratteri jolly, anteprima veloce
(`--preview`, `--from`, `--to`, `--watch`).
