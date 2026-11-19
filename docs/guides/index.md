# Guides

> Short, tested guides for Windows developer machines: leftover Claude Code and MCP server processes, ports already in use, and reclaiming disk from node_modules and package caches.

**These guides solve common problems on Windows developer machines with built-in commands first.** Each one shows how to do it by hand, what to check before you delete or end anything, and then how [RigSweep](https://rigsweep.com/) handles it.

- [Claude Code and MCP servers left running on Windows](https://rigsweep.com/guides/claude-code-leftover-processes-windows/): why agent CLIs, MCP servers and dev servers outlive their terminal, and how to find and end them.
- [Port 3000 is already in use on Windows](https://rigsweep.com/guides/port-already-in-use-windows/): find the process that holds a port, end it safely, and deal with WSL, Docker and reserved port ranges.
- [Delete node_modules, build output and package caches](https://rigsweep.com/guides/delete-node-modules-and-dev-caches-windows/): what is safe to delete, each tool's own clean command, and the caveats.

Comparing tools? See [RigSweep compared with CCleaner, BleachBit, npkill and PrivaZer](https://rigsweep.com/compare/).
