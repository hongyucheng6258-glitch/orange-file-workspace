[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = 'High')]
param(
    [ValidateSet('Safe', 'Deep')]
    [string]$Mode = 'Safe',
    [switch]$ScanOnly,
    [string[]]$IncludeIds,
    [switch]$Force,
    [switch]$ConfirmRecycleBin,
    [switch]$OutputJson
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$script:MaxScannedFiles = 250000

function Write-Ui([string]$Message) {
    if (-not $OutputJson) { Write-Host $Message }
}

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Normalize-Path([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { throw 'Cleanup path is empty.' }
    return [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($Path)).TrimEnd('\')
}

function Assert-SafeCleanupPath([string]$Path, [string[]]$AllowedRoots) {
    $full = Normalize-Path $Path
    $driveRoot = [IO.Path]::GetPathRoot($full).TrimEnd('\')
    $blocked = @(
        $driveRoot,
        (Normalize-Path $env:SystemRoot),
        (Normalize-Path $env:USERPROFILE),
        (Normalize-Path $env:LOCALAPPDATA),
        (Normalize-Path $env:APPDATA),
        (Normalize-Path $env:ProgramData)
    ) | Select-Object -Unique
    if ($blocked -contains $full) { throw "Refusing dangerous cleanup path: $full" }

    $approved = $false
    foreach ($root in $AllowedRoots) {
        if ([string]::IsNullOrWhiteSpace($root)) { continue }
        $normalizedRoot = (Normalize-Path $root).TrimEnd('\')
        if ($full.StartsWith($normalizedRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
            $approved = $true
            break
        }
    }
    if (-not $approved) { throw "Path is outside approved cleanup roots: $full" }
    return $full
}

function New-CleanupItem {
    param(
        [string]$Id,
        [string]$Name,
        [string]$Risk,
        [bool]$DefaultSelected,
        [bool]$RequiresAdmin,
        [string]$Kind,
        [string[]]$Paths,
        [string[]]$AllowedRoots,
        [string[]]$Patterns = @('*')
    )
    [pscustomobject]@{
        id = $Id
        name = $Name
        risk = $Risk
        default_selected = $DefaultSelected
        requires_admin = $RequiresAdmin
        kind = $Kind
        paths = @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        allowed_roots = @($AllowedRoots | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        patterns = @($Patterns)
    }
}

function Get-ChromiumCachePaths([string]$UserDataRoot) {
    $paths = New-Object System.Collections.Generic.List[string]
    if (-not (Test-Path -LiteralPath $UserDataRoot -PathType Container)) { return @() }
    $profiles = Get-ChildItem -LiteralPath $UserDataRoot -Directory -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq 'Default' -or $_.Name -eq 'Guest Profile' -or $_.Name -eq 'System Profile' -or $_.Name -like 'Profile *' }
    foreach ($profile in $profiles) {
        foreach ($relative in @('Cache', 'Code Cache', 'GPUCache', 'Service Worker\CacheStorage')) {
            $candidate = Join-Path $profile.FullName $relative
            if (Test-Path -LiteralPath $candidate -PathType Container) { $paths.Add($candidate) }
        }
    }
    return $paths.ToArray()
}

function Get-FirefoxCachePaths {
    $root = Join-Path $env:APPDATA 'Mozilla\Firefox\Profiles'
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $root -Directory -Force -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName 'cache2' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Container })
}

function Get-CleanupCatalog([string]$SelectedMode) {
    $windowsRoot = Normalize-Path $env:SystemRoot
    $userTemp = Normalize-Path $env:TEMP
    $localRoot = Normalize-Path $env:LOCALAPPDATA
    $roamingRoot = Normalize-Path $env:APPDATA
    $programDataRoot = Normalize-Path $env:ProgramData
    $explorerRoot = Join-Path $localRoot 'Microsoft\Windows\Explorer'
    $chromeRoot = Join-Path $localRoot 'Google\Chrome\User Data'
    $edgeRoot = Join-Path $localRoot 'Microsoft\Edge\User Data'

    $items = New-Object System.Collections.Generic.List[object]
    $items.Add((New-CleanupItem 'user_temp' 'User temporary files' 'low' $true $false 'directory' @($userTemp) @($userTemp)))
    $items.Add((New-CleanupItem 'windows_temp' 'Windows temporary files' 'low' $true $false 'directory' @((Join-Path $windowsRoot 'Temp')) @($windowsRoot)))
    $items.Add((New-CleanupItem 'icon_cache' 'Explorer icon cache' 'low' $true $false 'pattern' @($localRoot, $explorerRoot) @($localRoot) @('IconCache.db', 'iconcache_*.db')))
    $items.Add((New-CleanupItem 'thumbnail_cache' 'Explorer thumbnail cache' 'low' $true $false 'pattern' @($explorerRoot) @($localRoot) @('thumbcache_*.db')))
    $items.Add((New-CleanupItem 'edge_cache' 'Microsoft Edge ordinary cache' 'low' $true $false 'directory' @(Get-ChromiumCachePaths $edgeRoot) @($edgeRoot)))
    $items.Add((New-CleanupItem 'chrome_cache' 'Google Chrome ordinary cache' 'low' $true $false 'directory' @(Get-ChromiumCachePaths $chromeRoot) @($chromeRoot)))
    $items.Add((New-CleanupItem 'firefox_cache' 'Mozilla Firefox ordinary cache' 'low' $true $false 'directory' @(Get-FirefoxCachePaths) @((Join-Path $roamingRoot 'Mozilla\Firefox\Profiles'))))
    $items.Add((New-CleanupItem 'recycle_bin' 'C drive Recycle Bin' 'medium' $true $false 'recycle_bin' @('C:\$Recycle.Bin') @('C:\')))

    if ($SelectedMode -eq 'Deep') {
        $items.Add((New-CleanupItem 'wer_reports' 'Windows Error Reporting files' 'medium' $false $true 'directory' @(
            (Join-Path $programDataRoot 'Microsoft\Windows\WER\ReportArchive'),
            (Join-Path $programDataRoot 'Microsoft\Windows\WER\ReportQueue'),
            (Join-Path $localRoot 'Microsoft\Windows\WER')
        ) @($programDataRoot, $localRoot)))
        $items.Add((New-CleanupItem 'crash_dumps' 'Crash dump files' 'medium' $false $true 'directory' @(
            (Join-Path $localRoot 'CrashDumps'),
            (Join-Path $windowsRoot 'Minidump')
        ) @($localRoot, $windowsRoot)))
        $items.Add((New-CleanupItem 'windows_update_cache' 'Windows Update download cache' 'high' $false $true 'directory' @(
            (Join-Path $windowsRoot 'SoftwareDistribution\Download')
        ) @($windowsRoot)))
        $items.Add((New-CleanupItem 'delivery_optimization' 'Delivery Optimization cache' 'high' $false $true 'directory' @(
            (Join-Path $programDataRoot 'Microsoft\Windows\DeliveryOptimization\Cache')
        ) @($programDataRoot)))
    }
    return $items.ToArray()
}

function Test-Pattern([string]$Name, [string[]]$Patterns) {
    foreach ($pattern in $Patterns) {
        if ($Name -like $pattern) { return $true }
    }
    return $false
}

function Get-SafeFilesForItem($Item) {
    $files = New-Object System.Collections.Generic.List[object]
    if ($Item.kind -eq 'recycle_bin') { return @() }

    foreach ($rawPath in $Item.paths) {
        try { $path = Assert-SafeCleanupPath $rawPath $Item.allowed_roots } catch { continue }
        if (-not (Test-Path -LiteralPath $path)) { continue }

        if ((Test-Path -LiteralPath $path -PathType Leaf)) {
            $leaf = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
            if ($null -ne $leaf -and (Test-Pattern $leaf.Name $Item.patterns)) { $files.Add($leaf) }
            continue
        }

        $rootItem = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
        if ($null -eq $rootItem -or (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { continue }
        $queue = New-Object System.Collections.Generic.Queue[string]
        $queue.Enqueue($path)
        while ($queue.Count -gt 0 -and $files.Count -lt $script:MaxScannedFiles) {
            $current = $queue.Dequeue()
            foreach ($entry in @(Get-ChildItem -LiteralPath $current -Force -ErrorAction SilentlyContinue)) {
                if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
                if ($entry.PSIsContainer) {
                    if ($Item.kind -eq 'directory') { $queue.Enqueue($entry.FullName) }
                } elseif (Test-Pattern $entry.Name $Item.patterns) {
                    $files.Add($entry)
                }
                if ($files.Count -ge $script:MaxScannedFiles) { break }
            }
        }
    }
    return $files.ToArray()
}

function Ensure-RecycleBinApi {
    if ('RecycleBin.NativeMethods' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace RecycleBin {
    public static class NativeMethods {
        [StructLayout(LayoutKind.Sequential, Pack = 1)]
        public struct SHQUERYRBINFO {
            public int cbSize;
            public long i64Size;
            public long i64NumItems;
        }
        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        public static extern int SHQueryRecycleBin(string rootPath, ref SHQUERYRBINFO info);
        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        public static extern int SHEmptyRecycleBin(IntPtr hwnd, string rootPath, uint flags);
    }
}
'@
}

function Get-RecycleBinEstimate {
    Ensure-RecycleBinApi
    $info = New-Object RecycleBin.NativeMethods+SHQUERYRBINFO
    $info.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($info)
    $hr = [RecycleBin.NativeMethods]::SHQueryRecycleBin('C:\', [ref]$info)
    if ($hr -ne 0) { return [pscustomobject]@{ count = 0L; bytes = 0L; error = "HRESULT $hr" } }
    return [pscustomobject]@{ count = $info.i64NumItems; bytes = $info.i64Size; error = $null }
}

function Get-ItemEstimate($Item) {
    if ($Item.kind -eq 'recycle_bin') { return Get-RecycleBinEstimate }
    $files = @(Get-SafeFilesForItem $Item)
    $bytes = 0L
    foreach ($file in $files) { $bytes += [long]$file.Length }
    return [pscustomobject]@{ count = [long]$files.Count; bytes = $bytes; error = $null }
}

function Format-Bytes([long]$Bytes) {
    if ($Bytes -ge 1GB) { return ('{0:N2} GB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N2} MB' -f ($Bytes / 1MB)) }
    if ($Bytes -ge 1KB) { return ('{0:N2} KB' -f ($Bytes / 1KB)) }
    return "$Bytes B"
}

function Select-CleanupItems($Catalog, $Estimates) {
    if ($null -ne $IncludeIds -and $IncludeIds.Count -gt 0) {
        $known = @($Catalog | ForEach-Object { $_.id })
        foreach ($id in $IncludeIds) {
            if ($known -notcontains $id) { throw "Unknown cleanup item: $id" }
        }
        return @($Catalog | Where-Object { $IncludeIds -contains $_.id })
    }
    if ($ScanOnly -or $Force -or -not [Environment]::UserInteractive) {
        return @($Catalog | Where-Object { $_.default_selected })
    }

    Write-Ui ''
    Write-Ui "Mode: $Mode. Enter comma-separated item numbers, or press Enter for defaults."
    for ($i = 0; $i -lt $Catalog.Count; $i++) {
        $item = $Catalog[$i]
        $estimate = $Estimates[$item.id]
        $mark = if ($item.default_selected) { '*' } else { ' ' }
        $admin = if ($item.requires_admin) { ', admin' } else { '' }
        Write-Ui ("[{0}] {1}. {2} [{3}{4}] - {5}" -f $mark, ($i + 1), $item.name, $item.risk, $admin, (Format-Bytes $estimate.bytes))
    }
    $answer = Read-Host 'Selection'
    if ([string]::IsNullOrWhiteSpace($answer)) { return @($Catalog | Where-Object { $_.default_selected }) }
    $indexes = @($answer -split ',' | ForEach-Object {
        $value = 0
        if (-not [int]::TryParse($_.Trim(), [ref]$value) -or $value -lt 1 -or $value -gt $Catalog.Count) {
            throw "Invalid selection: $_"
        }
        $value - 1
    } | Select-Object -Unique)
    return @($indexes | ForEach-Object { $Catalog[$_] })
}

function Remove-EmptyDirectories([string]$Root, [string[]]$AllowedRoots) {
    try { $safeRoot = Assert-SafeCleanupPath $Root $AllowedRoots } catch { return }
    if (-not (Test-Path -LiteralPath $safeRoot -PathType Container)) { return }
    Get-ChildItem -LiteralPath $safeRoot -Directory -Force -Recurse -ErrorAction SilentlyContinue |
        Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0 } |
        Sort-Object { $_.FullName.Length } -Descending |
        ForEach-Object {
            if (@(Get-ChildItem -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue).Count -eq 0) {
                Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue
            }
        }
}

function Invoke-DirectoryCleanup($Item) {
    $files = @(Get-SafeFilesForItem $Item)
    $deleted = 0L
    $freed = 0L
    $skipped = 0L
    $errors = New-Object System.Collections.Generic.List[string]
    foreach ($file in $files) {
        try {
            $length = [long]$file.Length
            if ($PSCmdlet.ShouldProcess($file.FullName, 'Delete cache or temporary file')) {
                Remove-Item -LiteralPath $file.FullName -Force -ErrorAction Stop
                $deleted++
                $freed += $length
            }
        } catch {
            $skipped++
            if ($errors.Count -lt 10) { $errors.Add($_.Exception.Message) }
        }
    }
    if (-not $WhatIfPreference) {
        foreach ($path in $Item.paths) { Remove-EmptyDirectories $path $Item.allowed_roots }
    }
    return [pscustomobject]@{ deleted_files = $deleted; freed_bytes = $freed; skipped_files = $skipped; errors = $errors.ToArray() }
}

function Invoke-RecycleBinCleanup {
    if (-not $ConfirmRecycleBin) {
        if ($Force -or -not [Environment]::UserInteractive) { throw 'Recycle Bin cleanup requires -ConfirmRecycleBin.' }
        Write-Ui 'WARNING: Emptying the C drive Recycle Bin is permanent and cannot be undone.'
        $answer = Read-Host 'Type EMPTY to continue'
        if ($answer -cne 'EMPTY') { throw 'Recycle Bin cleanup was not confirmed.' }
    }
    $before = Get-RecycleBinEstimate
    if ($PSCmdlet.ShouldProcess('C:\$Recycle.Bin', 'Permanently empty C drive Recycle Bin')) {
        Ensure-RecycleBinApi
        $flags = 0x00000001 -bor 0x00000002 -bor 0x00000004
        $hr = [RecycleBin.NativeMethods]::SHEmptyRecycleBin([IntPtr]::Zero, 'C:\', $flags)
        if ($hr -ne 0) { throw "SHEmptyRecycleBin failed with HRESULT $hr" }
    }
    return [pscustomobject]@{ deleted_files = [long]$before.count; freed_bytes = [long]$before.bytes; skipped_files = 0L; errors = @() }
}

$startedAt = [DateTime]::UtcNow
$isAdmin = Test-IsAdministrator
$catalog = @(Get-CleanupCatalog $Mode)
$estimates = @{}
foreach ($item in $catalog) {
    $estimates[$item.id] = Get-ItemEstimate $item
}
$selected = @(Select-CleanupItems $catalog $estimates)

if (-not $ScanOnly -and -not $Force -and [Environment]::UserInteractive) {
    $total = 0L
    foreach ($item in $selected) { $total += [long]$estimates[$item.id].bytes }
    Write-Ui ''
    Write-Ui ("Selected {0} items, estimated reclaimable space: {1}" -f $selected.Count, (Format-Bytes $total))
    $answer = Read-Host 'Type CLEAN to start'
    if ($answer -cne 'CLEAN') { Write-Ui 'Cancelled.'; exit 0 }
}

$results = New-Object System.Collections.Generic.List[object]
foreach ($item in $catalog) {
    $estimate = $estimates[$item.id]
    $wasSelected = $selected.id -contains $item.id
    $status = if ($wasSelected) { 'scanned' } else { 'not_selected' }
    $cleaned = [pscustomobject]@{ deleted_files = 0L; freed_bytes = 0L; skipped_files = 0L; errors = @() }

    if ($wasSelected -and -not $ScanOnly) {
        if ($item.requires_admin -and -not $isAdmin) {
            $status = 'requires_admin'
        } else {
            try {
                if ($item.kind -eq 'recycle_bin') { $cleaned = Invoke-RecycleBinCleanup }
                else { $cleaned = Invoke-DirectoryCleanup $item }
                $status = 'cleaned'
            } catch {
                $status = 'failed'
                $cleaned = [pscustomobject]@{ deleted_files = 0L; freed_bytes = 0L; skipped_files = 1L; errors = @($_.Exception.Message) }
            }
        }
    }

    $results.Add([pscustomobject]@{
        id = $item.id
        name = $item.name
        risk = $item.risk
        selected = $wasSelected
        requires_admin = $item.requires_admin
        status = $status
        candidate_files = [long]$estimate.count
        candidate_bytes = [long]$estimate.bytes
        deleted_files = [long]$cleaned.deleted_files
        freed_bytes = [long]$cleaned.freed_bytes
        skipped_files = [long]$cleaned.skipped_files
        errors = @($cleaned.errors)
    })
}

$summary = [pscustomobject]@{
    mode = $Mode
    scan_only = [bool]$ScanOnly
    is_admin = $isAdmin
    started_at_utc = $startedAt.ToString('o')
    finished_at_utc = [DateTime]::UtcNow.ToString('o')
    candidate_files = [long](($results | Measure-Object candidate_files -Sum).Sum)
    candidate_bytes = [long](($results | Measure-Object candidate_bytes -Sum).Sum)
    deleted_files = [long](($results | Measure-Object deleted_files -Sum).Sum)
    freed_bytes = [long](($results | Measure-Object freed_bytes -Sum).Sum)
    skipped_files = [long](($results | Measure-Object skipped_files -Sum).Sum)
    items = $results.ToArray()
}

if ($OutputJson) {
    $summary | ConvertTo-Json -Depth 6
} else {
    Write-Ui ''
    Write-Ui ("Candidates: {0} files, {1}" -f $summary.candidate_files, (Format-Bytes $summary.candidate_bytes))
    if (-not $ScanOnly) {
        Write-Ui ("Deleted: {0} files, freed {1}, skipped {2}" -f $summary.deleted_files, (Format-Bytes $summary.freed_bytes), $summary.skipped_files)
    }
    $results | Select-Object name, risk, status, candidate_files, @{n='candidate_size';e={Format-Bytes $_.candidate_bytes}}, deleted_files, skipped_files | Format-Table -AutoSize
}

if (-not $ScanOnly -and (($results | Where-Object { $_.status -eq 'failed' }).Count -gt 0 -or $summary.skipped_files -gt 0)) { exit 2 }
exit 0
