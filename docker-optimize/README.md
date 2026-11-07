# 🐳 docker-optimize

**One-time tuning for Docker Desktop + WSL2 on Windows dev machines.**
A standalone script that lives in this repo but is **not part of the RigSweep app** and is not shipped in the installer.

## Why a script and not an app feature?
RigSweep sweeps things that pile up again and again (stale Node/agent processes, Chrome tabs). Docker/WSL tuning is a config change you make **once**, so it doesn't need UI, and the app shouldn't own Docker's or WSL's config files.

## The problem
With no `%USERPROFILE%\.wslconfig`, the WSL2 VM that Docker Desktop runs in (`vmmemWSL` in Task Manager) can grow to **half your RAM** and keep it after the load is gone. Docker Desktop also runs extras most dev setups don't need.

## What it changes

| Where | Setting | Value |
|---|---|---|
| `.wslconfig` `[wsl2]` | `memory` | RAM / 4, clamped to 4–16 GB |
| | `processors` | half the logical CPUs (min 2) |
| | `swap` | 2 GB |
| `.wslconfig` `[experimental]` | `autoMemoryReclaim` | `gradual`: freed RAM goes back to Windows |
| | `sparseVhd` | `true`: Docker's disk shrinks when data is deleted |
| Docker Desktop | `UseResourceSaver` | on: VM stops when no container runs |
| | `KubernetesEnabled` | off |
| | `ExtensionsEnabled` | off |
| | `SbomIndexing` | off |
| | `EnableDockerAI` (Ask Gordon) | off |

An existing `.wslconfig` is **merged**: only these keys are touched. Override limits with `-MemoryGB`, `-Processors`, `-SwapGB`.

## Usage
```powershell
powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1           # dry run: shows the plan, changes nothing
powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1 -Apply    # apply (asks once; -Yes to skip)
powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1 -Revert   # undo
```

## What `-Apply` does
1. **Inspects:** WSL 2.0+ check, RAM/CPU, containers with their restart policy, VM memory, disk size.
2. **Backs up** to `%USERPROFILE%\.rigsweep\docker-wsl-backup\`: original `.wslconfig` (or a "none" marker), `settings-store.json`, and the list of running containers. The first backup is kept on later runs, so `-Revert` always goes back to your original state.
3. **Writes** the config after quitting Docker Desktop, because the app rewrites its settings on exit.
4. **Restarts:** `wsl --shutdown`, starts Docker Desktop, restarts the containers that were running.
5. **Verifies:** the number of containers, images and volumes is unchanged, and the running containers are back up.

## Safety
- **Never deletes anything:** no `docker system prune`, no VHD compaction, no image or volume removal.
- Running containers stop for about a minute during the restart and are started again automatically.
- Running it a second time does nothing: "Everything is already tuned."
- Docker Desktop setting names are verified against Docker Desktop 4.91 (Windows, WSL2 backend).

## Not covered
Moving from Docker Desktop to Docker Engine inside WSL (lighter, no license limits) is a **separate migration**. Images, containers and volumes don't carry over, so volumes need a manual backup first.
