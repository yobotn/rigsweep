# De la place pour vos agents.

> Application Windows gratuite et open source qui libère la RAM, le CPU et le disque des PC de développement : arrête les serveurs de dev, serveurs MCP et CLI d'agents restés ouverts, supprime les anciens node_modules et les caches de paquets et d'IA, avec des réglages réversibles et des mesures avant/après réelles.

Libérez RAM et CPU sur les machines de dev Windows encombrées par les sessions d'agents IA, les serveurs de dev oubliés et les services Windows en arrière-plan. Chaque changement est réversible.

Libéré par les utilisateurs de RigSweep jusqu'ici

Totaux en direct de ceux qui partagent des statistiques anonymes

## Pas seulement Node

Le Nettoyage dev trouve les serveurs de dev, daemons de build et sessions d'agents oubliés dans les stacks que vous utilisez vraiment, ainsi que la VM WSL 2 / Docker qui garde discrètement des gigaoctets.

## 📖 Pourquoi ce projet

À l'ère de l'IA, un portable de dev ne fait plus tourner une seule chose. Une journée normale, ce sont plusieurs sessions Claude Code dans différents terminaux, un serveur de dev par projet, des watchers de build et un navigateur rempli de docs et de previews.

Chaque session lance de nouveaux processus, et beaucoup continuent de tourner après la fermeture du terminal. Le Gestionnaire des tâches n'affiche qu'un mur de node.exe, python.exe et chrome.exe, sans dire à quelle session ou quel projet ils appartiennent.

Ensuite, j'ai regardé le disque. Je développais trois applis Rust en parallèle : RigSweep lui-même, [KeepRun](https://github.com/yobotn/keeprun) (fait tourner des applis Node, .NET et Python comme services Windows) et [Focuside](https://github.com/yobotn/focuside) (un IDE épuré pour les agents de code IA). Le dossier `target` de chaque projet avait grossi jusqu'à environ **40 Go**, soit **~120 Go** au total, uniquement des sorties de build que Cargo ne nettoie jamais de lui-même. Et un seul projet comptait à lui seul environ **30 Go** de worktrees git : des copies de travail supplémentaires créées par les agents pour des tâches en parallèle, chacune avec ses propres sorties de build, toujours là bien après la fusion du travail. L'onglet **Disque dev** a tout trouvé en une seule analyse.

Rien de tout ça n'était un virus ou des fichiers indésirables. C'étaient les restes ordinaires du travail avec des agents IA, plus des tâches de fond de Windows dont un poste de dev n'a pas besoin. Les applis « PC booster » existantes font soit des choses inquiétantes et irréversibles, soit ignorent tout des workflows de dev. Alors j'ai construit l'outil que je voulais.

**RigSweep montre ce qui utilise vraiment votre machine, balaie ce qui traîne, et garde chaque changement réversible.**

## ✨ Ce qu'il fait

Pensé pour les développeurs qui vivent dans les terminaux et les agents IA, pas pour des astuces de « booster de PC ». Une seule application à la place du Gestionnaire des tâches, d'un outil d'infos matérielles comme CPU-Z et d'un nettoyeur.

### Nettoyage dev

Serveurs de dev, daemons de build et CLI d'agents orphelins ou inactifs pour Node, Python, Java, .NET et plus, chacun avec son arbre de processus, son projet, sa RAM et ses ports.

### Disque dev

Supprime node_modules, .next, target et .venv dans les projets que vous ne touchez plus, vide les caches de paquets, de shaders GPU et d'outils IA, retire les worktrees git laissés par les agents, nettoie les restes de Windows (vieux fichiers temporaires, téléchargements de mises à jour, vidages de plantage) et réduit les disques virtuels Docker et WSL. Vous voyez l'espace libre que vous aurez avant de cliquer, et les caches qui se reconstruisent seuls peuvent être vidés chaque semaine.

### Pour les agents IA

rigsweep --disk-report --json et --disk-clean --safe --dry-run fonctionnent sans droits administrateur, et rigsweep --mcp est un serveur MCP : Claude Code, Cursor ou Codex peuvent vérifier l'espace libre, voir ce qui l'occupe et vider les caches qui se reconstruisent seuls.

### Matériel

État des disques, test de vitesse et détails du processeur, de la carte graphique, de la RAM et de la batterie dans un seul onglet, au lieu de quatre outils. Aucun pilote installé.

### GPU et IA

Ce que fait chaque application sur la carte graphique et le NPU, à partir des compteurs du Gestionnaire des tâches, ainsi que les environnements d'IA en cours (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), leurs modèles chargés et la mémoire vidéo qu'ils occupent.

### Réseau

Le trafic par carte réseau, l'application qui détient chaque connexion ou port en écoute, et une recherche qui indique qui utilise le port 3000 ou un fichier ou dossier verrouillé.

### Processus et démarrage

CPU, RAM et disque par processus, les services de chaque svchost.exe, et les services et tâches à démarrage automatique que le Gestionnaire des tâches n'affiche jamais.

### Fenêtres de console

Cette fenêtre d'invite de commandes qui clignote puis disparaît : RigSweep enregistre la commande exacte qu'elle a exécutée, ce qui l'a lancée (une tâche planifiée, un service ou une application) et ce qui a tourné dedans, et peut désactiver la tâche en cause, avec une sauvegarde.

### Recherche

Voyez ce qui prend de la place sur chaque disque, dossier par dossier, déplacez les caches de paquets de C: vers un autre disque ou un Dev Drive, et passez en revue les programmes sans désinstalleur, les gros vieux fichiers des Téléchargements et les programmes de démarrage non signés, avec une analyse Microsoft Defender en un clic.

### Rapport de gains

De vraies mesures avant/après au lieu de pourcentages inventés. Vous pouvez les copier en Markdown et partager votre résultat.

### Réglages réversibles

Télémétrie, Xbox, indexeur, apps en arrière-plan, Edge empêché de revenir et plus, chacun avec un niveau de risque. Les originaux sont sauvegardés et restaurables en un clic.

### Nettoyage des navigateurs

Vide le cache de Chrome, Edge, Brave, Vivaldi, Opera et Firefox. Les sessions ouvertes, cookies, mots de passe et l'historique ne sont jamais touchés.

### Températures

Températures du CPU et du GPU avec la tendance sur 5 minutes et une alerte si votre PC reste trop chaud. Sans pilote ni droits d'administrateur.

### Assistant d'onglets

Une petite extension pour mettre en veille les onglets inactifs, fermer les doublons et fermer par motif comme localhost:*. Elle n'a aucun accès réseau.

### Simple ou Pro

Le mode Simple affiche le tableau de bord et les nettoyages en termes clairs. Pro ajoute Processus, Réseau, moteurs GPU et valeurs brutes. Désactivez les moniteurs inutiles : ils ne consomment alors aucun CPU.

### Mises à jour, à vous de choisir

Désactivées par défaut. Recherchez quand vous voulez, ou activez une vérification quotidienne. Les mises à jour ne s'installent que sur un clic de votre part, et seulement si elles sont signées.

### 11 langues

English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 et العربية dans l'app, le programme d'installation et l'extension Chrome, avec les termes Windows officiels de chaque langue.

## 🧹 Balayez les sessions inactives, en sécurité

- **Orphelins** : leur terminal est fermé mais ils tournent encore, comme le classique localhost:3000 zombie.
- **Les agents inactifs dans un terminal ouvert ne sont jamais présélectionnés**, car ils vous attendent.
- **Les serveurs de langage de l'éditeur sont verrouillés** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Termine par arbre de processus, jamais par port.** Chaque ligne est revérifiée juste avant l'arrêt, donc deux projets ne sont jamais confondus.
- **Les serveurs MCP et navigateurs headless laissés par vos agents** sont aussi détectés, et une mesure en direct affiche la RAM récupérée quelques secondes après chaque nettoyage.

## 🛟 Chaque changement peut être annulé

- **Les valeurs d'origine sont sauvegardées** dans SQLite avant le premier changement.
- **Point de restauration système** avant la première application.
- **La surveillance** réapplique les réglages annulés en silence par Windows Update.
- **Ne touche jamais** Windows Update, le moteur Defender ni ~60 services essentiels.

## 🛡️ Defender l'a supprimé ?

RigSweep est open source mais pas encore signé. Les outils qui modifient des services ressemblent à des malwares pour l'heuristique d'apprentissage automatique, donc Defender peut le signaler à tort (ex. `Trojan:Win32/Bearfoos!ml`, où `!ml` désigne une supposition heuristique).

Le programme d'installation **demande** si vous voulez exclure uniquement son propre dossier, et le programme de désinstallation retire ensuite cette exclusion. La signature de code via SignPath Foundation est prévue. [Lire la FAQ complète →](https://github.com/yobotn/rigsweep/blob/main/README.fr.md#faux-positifs-antivirus)

## ❓ Les questions des développeurs

### Que fait RigSweep ?

RigSweep est une application Windows gratuite et open source pour les développeurs qui travaillent avec des agents de code IA. Elle trouve les serveurs de dev, daemons de build, serveurs MCP et CLI d'agents qui continuent de tourner après la fermeture d'un terminal, affiche chacun avec son projet, sa RAM et ses ports, et termine ceux que vous choisissez. Elle supprime aussi les anciens node_modules et caches de paquets, vide les caches du navigateur et applique des réglages Windows réversibles.

### Comment arrêter les processus node.exe qui continuent de tourner après la fermeture du terminal ?

Ouvrez le Nettoyage dev. Les processus dont le terminal a disparu sont marqués comme orphelins et présélectionnés, avec tout leur arbre de processus, le dossier du projet et les ports en écoute (comme un localhost:3000 bloqué). Cliquez sur « Terminer la sélection » : RigSweep termine chaque arbre après avoir vérifié que l'ID de processus appartient toujours au même programme.

### Quels agents IA et stacks de dev sont pris en charge ?

Claude Code, Codex, Gemini CLI, opencode, Aider et Goose, ainsi que les serveurs MCP et navigateurs headless qu'ils lancent. Stacks de dev : Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust et Go, plus la VM WSL 2 / Docker.

### Est-ce sûr ? Puis-je annuler les changements ?

Rien n'est terminé ni supprimé sans votre confirmation. Chaque réglage Windows sauvegarde d'abord la valeur d'origine et peut être restauré un par un ou tous à la fois, et un point de restauration système est créé avant le premier changement. Les serveurs de langage des éditeurs et les services Windows essentiels sont verrouillés.

### RigSweep est-il gratuit ? Collecte-t-il des données ?

Oui, il est gratuit pour toujours sous licence MIT, sans compte et sans publicité. RigSweep demande une fois s'il peut partager des statistiques d'utilisation anonymes (ce qu'il a nettoyé, par type et par taille, les applications connues les plus gourmandes et les logiciels de constructeurs de matériel installés). Rien n'est envoyé avant votre réponse, le rapport exact est visible dans les Paramètres, et le désactiver supprime ce qui a été envoyé. Aucun chemin, nom de projet ni donnée personnelle n'est jamais inclus. La recherche de mises à jour reste désactivée sauf si vous l'activez ou cliquez sur Vérifier maintenant.

### Fonctionne-t-il sur macOS ou Linux ?

Non. RigSweep est conçu pour Windows 10 et 11, car il travaille avec les services Windows, le Planificateur de tâches et Defender. Il peut tout de même arrêter la VM WSL 2 depuis Windows.

## ⚠️ Avertissement

RigSweep modifie des services Windows, des tâches planifiées, des stratégies du registre et la configuration de Microsoft Defender, et peut terminer des processus en cours d'exécution. Chaque réglage est sauvegardé et restaurable, et rien ne se passe sans votre confirmation, mais vous l'utilisez à vos propres risques. Il est fourni « tel quel », sans aucune garantie (licence MIT). Vérifiez chaque changement avant de l'appliquer, gardez vos propres sauvegardes et ne l'utilisez pas sur un appareil géré par votre employeur ou votre école sans l'accord de leur service informatique. RigSweep est un projet indépendant, sans lien avec Microsoft, Google, Anthropic, OpenAI ni aucun autre outil mentionné ; toutes les marques appartiennent à leurs propriétaires.

## 🚀 Donnez de la place à vos agents

Installez, prenez une mesure de référence, balayez, puis mesurez à nouveau.

### 🔏 Signature de code

Signature de code gratuite fournie par SignPath.io, certificat de SignPath Foundation (demande en cours ; les versions actuelles ne sont pas encore signées). Les programmes d'installation sont compilés à partir du code tagué par GitHub Actions. Ni télémétrie ni analytique. La seule requête réseau est la recherche de mises à jour, et seulement si vous l'activez ou cliquez sur Rechercher maintenant.
