<div align="center">

<img src="docs/icon.png" width="80" alt="" />

# RigSweep

**De la place pour vos agents.** Libérez RAM, CPU et disque sur les machines de dev Windows encombrées par les sessions d'agents IA.

**[🌐 Site web](https://rigsweep.com)** · **[⬇️ Télécharger](https://github.com/yobotn/rigsweep/releases/latest)** · **[📖 FAQ antivirus](#faux-positifs-antivirus)**

[🇬🇧 English](README.md) · [🇩🇪 Deutsch](README.de.md) · [🇪🇸 Español](README.es.md) · 🇫🇷 Français · [🇮🇹 Italiano](README.it.md) · [🇧🇷 Português](README.pt-BR.md) · [🇷🇺 Русский](README.ru.md) · [🇯🇵 日本語](README.ja.md) · [🇰🇷 한국어](README.ko.md) · [🇨🇳 简体中文](README.zh-CN.md) · [🇸🇦 العربية](README.ar.md)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-orange?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-green)
![Price: free](https://img.shields.io/badge/price-free-brightgreen)

<img src="docs/screenshots/cleanup-fr.png" alt="Nettoyage dev RigSweep" width="860">

</div>

---

## Pourquoi je l'ai créé

<!-- Generated from docs/locales (story.*, stat.*) by scripts/prerender-site.js. Edit it there. -->

À l'ère de l'IA, un portable de dev ne fait plus tourner une seule chose. Une journée normale, ce sont plusieurs sessions Claude Code dans différents terminaux, un serveur de dev par projet, des watchers de build et un navigateur rempli de docs et de previews.

Chaque session lance de nouveaux processus, et beaucoup continuent de tourner après la fermeture du terminal. Le Gestionnaire des tâches n'affiche qu'un mur de node.exe, python.exe et chrome.exe, sans dire à quelle session ou quel projet ils appartiennent.

- 🌐 Chrome (37 processus): **6.5 GB**
- ⬢ Node + Claude (14 processus): **2.5 GB**
- 🧩 Services Windows en cours d'exécution: **~150**
- 💾 Dossiers target Rust (3 projets): **~120 GB**
- 🌿 Worktrees git (1 projet): **~30 GB**

*Mesuré sur le portable de 32 Go de l'auteur en pleine journée de travail.*

Ensuite, j'ai regardé le disque. Je développais trois applis Rust en parallèle : RigSweep lui-même, [KeepRun](https://github.com/yobotn/keeprun) (fait tourner des applis Node, .NET et Python comme services Windows) et [Focuside](https://github.com/yobotn/focuside) (un IDE épuré pour les agents de code IA). Le dossier `target` de chaque projet avait grossi jusqu'à environ **40 Go**, soit **~120 Go** au total, uniquement des sorties de build que Cargo ne nettoie jamais de lui-même. Et un seul projet comptait à lui seul environ **30 Go** de worktrees git : des copies de travail supplémentaires créées par les agents pour des tâches en parallèle, chacune avec ses propres sorties de build, toujours là bien après la fusion du travail. L'onglet **Disque dev** a tout trouvé en une seule analyse.

Rien de tout ça n'était un virus ou des fichiers indésirables. C'étaient les restes ordinaires du travail avec des agents IA, plus des tâches de fond de Windows dont un poste de dev n'a pas besoin. Les applis « PC booster » existantes font soit des choses inquiétantes et irréversibles, soit ignorent tout des workflows de dev. Alors j'ai construit l'outil que je voulais.

> **RigSweep montre ce qui utilise vraiment votre machine, balaie ce qui traîne, et garde chaque changement réversible.**

---

## Fonctionnalités

### Nettoyage dev : processus de dev oubliés, quel que soit le stack
Trouve les **serveurs de dev, daemons de build et CLI d'agents IA** et regroupe chacun avec **tout son arbre de processus**, son dossier de projet, son âge, sa RAM et ses ports en écoute.

| Stack | Ce qu'il trouve |
|---|---|
| ⬢ Node · Bun · Deno | serveurs de dev (Vite, Astro, Next…), watchers, esbuild |
| 🐍 Python | uvicorn, Flask, Django, Jupyter, Streamlit, Celery, processus de venv, serveurs MCP en Python |
| ☕ Java | daemons Gradle et Kotlin, Maven, Spring Boot, Quarkus |
| 🟣 .NET | `dotnet watch/run`, serveur de compilation Roslyn, nœuds MSBuild restants |
| 💎 Ruby · 🐘 PHP · 🦀 Rust · 🐹 Go | `rails`/`ruby`, `php -S`/artisan, `cargo watch/run`, `go run` |
| 🤖 CLI d'agents | Claude Code, Codex, Gemini CLI, opencode, Aider, Goose |
| 🔌 Serveurs MCP | serveurs lancés par votre agent (`@modelcontextprotocol/*`, `@playwright/mcp`, `mcp-server-*`, FastMCP…) et restés actifs après sa fermeture, même derrière un lanceur `cmd /c npx` |
| 🌐 Navigateurs automatisés | Chrome et Edge de Playwright / Puppeteer / `--headless` avec tout leur arbre de processus de rendu. Votre propre navigateur n'est jamais listé |
| 🐧 WSL 2 / Docker | la VM Linux (`vmmemWSL`), arrêtée proprement avec `wsl --shutdown`, jamais présélectionnée |

Python et Java font aussi tourner des applications de bureau ordinaires : pour eux, seuls les processus avec une signature de dev claire sont listés.

- 🧟 **Orphelin** : son terminal a été fermé mais le processus tourne encore. Il est présélectionné, même s'il occupe encore un port (le classique `localhost:3000` zombie).
- 🔌 **Serveur MCP** : signalé sur chaque ligne concernée, pour distinguer le serveur d'outils laissé par un agent de votre propre serveur de dev.
- 👯 **Ancienne copie** : un serveur MCP dont une copie plus récente tourne aussi (par exemple dans votre session d'agent actuelle) est signalé, pour repérer facilement celui qui reste.
- 📈 **Mesure en direct** : la RAM et le CPU sont mesurés toutes les 2 secondes tant que RigSweep tourne (fenêtre ou zone de notification). Juste après un nettoyage, manuel ou automatique, il affiche **la RAM réellement récupérée** et le CPU avant → après, mesurés et non estimés.
- 🌡️ **Suivi des températures** : les températures du CPU et du GPU s'affichent en direct sur le Tableau de bord avec la tendance et le pic des 5 dernières minutes, et une alerte si la machine reste au-dessus de 90 °C. Le GPU utilise le même compteur que le Gestionnaire des tâches ; le CPU, la zone thermique ACPI (aucun pilote installé, pas besoin des droits d'administrateur ; approximatif sur beaucoup d'ordinateurs de bureau). Cliquez sur la vignette pour basculer entre °C et °F.
- 💤 **Inactif** : aucune activité CPU depuis longtemps. Les runtimes sans port ouvert sont présélectionnés. Une **session d'agent inactive dans un terminal ouvert n'est jamais présélectionnée**, car elle vous attend simplement (et `claude --resume` la ramène de toute façon).
  La durée qui compte comme inactivité se règle dans les **Paramètres** (en minutes, 120 par défaut ; par ex. 5).
- 🧹 **Nettoyage automatique** (désactivé par défaut, dans les Paramètres) : chaque minute, RigSweep arrête les runtimes de dev inactifs qu'il a vus sans activité CPU pendant toute la durée d'inactivité. Les CLI d'agents, la VM WSL/Docker et les processus appartenant à l'éditeur ne sont jamais arrêtés automatiquement, et chaque arbre arrêté est consigné dans l'Historique.
- 🟠🔴 **Indicateur dans la zone de notification** : l'icône de la zone de notification (et le bouton de la barre des tâches tant que la fenêtre est ouverte) affiche un point orange quand les processus de dev inactifs occupent ≥ 1 Go et un point rouge à partir de ≥ 2 Go. Survolez l'icône pour voir combien. Les seuils se règlent dans les Paramètres, où le point peut aussi être désactivé.
- ♻️ **Daemon de build** : les serveurs Gradle, Kotlin et Roslyn sont faits pour rester en arrière-plan. Les arrêter est sans danger ; ils redémarrent au prochain build.
- 🔒 **Verrouillé** : les processus appartenant à **VS Code, Cursor, JetBrains, Visual Studio…** (vos serveurs de langage) ou à une app de bureau ne peuvent jamais être terminés d'ici.
- 🎯 **Une ligne ou plusieurs** : cochez des lignes et cliquez sur **Terminer la sélection**, ou utilisez le bouton **Terminer** d'une seule ligne. Les deux affichent la même confirmation, avec ce qui sera libéré.
- ✅ Rien n'est terminé sans votre confirmation, et la réutilisation de PID est vérifiée avant chaque arrêt.

### Disque dev : node_modules, caches et worktrees
Les vieux projets gardent des gigaoctets de `node_modules`, `.next`, `target`, `bin/obj` et `.venv` bien après que vous ayez arrêté d'y travailler, les caches de paquets ne font que grossir, et les outils et agents d'IA laissent derrière eux caches et worktrees. RigSweep repère tout cela et supprime ce que vous choisissez.

- 🗂️ **Projets** : dossiers de build et de dépendances de Node, Rust, .NET, Java/Gradle/Maven et Python. Un dossier ne compte que si son fichier de projet est juste à côté (`package.json`, `Cargo.toml`, `*.csproj`, `pyvenv.cfg`…), un dossier `build` quelconque n'est donc jamais touché. Seuls les projets que personne n'a modifiés depuis 30 jours (réglable) sont présélectionnés, et ceux qu'un processus utilise sont verrouillés 🔒.
- 📦 **Caches de paquets** : npm, npx, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Puppeteer, Hugging Face. uv et Go sont nettoyés avec leurs propres commandes (`uv cache prune`, `go clean`) ; pour Playwright et Puppeteer, seules les anciennes versions des navigateurs partent, la plus récente reste.
- 🎮 **GPU et apprentissage automatique** : caches de shaders NVIDIA, AMD, Intel et DirectX, cache de calcul CUDA, modèles PyTorch Hub et extensions compilées, cache de noyaux Triton et paquets conda inutilisés (via `conda clean --all`). Les modèles Ollama et LM Studio sont affichés avec leur taille, jamais supprimés.
- 🤖 **Outils d'IA et éditeurs** : les caches de Cursor, VS Code, Windsurf, de l'application de bureau Claude, de Codex et de ChatGPT, ignorés tant que l'application est ouverte. La sauvegarde d'état et les anciens checkpoints de Cursor peuvent aussi partir (jamais présélectionnés). L'historique des chats (la base de chats de Cursor, les conversations Claude Code, les sessions Codex) est seulement affiché avec sa taille, pour que vous voyiez où est passé l'espace. Une fois Cursor désinstallé, sa base de chats et ses checkpoints peuvent aller dans la Corbeille.
- 🌿 **Worktrees git** : les worktrees créés par Claude Code, Cursor, Codex ou vous-même, avec leur branche, leur dépôt et depuis combien de temps ils dorment. Ils sont retirés avec `git worktree remove`, qui refuse tout worktree contenant du travail non commité ou des fichiers non suivis ; la branche et ses commits restent. Les fichiers ignorés par git qui partiraient avec (comme `.env`) sont listés d'abord, et les worktrees ne sont jamais présélectionnés. Un doute ? **Vérifier avec l'IA** lance en lecture seule l'outil d'IA en ligne de commande que vous utilisez déjà (Claude Code, Codex, Gemini CLI ou Grok) : RigSweep lui transmet ce que git sait (modifications, fichiers ignorés, commits non poussés, stashes), il ne peut que lire les fichiers, et son verdict SAFE / NOT SAFE s'affiche sur la ligne. Aucun installé ? Le prompt est copié pour votre agent à la place.
- 🖥️ **Restes de Windows** : vos fichiers temporaires et ceux de Windows inchangés depuis 7 jours, les téléchargements de Windows Update, les fichiers de l'Optimisation de la distribution, les vidages mémoire et rapports d'erreurs, le cache des miniatures et la Corbeille (jamais cochée à votre place). Windows.old est seulement affiché avec sa taille.
- 🐳 **Disques virtuels Docker et WSL** : le `.vhdx` de Docker Desktop et de chaque distribution WSL grossit sans jamais rétrécir tout seul. **Compacter** le ramène à ce que Linux utilise vraiment. Docker Desktop est d'abord quitté et WSL arrêté, et rien n'est supprimé à l'intérieur du disque.
- 🎯 **Sélection auto** : directement dans l'onglet Disque dev, choisissez depuis combien de temps un projet doit être inactif (3 à 365 jours), quels dossiers sont cochés (fichiers de build, dépendances, caches de paquets) et une taille minimale.
- 🚫 **Ne jamais nettoyer** : chaque cache peut être désactivé. Ceux qui impliquent un gros retéléchargement (Playwright, Puppeteer, Hugging Face, Gradle, Maven, paquets NuGet, modules Go) sont désactivés au départ.
- 🙈 **Exclusions** : AppData, les dossiers d'outils de votre profil (`.cargo`, `.nvm`, `.vscode`…), `.git` et Windows sont toujours ignorés. Ajoutez vos propres chemins ou noms de dossier (`vendor`, `old-*`) dans les Paramètres, ou cliquez sur **Exclure** sur un projet.
- 💽 **Jauge du disque** : Disque dev, Nettoyage navigateurs et le Tableau de bord s'ouvrent sur l'espace libre de votre disque maintenant et après le nettoyage (« 58 Go libres → 81 Go libres »), avec un grand bouton **Libérer X Go**. Le Tableau de bord additionne tout ce que les analyses ont trouvé.
- 🔁 **Nettoyer automatiquement** (Paramètres → Disque dev, désactivé par défaut) : une fois par semaine, et si vous le souhaitez quand C: passe sous une taille de votre choix, RigSweep vide uniquement les caches qui se reconstruisent seuls. Jamais les projets, la Corbeille ni ce qui serait long à retélécharger.
- ⏸️ **Pause, reprise ou arrêt** d'un long nettoyage, avec une barre de progression. Un nettoyage interrompu par un redémarrage peut reprendre là où il s'était arrêté.
- ✅ Analyse par défaut votre profil utilisateur (Bureau, Documents, Téléchargements, OneDrive…). Rien n'est supprimé sans votre confirmation, et chaque dossier est revérifié juste avant la suppression.

### Matériel : un onglet au lieu de quatre outils
L'essentiel de ce pour quoi vous ouvririez CPU-Z, GPU-Z, CrystalDiskInfo et CrystalDiskMark, lu directement dans Windows. Aucun pilote n'est installé.

- 💽 **État des disques** : En bon état / Usé / Problème pour chaque disque, avec durée de vie restante, température, heures de fonctionnement et erreurs non corrigées. Un disque défaillant déclenche un avertissement sur le tableau de bord.
- ⚡ **Test de vitesse** : lecture et écriture séquentielles, plus lectures aléatoires 4K, pour chaque lecteur. Il écrit un fichier temporaire de 512 Mo puis le supprime.
- 🧮 **Processeur, carte graphique, mémoire** : modèle, cœurs et threads, cache, virtualisation (requise par WSL 2 et Docker), mémoire vidéo et pilote, type de RAM, vitesse et emplacements libres.
- 🔋 **Système et batterie** : carte mère, BIOS, build de Windows, durée depuis le démarrage et usure de la batterie sur les portables.

<img src="docs/screenshots/hardware.png" alt="Matériel" width="860">

### GPU et IA : ce qui tourne sur votre carte graphique et votre NPU
Ce que fait chaque application sur le GPU et le NPU, lu dans les mêmes compteurs que le Gestionnaire des tâches. Rien n'est installé, et rien n'est interrogé tant que la page est fermée.

- 🎮 **Par carte** : utilisation, mémoire dédiée et partagée utilisée et, en mode Pro, une jauge pour chaque moteur actif (3D, Compute, Copy, Video…).
- 📋 **Par application** : utilisation du GPU, moteur et mémoire vidéo dédiée de chaque application qui utilise le GPU, plus la mémoire partagée en mode Pro.
- 🧠 **NPU** : le processeur neuronal apparaît à côté des GPU, et l'utilisation du NPU par chaque application s'affiche avec son utilisation du GPU.
- 🤖 **Charge IA** : les environnements d'IA locaux en cours d'exécution (Ollama, LM Studio, llama.cpp, Jan, LocalAI, KoboldCpp, ComfyUI, vLLM, Python avec PyTorch ou Transformers…) et la mémoire vidéo qu'ils occupent. Pour Ollama, les modèles chargés et la VRAM de chacun sont aussi listés.

<img src="docs/screenshots/gpu.png" alt="GPU et IA" width="860">

### Rapport de gains : un vrai avant/après
Pas de faux chiffres du genre « votre PC est 87 % plus rapide ». RigSweep **mesure** votre machine et compare deux relevés : RAM utilisée/libre, CPU moyen, nombre de processus, services en cours d'exécution, RAM de Chrome et RAM Node/agents.

- 📸 Une **mesure de référence** est prise automatiquement avant votre premier réglage.
- 🔁 Une paire **avant/après** est prise autour de chaque Nettoyage dev.
- 📋 **Copier en Markdown** pour partager votre résultat dans une issue, un post ou ce README.

### Assistant d'onglets Chrome
Une app de bureau **ne peut pas** fermer les onglets Chrome, donc RigSweep fournit une petite extension ([`chrome-extension/`](chrome-extension/)) sans accès réseau, avec seulement les autorisations `tabs` / `storage` / `alarms` :
- 💤 **Mettre en veille les onglets inactifs** : leur mémoire est libérée, mais l'onglet reste dans la barre d'onglets
- 🧹 **Fermer les onglets en double**
- 🎯 **Fermer les onglets par motif**, comme `localhost:*` ou `*.vercel.app`
- ⏰ **Veille automatique** toutes les 10 minutes (facultative)

### Réglages Windows réversibles
Un catalogue soigné ([`catalog/tweaks.json`](src-tauri/catalog/tweaks.json)) : télémétrie, Xbox, indexeur de recherche, apps en arrière-plan, exclusions Defender pour les dossiers de dev, mode de gestion de l'alimentation, etc., chacun avec un niveau de risque :

| Niveau | Signification |
|---|---|
| 🟢 `safe` | Aucun effet secondaire notable (présélectionné) |
| 🟡 `moderate` | Léger compromis, expliqué |
| 🔴 `caution` | Désactive une fonctionnalité ; confirmation requise |

<img src="docs/screenshots/tweaks.png" alt="Réglages" width="860">

- 💾 **Chaque valeur d'origine est sauvegardée** dans SQLite avant le premier changement. Vous pouvez restaurer un réglage, une catégorie ou tout.
- 🛟 Un **point de restauration système** est créé avant la première application.
- 🔁 Une **surveillance** réapplique les réglages qu'une mise à jour Windows a annulés en silence, et compte combien de fois cela arrive.
- 🧭 **Edge ne revient pas** : désinstallez Edge depuis les Paramètres, puis un réglage empêche Edge Update et Windows Update de le réinstaller. Le runtime WebView2 dont d'autres applications ont besoin n'est pas touché.
- 🚫 Jamais touchés : Windows Update, le moteur Defender, RPC/DCOM/LSA et ~60 autres services essentiels ([`PROTECTED_SERVICES`](src-tauri/src/catalog.rs)).

### Analyseur de processus
CPU / RAM / disque par processus, et **quels services tournent dans chaque `svchost.exe`**. Désactivez un service ou terminez un processus directement depuis la liste (les éléments essentiels sont verrouillés 🔒). Chaque application s'affiche avec sa propre icône.

- ❓ **Pourquoi ?** : cliquez sur un processus pour voir de quoi il s'agit : CPU, mémoire, disque, threads, éditeur, fichier et ligne de commande, le programme qui l'a lancé et ceux qu'il a lancés. Les processus Windows connus (Defender, Windows Search, Windows Update, svchost, WMI…) sont accompagnés d'une explication simple de ce qui les occupe.
- 🛡️ **Ce que Defender analyse** : le Tableau de bord affiche **Voir pourquoi** à côté d'un processus qui consomme beaucoup de CPU, et plus tôt encore pour Microsoft Defender. Pour Defender, **Enregistrer ce qu'il analyse** l'observe pendant 20 secondes et liste les dossiers, les types de fichiers et les processus qui ont déclenché les analyses. Si l'essentiel vient de votre travail de dev, il vous oriente vers le réglage d'exclusions Defender pour les dossiers de dev.

<img src="docs/screenshots/processes.png" alt="Processus" width="860">

### Démarrage : ce que le Gestionnaire des tâches ne montre pas
La liste « Applications de démarrage » du Gestionnaire des tâches omet presque tout ce qui démarre avec Windows. RigSweep affiche tout, hors composants de Windows lui-même :

- 🧩 **Services à démarrage automatique** des applications et pilotes (clients VPN, utilitaires constructeur, programmes de mise à jour). Ils passent en Manuel : ils ne démarrent plus avec Windows, mais l'application peut toujours les lancer quand vous l'ouvrez.
- ⏰ **Tâches planifiées** exécutées à l'ouverture de session, au démarrage ou selon une planification.
- 🚀 **Applications de démarrage** du registre et du dossier Démarrage, désactivées avec le même interrupteur que le Gestionnaire des tâches.
- 💾 Chaque modification est sauvegardée et restaurable, et si une mise à jour réactive une application, la surveillance la désactive à nouveau.

<img src="docs/screenshots/startup.png" alt="Démarrage" width="860">

### Fenêtres de console : c'était quoi, cette fenêtre ?
Une fenêtre d'invite de commandes ou PowerShell s'ouvre, se referme un instant plus tard, et vous ne saurez jamais ce qui a tourné. **Accélérer → 🪟 Fenêtres de console** les enregistre toutes :

- 🪟 Chaque fenêtre de console qui s'ouvre et se ferme d'elle-même en moins d'une minute est listée avec la **commande exacte qu'elle a exécutée**, même si elle n'est restée ouverte que quelques millisecondes.
- 🔎 **Qui l'a lancée** : la tâche planifiée (avec son nom), le service, l'application ou l'Explorateur, plus la chaîne complète des processus parents dans Détails.
- 📜 **Ce qui a tourné dedans** : les programmes qu'elle a démarrés (par ex. un fichier `.bat` qui appelle `curl` et `reg`) et son code de sortie.
- 🔁 La même commande venant de la même source tient sur une seule ligne avec un compteur (×12) : une tâche qui se déclenche toutes les heures saute aux yeux.
- ⏹ **Désactiver…** désactive la tâche planifiée tierce qui se cache derrière, avec une sauvegarde, et s'annule depuis Démarrage ou Réglages comme toute autre modification du démarrage.
- 🙈 Les commandes lancées en mode masqué ne sont pas listées, seulement les fenêtres que vous pouviez vraiment voir. RigSweep lit les lancements de processus dans le flux d'événements du noyau de Windows (ETW) et ne modifie rien en observant. Désactivez-le dans Paramètres → Apparence et fonctions.

### Réseau : quelle application occupe quel port
- 📶 **Par carte réseau** : débit descendant et montant, vitesse de liaison et trafic depuis le démarrage. Windows ne donne le nombre d'octets par application qu'aux outils qui lancent un suivi en tant qu'administrateur, le trafic est donc affiché par carte, pas par application.
- 🔌 **Par application** : ports en écoute (TCP et UDP), nombre de connexions ouvertes et adresses distantes avec lesquelles chaque application communique.
- 🔍 **Qui l'utilise ?** : saisissez un port pour voir quel processus l'occupe, avec sa ligne de commande, ou un fichier ou un dossier pour voir quelles applications l'ont ouvert ou travaillent dedans. Simple consultation : rien n'est terminé d'ici.

<img src="docs/screenshots/network.png" alt="Réseau" width="860">

### Recherche : restes et fichiers qui méritent un second regard
Où est passé votre espace, et trois listes à passer en revue. Rien n'est supprimé sans que vous le choisissiez.

- 📊 **Ce qui prend de la place** : les plus gros dossiers de n'importe quel disque, un niveau à la fois. Cliquez sur un dossier pour descendre ; *Afficher* l'ouvre dans l'Explorateur. Rien n'est modifié, tout est en lecture seule.
- 🚚 **Déplacer les caches hors du disque système** : les caches npm, pip, uv, Yarn, Bun, NuGet, Go, Gradle, Playwright, Hugging Face et PyTorch passent sur un autre disque ou un Dev Drive en un clic. RigSweep définit pour votre compte la variable d'environnement propre à chaque outil.
- 📦 **Programmes sans désinstalleur** : les entrées des Applications installées dont le programme de désinstallation a disparu ou ne fait rien (une application web du navigateur déjà supprimée), et les dossiers de Program Files qu'aucun programme installé, service, raccourci ou tâche n'utilise. Chaque entrée est enregistrée dans un fichier de sauvegarde avant d'être supprimée, et les dossiers vont à la Corbeille.
- 🕒 **Fichiers anciens** : fichiers de plus de 50 Mo dans Téléchargements, Bureau et Documents qui n'ont pas changé depuis un an, et programmes d'installation de plus de 90 jours dans Téléchargements. Ils vont à la Corbeille, et un fichier synchronisé avec OneDrive est signalé, car le supprimer l'efface sur tous vos appareils.
- 🛡️ **À examiner** : programmes non signés qui démarrent avec Windows ou s'exécutent depuis AppData ou Temp, noms qui cachent leur vrai type (`facture.pdf.exe`, un caractère de droite à gauche caché) et téléchargements non signés. Figurer dans la liste n'est pas un verdict : **Analyser avec Defender** interroge Microsoft Defender sur ce seul fichier.

<img src="docs/screenshots/finders.png" alt="Recherche" width="860">

### Simple ou Pro
Dans **🛠️ Paramètres → Apparence et fonctions** :

- 🧭 **Mode** : **Simple** affiche le Tableau de bord, les nettoyages et des mots simples, et masque les pages expertes (Processus, Réseau). **Pro** affiche tout, y compris les moteurs du GPU, les connexions et les valeurs brutes. Les nouvelles installations démarrent en mode Simple.
- 📡 **Surveillance** : désactivez GPU et VRAM par processus, Charge IA, Réseau, Températures ou Alertes d'état des disques. Une surveillance désactivée n'est jamais interrogée et ne coûte donc aucun CPU.

### Pour les agents IA et les scripts
- ⌨️ **Commandes sans interface** qui ne demandent **aucun droit administrateur** : `rigsweep --disk-report --json`, `rigsweep --disk-clean --safe [--dry-run] --json` et `rigsweep --port 3000 --json`.
- 🔌 **Serveur MCP** : `rigsweep --mcp` donne à Claude Code, Cursor ou Codex les outils `drive_space`, `disk_report`, `free_disk_space` (une simulation, sauf indication contraire) et `port_owner`, plus `report_cleanup` : quand l'agent a supprimé des dossiers que RigSweep lui a demandé de vérifier, il donne les chemins, et RigSweep vérifie qu'ils ont disparu et compte la taille qu'il a mesurée. Pour Claude Code : `claude mcp add rigsweep -- "C:\Program Files\RigSweep\rigsweep.exe" --mcp`. Claude Desktop : installez `rigsweep-<version>.mcpb` depuis la [dernière version](https://github.com/yobotn/rigsweep/releases/latest). Aussi dans le [MCP Registry](https://registry.modelcontextprotocol.io/) sous `io.github.yobotn/rigsweep`.
- 📖 Référence complète : [rigsweep.com/cli](https://rigsweep.com/cli/).

### Mises à jour, à vous de choisir
**🛠️ Paramètres → 🔄 Mises à jour** propose un bouton **Rechercher maintenant** et l'option *Rechercher automatiquement les mises à jour* (au démarrage, puis une fois par jour), **désactivée par défaut**. Quand une nouvelle version sort, un bouton apparaît dans la barre latérale avec les nouveautés. Rien n'est installé tant que vous n'avez pas cliqué sur **Installer et redémarrer**, et seulement si le téléchargement est signé avec la clé de RigSweep. Vos réglages, sauvegardes et paramètres sont conservés. Consultez la [politique de confidentialité](#-politique-de-confidentialit%C3%A9) pour savoir exactement ce que fait une recherche.

### 11 langues
L'interface est en **English, Deutsch, Español, Français, Italiano, Português (Brasil), Русский, 日本語, 한국어, 简体中文 et العربية** (à changer dans la barre latérale), tout comme le programme d'installation et l'extension Chrome. Chaque langue reprend les termes officiels de Windows (Gestionnaire des tâches, Services, Planificateur de tâches…) plutôt qu'une traduction mot à mot. Une formulation vous gêne ? Ouvrez une issue ou une PR sur `ui/locales/<code>.js` (app), `docs/locales/<code>.js` (site web) ou `README.<code>.md`.

---

## Avant/après sur votre machine

Installez, ouvrez **📊 Rapport de gains → 📸 Mesurer maintenant**, faites un Nettoyage dev et appliquez les réglages sûrs, redémarrez, mesurez à nouveau, puis cliquez sur **📋 Copier en Markdown**. Vos résultats sont les bienvenus dans [Discussions](../../discussions) !

<!-- TODO(maintainer): paste your own "Copy as Markdown" result here once you've run it on your machine. -->

> ℹ️ Les réglages de services prennent tout leur effet après un redémarrage. Le Nettoyage dev et la mise en veille des onglets Chrome se voient immédiatement.

---

## Installation

1. Téléchargez `RigSweep_x.y.z_x64-setup.exe` depuis les [**Releases**](../../releases).
2. Lancez-le. Il demande les droits d'administrateur, car RigSweep gère des services et des stratégies.
3. Le programme d'installation vous demande si vous voulez autoriser RigSweep dans Microsoft Defender (voir plus bas). C'est votre choix.
4. Les versions suivantes peuvent être installées depuis **🛠️ Paramètres → 🔄 Mises à jour** (à partir de la 0.5.0).

Ou avec [Scoop](https://scoop.sh) :

```powershell
scoop bucket add yobotn https://github.com/yobotn/scoop-bucket
scoop install yobotn/rigsweep
```

🌐 **Assistant Chrome :** dans RigSweep, allez dans **🛠️ Paramètres → 🌐 Assistant d'onglets Chrome → Ouvrir le dossier de l'extension**, puis dans Chrome : `chrome://extensions` → **Mode développeur** → **Charger l'extension non empaquetée** → choisissez ce dossier.

## Faux positifs antivirus

RigSweep est open source mais **pas encore signé**. Les outils qui modifient des services et la configuration de Defender ressemblent beaucoup à des malwares pour l'heuristique d'apprentissage automatique, donc **Microsoft Defender peut supprimer l'exe par erreur**. C'est arrivé pendant le développement sous le nom `Trojan:Win32/Bearfoos.B!ml`, où `!ml` désigne une supposition heuristique et non une signature de malware connu.

Ce que nous faisons :
- ❓ **Question dans le programme d'installation (facultative) :** « Autoriser RigSweep dans Microsoft Defender ? » Oui exclut **uniquement le dossier d'installation**, et le programme de désinstallation retire ensuite cette exclusion. Les installations silencieuses n'en ajoutent jamais.
- 🛠️ **Paramètres → 🛡️ Antivirus et faux positifs :** consultez l'état, et ajoutez ou retirez l'exclusion à tout moment.
- ✍️ **La signature de code est prévue** via [SignPath Foundation](https://signpath.org) (gratuite pour les projets open source). C'est la vraie solution. Voir la [politique de signature de code](#politique-de-signature-de-code).

S'il a déjà été supprimé : **Sécurité Windows → Protection contre les virus et menaces → Historique de protection →** choisissez l'entrée RigSweep → **Actions → Restaurer**. Faites-le uniquement pour un téléchargement depuis la page Releases de ce dépôt, ou pour un build que vous avez compilé vous-même. Vous pouvez aussi [signaler vous-même le faux positif à Microsoft](https://www.microsoft.com/wdsi/filesubmission).

---

## Compiler depuis les sources

```powershell
winget install Rustlang.Rustup        # + VS 2022 Build Tools (charge de travail C++)
npm install
npm run build                         # -> src-tauri/target/release/rigsweep.exe + bundle/nsis/*-setup.exe
```
Exécution en dev : `npm run dev` (acceptez l'UAC ; l'app se relance elle-même avec des privilèges élevés).

> 💡 Defender peut aussi mettre en quarantaine vos builds locaux tout juste compilés. Ajoutez le dossier de votre clone à la liste *Dossiers dev* de RigSweep (Paramètres) et appliquez le réglage **Defender exclusions for dev folders**, ou excluez `src-tauri\target` manuellement.

### CLI
| Commande | Effet |
|---|---|
| `rigsweep.exe` | Interface graphique + zone de notification |
| `rigsweep.exe --apply` | Sans interface : vérifie le profil, réapplique les réglages annulés, puis quitte (utilisé par la tâche de démarrage) |
| `rigsweep.exe --apply --tray` | Idem, puis reste dans la zone de notification avec la surveillance |
| `rigsweep.exe --restore-all` | Rétablit tout aux valeurs d'origine sauvegardées |

### Tests
```powershell
npm run check                                       # tout ce que vérifie la CI ; avec -- --fast, sans les tests Rust
cd src-tauri
cargo test                                          # validité du catalogue, sauvegardes, relevés, analyse des commandes
cargo test live_status   -- --ignored --nocapture   # lecture seule : état détecté de chaque réglage
cargo test live_dev_scan -- --ignored --nocapture   # lecture seule : ce que voit le Nettoyage dev en ce moment
```

### Ajouter des réglages
Ajoutez une entrée à [`catalog/tweaks.json`](src-tauri/catalog/tweaks.json). Les types d'action sont `service` (start: disabled|manual), `task` (path), `registry` (HKLM|HKCU, dword|string), `defender_exclusion` et `power_plan`. Les éléments `caution` doivent inclure un `warning`. `cargo test` rejette les services protégés et les entrées mal formées.

## docker-optimize (outil séparé)
Ne fait pas partie de l'app. [`docker-optimize/`](docker-optimize/) est un script autonome, réversible et ponctuel qui plafonne la VM WSL2 de Docker Desktop (mémoire, CPU, restitution de RAM, disque fragmenté) et désactive les extras de Docker Desktop dont vous n'avez pas besoin. Il ne supprime jamais de conteneurs, d'images ni de volumes. Voir son [README](docker-optimize/README.md) (en anglais).

## Où sont stockées les données
| Quoi | Où |
|---|---|
| Base de données (sauvegardes, historique, relevés) | `%ProgramData%\RigSweep\rigsweep.db` (+ `.bak.1`–`.bak.5`) |
| Tâche de démarrage | Planificateur de tâches → `RigSweep-Reapply` |
| Profils exportés | `Documents\RigSweep-profile-*.json` |

Vous venez d'**aiBoost** (l'ancien nom) ? Votre base de données est migrée automatiquement au premier démarrage, et l'ancienne tâche de démarrage est remplacée.

## Politique de signature de code

Signature de code gratuite fournie par [SignPath.io](https://about.signpath.io/), certificat de [SignPath Foundation](https://signpath.org/).
*(Demande en cours : les versions jusqu'à la v0.3.0 ne sont **pas encore signées**.)*

- **Builds :** les programmes d'installation des versions sont compilés à partir du code tagué par [GitHub Actions](.github/workflows/release.yml) sur des runners hébergés par GitHub. Seuls ces builds seront soumis à la signature.
- **Committers et relecteurs :** [@yobotn](https://github.com/yobotn)
- **Approbateurs :** [@yobotn](https://github.com/yobotn)

### Politique de confidentialité
Ce programme ne transférera aucune information vers d'autres systèmes en réseau, sauf demande expresse de l'utilisateur ou de la personne qui l'installe ou l'utilise. RigSweep peut faire deux types de requêtes réseau, chacune seulement après que vous l'avez choisie. **Recherche de mises à jour** : quand vous cliquez sur *Vérifier maintenant* dans les Paramètres, ou activez *Rechercher les mises à jour automatiquement* (désactivé par défaut). Elle télécharge `latest.json` depuis les GitHub Releases de ce dépôt et n'envoie rien sur votre PC ; une mise à jour n'est installée qu'après un clic sur *Installer*, et seulement si sa signature correspond à la clé de signature de RigSweep. **Statistiques d'utilisation anonymes** : à la première ouverture de RigSweep, une fenêtre les explique et vous demande votre choix ; rien n'est envoyé avant votre réponse, et *Paramètres → Statistiques anonymes* les désactive à tout moment, ce qui supprime aussi ce que ce PC a envoyé. Environ une fois par heure, un rapport ne contient que ce qui a été nettoyé depuis le précédent (heure, type et espace libéré, une entrée par type), les applications connues les plus gourmandes (comme Chrome ou VS Code), une fois par jour les logiciels matériels et pilotes d'une liste fixe de constructeurs ([discovery.json](server/discovery.json) : ASUS, Dell, HP, NVIDIA, Logitech…) en indiquant s'ils tournent et ce qu'ils consomment, la RAM et le nombre de cœurs, la version de RigSweep et le build de Windows, et un ID d'installation aléatoire. Il ne contient jamais de chemins de fichiers, de noms de dossiers ou de projets, de noms de processus hors de ces deux listes, de noms d'utilisateur ou de PC, ni d'identifiants matériels ; votre adresse IP sert uniquement à trouver le pays et n'est jamais conservée. *Voir ce qui est envoyé* affiche le rapport exact. L'extension Chrome pose la même question dans sa fenêtre et, si vous acceptez, n'envoie que des chiffres horaires (onglets mis en veille et fermés, onglets et fenêtres ouverts), jamais d'URL, de titres ni de sites. Tout le reste de ce que stocke RigSweep (sauvegardes, historique, mesures) reste dans `%ProgramData%\RigSweep` sur votre PC. Politique complète : [rigsweep.com/privacy](https://rigsweep.com/privacy/).

## Avertissement

RigSweep modifie des services Windows, des tâches planifiées, des stratégies du registre et la configuration de Microsoft Defender, et peut terminer des processus en cours d'exécution. Chaque réglage est sauvegardé et peut être restauré, et rien ne se passe sans votre confirmation, mais **vous l'utilisez à vos propres risques**. Il est fourni **« tel quel », sans aucune garantie d'aucune sorte** (voir la [licence MIT](LICENSE)). Vérifiez chaque changement avant de l'appliquer, gardez vos propres sauvegardes et ne l'utilisez pas sur un appareil géré par votre employeur ou votre établissement scolaire sans l'accord de leur service informatique.

RigSweep est un projet indépendant, **sans aucun lien avec** Microsoft, Google, Anthropic, OpenAI ni aucun autre outil mentionné. Toutes les marques appartiennent à leurs propriétaires respectifs.

## Contribuer
Les issues et PR sont les bienvenues, en particulier les nouveaux réglages (avec un niveau de risque clair), d'autres stacks de dev et CLI d'agents pour le Nettoyage dev, et les traductions.

## Licence
[MIT](LICENSE) : gratuit pour toujours, faites-en ce que vous voulez, sans garantie.
