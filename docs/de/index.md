# Platz für Ihre Agents.

> Kostenlose Open-Source-App für Windows, die RAM, CPU und Speicherplatz auf Entwickler-PCs freigibt: beendet übrig gebliebene Dev-Server, MCP-Server und Agent-CLIs, löscht alte node_modules, Paket- und KI-Caches, mit umkehrbaren Windows-Optimierungen und echten Vorher/Nachher-Messungen.

Geben Sie RAM und CPU auf Windows-Entwicklerrechnern frei, die von KI-Agent-Sitzungen, vergessenen Dev-Servern und Windows-Hintergrunddiensten überladen sind. Jede Änderung lässt sich rückgängig machen.

Bisher von RigSweep-Nutzern freigegeben

Live-Summen von allen, die anonyme Statistik teilen

## Nicht nur Node

Die Dev-Bereinigung findet liegen gebliebene Dev-Server, Build-Daemons und Agent-Sitzungen in den Stacks, die Sie tatsächlich nutzen, dazu die WSL 2-/Docker-VM, die unbemerkt Gigabytes belegt.

## 📖 Warum es RigSweep gibt

Im KI-Zeitalter läuft auf einem Entwickler-Laptop nicht mehr nur eine Sache. Ein normaler Tag bedeutet mehrere Claude Code-Sitzungen in verschiedenen Terminals, einen Dev-Server pro Projekt, Build-Watcher und einen Browser voller Dokumentation und Vorschauen.

Jede Sitzung startet weitere Prozesse, und viele laufen weiter, nachdem Sie das Terminal geschlossen haben. Der Task-Manager zeigt nur eine Wand aus node.exe, python.exe und chrome.exe, ohne zu verraten, zu welcher Sitzung oder welchem Projekt sie gehören.

Dann habe ich auf die Festplatte geschaut. Ich habe parallel an drei Rust-Apps gebaut: RigSweep selbst, [KeepRun](https://github.com/yobotn/keeprun) (betreibt Node-, .NET- und Python-Apps als Windows-Dienste) und [Focuside](https://github.com/yobotn/focuside) (eine fokussierte IDE für KI-Coding-Agents). Der `target`-Ordner jedes Projekts war auf rund **40 GB** angewachsen, zusammen also **~120 GB**, alles Build-Ausgabe, die Cargo nie von selbst aufräumt. Und allein ein Projekt hatte rund **30 GB** an Git-Worktrees: zusätzliche Checkouts, die die Agents für parallele Aufgaben angelegt hatten, jeder mit eigener Build-Ausgabe, und alle noch da, lange nachdem die Arbeit gemergt war. Der Tab **Dev-Speicher** hat all das in einem einzigen Scan gefunden.

Nichts davon war ein Virus oder Datenmüll. Es waren die ganz normalen Reste der Arbeit mit KI-Agents, dazu Windows-Hintergrundkram, den ein Entwicklerrechner nicht braucht. Die üblichen „PC-Booster“-Apps machen entweder beängstigende, nicht umkehrbare Dinge oder wissen nichts über Dev-Workflows. Also habe ich das Tool gebaut, das ich selbst haben wollte.

**RigSweep zeigt, was Ihren Rechner wirklich belastet, räumt Liegengebliebenes auf und hält jede Änderung umkehrbar.**

## ✨ Was RigSweep kann

Entwickelt für Entwickler, die in Terminals und mit KI-Agents arbeiten, nicht für „PC-Booster“-Tricks. Eine App statt Task-Manager, einem Hardware-Info-Tool wie CPU-Z und einem Cleaner.

### Dev-Bereinigung

Verwaiste und inaktive Dev-Server, Build-Daemons und Agent-CLIs in Node, Python, Java, .NET und mehr, jeweils mit Prozessbaum, Projekt, RAM und Ports.

### Dev-Speicher

Löscht node_modules, .next, target und .venv in Projekten, die du nicht mehr anfasst, leert Paket-, GPU-Shader- und KI-Tool-Caches, entfernt Git-Worktrees, die Agents zurückgelassen haben, räumt Windows-Reste auf (alte temporäre Dateien, Update-Downloads, Absturzabbilder) und verkleinert virtuelle Docker- und WSL-Datenträger. Vor dem Klick siehst du, wie viel Platz danach frei ist, und die Caches, die sich selbst neu aufbauen, lassen sich wöchentlich automatisch leeren.

### Für KI-Agents

rigsweep --disk-report --json und --disk-clean --safe --dry-run laufen ohne Adminrechte, und rigsweep --mcp ist ein MCP-Server: Claude Code, Cursor oder Codex können freien Platz prüfen, sehen, was ihn belegt, und die Caches leeren, die sich selbst neu aufbauen.

### Hardware

Laufwerkszustand, ein Geschwindigkeitstest und alle Details zu CPU, GPU, RAM und Akku in einem Tab statt in vier separaten Tools. Ohne Treiberinstallation.

### GPU & KI

Was jede App auf Grafikkarte und NPU tut, aus denselben Zählern wie der Task-Manager, dazu die laufenden KI-Runtimes (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), ihre geladenen Modelle und der belegte Grafikspeicher.

### Netzwerk

Datenverkehr pro Adapter, welche App welche Verbindung oder welchen lauschenden Port hält, und eine Suche, die zeigt, wer Port 3000 oder eine gesperrte Datei bzw. einen Ordner nutzt.

### Prozesse & Autostart

CPU, RAM und Datenträger pro Prozess, die Dienste in jeder svchost.exe und die automatisch startenden Dienste und Aufgaben, die der Task-Manager nie zeigt.

### Konsolenfenster

Das Eingabeaufforderungsfenster, das kurz aufblitzt und verschwindet: RigSweep zeichnet den genauen Befehl auf, wer es gestartet hat (eine geplante Aufgabe, ein Dienst oder eine App) und was darin lief, und kann die Aufgabe dahinter abschalten, mit Sicherung.

### Finden

Sieh Ordner für Ordner, was auf jedem Laufwerk Platz belegt, verschiebe Paket-Caches von C: auf ein anderes Laufwerk oder ein Dev Drive und prüfe Programme, deren Deinstallationsprogramm fehlt oder nichts tut, große alte Dateien in Downloads und unsignierte Autostart-Programme, mit einem Microsoft-Defender-Scan per Klick.

### Boost-Bericht

Echte Vorher/Nachher-Messwerte statt erfundener Prozentangaben. Kopieren Sie sie als Markdown und teilen Sie Ihr Ergebnis.

### Umkehrbare Optimierungen

Telemetrie, Xbox, Indizierung, Hintergrund-Apps, eine dauerhafte Sperre für Edge und mehr, jeweils mit Risikostufe. Die Originalwerte werden gesichert und lassen sich mit einem Klick wiederherstellen.

### Browser-Bereinigung

Leert die Caches von Chrome, Edge, Brave, Vivaldi, Opera und Firefox. Anmeldungen, Cookies, Passwörter und Verlauf bleiben immer unberührt.

### Temperaturen

CPU- und GPU-Temperaturen mit 5-Minuten-Verlauf und einer Warnung, wenn Ihr PC dauerhaft heiß läuft. Kein Treiber, keine Administratorrechte.

### Chrome-Tab-Helfer

Eine winzige Erweiterung, die inaktive Tabs in den Ruhezustand versetzt, Duplikate schließt und Tabs nach Muster wie localhost:* schließt. Ohne Netzwerkzugriff.

### Einfach oder Profi

„Einfach“ zeigt das Dashboard und die Bereinigungen in klaren Worten. „Profi“ ergänzt Prozesse, Netzwerk, GPU-Engines und die Rohwerte. Schalte Monitore ab, die du nicht brauchst. Dann kosten sie keine CPU.

### Updates nach Ihrer Wahl

Standardmäßig aus. Suchen Sie bei Bedarf oder aktivieren Sie eine tägliche Suche. Updates werden nur installiert, wenn Sie klicken, und nur, wenn sie signiert sind.

### 11 Sprachen

English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 und العربية in App, Installationsprogramm und Chrome-Erweiterung, jeweils mit den offiziellen Windows-Begriffen der Sprache.

## 🧹 Liegen gebliebene Sitzungen sicher aufräumen

- **Verwaiste Prozesse**: Ihr Terminal ist geschlossen, aber sie laufen weiter, wie der klassische Zombie localhost:3000.
- **Inaktive Agents in offenen Terminals werden nie vorausgewählt**, denn sie warten auf Sie.
- **Language Server von Editoren sind gesperrt** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Beendet nach Prozessbaum, nie nach Port.** Jede Zeile wird direkt vor dem Beenden erneut geprüft, sodass zwei Projekte nie verwechselt werden.
- **MCP-Server und Headless-Browser, die Ihre Agents zurückgelassen haben,** werden ebenfalls gefunden, und eine Live-Anzeige zeigt Sekunden nach jeder Bereinigung, wie viel RAM Sie zurückbekommen haben.

## 🛟 Jede Änderung lässt sich rückgängig machen

- **Die Originalwerte werden in SQLite gesichert**, bevor die erste Änderung erfolgt.
- **Wiederherstellungspunkt** vor dem ersten Anwenden.
- **Watchdog** wendet Optimierungen erneut an, die Windows Update unbemerkt zurückgesetzt hat.
- **Bleibt immer unberührt:** Windows Update, das Defender-Modul und ~60 wichtige Dienste.

## 🛡️ Hat Defender es gelöscht?

RigSweep ist Open Source, aber noch nicht codesigniert. Tools, die Dienste ändern, sehen für Machine-Learning-Heuristiken wie Schadsoftware aus, daher kann Defender es fälschlicherweise erkennen (z. B. `Trojan:Win32/Bearfoos!ml`, wobei `!ml` für eine heuristische Vermutung steht).

Das Installationsprogramm **fragt**, ob nur sein eigener Ordner ausgeschlossen werden soll, und das Deinstallationsprogramm entfernt diesen Ausschluss wieder. Eine Codesignierung über die SignPath Foundation ist geplant. [Vollständige FAQ lesen →](https://github.com/yobotn/rigsweep/blob/main/README.de.md#antivirus-fehlalarme)

## ❓ Häufige Fragen von Entwicklern

### Was macht RigSweep?

RigSweep ist eine kostenlose Open-Source-App für Windows, gemacht für Entwickler, die mit KI-Coding-Agents arbeiten. Sie findet Dev-Server, Build-Daemons, MCP-Server und Agent-CLIs, die weiterlaufen, nachdem Sie ein Terminal geschlossen haben, zeigt jeden Prozess mit Projekt, RAM und Ports an und beendet die, die Sie auswählen. Außerdem löscht sie alte node_modules und Paket-Caches, leert Browser-Caches und wendet umkehrbare Windows-Optimierungen an.

### Wie beende ich node.exe-Prozesse, die nach dem Schließen des Terminals weiterlaufen?

Öffnen Sie die Dev-Bereinigung. Prozesse, deren Terminal nicht mehr existiert, werden als verwaist markiert und vorausgewählt, jeweils mit dem gesamten Prozessbaum, dem Projektordner und den belegten Ports (etwa ein hängendes localhost:3000). Klicken Sie auf „Auswahl beenden“, und RigSweep beendet jeden Baum, nachdem geprüft wurde, dass die Prozess-ID noch zum selben Programm gehört.

### Welche KI-Agents und Dev-Stacks werden unterstützt?

Claude Code, Codex, Gemini CLI, opencode, Aider und Goose sowie die MCP-Server und Headless-Browser, die sie starten. Dev-Stacks: Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust und Go sowie die WSL 2-/Docker-VM.

### Ist das sicher? Kann ich Änderungen rückgängig machen?

Nichts wird ohne Ihre Bestätigung beendet oder gelöscht. Jede Windows-Optimierung sichert zuerst den ursprünglichen Wert und lässt sich einzeln oder komplett wiederherstellen, und vor der ersten Änderung wird ein Systemwiederherstellungspunkt erstellt. Sprachserver von Editoren und zentrale Windows-Dienste sind gesperrt.

### Ist RigSweep kostenlos? Werden Daten gesammelt?

Ja, RigSweep ist dauerhaft kostenlos unter der MIT-Lizenz, ohne Konto und ohne Werbung. RigSweep fragt einmal, ob anonyme Nutzungsstatistiken geteilt werden sollen (was bereinigt wurde, nach Art und Größe, welche bekannten Apps am meisten brauchen und welche Hersteller-Software für die Hardware installiert ist). Vor deiner Antwort wird nichts gesendet, der genaue Bericht steht in den Einstellungen, und beim Ausschalten wird Gesendetes gelöscht. Pfade, Projektnamen oder persönliche Daten sind nie enthalten. Die Update-Prüfung bleibt aus, bis du sie einschaltest oder auf Jetzt prüfen klickst.

### Läuft es unter macOS oder Linux?

Nein. RigSweep ist für Windows 10 und 11 entwickelt, weil es mit Windows-Diensten, der Aufgabenplanung und Defender arbeitet. Die WSL 2-VM kann es trotzdem von Windows aus herunterfahren.

## ⚠️ Haftungsausschluss

RigSweep ändert Windows-Dienste, geplante Aufgaben, Registrierungsrichtlinien und Einstellungen von Microsoft Defender und kann laufende Prozesse beenden. Jede Optimierung wird gesichert und lässt sich wiederherstellen, und nichts geschieht ohne Ihre Bestätigung. Die Nutzung erfolgt dennoch auf eigenes Risiko. Die Software wird „wie besehen“ und ohne jegliche Gewährleistung bereitgestellt (MIT-Lizenz). Prüfen Sie jede Änderung, bevor Sie sie übernehmen, erstellen Sie eigene Sicherungen, und verwenden Sie RigSweep nicht auf Geräten, die von Ihrem Arbeitgeber oder Ihrer Schule verwaltet werden, ohne Zustimmung Ihrer IT-Abteilung. RigSweep ist ein unabhängiges Projekt und steht in keiner Verbindung zu Microsoft, Google, Anthropic, OpenAI oder anderen erwähnten Tools; alle Marken gehören ihren jeweiligen Inhabern.

## 🚀 Schaffen Sie Platz für Ihre Agents

Installieren, Referenzwert messen, aufräumen, dann erneut messen.

### 🔏 Codesignierung

Kostenlose Codesignierung bereitgestellt von SignPath.io, Zertifikat von der SignPath Foundation (Antrag läuft; aktuelle Releases sind noch nicht signiert). Die Installationsprogramme werden von GitHub Actions aus dem getaggten Quellcode erstellt. Keine Telemetrie, keine Analysen. Die einzige Netzwerkanfrage ist die Suche nach Updates, und nur, wenn Sie sie aktivieren oder auf „Jetzt suchen“ klicken.
