param([string]$List, [int]$Wait = 0)
# Runs one list of menu actions on the bot of the running test server and prints the state trace.
$sp = $PSScriptRoot
$n = ($List -split ",").Count; if ($Wait -eq 0) { $Wait = [int]($n * 0.7) + 4 }
& "$sp\rcon.ps1" -Port 27045 -Password bsmtest -Commands @("set bsm_t_stage running", "set bsm_t_list `"$List`"", "set bsm_t_go 1") | Out-Null
Start-Sleep $Wait
(& "$sp\rcon.ps1" -Port 27045 -Password bsmtest -Commands @("bsm_t_stage", "bsm_t_trace")) | ForEach-Object { ($_ -split "`n")[0] -replace '\^7', '' -replace ' default: .*', '' -replace '^print ', '  ' -replace '"0x[0-9A-F]+" is: ', '' } | Where-Object { $_ -notmatch '^>' }
