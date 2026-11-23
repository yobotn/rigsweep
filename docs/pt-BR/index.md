# Espaço para seus agentes.

> App gratuito e de código aberto para Windows que libera RAM, CPU e disco em PCs de desenvolvimento: encerra servidores de dev, servidores MCP e CLIs de agentes que ficaram abertos, apaga node_modules antigos e caches de pacotes e de IA, com ajustes reversíveis e medições reais de antes e depois.

Libere RAM e CPU em PCs Windows de desenvolvimento lotados de sessões de agentes de IA, servidores de dev esquecidos e serviços do Windows em segundo plano. Toda alteração pode ser desfeita.

Liberado pelos usuários do RigSweep até agora

Totais ao vivo de quem compartilha estatísticas anônimas

## Não é só Node

A Limpeza dev encontra servidores de dev, daemons de build e sessões de agentes esquecidos nos stacks que você realmente usa, além da VM do WSL 2 / Docker que segura gigabytes sem você perceber.

## 📖 Por que ele existe

Na era da IA, um notebook de desenvolvimento não roda mais uma coisa só. Um dia normal tem várias sessões do Claude Code em terminais diferentes, um servidor de dev por projeto, watchers de build e um navegador cheio de documentação e previews.

Cada sessão abre mais processos, e muitos continuam rodando depois que você fecha o terminal. O Gerenciador de Tarefas só mostra uma parede de node.exe, python.exe e chrome.exe, sem dizer a qual sessão ou projeto eles pertencem.

Depois olhei o disco. Eu estava desenvolvendo três apps em Rust ao mesmo tempo: o próprio RigSweep, [KeepRun](https://github.com/yobotn/keeprun) (mantém apps Node, .NET e Python rodando como serviços do Windows) e [Focuside](https://github.com/yobotn/focuside) (um IDE focado para agentes de programação com IA). A pasta `target` de cada projeto tinha chegado a uns **40 GB**, ou seja, **~120 GB** no total, tudo saída de build que o Cargo nunca limpa sozinho. A aba **Disco dev** encontrou as três em uma única varredura.

Nada disso era vírus nem lixo. Eram as sobras normais de trabalhar com agentes de IA, mais coisas do Windows em segundo plano de que uma máquina de dev não precisa. Os apps “aceleradores de PC” que existiam ou fazem coisas irreversíveis assustadoras ou não sabem nada de fluxos de desenvolvimento, então criei a ferramenta que eu queria.

**O RigSweep mostra o que realmente está usando sua máquina, limpa o que ficou abandonado e permite desfazer qualquer alteração.**

## ✨ O que ele faz

Feito para desenvolvedores que vivem em terminais e agentes de IA, não para truques de "acelerador de PC". Um só app no lugar do Gerenciador de Tarefas, de uma ferramenta de hardware como o CPU-Z e de um limpador.

### Limpeza dev

Servidores de dev, daemons de build e CLIs de agentes órfãos e ociosos em Node, Python, Java, .NET e outros, cada um com sua árvore de processos, projeto, RAM e portas.

### Relatório de ganhos

Medições reais de antes e depois, em vez de porcentagens inventadas. Copie como Markdown e compartilhe seu resultado.

### Assistente de guias do Chrome

Uma pequena extensão para suspender guias ociosas, fechar duplicadas e fechar guias por padrão, como localhost:*. Ela não tem acesso à rede.

### Ajustes reversíveis

Telemetria, Xbox, indexador, aplicativos em segundo plano, impedir a volta do Edge e mais, cada um com um nível de risco. Os valores originais têm backup e podem ser restaurados com um clique.

### Processos e inicialização

CPU, RAM e disco por processo, os serviços dentro de cada svchost.exe, e os serviços e tarefas de inicialização automática que o Gerenciador de Tarefas nunca mostra.

### 11 idiomas

English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 e العربية no aplicativo, no instalador e na extensão do Chrome, com os termos oficiais do Windows em cada idioma.

### Temperaturas

Temperaturas da CPU e da GPU com tendência de 5 minutos e um aviso quando o PC continua quente. Sem driver, sem direitos de administrador.

### Atualizações, quando você quiser

Desativadas por padrão. Verifique quando quiser ou ative uma verificação diária. As atualizações só são instaladas quando você clica, e só se forem assinadas.

### Limpeza de navegador

Esvazia os caches do Chrome, Edge, Brave, Vivaldi, Opera e Firefox. Logins, cookies, senhas e histórico nunca são tocados.

### Disco dev

Apaga node_modules, .next, target e .venv de projetos que você não mexe mais, limpa caches de pacotes, de shaders de GPU e de ferramentas de IA, remove os worktrees do git que os agentes deixaram, tira as sobras do Windows (temporários antigos, downloads de atualização, despejos de falha) e encolhe os discos virtuais do Docker e do WSL. Antes de clicar você vê quanto espaço livre vai ter, e dá para esvaziar toda semana os caches que se refazem sozinhos.

### Hardware

Saúde dos discos, um teste de velocidade e os detalhes de CPU, GPU, RAM e bateria em uma única aba, em vez de quatro ferramentas. Sem instalar drivers.

### GPU e IA

O que cada app faz na placa de vídeo e na NPU, pelos mesmos contadores do Gerenciador de Tarefas, além dos runtimes de IA em execução (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), os modelos carregados e a memória de vídeo que ocupam.

### Rede

Tráfego por adaptador, qual app mantém cada conexão ou porta em escuta, e uma busca que mostra quem está usando a porta 3000 ou um arquivo ou pasta bloqueados.

### Encontrar

Veja o que ocupa espaço em cada unidade, pasta por pasta, tire os caches de pacotes do C: para outra unidade ou um Dev Drive e revise programas sem desinstalador, arquivos grandes e antigos em Downloads e programas de inicialização sem assinatura, com uma verificação do Microsoft Defender em um clique.

### Simples ou Pro

O modo Simples mostra o painel e as limpezas em palavras claras. O Pro adiciona Processos, Rede, mecanismos da GPU e os números brutos. Desligue os monitores de que não precisa para que não gastem CPU.

### Janelas de console

Aquela janela do Prompt de Comando que pisca e some: o RigSweep registra o comando exato que ela rodou, o que a iniciou (uma tarefa agendada, um serviço ou um aplicativo) e o que rodou dentro dela, e pode desativar a tarefa por trás, com backup.

### Para agentes de IA

rigsweep --disk-report --json e --disk-clean --safe --dry-run funcionam sem direitos de administrador, e rigsweep --mcp é um servidor MCP: Claude Code, Cursor ou Codex podem ver o espaço livre, o que o ocupa e esvaziar os caches que se refazem sozinhos.

## 🧹 Limpe sessões abandonadas com segurança

- **Órfãos**: o terminal foi fechado, mas eles continuam rodando, como o clássico zumbi em localhost:3000.
- **Agentes ociosos em terminais abertos nunca vêm pré-selecionados**, porque estão esperando por você.
- **Servidores de linguagem do editor ficam bloqueados** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Finaliza pela árvore de processos, nunca pela porta.** Cada linha é verificada de novo logo antes de ser finalizada, então dois projetos nunca se confundem.
- **Servidores MCP e navegadores headless que seus agentes deixaram para trás** também são encontrados, e um medidor ao vivo mostra a RAM recuperada segundos depois de cada limpeza.

## 🛟 Toda alteração pode ser desfeita

- **Os valores originais têm backup** em SQLite antes da primeira alteração.
- **Ponto de restauração do sistema** antes da primeira aplicação.
- **Watchdog** reaplica os ajustes que o Windows Update reverteu sem avisar.
- **Nunca mexe** no Windows Update, no mecanismo do Defender nem em ~60 serviços essenciais.

## 🛡️ O Defender excluiu o RigSweep?

O RigSweep é open source, mas ainda não tem assinatura de código. Ferramentas que alteram serviços parecem malware para heurísticas de aprendizado de máquina, então o Defender pode sinalizá-lo por engano (por exemplo, `Trojan:Win32/Bearfoos!ml`, em que `!ml` indica um palpite heurístico).

O instalador **pergunta** se você quer excluir apenas a pasta dele, e o desinstalador remove essa exclusão depois. A assinatura de código pela SignPath Foundation está planejada. [Leia a FAQ completa →](https://github.com/yobotn/rigsweep/blob/main/README.pt-BR.md#falsos-positivos-do-antiv%C3%ADrus)

## ❓ Perguntas que desenvolvedores fazem

### O que o RigSweep faz?

O RigSweep é um app gratuito e de código aberto para Windows, feito para desenvolvedores que trabalham com agentes de IA para programação. Ele encontra servidores de desenvolvimento, daemons de build, servidores MCP e CLIs de agentes que continuam rodando depois que você fecha um terminal, mostra cada um com seu projeto, RAM e portas, e finaliza os que você escolher. Ele também exclui node_modules antigos e caches de pacotes, limpa os caches do navegador e aplica ajustes reversíveis no Windows.

### Como paro processos node.exe que continuam rodando depois de fechar o terminal?

Abra a Limpeza dev. Processos cujo terminal não existe mais são marcados como órfãos e já vêm selecionados, com toda a árvore de processos, a pasta do projeto e as portas em escuta (como um localhost:3000 travado). Clique em Finalizar, e o RigSweep finaliza cada árvore depois de verificar que o ID do processo ainda pertence ao mesmo programa.

### Quais agentes de IA e stacks de desenvolvimento são suportados?

Claude Code, Codex, Gemini CLI, opencode, Aider e Goose, além dos servidores MCP e navegadores headless que eles iniciam. Stacks de desenvolvimento: Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust e Go, e a VM do WSL 2 / Docker.

### É seguro? Posso desfazer as alterações?

Nada é finalizado ou excluído sem a sua confirmação. Cada ajuste do Windows faz backup do valor original antes e pode ser restaurado um por um ou todos de uma vez, e um ponto de restauração do sistema é criado antes da primeira alteração. Servidores de linguagem de editores e serviços essenciais do Windows ficam bloqueados.

### O RigSweep é gratuito? Ele coleta dados?

Sim, é gratuito para sempre sob a licença MIT, sem conta e sem anúncios. O RigSweep pergunta uma vez se pode compartilhar estatísticas de uso anônimas (o que limpou, por tipo e tamanho, quais apps conhecidos mais consomem e quais softwares de fabricantes de hardware estão instalados). Nada é enviado antes da sua resposta, o relatório exato aparece nas Configurações e desativar apaga o que foi enviado. Caminhos, nomes de projetos ou dados pessoais nunca são incluídos. A verificação de atualizações fica desligada, a menos que você a ative ou clique em Verificar agora.

### Funciona no macOS ou no Linux?

Não. O RigSweep foi feito para o Windows 10 e 11, porque trabalha com serviços do Windows, o Agendador de Tarefas e o Defender. Mesmo assim, ele consegue desligar a VM do WSL 2 pelo lado do Windows.

## ⚠️ Aviso legal

O RigSweep altera serviços do Windows, tarefas agendadas, políticas do Registro e configurações do Microsoft Defender, e pode finalizar processos em execução. Todo ajuste tem backup e pode ser restaurado, e nada acontece sem a sua confirmação, mas o uso é por sua conta e risco. Ele é fornecido "no estado em que se encontra", sem garantias de qualquer tipo (licença MIT). Revise cada alteração antes de aplicá-la, mantenha seus próprios backups e não o use em dispositivos gerenciados pela empresa ou pela escola sem a aprovação da equipe de TI. O RigSweep é um projeto independente, sem vínculo com a Microsoft, o Google, a Anthropic, a OpenAI ou qualquer outra ferramenta mencionada; todas as marcas pertencem aos respectivos proprietários.

## 🚀 Dê espaço para seus agentes

Instale, faça uma medição de linha de base, limpe e meça de novo.

### 🔏 Assinatura de código

Assinatura de código gratuita fornecida pela SignPath.io, certificado da SignPath Foundation (solicitação em andamento; as versões atuais ainda não são assinadas). Os instaladores são compilados a partir do código-fonte com tag pelo GitHub Actions. Sem telemetria, sem análises. A única solicitação de rede é a verificação de atualizações, e somente se você ativá-la ou clicar em Verificar agora.
