# Гейт M12 (P9): модель процессов — общий child vs child-на-плагин.
# Метрики: commit пустого child / с 10 МБ документом / N×child; время активации; изоляция
# отказа (crash и spin одного плагина). Код выхода: 0 — все сценарии подтвердили ожидание.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
  cargo build -p lua-rpc-spike --release
  $mp    = (Resolve-Path "target\release\model-parent.exe").Path
  $child = (Resolve-Path "target\release\rpc-child.exe").Path
  $pl    = Join-Path $root "crates\lua-rpc-spike\plugins"

  $cases = @(
    @{ m = "shared";     s = "empty";       n = 5; timeout = 15000 },
    @{ m = "shared";     s = "doc";         n = 1; timeout = 15000 },
    @{ m = "shared";     s = "stress";      n = 5; timeout = 15000 },
    @{ m = "per-plugin"; s = "stress";      n = 5; timeout = 15000 },
    @{ m = "shared";     s = "fault";       n = 3; timeout = 5000  },
    @{ m = "per-plugin"; s = "fault";       n = 3; timeout = 5000  },
    @{ m = "shared";     s = "fault-spin";  n = 3; timeout = 2500  },
    @{ m = "per-plugin"; s = "fault-spin";  n = 3; timeout = 2500  }
  )

  $failed = 0
  foreach ($c in $cases) {
    Write-Host "=== $($c.m) / $($c.s) (n=$($c.n)) ==="
    $out = & $mp --child $child --plugins $pl --model $c.m --scenario $c.s --n $c.n --timeout-ms $c.timeout
    $out | Where-Object { $_ -match 'JOB_PEAK_MIB|WALL_MS|DONE_MARKERS|SURVIVORS|DISCONNECTED|TIMEOUTS|RESULT' }
    if (-not ($out | Where-Object { $_ -match 'pass=1' })) { $failed++ }
  }
  if ($failed -gt 0) { Write-Host "ГЕЙТ M12 НЕ ПРОЙДЕН ($failed)"; exit 1 }
  Write-Host "ГЕЙТ M12 ПРОЙДЕН"
  exit 0
}
finally { Pop-Location }
