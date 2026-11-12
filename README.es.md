<div align="center">

# 🧹 RigSweep

**Espacio para tus agentes.** Libera RAM, CPU y disco en equipos Windows de desarrollo saturados por sesiones de agentes de IA.

**[🌐 Sitio web](https://rigsweep.com)** · **[⬇️ Descargar](https://github.com/yobotn/rigsweep/releases/latest)** · **[📖 FAQ antivirus](#%EF%B8%8F-falsos-positivos-del-antivirus)**

[🇬🇧 English](README.md) · [🇩🇪 Deutsch](README.de.md) · 🇪🇸 Español · [🇫🇷 Français](README.fr.md) · [🇮🇹 Italiano](README.it.md) · [🇧🇷 Português](README.pt-BR.md) · [🇷🇺 Русский](README.ru.md) · [🇯🇵 日本語](README.ja.md) · [🇰🇷 한국어](README.ko.md) · [🇨🇳 简体中文](README.zh-CN.md) · [🇮🇱 עברית](README.he.md) · [🇸🇦 العربية](README.ar.md)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-orange?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-green)
![Price: free](https://img.shields.io/badge/price-free-brightgreen)

<img src="docs/screenshots/cleanup-es.png" alt="Limpieza dev de RigSweep" width="860">

</div>

---

## 📖 Por qué lo creé

En la era de la IA mi portátil ya no ejecuta *una* sola cosa. En un día normal tengo varias sesiones de **Claude Code** en distintas terminales, un servidor dev por proyecto, watchers de compilación y un navegador lleno de documentación, previsualizaciones y paneles. Cada sesión nueva lanza más procesos de Node, y al cerrar una terminal algunos **siguen ejecutándose en segundo plano**.

Empezó a sentirse *saturado*: ventiladores a tope, la RAM siempre al límite y ninguna idea clara de qué la estaba consumiendo. El Administrador de tareas solo me mostraba una pared de `node.exe` y `chrome.exe`, sin decirme de qué sesión o proyecto venían ni si algo los seguía necesitando.

Así que medí mi propio equipo (32 GB de RAM) en plena jornada de trabajo:

| 🔎 Lo que encontré | Cantidad | RAM |
|---|---:|---:|
| 🌐 Procesos de Chrome | 37 | **6,5 GB** |
| ⬢ Procesos de Node + Claude (CLI de agentes, servidores dev, la app de escritorio) | 14 | **2,5 GB** |
| 🧩 Servicios de Windows en ejecución (telemetría, Xbox, indexador de búsqueda…) | ~150 | — |

Nada de eso era "un virus" ni "basura". Eran los **restos normales de trabajar con agentes de IA**, más procesos de fondo de Windows que un equipo de desarrollo no necesita. Las apps "aceleradoras de PC" existentes o hacen cosas irreversibles que dan miedo o no saben nada de flujos de desarrollo. Así que hice la herramienta que yo quería:

> **Ver qué está usando realmente tu equipo, barrer lo que sobra y que cada cambio sea reversible.**

---

## ✨ Funciones

### 🧹 Limpieza dev: procesos dev olvidados en cualquier stack
Encuentra **servidores dev, daemons de compilación y CLI de agentes de IA** y agrupa cada uno con **todo su árbol de procesos**, carpeta del proyecto, antigüedad, RAM y puertos en escucha.

| Stack | Qué encuentra |
|---|---|
| ⬢ Node · Bun · Deno | servidores dev (Vite, Astro, Next…), watchers, esbuild |
| 🐍 Python | uvicorn, Flask, Django, Jupyter, Streamlit, Celery, procesos de venv, servidores MCP en Python |
| ☕ Java | daemons de Gradle y Kotlin, Maven, Spring Boot, Quarkus |
| 🟣 .NET | `dotnet watch/run`, servidor de compilación Roslyn, nodos de MSBuild sobrantes |
| 💎 Ruby · 🐘 PHP · 🦀 Rust · 🐹 Go | `rails`/`ruby`, `php -S`/artisan, `cargo watch/run`, `go run` |
| 🤖 CLI de agentes | Claude Code, Codex, Gemini CLI, opencode, Aider, Goose |
| 🔌 Servidores MCP | servidores que inició tu agente (`@modelcontextprotocol/*`, `@playwright/mcp`, `mcp-server-*`, FastMCP…) y que siguieron en ejecución cuando el agente terminó, incluso detrás de un lanzador `cmd /c npx` |
| 🌐 Navegadores automatizados | Chrome y Edge de Playwright / Puppeteer / `--headless` con todo su árbol de procesos de representación. Tu propio navegador nunca aparece |
| 🐧 WSL 2 / Docker | la VM de Linux (`vmmemWSL`), que se apaga limpiamente con `wsl --shutdown` y nunca se preselecciona |

Python y Java también ejecutan apps de escritorio normales, así que en esos casos solo se listan los procesos con una firma clara de desarrollo.

- 🧟 **Huérfano**: su terminal se cerró pero el proceso siguió en ejecución. Se preselecciona aunque siga ocupando un puerto (el clásico `localhost:3000` zombi).
- 🔌 **Servidor MCP**: se marca en cada fila que lo es, para distinguir el servidor de herramientas que dejó un agente de tu propio servidor dev.
- 👯 **Copia antigua**: un servidor MCP del que también se ejecuta una copia más nueva (por ejemplo, en tu sesión de agente actual) queda marcado, para que el sobrante sea fácil de identificar.
- 📈 **Medidor en vivo**: la RAM y la CPU se miden cada 2 segundos mientras RigSweep está en ejecución (ventana o área de notificación). Justo después de una limpieza, manual o automática, muestra **cuánta RAM se recuperó de verdad** y la CPU antes → después, medida y no estimada.
- 🌡️ **Seguimiento de temperatura**: las temperaturas de CPU y GPU aparecen en directo en el Panel con la tendencia y el máximo de los últimos 5 minutos, y un aviso si el equipo se mantiene por encima de 90 °C. La GPU usa el mismo contador que el Administrador de tareas; la CPU, la zona térmica ACPI (sin instalar ningún controlador ni requerir permisos de administrador; aproximada en muchos equipos de sobremesa). Haz clic en el recuadro para cambiar entre °C y °F.
- 💤 **Inactivo**: sin actividad de CPU durante mucho tiempo. Los runtimes sin ningún puerto abierto se preseleccionan. Una **sesión de agente inactiva en una terminal abierta nunca se preselecciona**, porque solo te está esperando (y `claude --resume` la recupera de todos modos).
  El tiempo que cuenta como inactivo se define en **Configuración** (en minutos, 120 de forma predeterminada; p. ej., 5).
- 🧹 **Limpieza automática** (desactivada de forma predeterminada, en Configuración): cada minuto, RigSweep finaliza los runtimes dev inactivos que ha visto sin uso de CPU durante todo el período de inactividad. Las CLI de agentes, la VM de WSL/Docker y los procesos del editor nunca se finalizan automáticamente, y cada árbol finalizado queda registrado en el Historial.
- 🟠🔴 **Indicador en el área de notificación**: el icono del área de notificación (y el botón de la barra de tareas mientras la ventana está abierta) muestra un punto naranja cuando los procesos dev inactivos ocupan ≥ 1 GB y un punto rojo a partir de ≥ 2 GB. Pasa el puntero por el icono para ver cuánto. Los umbrales se definen en Configuración, donde también se puede desactivar el punto.
- ♻️ **Daemon de compilación**: los servidores de Gradle, Kotlin y Roslyn están pensados para quedarse en segundo plano. Finalizarlos no hace daño; se reinician en la siguiente compilación.
- 🔒 **Bloqueado**: los procesos que pertenecen a **VS Code, Cursor, JetBrains, Visual Studio…** (tus servidores de lenguaje) o a una app de escritorio nunca se pueden finalizar desde aquí.
- ✅ Nada se finaliza sin tu confirmación, y antes de cada cierre se comprueba que el PID no se haya reutilizado.

### 💾 Disco dev: node_modules y cachés de paquetes
Los proyectos antiguos conservan gigas de `node_modules`, `.next`, `target`, `bin/obj` y `.venv` mucho después de que dejes de trabajar en ellos, y las cachés de paquetes no paran de crecer. RigSweep encuentra ambas cosas y borra lo que elijas.

- 🗂️ **Proyectos**: carpetas de compilación y de dependencias de Node, Rust, .NET, Java/Gradle/Maven y Python. Una carpeta solo cuenta si su archivo de proyecto está al lado (`package.json`, `Cargo.toml`, `*.csproj`, `pyvenv.cfg`…), así que nunca se toca una carpeta `build` cualquiera. Solo se preseleccionan los proyectos que nadie ha editado en 30 días (ajustable), y los que usa un proceso en marcha quedan bloqueados 🔒.
- 📦 **Cachés de paquetes**: npm, npx, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Puppeteer, Hugging Face. pnpm, uv y Go se limpian con sus propios comandos (`pnpm store prune`…); en Playwright y Puppeteer solo se quitan las versiones antiguas del navegador y se conserva la más reciente.
- 🎯 **Selección automática**: en la propia pestaña Disco dev eliges cuánto tiempo debe pasar un proyecto sin tocarse (de 3 a 365 días), qué carpetas se marcan (resultados de compilación, dependencias, cachés de paquetes) y un tamaño mínimo.
- 🚫 **No limpiar nunca**: puedes desactivar cualquier caché. Las que implican una descarga grande (Playwright, Puppeteer, Hugging Face, Gradle, Maven, paquetes de NuGet, módulos de Go) vienen desactivadas.
- 🙈 **Exclusiones**: AppData, las carpetas de herramientas de tu perfil (`.cargo`, `.nvm`, `.vscode`…), `.git` y Windows se omiten siempre. Añade tus propias rutas o nombres de carpeta (`vendor`, `old-*`) en Ajustes, o pulsa **Excluir** en un proyecto.
- ⏸️ **Pausa, reanuda o detén** una limpieza larga y síguela con una barra de progreso. Si un reinicio la interrumpe, puede continuar donde se quedó.
- ✅ Por defecto analiza tu perfil de usuario (Escritorio, Documentos, Descargas, OneDrive…). No se borra nada sin tu confirmación, y cada carpeta se vuelve a comprobar justo antes de borrarla.

### 🖥️ Hardware: una pestaña en lugar de cuatro herramientas
Lo básico para lo que normalmente abrirías CPU-Z, GPU-Z, CrystalDiskInfo y CrystalDiskMark, leído directamente de Windows. No se instala ningún controlador.

- 💽 **Estado de los discos**: En buen estado / Desgastándose / Problema para cada disco, con vida restante, temperatura, horas encendido y errores sin corregir. Si un disco empieza a fallar, aparece un aviso en el panel.
- ⚡ **Prueba de velocidad**: lectura y escritura secuenciales y lecturas aleatorias 4K, por unidad. Escribe un archivo temporal de 512 MB y luego lo elimina.
- 🧮 **CPU, GPU, memoria**: modelo, núcleos e hilos, caché, virtualización (necesaria para WSL 2 y Docker), memoria de vídeo y controlador, tipo de RAM, velocidad y ranuras libres.
- 🔋 **Sistema y batería**: placa base, BIOS, compilación de Windows, tiempo encendido y desgaste de la batería en portátiles.

### 📊 Informe de mejora: antes y después reales
Nada de cifras falsas como "tu PC es un 87 % más rápido". RigSweep **mide** tu equipo y compara dos capturas: RAM usada/libre, CPU media, número de procesos, servicios en ejecución, RAM de Chrome y RAM de Node/agentes.

- 📸 Se toma una **línea base** automáticamente antes de tu primer ajuste.
- 🔁 Se toma un par **antes/después** en cada Limpieza dev.
- 📋 **Copiar como Markdown** para compartir tu resultado en un issue, una publicación o este README.

### 🌐 Asistente de pestañas para Chrome
Una app de escritorio **no puede** cerrar pestañas de Chrome, así que RigSweep incluye una pequeña extensión ([`chrome-extension/`](chrome-extension/)) sin acceso a la red y solo con los permisos `tabs` / `storage` / `alarms`:
- 💤 **Suspender pestañas inactivas**: libera su memoria, pero la pestaña se queda en la barra de pestañas
- 🧹 **Cerrar pestañas duplicadas**
- 🎯 **Cerrar pestañas por patrón**, como `localhost:*` o `*.vercel.app`
- ⏰ **Suspensión automática** cada 10 minutos (opcional)

### ⚙️ Ajustes de Windows reversibles
Un catálogo seleccionado ([`catalog/tweaks.json`](src-tauri/catalog/tweaks.json)) de telemetría, Xbox, indexador de búsqueda, apps en segundo plano, exclusiones de Defender para carpetas dev, plan de energía y más, cada uno con un nivel de riesgo:

| Nivel | Significado |
|---|---|
| 🟢 `safe` | Sin efectos secundarios apreciables (preseleccionado) |
| 🟡 `moderate` | Pequeña contrapartida, explicada |
| 🔴 `caution` | Desactiva una función; debes confirmarlo |

<img src="docs/screenshots/tweaks.png" alt="Ajustes" width="860">

- 💾 **Cada valor original se guarda** en SQLite antes del primer cambio. Puedes restaurar un ajuste, una categoría o todo.
- 🛟 Se crea un **punto de restauración del sistema** antes de la primera aplicación.
- 🔁 Un **vigilante** vuelve a aplicar los ajustes que una actualización de Windows revirtió sin avisar, y cuenta cuántas veces ocurre.
- 🧭 **Edge no vuelve**: desinstala Edge desde Configuración y un ajuste impide que Edge Update o Windows Update lo reinstalen. No se toca el runtime de WebView2 que necesitan otras aplicaciones.
- 🚫 Nunca se tocan: Windows Update, el motor de Defender, RPC/DCOM/LSA ni otros ~60 servicios esenciales ([`PROTECTED_SERVICES`](src-tauri/src/catalog.rs)).

### 🔍 Analizador de procesos
CPU / RAM / disco por proceso, y **qué servicios se ejecutan dentro de cada `svchost.exe`**. Deshabilita un servicio o finaliza un proceso directamente desde la lista (los elementos esenciales están bloqueados 🔒). Cada aplicación se muestra con su propio icono.

### 🚀 Inicio: lo que el Administrador de tareas no muestra
La lista de aplicaciones de arranque del Administrador de tareas omite casi todo lo que se inicia con Windows. RigSweep lo muestra todo, salvo los componentes del propio Windows:

- 🧩 **Servicios de inicio automático** de aplicaciones y controladores (clientes VPN, utilidades del fabricante, actualizadores). Pasan a Manual: ya no se inician al arrancar, pero la aplicación puede iniciarlos cuando la abres.
- ⏰ **Tareas programadas** que se ejecutan al iniciar sesión, al arrancar o según una programación.
- 🚀 **Aplicaciones de arranque** del registro y de la carpeta Inicio, desactivadas con el mismo interruptor que usa el Administrador de tareas.
- 💾 Cada cambio se respalda y puede restaurarse, y si una actualización vuelve a activar una aplicación, la supervisión la desactiva de nuevo.

### 🔄 Actualizaciones, tú decides
**🛠️ Configuración → 🔄 Actualizaciones** tiene un botón **Buscar ahora** y la opción *Buscar actualizaciones automáticamente* (al iniciar y después una vez al día), que está **desactivada de forma predeterminada**. Cuando sale una versión nueva, aparece un botón en la barra lateral con las novedades. No se instala nada hasta que haces clic en **Instalar y reiniciar**, y solo si la descarga está firmada con la clave de RigSweep. Tus ajustes, copias de seguridad y configuración se conservan. Consulta la [política de privacidad](#-pol%C3%ADtica-de-privacidad) para saber exactamente qué hace una búsqueda.

### 🌍 12 idiomas
La interfaz está en **English, Deutsch, Español, Français, Italiano, Português (Brasil), Русский, 日本語, 한국어, 简体中文, עברית y العربية** (se cambia en la barra lateral), igual que el instalador y la extensión de Chrome. Cada idioma usa los términos oficiales de Windows (Administrador de tareas, Servicios, Programador de tareas…) en lugar de una traducción palabra por palabra. ¿Ves algo raro? Abre un issue o un PR sobre `ui/locales/<código>.js` (app), `docs/locales/<código>.js` (sitio web) o `README.<código>.md`.

---

## 🏁 Antes y después en tu equipo

Instala, abre **📊 Informe de mejora → 📸 Medir ahora**, haz una Limpieza dev y aplica los ajustes seguros, reinicia, vuelve a medir y haz clic en **📋 Copiar como Markdown**. ¡Comparte tus resultados en [Discussions](../../discussions)!

<!-- TODO(maintainer): paste your own "Copy as Markdown" result here once you've run it on your machine. -->

> ℹ️ Los ajustes de servicios surten todo su efecto tras reiniciar. La Limpieza dev y la suspensión de pestañas de Chrome se notan al instante.

---

## 📦 Instalación

1. Descarga `RigSweep_x.y.z_x64-setup.exe` desde [**Releases**](../../releases).
2. Ejecútalo. Pide permisos de administrador porque RigSweep gestiona servicios y directivas.
3. El instalador te pregunta si quieres permitir RigSweep en Microsoft Defender (ver abajo). Tú decides.
4. Las versiones posteriores se pueden instalar desde **🛠️ Configuración → 🔄 Actualizaciones** (a partir de la 0.5.0).

🌐 **Asistente de Chrome:** en RigSweep ve a **🛠️ Configuración → 🌐 Asistente de pestañas para Chrome → Abrir carpeta de la extensión**, y en Chrome: `chrome://extensions` → **Modo desarrollador** → **Cargar descomprimida** → elige esa carpeta.

## 🛡️ Falsos positivos del antivirus

RigSweep es de código abierto pero **aún no tiene firma de código**. Las herramientas que cambian servicios y la configuración de Defender se parecen mucho al malware para la heurística de aprendizaje automático, así que **Microsoft Defender puede eliminar el exe por error**. Nos pasó durante el desarrollo como `Trojan:Win32/Bearfoos.B!ml`, donde `!ml` indica una suposición heurística y no una firma de malware conocido.

Qué hacemos al respecto:
- ❓ **Pregunta en el instalador (opcional):** "¿Permitir RigSweep en Microsoft Defender?" Si respondes Sí, se excluye **solo la carpeta de instalación**, y el desinstalador vuelve a quitar esa exclusión. Las instalaciones silenciosas nunca la añaden.
- 🛠️ **Configuración → 🛡️ Antivirus y falsos positivos:** consulta el estado y añade o quita la exclusión cuando quieras.
- ✍️ **Está prevista la firma de código** con [SignPath Foundation](https://signpath.org) (gratis para proyectos de código abierto). Es la solución de verdad. Consulta la [política de firma de código](#-pol%C3%ADtica-de-firma-de-c%C3%B3digo).

Si ya se eliminó: **Seguridad de Windows → Protección antivirus y contra amenazas → Historial de protección →** elige la entrada de RigSweep → **Acciones → Restaurar**. Hazlo solo con una descarga de la página de Releases de este repositorio o con una compilación que hayas hecho tú. También puedes [notificar el falso positivo a Microsoft](https://www.microsoft.com/wdsi/filesubmission) por tu cuenta.

---

## 🛠️ Compilar desde el código fuente

```powershell
winget install Rustlang.Rustup        # + VS 2022 Build Tools (carga de trabajo C++)
npm install
npm run build                         # -> src-tauri/target/release/rigsweep.exe + bundle/nsis/*-setup.exe
```
Ejecución en desarrollo: `npm run dev` (acepta el UAC; la app se vuelve a iniciar con privilegios elevados).

> 💡 Puede que Defender también ponga en cuarentena tus compilaciones locales recién hechas. Añade la carpeta de tu clon a la lista de *Carpetas dev* de RigSweep (Configuración) y aplica el ajuste **Defender exclusions for dev folders**, o excluye `src-tauri\target` manualmente.

### ⌨️ CLI
| Comando | Efecto |
|---|---|
| `rigsweep.exe` | Interfaz gráfica + área de notificación |
| `rigsweep.exe --apply` | Sin interfaz: comprueba el perfil, vuelve a aplicar los ajustes revertidos y sale (lo usa la tarea de inicio) |
| `rigsweep.exe --apply --tray` | Igual, y después se queda en el área de notificación con el vigilante |
| `rigsweep.exe --restore-all` | Revierte todo a los valores originales guardados |

### 🧪 Pruebas
```powershell
cd src-tauri
cargo test                                          # validez del catálogo, copias de seguridad, capturas, análisis de comandos
cargo test live_status   -- --ignored --nocapture   # solo lectura: estado detectado de cada ajuste
cargo test live_dev_scan -- --ignored --nocapture   # solo lectura: lo que ve la Limpieza dev ahora mismo
```

### ➕ Añadir ajustes
Añade una entrada a [`catalog/tweaks.json`](src-tauri/catalog/tweaks.json). Los tipos de acción son `service` (start: disabled|manual), `task` (path), `registry` (HKLM|HKCU, dword|string), `defender_exclusion` y `power_plan`. Los elementos `caution` deben incluir un `warning`. `cargo test` rechaza los servicios protegidos y las entradas mal formadas.

## 🐳 docker-optimize (herramienta aparte)
No forma parte de la app. [`docker-optimize/`](docker-optimize/) es un script independiente, reversible y de un solo uso que limita la VM WSL2 de Docker Desktop (memoria, CPU, recuperación de RAM, disco disperso) y desactiva extras de Docker Desktop que no necesitas. Nunca elimina contenedores, imágenes ni volúmenes. Consulta su [README](docker-optimize/README.md) (en inglés).

## 🗂️ Dónde se guarda cada cosa
| Qué | Dónde |
|---|---|
| Base de datos (copias de seguridad, historial, capturas) | `%ProgramData%\RigSweep\rigsweep.db` (+ `.bak.1`–`.bak.5`) |
| Tarea de inicio | Programador de tareas → `RigSweep-Reapply` |
| Perfiles exportados | `Documents\RigSweep-profile-*.json` |

¿Vienes de **aiBoost** (el nombre anterior)? Tu base de datos se migra automáticamente en el primer inicio, y la antigua tarea de inicio se reemplaza.

## 🔏 Política de firma de código

Firma de código gratuita proporcionada por [SignPath.io](https://about.signpath.io/), certificado de [SignPath Foundation](https://signpath.org/).
*(Solicitud en curso: las versiones hasta la v0.3.0 aún **no están firmadas**.)*

- **Compilaciones:** los instaladores de cada versión se compilan desde el código etiquetado con [GitHub Actions](.github/workflows/release.yml) en ejecutores hospedados por GitHub. Solo esas compilaciones se enviarán a firmar.
- **Committers y revisores:** [@yobotn](https://github.com/yobotn)
- **Aprobadores:** [@yobotn](https://github.com/yobotn)

### 🔒 Política de privacidad
Este programa no transferirá ninguna información a otros sistemas en red a menos que lo solicite expresamente el usuario o la persona que lo instala o lo utiliza. RigSweep no tiene telemetría ni analíticas, y la extensión de Chrome no hace ninguna petición de red. La única petición de red que puede hacer RigSweep es la **búsqueda de actualizaciones**, y solo cuando tú la pides: al hacer clic en *Buscar ahora* en Configuración o al activar *Buscar actualizaciones automáticamente* (desactivado de forma predeterminada). Descarga `latest.json` de las Releases de GitHub de este repositorio y no envía ningún dato sobre tu equipo; una actualización solo se instala después de que hagas clic en *Instalar*, y solo si su firma coincide con la clave de firma de RigSweep. Todo lo que guarda (copias de seguridad, historial, mediciones) se queda en `%ProgramData%\RigSweep` en tu PC.

## ⚠️ Aviso legal

RigSweep modifica servicios de Windows, tareas programadas, directivas del registro y la configuración de Microsoft Defender, y puede finalizar procesos en ejecución. Cada ajuste se guarda y se puede restaurar, y nada ocurre sin tu confirmación, pero **lo usas bajo tu propia responsabilidad**. Se proporciona **"tal cual", sin garantía de ningún tipo** (consulta la [licencia MIT](LICENSE)). Revisa cada cambio antes de aplicarlo, guarda tus propias copias de seguridad y no lo uses en equipos administrados por tu empresa o centro educativo sin la aprobación de su equipo de TI.

RigSweep es un proyecto independiente, **sin relación con** Microsoft, Google, Anthropic, OpenAI ni ninguna otra herramienta que mencione. Todas las marcas comerciales pertenecen a sus propietarios.

## 🤝 Contribuir
Los issues y los PR son bienvenidos, sobre todo ajustes nuevos (con un nivel de riesgo claro), más stacks de desarrollo y CLI de agentes para la Limpieza dev, y traducciones.

## 📄 Licencia
[MIT](LICENSE): gratis para siempre, haz lo que quieras, sin garantía.
