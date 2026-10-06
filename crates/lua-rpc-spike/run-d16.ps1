# Гейт D16 (ADR-0021): обязательные требования до H2 поверх C-прототипа.
#   (1) F33/F32 — прогресс-таймаут (watchdog) против грубого Job CPU-лимита;
#   (2) F35 — абсолютный дедлайн invocation, не сбрасываемый событиями (chatty-плагин);
#   (3) F34/F22 — range/delta host-API кратно легче полной передачи документа по памяти;
#   (4) F36 — host не падает на искажённом кадре child'а (протокол hostile).
# Код выхода: 0 — все гейты пройдены. См. docs/proto/FINDINGS.md, docs/adr/0021-*.md.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
  cargo build -p lua-rpc-spike --release
  $child = (Resolve-Path "target\release\rpc-child.exe").Path
  $bad   = (Resolve-Path "target\release\bad-child.exe").Path
  $wd    = (Resolve-Path "target\release\watchdog-parent.exe").Path
  $rp    = (Resolve-Path "target\release\range-parent.exe").Path
  $pl    = Join-Path $root "crates\lua-rpc-spike\plugins"
  $doc   = 10485760
  $failed = 0

  function Get-Line($out, $pattern) { $out | Where-Object { $_ -match $pattern } | Select-Object -First 1 }

  # ---- (4) F36: искажённый кадр child'а не роняет host ----
  Write-Host "=== F36: bad-child, неполный кадр события ==="
  $out = & $wd --child $bad --plugin (Join-Path $pl "hello.lua") --watchdog-ms 3000 --deadline-ms 0 --overall-ms 5000
  $code = $LASTEXITCODE
  $out | Where-Object { $_ -match 'PROTOCOL_ERROR|CHILD_GONE|RESULT' }
  # Ожидание: host распознал нарушение, снял child, завершился кодом 1 (не abort 0xC0000409).
  if (-not ($out | Where-Object { $_ -match 'PROTOCOL_ERROR 1' })) { Write-Host "  ! нет PROTOCOL_ERROR"; $failed++ }
  if ($code -ne 1) { Write-Host "  ! неожиданный код выхода $code (ожидался 1, не abort)"; $failed++ }

  # ---- (2) F35: прогресс-таймаут НЕ ловит chatty; абсолютный дедлайн — ловит ----
  Write-Host "=== F35 (пробел): chatty.lua, только прогресс-таймаут 500 мс ==="
  $gapOut = & $wd --child $child --plugin (Join-Path $pl "chatty.lua") --watchdog-ms 500 --deadline-ms 0 --overall-ms 3000
  $gapOut | Where-Object { $_ -match 'PROGRESS_FIRED|DEADLINE_FIRED|OVERALL_TIMEOUT|RESULT' }
  if (-not ($gapOut | Where-Object { $_ -match 'PROGRESS_FIRED 0' })) { Write-Host "  ! прогресс-таймаут неожиданно сработал"; $failed++ }
  if (-not ($gapOut | Where-Object { $_ -match 'OVERALL_TIMEOUT 1' })) { Write-Host "  ! chatty пережил прогресс-таймаут не так, как ожидалось"; $failed++ }

  Write-Host "=== F35 (решение): chatty.lua, абсолютный дедлайн 1500 мс ==="
  $fixOut = & $wd --child $child --plugin (Join-Path $pl "chatty.lua") --watchdog-ms 500 --deadline-ms 1500 --overall-ms 30000
  $fixOut | Where-Object { $_ -match 'LATENCY_MS|DEADLINE_FIRED|CHILD_GONE|RESULT' }
  if (-not ($fixOut | Where-Object { $_ -match 'pass=1' })) { $failed++ }
  if (-not ($fixOut | Where-Object { $_ -match 'DEADLINE_FIRED 1' })) { Write-Host "  ! дедлайн не сработал"; $failed++ }

  # ---- (1) F33/F32: прогресс-таймаут против грубого Job CPU-лимита ----
  Write-Host "=== F33: hang.lua, прогресс-таймаут 300 мс ==="
  $wdOut = & $wd --child $child --plugin (Join-Path $pl "hang.lua") --watchdog-ms 300 --deadline-ms 0 --cpu-ms 0
  $wdOut | Where-Object { $_ -match 'LATENCY_MS|PROGRESS_FIRED|CHILD_GONE|RESULT' }
  if (-not ($wdOut | Where-Object { $_ -match 'pass=1' })) { $failed++ }
  $wdLat = [double]((Get-Line $wdOut '^LATENCY_MS ') -replace 'LATENCY_MS ', '')

  Write-Host "=== CPU-only: hang.lua, Job CPU 1000 мс (грубый backstop) ==="
  $cpuOut = & $wd --child $child --plugin (Join-Path $pl "hang.lua") --watchdog-ms 0 --deadline-ms 0 --cpu-ms 1000 --overall-ms 30000
  $cpuOut | Where-Object { $_ -match 'LATENCY_MS|DISCONNECTED|CHILD_GONE|RESULT' }
  if (-not ($cpuOut | Where-Object { $_ -match 'pass=1' })) { $failed++ }
  $cpuLat = [double]((Get-Line $cpuOut '^LATENCY_MS ') -replace 'LATENCY_MS ', '')
  if (-not ($wdLat -lt $cpuLat)) { Write-Host "ГЕЙТ F32: watchdog не точнее CPU-лимита"; $failed++ }

  # ---- (3) F22/F30: range/delta vs полная передача ----
  $peaks = @{}
  foreach ($s in @(@{ l = 'full'; p = 'large.lua' }, @{ l = 'range'; p = 'range.lua' }, @{ l = 'delta'; p = 'delta.lua' })) {
    Write-Host "=== range: $($s.l) ==="
    $out = & $rp --child $child --plugin (Join-Path $pl $s.p) --label $s.l --doc-size $doc
    $out | Where-Object { $_ -match 'CHILD_SUMMARY|JOB_PEAK_MIB|RESULT' }
    if (-not ($out | Where-Object { $_ -match 'pass=1' })) { $failed++ }
    $line = Get-Line $out "^RESULT label=$($s.l) "
    $peaks[$s.l] = [double](($line -replace '.*peak_mib=', ''))
  }
  if (-not ($peaks['range'] -lt $peaks['full'] / 2)) { Write-Host "ГЕЙТ F22: range не легче full/2"; $failed++ }
  if (-not ($peaks['delta'] -lt $peaks['full'] / 2)) { Write-Host "ГЕЙТ F22: delta не легче full/2"; $failed++ }

  # ---- (5) ADR-0021 §3: карантин N=3 и формулировка границы изоляции ----
  $qp = (Resolve-Path "target\release\quarantine-parent.exe").Path
  Write-Host "=== карантин: crash x4, порог 3 ==="
  $qOut = & $qp --child $child --plugins $pl --sequence "crash.lua,crash.lua,crash.lua,crash.lua" --threshold 3 --permissions "document:read"
  $qOut | Where-Object { $_ -match 'STEP|QUARANTINE|SKIP|RESULT|NOTICE' }
  if (-not ($qOut | Where-Object { $_ -match 'RESULT .*disabled=1' })) { Write-Host "  ! карантин не включился"; $failed++ }
  if (-not ($qOut | Where-Object { $_ -match 'SKIP name=crash.lua reason=quarantined' })) { Write-Host "  ! карантинированный плагин перезапущен"; $failed++ }
  # Формулировки: изоляция отказов всегда + предупреждение о document при соответствующем праве.
  $notices = @($qOut | Where-Object { $_ -match '^NOTICE ' }).Count
  if ($notices -ne 2) { Write-Host "  ! ожидалось 2 NOTICE (изоляция + document), получено $notices"; $failed++ }

  Write-Host "=== карантин: сброс серии успехом (crash,crash,hello,crash) ==="
  $rOut = & $qp --child $child --plugins $pl --sequence "crash.lua,crash.lua,hello.lua,crash.lua" --threshold 3
  $rOut | Where-Object { $_ -match 'STEP|RESULT' }
  if (-not ($rOut | Where-Object { $_ -match 'RESULT .*disabled=0' })) { Write-Host "  ! серия не сброшена успехом"; $failed++ }

  Write-Host ("СВОДКА: full={0:N2} МиБ range={1:N2} МиБ delta={2:N2} МиБ; watchdog={3:N0} мс vs CPU={4:N0} мс" -f `
      $peaks['full'], $peaks['range'], $peaks['delta'], $wdLat, $cpuLat)

  if ($failed -gt 0) { Write-Host "ГЕЙТ D16 НЕ ПРОЙДЕН ($failed)"; exit 1 }
  Write-Host "ГЕЙТ D16 ПРОЙДЕН"
  exit 0
}
finally { Pop-Location }
