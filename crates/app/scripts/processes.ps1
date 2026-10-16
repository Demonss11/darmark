<#
.SYNOPSIS
    Отчёт по процессам, мешающим сборке и E2E-прогону darmark.

.DESCRIPTION
    Показывает, что именно держит бинарники и node-процессы вебдрайвера:
    PID, родителя, реальный путь к exe и возраст. Скрипт только читает —
    ничего не снимает.

    Вызывай перед `npm run tauri build` и перед `npm run test:e2e`: если сборка
    падает на занятом файле, этот отчёт покажет, кто именно держит файл.

.PARAMETER Quiet
    Только сводка «есть ли что снимать» и выход с кодом 1, если процессы есть.
    Пригодно для проверки в других скриптах.

.PARAMETER CmdLine
    Добавить колонку с командной строкой. Видна только под администратором,
    иначе печатается заглушка. Замедляет отчёт (полный WMI-опрос).

.EXAMPLE
    .\processes.ps1
    Полный отчёт: процессы darmark, сайдкары, драйверы, node от wdio.

.EXAMPLE
    .\processes.ps1 -CmdLine
    То же плюс командная строка — чтобы понять, откуда запущен процесс.

.EXAMPLE
    .\processes.ps1 -Quiet
    Сводка и код возврата 1, если есть что снимать.
#>

[CmdletBinding()]
param(
    [switch]$Quiet,
    [switch]$CmdLine
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Имена exe, которые занимают артефакты сборки или держат браузер E2E.
$Targets = @('darmark', 'darmark-plugin-host', 'tauri-driver', 'msedgedriver')

# Командная строка с защитой от выхода за пределы строки.
function Get-Truncated {
    param([string]$Text, [int]$Limit = 140)

    if ([string]::IsNullOrEmpty($Text)) { return '<пусто>' }
    $clean = $Text -replace '\s+', ' '
    if ($clean.Length -le $Limit) { return $clean }
    return $clean.Substring(0, $Limit) + '…'
}

function Get-Age {
    param($Process)

    try {
        $minutes = [math]::Round(((Get-Date) - $Process.CreationDate).TotalMinutes, 1)
        return "$minutes мин"
    } catch {
        return '?'
    }
}

function Get-ParentName {
    param([uint32]$ParentId)

    if ($ParentId -eq 0) { return '<система>' }
    try {
        return (Get-Process -Id $ParentId -ErrorAction Stop).ProcessName
    } catch {
        return '<умер>'
    }
}

# Список процессов darmark/сайдкаров/драйверов. WMI нужен ради CommandLine,
# поэтому при -Quiet идём по Get-Process — он в десятки раз быстрее.
function Get-TargetProcess {
    if ($CmdLine) {
        return @(Get-CimInstance Win32_Process | Where-Object {
            $name = $_.Name
            $Targets | Where-Object { $name -like "$_*" }
        })
    }
    return @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -in $Targets
    })
}

# Node-процессы E2E: taskkill по списку целей их не покрывает, а именно они
# чаще всего остаются после отмены `npm run test:e2e`.
function Get-WdioNode {
    $found = @(
        Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -match 'wdio|webdriverio|@wdio|test:e2e' }
    )
    return @($found)
}

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Общая сводка: PID, имя, родитель, возраст, путь.
function Get-Report {
    param($Processes, [string]$Kind)

    $rows = foreach ($p in $Processes) {
        $procId = if ($CmdLine -and $p.PSObject.Properties['ProcessId']) { $p.ProcessId } else { $p.Id }
        $name = if ($CmdLine) { [IO.Path]::GetFileNameWithoutExtension($p.Name) } else { $p.ProcessName }

        $path = try { $p.ExecutablePath } catch { $null }
        if (-not $path) {
            $path = try { $p.Path } catch { $null }
        }

        $parentId = if ($CmdLine) { $p.ParentProcessId } else { 0 }

        [pscustomobject]@{
            # Имена свойств латиницей: скрипт читается как ANSI при отсутствии BOM,
            # кириллица в идентификаторах сделала бы его непереносимым.
            PID     = $procId
            Name    = $name
            Parent  = if ($CmdLine) { "$(Get-ParentName $parentId) ($parentId)" } else { '-' }
            Age     = Get-Age $p
            Path    = if ($path) { $path } else { '<нет доступа: нужен админ>' }
        }
    }

    if (-not $rows) {
        Write-Host "  [$Kind] нет" -ForegroundColor DarkGray
        return @()
    }

    $rows | Format-Table -AutoSize | Out-String | Write-Host

    if ($CmdLine) {
        foreach ($p in $Processes) {
            $cmd = try { $p.CommandLine } catch { $null }
            Write-Host ("    PID {0}: {1}" -f $p.ProcessId, (Get-Truncated $cmd)) -ForegroundColor DarkGray
        }
    }

    return @($rows)
}

# Режим -Quiet: ничего не печатаем кроме вердикта, код 1 — есть что снимать.
if ($Quiet) {
    $targets = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -in $Targets })
    $nodes = @(Get-WdioNode)
    $total = $targets.Count + $nodes.Count

    if ($total -eq 0) {
        Write-Host 'ЧИСТО: снимать нечего'
        exit 0
    }
    Write-Host "СНЯТЬ: $total (darmark/сайдкары/драйверы: $($targets.Count), node-wdio: $($nodes.Count))"
    foreach ($p in $targets) { Write-Host ("  PID {0}  {1}" -f $p.Id, $p.ProcessName) }
    foreach ($p in $nodes) { Write-Host ("  PID {0}  node.exe (wdio)" -f $p.ProcessId) }
    exit 1
}

# Полный отчёт.
Write-Host ''
Write-Host '=== Процессы darmark: сборочная сессия и E2E ===' -ForegroundColor Cyan
if (-not (Test-Admin)) {
    Write-Host '  ВНИМАНИЕ: не администратор — команда строка и путь к exe могут быть недоступны.' -ForegroundColor Yellow
}

$found = @()
$found += Get-Report (Get-TargetProcess) 'Приложение и драйверы'
$found += Get-Report (Get-WdioNode) 'Node / WebdriverIO'

Write-Host ''
if ($found.Count -eq 0) {
    Write-Host 'ИТОГ: процессов нет — можно собирать.' -ForegroundColor Green
    Write-Host ''
    exit 0
}

Write-Host "ИТОГ: найдено $($found.Count). Снять:" -ForegroundColor Yellow
foreach ($p in $found) { Write-Host ("  PID {0}  {1}" -f $p.PID, $p.Name) }
Write-Host ''
Write-Host 'Пример команды снятия (проверь глазами PID выше):' -ForegroundColor DarkGray
Write-Host '  taskkill /PID <pid> /F /T' -ForegroundColor DarkGray
Write-Host ''
exit 0