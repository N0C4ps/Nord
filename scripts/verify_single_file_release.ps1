[CmdletBinding()]
param(
    [ValidateRange(1, 60)]
    [int]$StartupTimeoutSeconds = 8
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exePath = Join-Path $projectRoot 'target\release\nord.exe'
$checkDirectory = Join-Path $projectRoot 'target\single-file-release-check'

Push-Location $projectRoot
try {
    cargo build --release --locked -p nord
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
    throw "Release executable was not produced: $exePath"
}

$resolvedCheckDirectory = [System.IO.Path]::GetFullPath($checkDirectory)
$resolvedTargetDirectory = [System.IO.Path]::GetFullPath((Join-Path $projectRoot 'target'))
if (-not $resolvedCheckDirectory.StartsWith(
        $resolvedTargetDirectory + [System.IO.Path]::DirectorySeparatorChar,
        [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to recreate a check directory outside target: $resolvedCheckDirectory"
}

if (Test-Path -LiteralPath $resolvedCheckDirectory) {
    Remove-Item -LiteralPath $resolvedCheckDirectory -Recurse -Force
}
New-Item -ItemType Directory -Path $resolvedCheckDirectory | Out-Null
$isolatedExe = Join-Path $resolvedCheckDirectory 'Nord.exe'
Copy-Item -LiteralPath $exePath -Destination $isolatedExe

$files = @(Get-ChildItem -LiteralPath $resolvedCheckDirectory -Force)
if ($files.Count -ne 1 -or $files[0].Name -ne 'Nord.exe') {
    throw 'The isolated deployment folder must contain only Nord.exe.'
}

$oldDxcPath = [Environment]::GetEnvironmentVariable('NORD_DXC_PATH', 'Process')
$oldConfigFile = [Environment]::GetEnvironmentVariable('NORD_CONFIG_FILE', 'Process')
$oldLocalAppData = [Environment]::GetEnvironmentVariable('LOCALAPPDATA', 'Process')
$oldRustLog = [Environment]::GetEnvironmentVariable('RUST_LOG', 'Process')
$process = $null
try {
    $env:NORD_DXC_PATH = 'Z:\missing\dxc.exe'
    Remove-Item Env:NORD_CONFIG_FILE -ErrorAction SilentlyContinue
    $env:LOCALAPPDATA = Join-Path $resolvedCheckDirectory 'appdata'
    $env:RUST_LOG = 'info'
    $process = Start-Process -FilePath $isolatedExe `
        -WorkingDirectory $resolvedCheckDirectory `
        -WindowStyle Minimized `
        -PassThru

    Start-Sleep -Seconds $StartupTimeoutSeconds
    $process.Refresh()
    if ($process.HasExited) {
        $logDirectory = Join-Path $env:LOCALAPPDATA 'nord\logs'
        $recentLog = Get-ChildItem -LiteralPath $logDirectory -Filter 'nord.*.jsonl' `
            -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        $logDetail = if ($null -ne $recentLog) {
            (Get-Content -LiteralPath $recentLog.FullName -Tail 8) -join [Environment]::NewLine
        } else {
            'No Nord log file was produced.'
        }
        throw "Nord.exe exited during startup with code $($process.ExitCode). Recent application log:`n$logDetail"
    }

    Write-Host "PASS: Nord.exe stayed running for $StartupTimeoutSeconds seconds from a folder containing only the executable."
    $logDirectory = Join-Path $env:LOCALAPPDATA 'nord\logs'
    $latestLog = Get-ChildItem -LiteralPath $logDirectory -Filter 'nord.*.jsonl' `
        -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    $metrics = if ($null -ne $latestLog) {
        Get-Content -LiteralPath $latestLog.FullName |
            Where-Object { $_ -match 'CPU frustum culling and mesh/material batching measured' } |
            Select-Object -Last 1
    } else {
        $null
    }
    if (-not $metrics) {
        throw "The game stayed open, but did not write the CPU visibility/instancing measurement to $logDirectory."
    }
    Write-Host "CPU visibility/instancing measurement: $metrics"
} finally {
    if ($null -ne $process) {
        $process.Refresh()
        if (-not $process.HasExited) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(10000)) {
                Stop-Process -Id $process.Id -Force
                throw 'Nord.exe did not close after the smoke check; the process was stopped.'
            }
        }
    }

    if ($null -eq $oldDxcPath) {
        Remove-Item Env:NORD_DXC_PATH -ErrorAction SilentlyContinue
    } else {
        $env:NORD_DXC_PATH = $oldDxcPath
    }
    if ($null -eq $oldConfigFile) {
        Remove-Item Env:NORD_CONFIG_FILE -ErrorAction SilentlyContinue
    } else {
        $env:NORD_CONFIG_FILE = $oldConfigFile
    }
    if ($null -eq $oldLocalAppData) {
        Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue
    } else {
        $env:LOCALAPPDATA = $oldLocalAppData
    }
    if ($null -eq $oldRustLog) {
        Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue
    } else {
        $env:RUST_LOG = $oldRustLog
    }
}
