# trackviz — Miglioramenti e punti aperti

Promemoria di ciò che resta da fare o da decidere. Le funzioni già realizzate sono
descritte in `README.md` e `PROJECT.md`; qui sono solo riassunte in fondo.

## 1. Miglioramenti proposti, non ancora fatti

Le voci 1.1–1.4 servono a scrivere le scene più in fretta, le 1.5–1.8 a controllarle e
consegnarle. Nuovi scenari e contenuti sono nella Roadmap di `PROJECT.md`, sezione 11.

### 1.1 Highlight agganciato all'evento

59 `highlight` su 66 erano a ridosso di un messaggio (circa 350 righe).

- Una chiave sullo step, `highlight = "nsd"`, al posto di uno step separato.
- In alternativa il riquadro dell'NSD si accende da solo al passaggio del carrier.

### 1.2 Scheletro di una nuova scena

`trackviz new mia_scena --geometry spur2` crea il file con gli `include`, i carrier
e una prima didascalia.

### 1.3 Autocompletamento nell'editor

Schema per i file `.toml` e snippet che suggeriscono azioni, chiavi e i 20 tipi di
messaggio, segnalando gli errori di battitura mentre si scrive. Utile solo con un
editor compatibile, come VS Code.

### 1.4 Risolvere i conflitti che la catena segnala

Rallentando i carrier del 25% due scene danno un conflitto fisico: i tempi si spostano
a catena, ma il conflitto va risolto a mano con un `delay`.

- Scena 07: C2 e C3 si toccano in coda.
- Scena 08: la ruota partirebbe prima che C2 sia fuori dal cerchio. Si potrebbe
  agganciare la rotazione al punto in cui il carrier è fuori dalla ruota.

### 1.5 Foglio riassuntivo di una scena

Un'immagine con un fotogramma per ogni didascalia, per controllare una scena in pochi
secondi senza guardare il video.

### 1.6 Esportare il sequence diagram come immagine o PDF

Il diagramma come file Mermaid c'è già (`trackviz export`). Manca l'immagine o il PDF,
da allegare alla documentazione per i produttori di analizzatori, senza dipendere da
un visualizzatore Mermaid.

### 1.7 Indice dei video

Una pagina HTML o un README con l'elenco degli scenari, per consegnarli come pacchetto
unico.

### 1.8 Didascalie in più lingue

La stessa scena con didascalie in inglese e in italiano o altre lingue, se i destinatari
lo richiedono. Oggi tutti i testi dei video sono in inglese.

## 2. Da confermare

- **Results e Clear:** nessun Results per il campione del Clear (C2 nelle scene 03, 04, 10,
  14). È una deduzione: il campione non è mai arrivato al punto di campionamento.
- **Durata dei video:** con i nuovi OK e i Results alcune scene superano il minuto
  (02: 78 s; 03: 61 s; 04: 63 s; 13: 60 s). Il limite iniziale era 1 minuto.

Le assunzioni A1–A10 di `PROJECT.md`, sezione 10, sono confermate.

## 3. Limiti noti

- `trackviz export` non è stato provato con un visualizzatore Mermaid reale: il formato è
  verificato solo dall'import del tool.
- Un messaggio scritto nel `.mmd` che non corrisponde a un tipo di `messages.toml` resta testo
  libero, senza avvisi: rinominare un tipo non aggiorna i diagrammi.
- `when` si riferisce all'ultimo step corrispondente già scritto nel file: gli step vanno
  tenuti in ordine di tempo.
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

Parti comuni con `include`, messaggi con nome e `{Needle}`, nota "Pipetting" automatica,
tempi legati agli eventi con `when`, scene come catena di cause, righe del diagramma in
ordine di tempo, avvisi in `--check`, più scene in parallelo con i caratteri jolly,
anteprima veloce (`--preview`, `--from`, `--to`, `--watch`), scene scritte in Mermaid con parti comuni, export e import.
