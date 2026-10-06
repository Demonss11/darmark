# Гейт M10 (P9): stdio-round-trip в GUI-конфигурации без консоли.
# Собирает release-бинарники и прогоняет три сценария через gui-parent (subsystem 2, как mdedit.exe):
#   roundtrip   — кадр 10 МБ parent<->child;
#   break       — child убит, parent пишет в мёртвый пайп → BrokenPipe, не виснет;
#   break-write — child жив, но не читает (spin.lua); parent блокируется в записи 10 МБ,
#                 watchdog убивает child → запись разблокируется, не виснет.
# Код выхода: 0 — гейт пройден по всем сценариям.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
  cargo build -p lua-rpc-spike --release
  $exe   = (Resolve-Path "target\release\gui-parent.exe").Path
  $child = (Resolve-Path "target\release\rpc-child.exe").Path
  $plugins = Join-Path $root "crates\lua-rpc-spike\plugins"
  $out = Join-Path $env:TEMP "m10-gui-stdio"
  New-Item -ItemType Directory -Force -Path $out | Out-Null

  $cases = @(
    @{ name = "roundtrip";   mode = "roundtrip";   plugin = "large.lua" },
    @{ name = "break";       mode = "break";       plugin = "hello.lua" },
    @{ name = "break-write"; mode = "break-write"; plugin = "spin.lua"  }
  )

  $failed = 0
  foreach ($c in $cases) {
    $plugin = (Resolve-Path (Join-Path $plugins $c.plugin)).Path
    $rep = Join-Path $out "$($c.name).txt"
    Remove-Item "$rep*" -Force -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath $exe -Wait -PassThru -ArgumentList @(
      "--child", $child, "--plugin", $plugin,
      "--doc-size", "10485760", "--report", $rep,
      "--mode", $c.mode, "--timeout-ms", "3000"
    )
    Write-Host "=== $($c.name): exit=$($p.ExitCode) ==="
    if (Test-Path $rep) { Get-Content $rep } else { Write-Host "(no report)"; $failed++ }
    if ($p.ExitCode -ne 0) { $failed++ }
  }
  if ($failed -gt 0) { Write-Host "ГЕЙТ M10 НЕ ПРОЙДЕН ($failed)"; exit 1 }
  Write-Host "ГЕЙТ M10 ПРОЙДЕН"
  exit 0
}
finally { Pop-Location }
