# Гейт M11 (P9): лимиты child'а через Job Object (RSS/CPU/время), без ручного kill.
# Сценарии:
#   spin  — CPU-лимит: Job завершает процесс, parent жив (латентность лимита времени — см. FINDINGS);
#   hog   — memory-лимит: аллокации падают, RSS ограничен сверху, parent жив;
#   hello — контроль: нормальный плагин под лимитами завершается сам (status=0).
# Код выхода: 0 — все сценарии прошли.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
  cargo build -p lua-rpc-spike --release
  $jp    = (Resolve-Path "target\release\job-parent.exe").Path
  $child = (Resolve-Path "target\release\rpc-child.exe").Path
  $pl    = Join-Path $root "crates\lua-rpc-spike\plugins"

  $cases = @(
    @{ name = "spin";  plugin = "spin.lua";  cpu = 1000; mem = 0;  timeout = 20000 },
    @{ name = "hog";   plugin = "hog.lua";   cpu = 0;    mem = 64; timeout = 10000 },
    @{ name = "hello"; plugin = "hello.lua"; cpu = 500;  mem = 64; timeout = 10000 }
  )

  $failed = 0
  foreach ($c in $cases) {
    $plugin = (Resolve-Path (Join-Path $pl $c.plugin)).Path
    Write-Host "=== $($c.name) (cpu=$($c.cpu) mem=$($c.mem)MB) ==="
    $out = & $jp --child $child --plugin $plugin --cpu-ms $c.cpu --mem-mb $c.mem --timeout-ms $c.timeout
    $out | Where-Object { $_ -match 'JOB_LIMITS|NESTED|JOB_CPU_MS|JOB_PEAK_MEM|WALL_MS|EXIT|STATUS|RESULT' }
    if (-not ($out | Where-Object { $_ -match 'pass=1' })) { $failed++ }
  }
  if ($failed -gt 0) { Write-Host "ГЕЙТ M11 НЕ ПРОЙДЕН ($failed)"; exit 1 }
  Write-Host "ГЕЙТ M11 ПРОЙДЕН"
  exit 0
}
finally { Pop-Location }
