<div align="center">

# 🧹 RigSweep

**Espaço para seus agentes.** Libere RAM, CPU e disco em PCs Windows de desenvolvimento lotados de sessões de agentes de IA.

**[🌐 Site](https://rigsweep.com)** · **[⬇️ Baixar](https://github.com/yobotn/rigsweep/releases/latest)** · **[📖 FAQ do antivírus](#%EF%B8%8F-falsos-positivos-do-antiv%C3%ADrus)**

[🇬🇧 English](README.md) · [🇩🇪 Deutsch](README.de.md) · [🇪🇸 Español](README.es.md) · [🇫🇷 Français](README.fr.md) · [🇮🇹 Italiano](README.it.md) · 🇧🇷 Português · [🇷🇺 Русский](README.ru.md) · [🇯🇵 日本語](README.ja.md) · [🇰🇷 한국어](README.ko.md) · [🇨🇳 简体中文](README.zh-CN.md)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-orange?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-green)
![Price: free](https://img.shields.io/badge/price-free-brightgreen)

<img src="docs/screenshots/cleanup-pt-BR.png" alt="Limpeza dev do RigSweep" width="860">

</div>

---

## 📖 Por que eu criei isto

Na era da IA, meu notebook não roda mais *uma* coisa só. Em um dia normal eu tenho várias sessões do **Claude Code** em terminais diferentes, um servidor de dev por projeto, watchers de build e um navegador cheio de documentação, previews e dashboards. Cada nova sessão abre mais processos Node e, quando eu fecho um terminal, alguns deles **simplesmente continuam rodando em segundo plano**.

A máquina começou a parecer *entulhada*: ventoinhas girando, RAM sempre quase no limite e nenhuma ideia clara do que estava consumindo tudo. O Gerenciador de Tarefas só me mostrava uma parede de `node.exe` e `chrome.exe`, sem dizer qual sessão, qual projeto ou se algo ainda precisava daquilo.

Então medi minha própria máquina (32 GB de RAM) no meio de um dia de trabalho:

| 🔎 O que eu encontrei | Quantidade | RAM |
|---|---:|---:|
| 🌐 Processos do Chrome | 37 | **6,5 GB** |
| ⬢ Processos de Node + Claude (CLIs de agentes, servidores de dev, o app desktop) | 14 | **2,5 GB** |
| 🧩 Serviços do Windows em execução (telemetria, Xbox, indexador de pesquisa…) | ~150 | — |

Nada disso era "vírus" ou "lixo". Eram as **sobras normais de trabalhar com agentes de IA**, mais coisas do Windows em segundo plano de que uma máquina de desenvolvimento não precisa. Os apps de "acelerar o PC" que existem ou fazem coisas assustadoras e irreversíveis, ou não entendem nada de fluxos de desenvolvimento. Então criei a ferramenta que eu queria:

> **Ver o que realmente está usando sua máquina, limpar o que ficou abandonado e poder desfazer qualquer alteração.**

---

## ✨ Recursos

### 🧹 Limpeza dev: processos de dev abandonados em qualquer stack
Encontra **servidores de dev, daemons de build e CLIs de agentes de IA** e agrupa cada um com a sua **árvore de processos inteira**, pasta do projeto, idade, RAM e portas em escuta.

| Stack | O que encontra |
|---|---|
| ⬢ Node · Bun · Deno | servidores de dev (Vite, Astro, Next…), watchers, esbuild |
| 🐍 Python | uvicorn, Flask, Django, Jupyter, Streamlit, Celery, processos de venv, servidores MCP em Python |
| ☕ Java | daemons do Gradle e do Kotlin, Maven, Spring Boot, Quarkus |
| 🟣 .NET | `dotnet watch/run`, servidor do compilador Roslyn, nós do MSBuild que ficaram para trás |
| 💎 Ruby · 🐘 PHP · 🦀 Rust · 🐹 Go | `rails`/`ruby`, `php -S`/artisan, `cargo watch/run`, `go run` |
| 🤖 CLIs de agentes | Claude Code, Codex, Gemini CLI, opencode, Aider, Goose |
| 🔌 Servidores MCP | servidores iniciados pelo seu agente (`@modelcontextprotocol/*`, `@playwright/mcp`, `mcp-server-*`, FastMCP…) que continuaram rodando depois que ele saiu, mesmo atrás de um inicializador `cmd /c npx` |
| 🌐 Navegadores de automação | Chrome e Edge do Playwright / Puppeteer / `--headless` com toda a árvore de renderizadores. O seu próprio navegador nunca aparece na lista |
| 🐧 WSL 2 / Docker | a VM Linux (`vmmemWSL`), desligada de forma limpa com `wsl --shutdown`, nunca pré-selecionada |

Python e Java também executam aplicativos comuns da área de trabalho, por isso, nesses casos, só aparecem processos com uma assinatura clara de desenvolvimento.

- 🧟 **Órfão**: o terminal foi fechado, mas o processo continuou rodando. Vem pré-selecionado, mesmo que ainda ocupe uma porta (o clássico zumbi em `localhost:3000`).
- 🔌 **Servidor MCP**: marcado em cada linha que for um, para você distinguir o servidor de ferramentas que um agente deixou para trás do seu próprio servidor de dev.
- 👯 **Cópia mais antiga**: um servidor MCP que também está rodando como cópia mais nova (por exemplo, na sua sessão atual do agente) é marcado, para que a sobra seja fácil de identificar.
- 📈 **Medidor ao vivo**: RAM e CPU são amostradas a cada 2 segundos enquanto o RigSweep está em execução (janela ou área de notificação). Logo depois de uma limpeza, manual ou automática, ele mostra **quanta RAM foi realmente recuperada** e a CPU antes → depois, medidas, não estimadas.
- 🌡️ **Monitoramento de temperatura**: as temperaturas da CPU e da GPU aparecem ao vivo no Painel, com tendência e pico dos últimos 5 minutos, e um aviso quando a máquina continua acima de 90 °C. A da GPU vem do mesmo contador do Gerenciador de Tarefas; a da CPU vem da zona térmica ACPI (nenhum driver instalado, sem precisar de administrador; aproximada em muitos desktops). Clique no bloco para alternar entre °C e °F.
- 💤 **Ocioso**: sem atividade de CPU por muito tempo. Runtimes sem porta aberta vêm pré-selecionados. Uma **sessão de agente ociosa em um terminal aberto nunca vem pré-selecionada**, porque ela só está esperando por você (e `claude --resume` a traz de volta de qualquer jeito).
  O tempo para considerar ocioso é definido em **Configurações** (em minutos, padrão 120; por exemplo, 5).
- 🧹 **Limpeza automática** (desativada por padrão, em Configurações): a cada minuto o RigSweep finaliza os runtimes de dev abandonados que ele observou sem uso de CPU durante todo o período de ociosidade. CLIs de agentes, a VM do WSL/Docker e processos do editor nunca são finalizados automaticamente, e cada árvore finalizada é registrada no Histórico.
- 🟠🔴 **Indicador na área de notificação**: o ícone na área de notificação (e o botão na barra de tarefas enquanto a janela está aberta) ganha um ponto laranja quando processos de dev abandonados ocupam ≥ 1 GB e um ponto vermelho a partir de ≥ 2 GB. Passe o mouse sobre o ícone para ver quanto. Os limites são definidos em Configurações, onde o ponto também pode ser desativado.
- ♻️ **Daemon de build**: os servidores do Gradle, do Kotlin e do Roslyn foram feitos para ficar rodando. Finalizá-los não causa problemas; eles reiniciam no próximo build.
- 🔒 **Bloqueado**: processos que pertencem ao **VS Code, Cursor, JetBrains, Visual Studio…** (seus servidores de linguagem) ou a um aplicativo da área de trabalho nunca podem ser finalizados por aqui.
- ✅ Nada é finalizado sem a sua confirmação, e a reutilização de PID é verificada antes de cada finalização.

### 💾 Disco dev: node_modules e caches de pacotes
Projetos antigos guardam gigas de `node_modules`, `.next`, `target`, `bin/obj` e `.venv` muito depois de você parar de mexer neles, e os caches de pacotes só crescem. O RigSweep encontra os dois e apaga o que você escolher.

- 🗂️ **Projetos**: pastas de build e de dependências de Node, Rust, .NET, Java/Gradle/Maven e Python. Uma pasta só conta se o arquivo do projeto estiver ao lado dela (`package.json`, `Cargo.toml`, `*.csproj`, `pyvenv.cfg`…), então uma pasta `build` qualquer nunca é tocada. Só vêm pré-selecionados os projetos que ninguém edita há 30 dias (ajustável), e os que um processo em execução está usando ficam bloqueados 🔒.
- 📦 **Caches de pacotes**: npm, npx, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Puppeteer, Hugging Face. pnpm, uv e Go são limpos com os próprios comandos (`pnpm store prune`…); no Playwright e no Puppeteer só saem as versões antigas dos navegadores, a mais nova fica.
- 🚫 **Nunca limpar**: dá para desligar qualquer cache. Os que significam um download grande (Playwright, Puppeteer, Hugging Face, Gradle, Maven, pacotes do NuGet, módulos do Go) já vêm desligados.
- 🙈 **Exclusões**: AppData, as pastas de ferramentas do seu perfil (`.cargo`, `.nvm`, `.vscode`…), `.git` e o Windows são sempre ignorados. Adicione seus próprios caminhos ou nomes de pasta (`vendor`, `old-*`) nas Configurações, ou clique em **Excluir** em um projeto.
- ✅ Por padrão analisa o seu perfil de usuário (Área de Trabalho, Documentos, Downloads, OneDrive…). Nada é apagado sem a sua confirmação, e cada pasta é verificada de novo logo antes de ser apagada.

### 📊 Relatório de ganhos: antes e depois reais
Nada de números falsos como "seu PC está 87% mais rápido". O RigSweep **mede** sua máquina e compara dois snapshots: RAM usada/livre, CPU média, quantidade de processos, serviços em execução, RAM do Chrome, RAM de Node/agentes.

- 📸 Uma **linha de base** é registrada automaticamente antes do seu primeiro ajuste.
- 🔁 Um par **antes/depois** é registrado em cada Limpeza dev.
- 📋 **Copiar como Markdown** para compartilhar seu resultado em uma issue, um post ou neste README.

### 🌐 Assistente de guias do Chrome
Um aplicativo da área de trabalho **não consegue** fechar guias do Chrome, por isso o RigSweep inclui uma pequena extensão ([`chrome-extension/`](chrome-extension/)) sem acesso à rede e apenas com as permissões `tabs` / `storage` / `alarms`:
- 💤 **Suspender guias ociosas**: libera a memória delas, mas a guia continua na barra de guias
- 🧹 **Fechar guias duplicadas**
- 🎯 **Fechar guias por padrão**, como `localhost:*` ou `*.vercel.app`
- ⏰ **Suspensão automática** a cada 10 minutos (opcional)

### ⚙️ Ajustes reversíveis do Windows
Um catálogo selecionado ([`catalog/tweaks.json`](src-tauri/catalog/tweaks.json)) com telemetria, Xbox, indexador de pesquisa, aplicativos em segundo plano, exclusões do Defender para pastas de desenvolvimento, plano de energia e mais, cada um com um nível de risco:

| Nível | Significado |
|---|---|
| 🟢 `safe` | Sem efeitos colaterais perceptíveis (pré-selecionado) |
| 🟡 `moderate` | Pequena concessão, explicada |
| 🔴 `caution` | Desativa um recurso; exige a sua confirmação |

<img src="docs/screenshots/tweaks.png" alt="Ajustes" width="860">

- 💾 **Todo valor original tem backup** em SQLite antes da primeira alteração. Restaure um ajuste, uma categoria ou tudo.
- 🛟 Um **ponto de restauração do sistema** é criado antes da primeira aplicação.
- 🔁 Um **watchdog** reaplica os ajustes que uma atualização do Windows Update reverteu sem avisar e conta quantas vezes isso acontece.
- 🚫 Nunca são tocados: o Windows Update, o mecanismo do Defender, RPC/DCOM/LSA e ~60 outros serviços essenciais ([`PROTECTED_SERVICES`](src-tauri/src/catalog.rs)).

### 🔍 Analisador de processos
CPU / RAM / disco por processo, e **quais serviços rodam dentro de cada `svchost.exe`**. Desative um serviço ou finalize um processo direto da lista (os itens essenciais ficam bloqueados 🔒).

### 🔄 Atualizações, quando você quiser
Em **🛠️ Configurações → 🔄 Atualizações** há um botão **Verificar agora** e uma opção *Verificar atualizações automaticamente* (ao iniciar e depois uma vez por dia), que vem **desativada por padrão**. Quando sai uma nova versão, aparece um botão na barra lateral com as novidades. Nada é instalado até você clicar em **Instalar e reiniciar**, e só se o download estiver assinado com a chave do RigSweep. Seus ajustes, backups e configurações são mantidos. Veja na [política de privacidade](#-pol%C3%ADtica-de-privacidade) exatamente o que uma verificação faz.

### 🌍 10 idiomas
A interface está em **English, Deutsch, Español, Français, Italiano, Português (Brasil), Русский, 日本語, 한국어 e 简体中文** (troque na barra lateral), assim como o instalador e a extensão do Chrome. Cada idioma é localizado com os termos oficiais do Windows naquele idioma (Gerenciador de Tarefas, Serviços, Agendador de Tarefas…), não traduzido palavra por palavra. Encontrou algo estranho? Abra uma issue ou um PR em `ui/locales/<code>.js` (aplicativo), `docs/locales/<code>.js` (site) ou `README.<code>.md`.

---

## 🏁 Antes e depois na sua máquina

Instale, abra **📊 Relatório → 📸 Medir agora**, faça uma Limpeza dev e aplique os ajustes seguros, reinicie, meça de novo e clique em **📋 Copiar como Markdown**. Resultados são bem-vindos nas [Discussions](../../discussions)!

<!-- TODO(maintainer): paste your own "Copy as Markdown" result here once you've run it on your machine. -->

> ℹ️ Os ajustes de serviços só têm efeito completo depois de reiniciar. A Limpeza dev e a suspensão de guias do Chrome aparecem na hora.

---

## 📦 Instalação

1. Baixe `RigSweep_x.y.z_x64-setup.exe` em [**Releases**](../../releases).
2. Execute-o. Ele pede direitos de administrador, porque o RigSweep gerencia serviços e políticas.
3. O instalador pergunta se o RigSweep deve ser permitido no Microsoft Defender (veja abaixo). A escolha é sua.
4. Versões posteriores podem ser instaladas em **🛠️ Configurações → 🔄 Atualizações** (a partir da 0.5.0).

🌐 **Assistente do Chrome:** no RigSweep, vá em **🛠️ Configurações → 🌐 Assistente de guias do Chrome → Abrir pasta da extensão** e, no Chrome: `chrome://extensions` → **Modo do desenvolvedor** → **Carregar sem compactação** → escolha essa pasta.

## 🛡️ Falsos positivos do antivírus

O RigSweep é open source, mas **ainda não tem assinatura de código**. Ferramentas que alteram serviços e configurações do Defender se parecem muito com malware para heurísticas de aprendizado de máquina, então **o Microsoft Defender pode excluir o exe por engano**. Isso aconteceu durante o desenvolvimento como `Trojan:Win32/Bearfoos.B!ml`, em que `!ml` indica um palpite heurístico, não a assinatura de um malware conhecido.

O que fazemos a respeito:
- ❓ **Pergunta no instalador (opcional):** "Permitir o RigSweep no Microsoft Defender?" Sim exclui **apenas a pasta de instalação**, e o desinstalador remove essa exclusão depois. Instalações silenciosas nunca adicionam exclusão.
- 🛠️ **Configurações → 🛡️ Confiança do antivírus:** veja o status e adicione ou remova a exclusão a qualquer momento.
- ✍️ **A assinatura de código está planejada** pela [SignPath Foundation](https://signpath.org) (gratuita para projetos open source). Essa é a solução de verdade. Veja a [política de assinatura de código](#-pol%C3%ADtica-de-assinatura-de-c%C3%B3digo).

Se ele já foi excluído: **Segurança do Windows → Proteção contra vírus e ameaças → Histórico de proteção →** escolha a entrada do RigSweep → **Ações → Restaurar**. Faça isso apenas com um download da página de Releases deste repositório ou com um build que você mesmo compilou. Você também pode [relatar o falso positivo à Microsoft](https://www.microsoft.com/wdsi/filesubmission).

---

## 🛠️ Compilar do código-fonte

```powershell
winget install Rustlang.Rustup        # + VS 2022 Build Tools (carga de trabalho C++)
npm install
npm run build                         # -> src-tauri/target/release/rigsweep.exe + bundle/nsis/*-setup.exe
```
Execução em modo dev: `npm run dev` (aceite o UAC; o aplicativo se reinicia com privilégios elevados).

> 💡 O Defender também pode colocar em quarentena seus builds locais recém-compilados. Adicione a pasta do seu clone à lista de *Pastas de desenvolvimento* do RigSweep (Configurações) e aplique o ajuste **Defender exclusions for dev folders**, ou exclua `src-tauri\target` manualmente.

### ⌨️ CLI
| Comando | Efeito |
|---|---|
| `rigsweep.exe` | Interface gráfica + área de notificação |
| `rigsweep.exe --apply` | Sem interface: verifica o perfil, reaplica ajustes revertidos e sai (usado pela tarefa de inicialização) |
| `rigsweep.exe --apply --tray` | O mesmo, depois fica na área de notificação com o watchdog |
| `rigsweep.exe --restore-all` | Reverte tudo para os valores originais do backup |

### 🧪 Testes
```powershell
cd src-tauri
cargo test                                          # validade do catálogo, backups, snapshots, análise de comandos
cargo test live_status   -- --ignored --nocapture   # somente leitura: status detectado de cada ajuste
cargo test live_dev_scan -- --ignored --nocapture   # somente leitura: o que a Limpeza dev vê agora
```

### ➕ Adicionar ajustes
Adicione uma entrada em [`catalog/tweaks.json`](src-tauri/catalog/tweaks.json). Os tipos de ação são `service` (start: disabled|manual), `task` (path), `registry` (HKLM|HKCU, dword|string), `defender_exclusion` e `power_plan`. Itens `caution` precisam incluir um `warning`. O `cargo test` rejeita serviços protegidos e entradas malformadas.

## 🐳 docker-optimize (ferramenta separada)
Não faz parte do aplicativo. [`docker-optimize/`](docker-optimize/) é um script independente, reversível e de execução única que limita a VM WSL2 do Docker Desktop (memória, CPU, recuperação de RAM, disco esparso) e desativa extras do Docker Desktop de que você não precisa. Ele nunca exclui contêineres, imagens nem volumes. Veja o [README](docker-optimize/README.md) dele.

## 🗂️ Onde ficam os arquivos
| O quê | Onde |
|---|---|
| Banco de dados (backups, histórico, snapshots) | `%ProgramData%\RigSweep\rigsweep.db` (+ `.bak.1`–`.bak.5`) |
| Tarefa de inicialização | Agendador de Tarefas → `RigSweep-Reapply` |
| Perfis exportados | `Documents\RigSweep-profile-*.json` |

Vindo do **aiBoost** (o nome antigo)? Seu banco de dados é migrado automaticamente na primeira inicialização, e a tarefa de inicialização antiga é substituída.

## 🔏 Política de assinatura de código

Assinatura de código gratuita fornecida pela [SignPath.io](https://about.signpath.io/), certificado da [SignPath Foundation](https://signpath.org/).
*(Solicitação em andamento: as versões até a v0.3.0 **não são assinadas** ainda.)*

- **Builds:** os instaladores das versões são compilados a partir do código-fonte com tag pelo [GitHub Actions](.github/workflows/release.yml) em runners hospedados pelo GitHub. Somente esses builds serão enviados para assinatura.
- **Committers e revisores:** [@yobotn](https://github.com/yobotn)
- **Aprovadores:** [@yobotn](https://github.com/yobotn)

### 🔒 Política de privacidade
Este programa não transfere nenhuma informação para outros sistemas em rede, a menos que isso seja especificamente solicitado pelo usuário ou pela pessoa que o instala ou opera. O RigSweep não tem telemetria nem análises, e a extensão do Chrome não faz nenhuma solicitação de rede. A única solicitação de rede que o RigSweep pode fazer é a **verificação de atualizações**, e somente quando você pede: clicando em *Verificar agora* nas Configurações ou ativando *Verificar atualizações automaticamente* (desativada por padrão). Ela baixa o `latest.json` das GitHub Releases deste repositório e não envia nada sobre o seu PC; uma atualização só é instalada depois que você clica em *Instalar e reiniciar*, e somente se a assinatura dela corresponder à chave de assinatura do RigSweep. Tudo o que ele armazena (backups, histórico, medições) fica em `%ProgramData%\RigSweep` no seu PC.

## ⚠️ Aviso legal

O RigSweep altera serviços do Windows, tarefas agendadas, políticas do Registro e configurações do Microsoft Defender, e pode finalizar processos em execução. Todo ajuste tem backup e pode ser restaurado, e nada acontece sem a sua confirmação, mas **o uso é por sua conta e risco**. Ele é fornecido **"no estado em que se encontra", sem garantias de qualquer tipo** (veja a [licença MIT](LICENSE)). Revise cada alteração antes de aplicá-la, mantenha seus próprios backups e não o use em dispositivos gerenciados pela empresa ou pela escola sem a aprovação da equipe de TI.

O RigSweep é um projeto independente, **sem vínculo com** a Microsoft, o Google, a Anthropic, a OpenAI ou qualquer outra ferramenta mencionada. Todas as marcas pertencem aos respectivos proprietários.

## 🤝 Como contribuir
Issues e PRs são bem-vindos, especialmente novos ajustes (com um nível de risco claro), mais stacks de desenvolvimento e CLIs de agentes para a Limpeza dev, e traduções.

## 📄 Licença
[MIT](LICENSE): gratuito para sempre, faça o que quiser, sem garantia.
