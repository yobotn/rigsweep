# Claude Code and MCP servers left running on Windows

> Why Claude Code sessions, MCP servers and dev servers keep running on Windows after you close the terminal, how to find the leftovers with Task Manager and PowerShell, and how to end them safely.

**On Windows, a process keeps running when the process that started it exits.** So when you close a terminal, or an agent CLI like Claude Code exits, the MCP servers, dev servers and test runners it started can stay alive, holding RAM, CPU and ports. You can find them with Task Manager or PowerShell and end them with `taskkill /PID <pid> /T /F` after checking what they are.

## Why it happens

Windows doesn't end a process when its parent exits. Closing a console window signals the programs attached to that console, but processes started in the background, detached or with their own hidden window get no signal. Agent CLIs and many dev tools start MCP servers, watchers and long commands that way. A child only ends with its parent if the launcher put both in a job object set to end them together, and most dev tools don't. Common ways it happens:

- **You close the terminal window or tab.** A dev server, watcher or MCP server running in the background or in its own hidden window keeps running.
- **The agent CLI exits but its MCP servers don't.** Claude Code starts each stdio MCP server as a child process. If the CLI exits abnormally, or a server was started through a wrapper such as `cmd /c npx`, the real server process can outlive it.
- **An agent command times out.** A long build or test run that the agent moved to the background can keep running after the session ends.
- **Ctrl+C stops only the wrapper.** With `npm run dev` or `npx`, you stop npm or the `cmd` launcher, but a child that ignores the signal or was started detached keeps the port.

These are known, reported problems. Some public reports, with their status in November 2026:

- [anthropics/claude-code#92583](https://github.com/anthropics/claude-code/issues/92583) (open): on Windows, Bash tool commands moved to the background on timeout are not cleaned up when the session ends.
- [anthropics/claude-code#84464](https://github.com/anthropics/claude-code/issues/84464) (closed): on Windows, a stopped background task's shell was ended but its process tree kept running.
- [anthropics/claude-code#66151](https://github.com/anthropics/claude-code/issues/66151) (closed): Stop hook commands on Windows left orphan child processes when Claude Code ended abnormally.
- [anthropics/claude-code#1935](https://github.com/anthropics/claude-code/issues/1935) (open, reported on macOS): MCP servers not terminated when Claude Code exits.
- [modelcontextprotocol/typescript-sdk#2023](https://github.com/modelcontextprotocol/typescript-sdk/issues/2023) (open): closing a stdio transport signals only the direct child, so a server started through `npx` or `uvx` can be orphaned. On Windows, Node's `ChildProcess.kill()` doesn't end the process tree.

## How to see what's still running

### Task Manager

Open Task Manager (Ctrl+Shift+Esc), go to **Details**, and sort by name. Right-click a column header, choose **Select columns** and add **Command line**. Look for `node.exe`, `python.exe`, `uv.exe`, `bun.exe` or `claude.exe` rows whose command line names an MCP server, a dev server or a project you closed.

### tasklist

```
tasklist /FI "IMAGENAME eq node.exe"
```

### PowerShell: list orphans

Each process records the ID of its parent. When that parent is gone, the process is an orphan. This lists Node and Python processes whose parent no longer exists, with their command line:

```
$all = Get-CimInstance Win32_Process
$alive = @{}; foreach ($p in $all) { $alive[$p.ProcessId] = $true }
$all | Where-Object { $_.Name -in 'node.exe','python.exe','uv.exe','bun.exe' -and -not $alive[$_.ParentProcessId] } |
  Select-Object ProcessId, Name, CreationDate, CommandLine | Format-List
```

Windows reuses process IDs, so a parent ID can point to an unrelated newer process. If a row looks wrong, compare the `CreationDate` of the parent and the child: a real parent started first.

### Which process holds a port

```
netstat -ano | findstr :3000
```

The last column is the PID. The [port 3000 guide](https://rigsweep.com/guides/port-already-in-use-windows/) covers this in more detail.

## The manual fix

First check what the process is. Ending the wrong `node.exe` can close an editor's language server or a tool you still use.

```
Get-CimInstance Win32_Process -Filter "ProcessId = 12345" | Select-Object Name, ParentProcessId, CommandLine
```

If the command line shows a leftover MCP server or dev server, end it together with its children:

```
taskkill /PID 12345 /T /F
```

`/T` ends the whole tree below that PID and `/F` forces it. Unsaved state in that process is lost, and there is no undo. Skip processes whose command line points into VS Code, Cursor or another editor's folder, and processes run by pm2 or forever on purpose.

## How RigSweep does it

[RigSweep](https://rigsweep.com/) is a free, open-source Windows app that does the steps above for you. Its Dev Cleanup tab groups each dev server, build daemon, MCP server and agent CLI with its whole process tree, project folder, age, RAM and listening ports.

| Case | What RigSweep does |
| --- | --- |
| Orphan (its terminal or launcher is gone) | Pre-selected, even when it still holds a port |
| MCP server | Tagged as an MCP server; an older copy of a server that also runs under your current session is marked |
| Idle agent session in an open terminal | Listed, never pre-selected |
| pm2 or forever daemon and the apps it runs | Listed as managed, never pre-selected or auto-ended |
| Process owned by VS Code, Cursor, JetBrains or Visual Studio | Locked, can't be ended from RigSweep |
| WSL 2 / Docker VM | Shut down with `wsl --shutdown`, never pre-selected |
| After a sweep | Shows the RAM it got back and CPU before and after, measured |

Nothing is ended until you click, and RigSweep checks for PID reuse before every kill. [Download RigSweep](https://github.com/yobotn/rigsweep/releases/latest) (Windows 10 and 11, MIT license).

## FAQ

### Why does Claude Code leave node.exe processes running on Windows?

Windows doesn't end child processes when their parent exits. MCP servers, dev servers and test runners that Claude Code or your terminal started keep running if the CLI or terminal closes before they are stopped.

### Is it safe to end all node.exe processes?

No. Some belong to your editor (language servers), to tools you run on purpose with pm2, or to a dev server you still use. Check each command line first, then end only the leftovers with `taskkill /PID <pid> /T /F`.

### How do I find orphaned processes on Windows?

In PowerShell, list processes with `Get-CimInstance Win32_Process` and keep those whose `ParentProcessId` no longer exists. Because Windows reuses process IDs, compare creation times when a result looks wrong.

### Does RigSweep end processes automatically?

Only if you turn on auto-cleanup in Settings, which is off by default. Even then, agent CLIs, the WSL/Docker VM, editor-owned processes and pm2 or forever apps are never auto-ended.
