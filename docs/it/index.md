# Spazio per i tuoi agenti.

> App gratuita e open source per Windows che libera RAM, CPU e disco sui PC di sviluppo: chiude server di sviluppo, server MCP e CLI degli agenti rimasti aperti, elimina vecchi node_modules e cache di pacchetti e strumenti IA, con modifiche reversibili e misurazioni reali prima/dopo.

Libera RAM e CPU sui PC Windows di sviluppo intasati da sessioni di agenti AI, dev server dimenticati e servizi di Windows in background. Ogni modifica è reversibile.

Liberato finora dagli utenti di RigSweep

Totali in tempo reale da chi condivide statistiche anonime

## Non solo Node

Pulizia dev trova dev server, daemon di build e sessioni di agenti rimasti attivi negli stack che usi davvero, più la VM di WSL 2 / Docker che occupa gigabyte senza farsi notare.

## 📖 Perché esiste

Nell'era dell'AI un portatile da sviluppatore non esegue più una sola cosa. Una giornata normale significa diverse sessioni di Claude Code in terminali diversi, un dev server per progetto, watcher di build e un browser pieno di documentazione e anteprime.

Ogni sessione avvia altri processi, e molti restano in esecuzione anche dopo aver chiuso il terminale. Gestione attività mostra solo un muro di node.exe, python.exe e chrome.exe, senza dirti a quale sessione o progetto appartengono.

Poi ho guardato il disco. Stavo sviluppando tre app Rust in parallelo: RigSweep stesso, [KeepRun](https://github.com/yobotn/keeprun) (mantiene in esecuzione app Node, .NET e Python come servizi di Windows) e [Focuside](https://github.com/yobotn/focuside) (un IDE essenziale per agenti di coding IA). La cartella `target` di ogni progetto era arrivata a circa **40 GB**, quindi **~120 GB** in totale, tutto output di build che Cargo non pulisce mai da solo. La scheda **Disco dev** le ha trovate tutte e tre con una sola scansione.

Niente di tutto questo era un virus o spazzatura. Erano i normali avanzi del lavoro con gli agenti IA, più roba di Windows in background che a un PC da sviluppo non serve. Le app “PC booster” esistenti o fanno cose spaventose e irreversibili, o non sanno niente dei flussi di lavoro di sviluppo, così ho costruito lo strumento che volevo.

**RigSweep mostra cosa sta davvero usando il tuo PC, fa pulizia di ciò che non serve più e rende ogni modifica reversibile.**

## ✨ Cosa fa

Pensato per gli sviluppatori che vivono tra terminali e agenti AI, non per i trucchi da "PC booster". Un'unica app al posto di Gestione attività, di uno strumento hardware come CPU-Z e di un programma di pulizia.

### Pulizia dev

Dev server, daemon di build e CLI di agenti orfani o inattivi in Node, Python, Java, .NET e altro, ognuno con il suo albero di processi, progetto, RAM e porte.

### Report boost

Misurazioni reali prima/dopo invece di percentuali inventate. Puoi copiarle come Markdown e condividere il tuo risultato.

### Helper schede per Chrome

Una piccola estensione per sospendere le schede inattive, chiudere i duplicati e chiudere le schede in base a un pattern come localhost:*. Non ha accesso alla rete.

### Ottimizzazioni reversibili

Telemetria, Xbox, indicizzazione, app in background, blocco del ritorno di Edge e altro, ognuna con un livello di rischio. I valori originali vengono salvati in un backup e si ripristinano con un clic.

### Processi e avvio

CPU, RAM e disco per processo, i servizi in ogni svchost.exe, e i servizi e le attività ad avvio automatico che Gestione attività non mostra mai.

### 11 lingue

English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 e العربية nell'app, nel programma di installazione e nell'estensione per Chrome, con i termini ufficiali di Windows di ogni lingua.

### Temperature

Temperature di CPU e GPU con l'andamento degli ultimi 5 minuti e un avviso quando il PC resta troppo caldo. Nessun driver, nessun privilegio di amministratore.

### Aggiornamenti, decidi tu

Disattivati per impostazione predefinita. Cerca quando vuoi o attiva un controllo giornaliero. Gli aggiornamenti si installano solo quando fai clic, e solo se sono firmati.

### Pulizia browser

Svuota le cache di Chrome, Edge, Brave, Vivaldi, Opera e Firefox. Accessi, cookie, password e cronologia non vengono mai toccati.

### Disco dev

Elimina node_modules, .next, target e .venv nei progetti che non tocchi più, pulisce le cache di pacchetti, shader GPU e strumenti IA, rimuove i worktree git lasciati dagli agenti, toglie gli avanzi di Windows (vecchi file temporanei, download degli aggiornamenti, dump di arresto) e riduce i dischi virtuali di Docker e WSL. Prima del clic vedi quanto spazio libero avrai, e può svuotare ogni settimana le cache che si ricostruiscono da sole.

### Hardware

Salute dei dischi, un test di velocità e i dettagli di CPU, GPU, RAM e batteria in un'unica scheda, invece di quattro strumenti separati. Nessun driver installato.

### GPU e IA

Cosa fa ogni app sulla scheda video e sulla NPU, dagli stessi contatori di Gestione attività, più i runtime di IA in esecuzione (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), i modelli caricati e la memoria video che occupano.

### Rete

Traffico per scheda di rete, quale app tiene ogni connessione o porta in ascolto, e una ricerca che ti dice chi usa la porta 3000 o un file o una cartella bloccati.

### Trova

Guarda cosa occupa spazio su ogni unità cartella per cartella, sposta le cache dei pacchetti da C: su un'altra unità o un Dev Drive e controlla i programmi senza programma di disinstallazione, i file grandi e vecchi in Download e i programmi di avvio non firmati, con una scansione di Microsoft Defender in un clic.

### Semplice o Pro

Semplice mostra la dashboard e le pulizie con parole chiare. Pro aggiunge Processi, Rete, motori GPU e i dati grezzi. Spegni i monitor che non ti servono, così non consumano CPU.

### Finestre console

Quella finestra del Prompt dei comandi che lampeggia e sparisce: RigSweep registra il comando esatto che ha eseguito, chi l'ha avviata (un'attività pianificata, un servizio o un'app) e cosa è girato al suo interno, e può disattivare l'attività che c'è dietro, con un backup.

### Per gli agenti IA

rigsweep --disk-report --json e --disk-clean --safe --dry-run funzionano senza diritti di amministratore, e rigsweep --mcp è un server MCP: Claude Code, Cursor o Codex possono controllare lo spazio libero, vedere cosa lo occupa e svuotare le cache che si ricostruiscono da sole.

## 🧹 Elimina le sessioni inutilizzate, in sicurezza

- **Orfani**: il loro terminale è chiuso ma sono ancora in esecuzione, come il classico zombie localhost:3000.
- **Gli agenti inattivi nei terminali aperti non vengono mai preselezionati**, perché stanno aspettando te.
- **I language server degli editor sono bloccati** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Termina per albero di processi, mai per porta.** Ogni riga viene verificata di nuovo subito prima di terminarla, così due progetti non vengono mai confusi.
- **Anche i server MCP e i browser headless lasciati in giro dai tuoi agenti** vengono trovati, e un indicatore in tempo reale mostra la RAM recuperata pochi secondi dopo ogni pulizia.

## 🛟 Ogni modifica si può annullare

- **I valori originali vengono salvati in un backup** in SQLite prima della prima modifica.
- **Punto di ripristino del sistema** prima della prima applicazione.
- **Watchdog**: riapplica le ottimizzazioni che Windows Update ha annullato senza dirtelo.
- **Non tocca mai** Windows Update, il motore di Defender o ~60 servizi di sistema essenziali.

## 🛡️ Defender l'ha eliminato?

RigSweep è open source ma non ha ancora la firma del codice. Gli strumenti che modificano i servizi sembrano malware all'euristica basata su machine learning, quindi Defender può segnalarlo per errore (ad es. `Trojan:Win32/Bearfoos!ml`, dove `!ml` indica un'ipotesi euristica).

Il programma di installazione **chiede** se escludere solo la propria cartella, e il programma di disinstallazione rimuove di nuovo l'esclusione. È prevista la firma del codice tramite SignPath Foundation. [Leggi le FAQ complete →](https://github.com/yobotn/rigsweep/blob/main/README.it.md#falsi-positivi-antivirus)

## ❓ Le domande degli sviluppatori

### Cosa fa RigSweep?

RigSweep è un'app gratuita e open source per Windows pensata per gli sviluppatori che lavorano con agenti di programmazione IA. Trova server di sviluppo, daemon di build, server MCP e CLI di agenti che restano in esecuzione dopo la chiusura di un terminale, mostra ciascuno con progetto, RAM e porte, e termina quelli che scegli. Elimina anche vecchie cartelle node_modules e cache dei pacchetti, svuota le cache del browser e applica ottimizzazioni di Windows reversibili.

### Come fermo i processi node.exe che restano attivi dopo aver chiuso il terminale?

Apri Pulizia dev. I processi il cui terminale non esiste più sono segnati come orfani e preselezionati, con l'intero albero dei processi, la cartella del progetto e le porte in ascolto (come un localhost:3000 bloccato). Fai clic su Termina selezionati e RigSweep termina ogni albero dopo aver verificato che l'ID del processo appartenga ancora allo stesso programma.

### Quali agenti IA e stack di sviluppo supporta?

Claude Code, Codex, Gemini CLI, opencode, Aider e Goose, oltre ai server MCP e ai browser headless che avviano. Stack di sviluppo: Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust e Go, più la VM di WSL 2 / Docker.

### È sicuro? Posso annullare le modifiche?

Nulla viene terminato o eliminato senza la tua conferma. Ogni ottimizzazione di Windows salva prima il valore originale e si può ripristinare singolarmente o tutte insieme, e prima della prima modifica viene creato un punto di ripristino del sistema. I language server degli editor e i servizi essenziali di Windows sono bloccati.

### RigSweep è gratuito? Raccoglie dati?

Sì, è gratuito per sempre con licenza MIT, senza account e senza pubblicità. RigSweep chiede una volta se condividere statistiche d'uso anonime (cosa ha pulito, per tipo e dimensione, quali app note consumano di più e quale software dei produttori di hardware è installato). Non viene inviato nulla prima della tua risposta, il report esatto è visibile nelle Impostazioni e disattivandolo si elimina quanto inviato. Non include mai percorsi, nomi di progetti o dati personali. Il controllo aggiornamenti resta disattivato finché non lo attivi o fai clic su Controlla ora.

### Funziona su macOS o Linux?

No. RigSweep è progettato per Windows 10 e 11, perché lavora con i servizi di Windows, l'Utilità di pianificazione e Defender. Può comunque arrestare la VM di WSL 2 da Windows.

## ⚠️ Avviso legale

RigSweep modifica servizi di Windows, attività pianificate, criteri del Registro di sistema e impostazioni di Microsoft Defender, e può terminare processi in esecuzione. Ogni ottimizzazione viene salvata in un backup e può essere ripristinata, e nulla avviene senza la tua conferma, ma lo usi a tuo rischio. Il software è fornito "così com'è", senza garanzie di alcun tipo (licenza MIT). Controlla ogni modifica prima di applicarla, conserva i tuoi backup e non usarlo su dispositivi gestiti dal lavoro o dalla scuola senza l'approvazione del reparto IT. RigSweep è un progetto indipendente, non affiliato a Microsoft, Google, Anthropic, OpenAI o ad altri strumenti citati; tutti i marchi appartengono ai rispettivi proprietari.

## 🚀 Dai un po' di spazio ai tuoi agenti

Installa, acquisisci una misurazione di riferimento, fai pulizia e poi misura di nuovo.

### 🔏 Firma del codice

Firma del codice gratuita fornita da SignPath.io, certificato di SignPath Foundation (richiesta in corso; le release attuali non sono ancora firmate). I programmi di installazione vengono compilati da GitHub Actions a partire dal sorgente con tag. Nessuna telemetria, nessuno strumento di analisi. L'unica richiesta di rete è la ricerca di aggiornamenti, e solo se la attivi o fai clic su Cerca ora.
