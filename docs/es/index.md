# Espacio para tus agentes.

> App gratuita y de código abierto para Windows que libera RAM, CPU y disco en equipos de desarrollo: cierra servidores de desarrollo, servidores MCP y CLI de agentes que quedaron abiertos, borra node_modules antiguos y cachés de paquetes e IA, con ajustes reversibles y mediciones reales de antes y después.

Libera RAM y CPU en equipos Windows de desarrollo saturados por sesiones de agentes de IA, servidores dev olvidados y servicios de Windows en segundo plano. Cada cambio es reversible.

Liberado por los usuarios de RigSweep hasta ahora

Totales en vivo de quienes comparten estadísticas anónimas

## No solo Node

La Limpieza dev encuentra servidores dev, daemons de compilación y sesiones de agentes sobrantes en los stacks que realmente usas, además de la VM de WSL 2 / Docker que acapara gigas en silencio.

## 📖 Por qué existe

En la era de la IA un portátil de desarrollo ya no ejecuta una sola cosa. Un día normal significa varias sesiones de Claude Code en distintas terminales, un servidor dev por proyecto, watchers de compilación y un navegador lleno de documentación y previsualizaciones.

Cada sesión lanza más procesos, y muchos siguen ejecutándose después de cerrar la terminal. El Administrador de tareas solo muestra una pared de node.exe, python.exe y chrome.exe, sin decir a qué sesión o proyecto pertenecen.

Luego miré el disco. Estaba desarrollando tres apps en Rust a la vez: el propio RigSweep, [KeepRun](https://github.com/yobotn/keeprun) (mantiene apps de Node, .NET y Python funcionando como servicios de Windows) y [Focuside](https://github.com/yobotn/focuside) (un IDE enfocado para agentes de programación con IA). La carpeta `target` de cada proyecto había llegado a unos **40 GB**, es decir, **~120 GB** en total, todo salida de compilación que Cargo nunca limpia por sí solo. La pestaña **Disco dev** encontró las tres en un solo análisis.

Nada de eso era un virus ni basura. Eran los restos normales de trabajar con agentes de IA, más cosas de Windows en segundo plano que un equipo de desarrollo no necesita. Las apps “aceleradoras de PC” que ya existían o hacen cosas irreversibles que dan miedo o no saben nada de flujos de desarrollo, así que construí la herramienta que yo quería.

**RigSweep muestra qué está usando realmente tu equipo, barre lo que sobra y que cada cambio sea reversible.**

## ✨ Qué hace

Hecho para desarrolladores que viven en terminales y agentes IA, no para trucos de 'acelerador de PC'. Una sola aplicación en lugar del Administrador de tareas, una herramienta de hardware como CPU-Z y un limpiador.

### Limpieza dev

Servidores dev, daemons de compilación y CLIs de agentes huérfanos o inactivos en Node, Python, Java, .NET y más, cada uno con su árbol de procesos, proyecto, RAM y puertos.

### Informe de mejora

Mediciones reales de antes y después en lugar de porcentajes inventados. Puedes copiarlas como Markdown y compartir tu resultado.

### Asistente de pestañas

Una pequeña extensión para dormir pestañas inactivas, cerrar duplicadas y cerrar por patrón como localhost:*. No tiene acceso a red.

### Ajustes reversibles

Telemetría, Xbox, indexador, apps en segundo plano, impedir que Edge vuelva y más, cada uno con nivel de riesgo. Los originales se respaldan y se restauran con un clic.

### Procesos e inicio

CPU, RAM y disco por proceso, los servicios dentro de cada svchost.exe, y los servicios y tareas de inicio automático que el Administrador de tareas nunca muestra.

### 11 idiomas

English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 y العربية en la app, el instalador y la extensión de Chrome, con los términos oficiales de Windows de cada idioma.

### Temperaturas

Temperaturas de CPU y GPU con la tendencia de 5 minutos y un aviso si tu PC se mantiene caliente. Sin controladores ni permisos de administrador.

### Actualizaciones, tú decides

Desactivadas de forma predeterminada. Busca cuando quieras o activa una comprobación diaria. Las actualizaciones solo se instalan cuando haces clic, y solo si están firmadas.

### Limpieza de navegadores

Vacía la caché de Chrome, Edge, Brave, Vivaldi, Opera y Firefox. Nunca toca los inicios de sesión, las cookies, las contraseñas ni el historial.

### Disco dev

Borra node_modules, .next, target y .venv de proyectos que ya no tocas, limpia cachés de paquetes, de shaders de GPU y de herramientas de IA, quita los worktrees de git que dejan los agentes, elimina restos de Windows (temporales viejos, descargas de actualizaciones, volcados de errores) y reduce los discos virtuales de Docker y WSL. Antes de hacer clic ves cuánto espacio libre te quedará, y puede vaciar cada semana las cachés que se reconstruyen solas.

### Hardware

Estado de los discos, una prueba de velocidad y los datos de tu CPU, GPU, RAM y batería en una sola pestaña, en lugar de cuatro herramientas. Sin instalar controladores.

### GPU e IA

Lo que hace cada aplicación en la tarjeta gráfica y la NPU, con los mismos contadores que el Administrador de tareas, además de los entornos de IA en ejecución (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), sus modelos cargados y la memoria de vídeo que ocupan.

### Red

Tráfico por adaptador, qué aplicación tiene cada conexión o puerto de escucha, y una búsqueda que te dice quién usa el puerto 3000 o un archivo o carpeta bloqueados.

### Buscar

Mira qué ocupa espacio en cada unidad carpeta por carpeta, saca las cachés de paquetes de C: a otra unidad o a un Dev Drive y revisa programas sin desinstalador, archivos grandes y viejos en Descargas y programas de inicio sin firma, con un análisis de Microsoft Defender en un clic.

### Sencillo o Pro

El modo Sencillo muestra el panel y las limpiezas con palabras claras. Pro añade Procesos, Red, motores de GPU y los datos sin procesar. Apaga los monitores que no necesites para que no gasten CPU.

### Ventanas de consola

Esa ventana de Símbolo del sistema que aparece y desaparece: RigSweep registra el comando exacto que ejecutó, qué la inició (una tarea programada, un servicio o una aplicación) y qué se ejecutó dentro, y puede desactivar la tarea que hay detrás, con copia de seguridad.

### Para agentes de IA

rigsweep --disk-report --json y --disk-clean --safe --dry-run funcionan sin permisos de administrador, y rigsweep --mcp es un servidor MCP: Claude Code, Cursor o Codex pueden ver el espacio libre, qué lo ocupa y vaciar las cachés que se reconstruyen solas.

## 🧹 Barre sesiones inactivas, con seguridad

- **Huérfanos**: su terminal se cerró pero siguen en ejecución, como el clásico localhost:3000 zombi.
- **Los agentes inactivos en terminales abiertas nunca se preseleccionan**, porque te están esperando.
- **Los servidores de lenguaje del editor están bloqueados** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Finaliza por árbol de procesos, nunca por puerto.** Cada fila se vuelve a comprobar justo antes de finalizarla, así que nunca se confunden dos proyectos.
- **Servidores MCP y navegadores headless que dejaron tus agentes** también se detectan, y un medidor en vivo muestra la RAM recuperada segundos después de cada limpieza.

## 🛟 Cada cambio se puede deshacer

- **Los valores originales se guardan** en SQLite antes del primer cambio.
- **Punto de restauración del sistema** antes de la primera aplicación.
- **El vigilante** reaplica los ajustes que Windows Update revirtió sin avisar.
- **Nunca toca** Windows Update, el motor de Defender ni ~60 servicios esenciales.

## 🛡️ ¿Defender lo borró?

RigSweep es de código abierto pero aún no está firmado. Las herramientas que cambian servicios parecen malware para la heurística de aprendizaje automático, así que Defender puede marcarlo por error (p. ej. `Trojan:Win32/Bearfoos!ml`, donde `!ml` indica una suposición heurística).

El instalador **pregunta** si quieres excluir solo su propia carpeta, y el desinstalador quita esa exclusión. La firma de código con SignPath Foundation está prevista. [Leer la FAQ completa →](https://github.com/yobotn/rigsweep/blob/main/README.es.md#falsos-positivos-del-antivirus)

## ❓ Lo que preguntan los desarrolladores

### ¿Qué hace RigSweep?

RigSweep es una app gratuita y de código abierto para Windows pensada para desarrolladores que trabajan con agentes de IA para programar. Encuentra servidores de desarrollo, daemons de compilación, servidores MCP y CLI de agentes que siguen en marcha después de cerrar una terminal, muestra cada uno con su proyecto, RAM y puertos, y finaliza los que elijas. También borra node_modules antiguos y cachés de paquetes, vacía las cachés del navegador y aplica ajustes de Windows reversibles.

### ¿Cómo detengo los procesos node.exe que siguen ejecutándose después de cerrar la terminal?

Abre Limpieza dev. Los procesos cuya terminal ya no existe aparecen marcados como huérfanos y preseleccionados, con todo su árbol de procesos, la carpeta del proyecto y los puertos en escucha (como un localhost:3000 atascado). Haz clic en Finalizar seleccionados y RigSweep finaliza cada árbol tras comprobar que el ID de proceso sigue perteneciendo al mismo programa.

### ¿Qué agentes de IA y stacks de desarrollo admite?

Claude Code, Codex, Gemini CLI, opencode, Aider y Goose, además de los servidores MCP y navegadores headless que inician. Stacks de desarrollo: Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust y Go, y la VM de WSL 2 / Docker.

### ¿Es seguro? ¿Puedo deshacer los cambios?

No se finaliza ni se borra nada sin tu confirmación. Cada ajuste de Windows guarda primero el valor original y se puede restaurar uno a uno o todos a la vez, y antes del primer cambio se crea un punto de restauración del sistema. Los servidores de lenguaje de los editores y los servicios esenciales de Windows están bloqueados.

### ¿RigSweep es gratis? ¿Recopila datos?

Sí, es gratis para siempre con licencia MIT, sin cuenta y sin anuncios. RigSweep pregunta una vez si quieres compartir estadísticas de uso anónimas (qué limpió, por tipo y tamaño, qué apps conocidas consumen más y qué software de fabricantes de hardware está instalado). No se envía nada antes de que respondas, el informe exacto se ve en Configuración y, al desactivarlo, se borra lo enviado. Nunca incluye rutas, nombres de proyectos ni datos personales. La búsqueda de actualizaciones está desactivada salvo que la actives o pulses Buscar ahora.

### ¿Funciona en macOS o Linux?

No. RigSweep está hecho para Windows 10 y 11, porque trabaja con los servicios de Windows, el Programador de tareas y Defender. Aun así, puede apagar la VM de WSL 2 desde Windows.

## ⚠️ Aviso legal

RigSweep modifica servicios de Windows, tareas programadas, directivas del registro y la configuración de Microsoft Defender, y puede finalizar procesos en ejecución. Cada ajuste se guarda y se puede restaurar, y nada ocurre sin tu confirmación, pero lo usas bajo tu propia responsabilidad. Se proporciona "tal cual", sin garantía de ningún tipo (licencia MIT). Revisa cada cambio antes de aplicarlo, guarda tus propias copias de seguridad y no lo uses en equipos gestionados por tu empresa o centro educativo sin la aprobación de su equipo de TI. RigSweep es un proyecto independiente, sin relación con Microsoft, Google, Anthropic, OpenAI ni ninguna otra herramienta que mencione; todas las marcas pertenecen a sus dueños.

## 🚀 Dale espacio a tus agentes

Instala, toma una medición base, limpia y vuelve a medir.

### 🔏 Firma de código

Firma de código gratuita proporcionada por SignPath.io, certificado de SignPath Foundation (solicitud en curso; las versiones actuales aún no están firmadas). Los instaladores se compilan desde el código etiquetado con GitHub Actions. Sin telemetría ni analíticas. La única petición de red es la búsqueda de actualizaciones, y solo si la activas o haces clic en Buscar ahora.
