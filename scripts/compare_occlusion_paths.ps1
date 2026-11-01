[CmdletBinding()]
param(
    [switch]$NoBuild,
    [ValidateRange(8, 60)]
    [int]$RunSeconds = 12,
    [ValidateRange(10, 180)]
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $repoRoot 'target\release\nord.exe'
$localAppData = [Environment]::GetEnvironmentVariable('LOCALAPPDATA', 'Process')
if ([string]::IsNullOrWhiteSpace($localAppData)) {
    $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
}
$logDirectory = Join-Path $localAppData 'nord\logs'
$logPath = Join-Path $logDirectory ("nord.{0}.jsonl" -f (Get-Date -Format 'yyyy-MM-dd'))

if (-not $NoBuild) {
    & cargo build --package nord --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE."
    }
}
if (-not (Test-Path -LiteralPath $executable)) {
    throw "Release executable not found: $executable"
}

function Get-ProcessEvents {
    param([int]$ProcessId)

    if (-not (Test-Path -LiteralPath $logPath)) {
        return @()
    }
    $events = [System.Collections.Generic.List[object]]::new()
    $stream = [System.IO.File]::Open(
        $logPath,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::ReadWrite
    )
    try {
        $reader = [System.IO.StreamReader]::new($stream)
        while ($null -ne ($line = $reader.ReadLine())) {
            try {
                $event = $line | ConvertFrom-Json
            }
            catch {
                continue
            }
            $eventProcessId = $null
            if ($event.spans -and $event.spans.Count -gt 0) {
                $eventProcessId = $event.spans[0].fields.process_id
            }
            if ($eventProcessId -eq $ProcessId) {
                $events.Add($event)
            }
        }
    }
    finally {
        $stream.Dispose()
    }
    return $events.ToArray()
}

function Invoke-PathBenchmark {
    param(
        [string]$PathName,
        [bool]$EnableHiZ
    )

    $previous = @{}
    foreach ($name in @(
        'NORD_OCCLUSION_CULLING',
        'NORD_OCCLUSION_STRESS_SCENE',
        'NORD_RENDER_BENCHMARK',
        'NORD_GPU_DRIVEN_VALIDATE',
        'NORD_LOG'
    )) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    try {
        [Environment]::SetEnvironmentVariable(
            'NORD_OCCLUSION_CULLING',
            $(if ($EnableHiZ) { '1' } else { '0' }),
            'Process'
        )
        [Environment]::SetEnvironmentVariable('NORD_OCCLUSION_STRESS_SCENE', '1', 'Process')
        [Environment]::SetEnvironmentVariable('NORD_RENDER_BENCHMARK', '1', 'Process')
        [Environment]::SetEnvironmentVariable('NORD_GPU_DRIVEN_VALIDATE', '1', 'Process')
        [Environment]::SetEnvironmentVariable('NORD_LOG', 'info', 'Process')

        $process = Start-Process -FilePath $executable -WorkingDirectory $repoRoot -PassThru
        Start-Sleep -Seconds $RunSeconds
        if (-not $process.HasExited) {
            $null = $process.CloseMainWindow()
            if (-not $process.WaitForExit(10000)) {
                Stop-Process -Id $process.Id -Force
                $process.WaitForExit()
            }
        }
        $events = Get-ProcessEvents -ProcessId $process.Id
        $comparison = $events | Where-Object event_name -eq 'render_path_comparison' | Select-Object -Last 1
        $validation = $events | Where-Object event_name -eq 'gpu_driven_culling_validated' | Select-Object -Last 1
        if (-not $comparison -or -not $validation) {
            throw "$PathName run did not produce both benchmark and GPU validation events within $TimeoutSeconds seconds."
        }
        if ($comparison.fields.path -ne $PathName) {
            throw "$PathName run selected unexpected path '$($comparison.fields.path)'."
        }
        $failure = $events | Where-Object severity_text -eq 'ERROR' | Select-Object -Last 1
        if ($failure) {
            throw "$PathName run reported an application error: $($failure.fields.message)"
        }
        if ($process.ExitCode -ne 0) {
            Write-Warning "$PathName benchmark data was recorded, but the process exited with code $($process.ExitCode)."
        }
        [pscustomobject]@{
            Path = $comparison.fields.path
            P50Milliseconds = [double]$comparison.fields.p50_complete_frame_ns / 1000000
            P95Milliseconds = [double]$comparison.fields.p95_complete_frame_ns / 1000000
            FrustumVisible = [int]$validation.fields.frustum_visible_instances
            OcclusionCulled = [int]$validation.fields.occlusion_culled_instances
            FinalVisible = [int]$validation.fields.visible_instances
            VisualChecksum = [string]$comparison.fields.visual_checksum
        }
    }
    finally {
        foreach ($name in $previous.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
        }
    }
}

$baseline = Invoke-PathBenchmark -PathName 'frustum-only' -EnableHiZ $false
$hiz = Invoke-PathBenchmark -PathName 'same-frame-hiz' -EnableHiZ $true

if ($baseline.VisualChecksum -ne $hiz.VisualChecksum) {
    throw "Visual checksums differ: frustum=$($baseline.VisualChecksum), Hi-Z=$($hiz.VisualChecksum)."
}

Write-Host 'Path comparison (same image checksum verified):'
@($baseline, $hiz) | Format-Table -AutoSize
$p50Change = ($hiz.P50Milliseconds / $baseline.P50Milliseconds - 1) * 100
Write-Host ("Hi-Z p50 cost change: {0:+0.0;-0.0;0.0}%" -f $p50Change)
Write-Host ("Image checksum: {0}" -f $baseline.VisualChecksum)
