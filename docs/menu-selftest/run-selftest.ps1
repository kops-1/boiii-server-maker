param([string[]]$SetupArgs, [int]$MaxSeconds = 420)
# Starts the test server with the given map/mod and the temporary self-test
# script, waits until the self-test reports "finished", prints its results.
$sp = $PSScriptRoot
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)   # the project folder
if (-not $env:BSM_DATA_DIR) { throw "Set BSM_DATA_DIR to an empty test folder first, e.g. D:\BSM-test-data" }
$ErrorActionPreference = 'Stop'
$bsm = Join-Path $root 'target\release\bsm.exe'
$srv = Join-Path ([IO.Path]::GetFullPath($env:BSM_DATA_DIR)) 'server\UnrankedServer'
if (-not (Test-Path -LiteralPath $bsm)) { throw 'Build the CLI first with cargo build --release --locked.' }
& $bsm stop | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not stop the test server.' }
& $bsm setup @SetupArgs --server-dir $srv --port 27045 --menu off --start-points 0 | Select-String "^Map:|Linked|error"
if ($LASTEXITCODE -ne 0) { throw 'Test server setup failed.' }
New-Item -ItemType Directory -Path "$srv\boiii\custom_scripts\zm" -Force | Out-Null
$menu = [IO.File]::ReadAllText("$root\crates\bsm-core\assets\servermaker_menu.gsc")
$menu = $menu.Replace("`r`n", "`n"); $hook = "    level thread bsm_level_keep();`n"; if (-not $menu.Contains($hook)) { "hook missing"; exit 1 }
$tail = [IO.File]::ReadAllText("$sp\selftest-tail.gsc")
[IO.File]::WriteAllText("$srv\boiii\custom_scripts\zm\bsm_menu_selftest.gsc", $menu.Replace($hook, $hook + "    level thread bsm_selftest();`n") + $tail)
[IO.File]::WriteAllText("$srv\zone\servermaker_custom.cfg", "set rcon_password `"bsmtest`"`nset bsm_menu `"1`"`nset bsm_selftest `"1`"`n")
& $bsm start --wait 240 | Select-Object -Last 2
if ($LASTEXITCODE -ne 0) { throw 'Test server start failed.' }
$deadline = (Get-Date).AddSeconds($MaxSeconds); $stage = ""
while ((Get-Date) -lt $deadline) {
    Start-Sleep 20
    $stage = (& "$sp\rcon.ps1" -Port 27045 -Password bsmtest -Commands @("bsm_t_stage")) -join " "
    if ($stage -match '"(finished|no_bot|bot_never_spawned)') { break }
}
"stage: " + (($stage -replace '\^7.*', '') -replace '.*is: ', '')
& "$sp\rcon.ps1" -Port 27045 -Password bsmtest -Commands @("mapname","fs_game","workshop_id","bsm_t_menus","bsm_t_select","bsm_t_actions","bsm_t_points","bsm_t_perks","bsm_t_give","bsm_t_give_bad","bsm_t_pap","bsm_t_powerups","bsm_t_state") | ForEach-Object { ($_ -split "`n")[0] -replace '\^7', '' -replace ' default: .*', '' -replace '^print ', '  ' }
& $bsm status | Select-Object -Last 1
$p = Get-Process boiii -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p) {
    Remove-Item "$sp\console.txt" -ErrorAction SilentlyContinue
    Start-Process powershell -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$sp\read-console.ps1`" -ProcessId $($p.Id) -Out `"$sp\console.txt`"" -Wait -WindowStyle Hidden
    $c = Get-Content "$sp\console.txt"
    "console: " + @($c | Select-String "Loaded script 'custom_scripts/zm/bsm_menu_selftest.gsc' into the VM").Count + " x script loaded into the VM; script error lines: " + @($c | Select-String -Pattern "Unresolved|script error|runtime error|Compile error|failed to compile|Error compiling").Count
    $c | Select-String -Pattern "Unresolved|script error|runtime error|Compile error|failed to compile|Error compiling" | Select-Object -First 8 | ForEach-Object { "  " + $_.Line }
}
