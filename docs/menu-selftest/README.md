# Mod menu self-test

Test helpers used to check `crates/bsm-core/assets/servermaker_menu.gsc` inside a real server. They are not part of the product and are not shipped in the installer.

How it works: `selftest-tail.gsc` is appended to a copy of the menu script and placed in the test server's `boiii\custom_scripts\zm` folder. With the dvar `bsm_selftest` set, it adds a test client (bot), runs the menu's own functions on it and reports through `bsm_t_*` dvars, which are read over the server's remote console.

| File | Purpose |
| --- | --- |
| `selftest-tail.gsc` | The harness. `bsm_selftest 1` = full run, `2` = idle bot, `3` = quick weapon run, `4` = run the actions in `bsm_t_list` when `bsm_t_go` is set to 1. |
| `run-selftest.ps1` | Sets up a map/mod, starts the server, waits for `finished`, prints the results. |
| `run-list.ps1` | Mode 4: runs one list of actions and prints the bot's state trace. |
| `rcon.ps1` | Sends console commands to the local server and prints the answers. |
| `read-console.ps1` | Reads the text of the server's hidden console (compile and script errors show there). |

Before running them: build the project (`cargo build --release --locked`), set the environment variable `BSM_DATA_DIR` to a dedicated test folder, and note that the scripts use UDP port 27045 and the rcon password `bsmtest` (written into the test server's `servermaker_custom.cfg`). Use this only for a local test server, not a publicly accessible host.

From the repository root, with the required game and server files installed:

```powershell
$env:BSM_DATA_DIR = Join-Path $env:TEMP 'BsmMenuSelftest'
.\docs\menu-selftest\run-selftest.ps1 -SetupArgs @('--map', 'zm_zod')
```

The runner uses `target\release\bsm.exe`, prepares `server\UnrankedServer` inside the test data folder, and then installs the harness. It overwrites the test server's custom configuration and leaves the server running for inspection. Stop it afterward with `.\target\release\bsm.exe stop` in the same shell, then clear the override with `Remove-Item Env:BSM_DATA_DIR`.

Limits found while using it:

- With only a bot on the server the game does not start its round logic, so round skipping cannot be checked this way.
- Without a mod the bot stands still. With All-around Enhancement the bot walks and sprints by itself.
- The server's rcon answers about one query per second; `rcon.ps1` waits between commands.
