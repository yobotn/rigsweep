# Port 3000 is already in use on Windows

> How to find which process holds port 3000 (or any port) on Windows with netstat, Get-NetTCPConnection and tasklist, end it safely, and handle Docker, WSL and Hyper-V reserved port ranges.

**"Port 3000 is already in use" (`EADDRINUSE`) means another process is listening on that port.** On a dev machine it is usually a dev server that kept running after its terminal closed. Find the owner with `netstat -ano | findstr :3000`, check it with `tasklist`, and end it if it is a leftover.

## 1. Find the process that owns the port

### Command Prompt

```
netstat -ano | findstr :3000
```

Look for a line in the `LISTENING` state. The last number is the process ID (PID).

### PowerShell

```
Get-NetTCPConnection -LocalPort 3000 -State Listen | Select-Object LocalAddress, LocalPort, OwningProcess
```

### What is that PID?

```
tasklist /FI "PID eq 12345"
Get-CimInstance Win32_Process -Filter "ProcessId = 12345" | Select-Object Name, ParentProcessId, CommandLine
```

The command line tells you which project and tool it is, for example `node ... next dev` or `python -m uvicorn ...`.

## 2. End it safely

If it is a dev server you no longer need, end it and its child processes:

```
taskkill /PID 12345 /T /F
```

Or in PowerShell, for a single process:

```
Stop-Process -Id 12345
```

Don't end a process you can't identify. If the owner is `svchost.exe`, `System` or a database you run on purpose, pick another port for your app instead.

## Common causes

| Owner you see | Likely cause | What to do |
| --- | --- | --- |
| `node.exe`, `python.exe`, `bun.exe`, `dotnet.exe` | A dev server left running after its terminal closed, or after Ctrl+C stopped only the npm or cmd wrapper | Check the command line, then end the tree with `taskkill /T /F` |
| `wslrelay.exe` | A server running inside WSL 2. Windows forwards its localhost port through this relay | Stop the server inside WSL. `wsl -e ss -ltnp` shows Linux listeners. `wsl --shutdown` stops everything in WSL |
| A Docker Desktop process | A container publishes the port | `docker ps --filter "publish=3000"`, then stop that container |
| Nothing listens, yet the bind fails with `EACCES` or a permissions error | The port is inside a range Windows reserved, often for Hyper-V, WSL 2 or Docker | See the next section |

## Reserved port ranges (Hyper-V, WSL 2, Docker)

Windows can reserve blocks of ports for Hyper-V and related features. A port inside a reserved block can't be used even though no process owns it. Node reports this as `listen EACCES: permission denied` instead of `EADDRINUSE`. List the reserved ranges:

```
netsh interface ipv4 show excludedportrange protocol=tcp
```

If your port falls inside a listed range, the simplest fix is to use a port outside it. Restarting the WinNAT service (`net stop winnat`, then `net start winnat`, as administrator) often releases the reservation until the next restart. It briefly interrupts networking for WSL and containers.

## How RigSweep helps

[RigSweep](https://rigsweep.com/)'s Dev Cleanup tab lists every dev server, build daemon, MCP server and agent CLI with its project folder, age, RAM and listening ports, so the process holding `localhost:3000` is visible by name and project. A dev server whose terminal is gone is pre-selected, even when it still holds a port. Processes owned by your editor are locked, the WSL 2 / Docker VM is never pre-selected, and nothing is ended until you click. RigSweep doesn't change reserved port ranges.

See also: [Claude Code and MCP servers left running on Windows](https://rigsweep.com/guides/claude-code-leftover-processes-windows/). [Download RigSweep](https://github.com/yobotn/rigsweep/releases/latest) (free, MIT).

## FAQ

### How do I find what is using port 3000 on Windows?

Run `netstat -ano | findstr :3000` and read the PID in the last column of the LISTENING line, or in PowerShell run `Get-NetTCPConnection -LocalPort 3000 -State Listen`. Then `tasklist /FI "PID eq <pid>"` shows the program.

### How do I kill the process on port 3000?

After checking what it is, run `taskkill /PID <pid> /T /F`. `/T` also ends its child processes, which matters for npm and npx wrappers.

### Why does the port show as used by wslrelay.exe?

A server inside WSL 2 is listening on that port, and Windows forwards localhost traffic to it through wslrelay.exe. Stop the server inside WSL, or run `wsl --shutdown`.

### Why do I get EACCES on a port nothing is using?

The port is probably in a range Windows reserved. Check with `netsh interface ipv4 show excludedportrange protocol=tcp` and pick a port outside those ranges.
