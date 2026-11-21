# Paint CP

Paint CP è un programma di disegno e fotoritocco leggero, scritto in Rust, pensato per
Windows ma funzionante anche su Linux e macOS. Serve per le piccole modifiche alle foto
e per disegnare al volo con la semplicità del vecchio Paint, aggiungendo però gli
strumenti che in Paint mancano: livelli, selezione, trasparenza e cronologia.

È gratis, senza pubblicità e senza account: si scarica, si apre e si usa.

Scaricare l'ultima versione: <https://github.com/gsepp86/paint-cp/releases/latest>

## Funzioni

### Disegno e ritocco

- **Pennello**, **Matita**, **Gomma** (anche trasparente) e **Spray**, con spessore
  regolabile da 1 a 64 pixel.
- **Fluidifica**: sfuma e trascina i colori della foto, utile per ritocchi morbidi.
- **Secchiello** per riempire aree, **Contagocce** per prendere un colore dall'immagine.
- **Testo** scritto direttamente sulla tela, con dimensione del carattere regolabile.
- **Forme**: linea, rettangolo ed ellisse, disegnate a contorno, piene oppure
  contorno + riempimento.
- Due colori pronti (**primario** e **secondario**) e selettore colore completo.

### Selezione

- Selezione rettangolare con spostamento del contenuto (anche su un altro punto della
  tela), taglia, copia e incolla.
- **Ritaglia** l'immagine sulla selezione.
- Cancella il contenuto selezionato con il tasto Canc.

### Livelli

- Più livelli sovrapposti, ognuno con nome, visibilità e opacità.
- Nuovo livello, duplica, elimina, sposta su/giù, unisci al livello sotto,
  appiattisci tutto.
- Pannello livelli richiudibile, con anteprima di ogni livello.

### Immagine

- Nuovo documento con dimensioni a scelta e ridimensionamento con o senza
  mantenimento delle proporzioni.
- Ritaglio sulla selezione e ridimensionamento della tela trascinando i margini del
  foglio, come in Paint.
- Rotazione di 90° a destra e a sinistra, capovolgimento orizzontale e verticale.
- Inversione dei colori.

### File

- Apre **PNG, JPG/JPEG e BMP**, salva in **PNG, JPEG e BMP**.
- "Salva" e "Salva con nome", con il nome del file e l'asterisco di modifica non
  salvata nella barra del titolo.

### Comodità d'uso

- **Cronologia** con annulla e ripeti (fino a 20 passi, con un limite di memoria).
- **Zoom** da 25% a 1600%, righello attorno al foglio e coordinate del puntatore.
- **Interfaccia in italiano e in inglese**, tema **chiaro** e **scuro**, scelte
  ricordate tra un avvio e l'altro.
- Finestra iniziale compatta, centrata sullo schermo, con dimensione minima garantita.
- **Controllo aggiornamenti** nella sezione Info: un tasto interroga GitHub e dice se
  è uscita una versione nuova, con il collegamento diretto al download.

## Scorciatoie da tastiera

| Azione | Scorciatoia |
| --- | --- |
| Nuovo documento | `Ctrl+N` |
| Apri | `Ctrl+O` |
| Salva | `Ctrl+S` |
| Salva con nome | `Ctrl+Maiusc+S` |
| Annulla | `Ctrl+Z` |
| Ripeti | `Ctrl+Y` (oppure `Ctrl+Maiusc+Z`) |
| Taglia / Copia / Incolla | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` |
| Seleziona tutto | `Ctrl+A` |
| Cancella selezione | `Canc` |
| Annulla selezione o testo in corso | `Esc` |
| Nuovo livello | `Ctrl+Maiusc+N` |

## Installazione

Scarica dalla pagina delle release uno dei due file:

- `PaintCP-Setup-1.3.exe` — installer per Windows (consigliato): crea le voci nel menu
  Start e nel menu di disinstallazione.
- `paint_cp.exe` — eseguibile portabile, da avviare senza installare nulla.

Non serve installare altro: il programma è un singolo eseguibile.

## Compilare dai sorgenti

Serve una toolchain Rust stabile.

```bash
cargo build --release
```

L'eseguibile viene creato in `target/release/paint_cp`. Su Windows l'installer si
costruisce con [Inno Setup](https://jrsoftware.org/isinfo.php):

```bash
iscc installer/paint_cp.iss
```

## Note

Paint CP è creato da Peppe CP con l'aiuto della IA. Se ti è utile, nella sezione Info
del programma trovi il pulsante per una piccola donazione simbolica via PayPal.

## English summary

Paint CP is a lightweight drawing and photo-editing app written in Rust for Windows,
Linux and macOS. It offers brush, pencil, eraser, spray, smudge, fill, color picker,
text and shapes; rectangular selection with cut/copy/paste and crop; multiple layers
with opacity and merge/flatten; new, resize, rotate and flip; PNG/JPEG/BMP open and
save; undo/redo; zoom and ruler; Italian and English interface with light and dark
themes; and a built-in "Check for updates" button in the Info dialog. Download the
latest build from the releases page above.
