# trackviz — Miglioramenti e punti aperti

Promemoria di ciò che resta da fare o da decidere. Le funzioni già realizzate sono
descritte in `README.md` e `PROJECT.md`; qui sono solo riassunte in fondo.

## 1. Miglioramenti proposti, non ancora fatti

In ordine di utilità.

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

## 2. Da confermare

Scelte fatte senza un'indicazione esplicita.

### Messaggi

- **Sample Acquisition prima dei test:** `--check` avvisa se parte prima che i test del
  campione siano noti. Non è una regola data dall'utente: se lo strumento può riceverla
  prima, il controllo va tolto.
- **Status Ready:** riporta `Needle=n`, come Status Error.
- **Clear All (scene 16):** è senza parametri, ma va a un analizzatore con l'ago 2 ancora
  al lavoro. Se riguarda solo l'ago 1, forse serve `Needle=1`.
- **Clear e Cancel:** sono senza `Needle`.
- **Errore dello strumento:** l'ordine è Status → Sample Complete → OK, quindi a colpo
  d'occhio l'OK sembra la risposta a Sample Complete. In alternativa
  Status → OK → Sample Complete.
- **Assunzioni precedenti:** restano aperte A1–A8 in `PROJECT.md`, sezione 10.

### Scenari

- **Run-away, turnstile (08):** durante lo sgombero C3 si ferma all'antenna ma non riceve
  il Prepare viola.
- **Run-away (07, 08, 12, 16):** il Clear All parte quando l'ultimo carrier arriva sulla
  Main-Lane, non quando esce dal pannello.
- **Run-away, più aghi (16):** C1 in fuga si ferma comunque al Divert 2, e in quella sosta
  la scritta RUN-AWAY sparisce per un attimo. Il carrier per l'ago 2 viaggia per secondo,
  altrimenti si scontra con C1.
- **Run-away, più aghi (16):** la pit-lane torna grigia quando i suoi carrier l'hanno
  lasciata, non quando escono da tutta l'interfaccia.
- **Clear, più aghi (14):** C3 resta circa 3,5 s al Divert 1 in attesa della risposta al
  Clear.

### Grafica

- **Pit-lane nello spur a due aghi:** i riquadri tratteggiati restano visibili anche
  quando non sono rossi.
- **Operatore:** tiene in mano una chiave inglese; per una recovery di laboratorio
  potrebbe lavorare a mani nude.
- **Analizzatore dopo la recovery:** resta verde fino alla fine del video.

## 3. Limiti noti

- `when` si riferisce all'ultimo step corrispondente già scritto nel file: gli step vanno
  tenuti in ordine di tempo.
- Una scena non può estendere una lane definita in un file comune: le scene 03, 07 e 08
  tengono la propria geometria.
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
anteprima veloce (`--preview`, `--from`, `--to`, `--watch`).
