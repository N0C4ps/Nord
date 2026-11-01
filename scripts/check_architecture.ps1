[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"

$sourceFiles = Get-ChildItem (Join-Path $script:NordRoot 'src') -Filter '*.rs' -Recurse -File
$violations = @()
foreach ($file in $sourceFiles) {
    $matches = Select-String -Path $file.FullName -Pattern '\bnord_dependency\b|\bnord_platform\b|\bnord_events\b|\bnord_inputs\b|\bnord_math\b|\bnord_render\b|\bglam\b|\bnord_log\b|\bnord_crash\b|\bnord_diagnostics\b|\bnord_memory\b|\bnord_hash\b|\bnord_time\b|\bnord_config\b|\bnord_threading\b|\bnord_jobs\b|\bnord_scheduler\b|\brayon\b|\bDependencyBuilder\b|\bDependencyContainer\b|\bDependencyError\b'
    if ($matches) {
        $violations += $matches | ForEach-Object { "$($_.Path):$($_.LineNumber): $($_.Line.Trim())" }
    }
}

$appManifest = Get-Content (Join-Path $script:NordRoot 'crates/nord_app/Cargo.toml') -Raw
if ($appManifest -notmatch '(?m)^nord_dependency\s*=\s*\{\s*path\s*=\s*"\.\./nord_dependency"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must be the sole adapter dependency on nord_dependency.'
}
if ($appManifest -notmatch '(?m)^nord_platform\s*=\s*\{\s*path\s*=\s*"\.\./nord_platform"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_platform.'
}
if ($appManifest -notmatch '(?m)^nord_render\s*=\s*\{\s*path\s*=\s*"\.\./nord_render"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_render.'
}
if ($appManifest -notmatch '(?m)^nord_events\s*=\s*\{\s*path\s*=\s*"\.\./nord_events"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_events.'
}
if ($appManifest -notmatch '(?m)^nord_inputs\s*=\s*\{\s*path\s*=\s*"\.\./nord_inputs"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_inputs.'
}
if ($appManifest -notmatch '(?m)^nord_math\s*=\s*\{\s*path\s*=\s*"\.\./nord_math"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_math.'
}
if ($appManifest -notmatch '(?m)^nord_log\s*=\s*\{\s*path\s*=\s*"\.\./nord_log"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_log.'
}
if ($appManifest -notmatch '(?m)^nord_diagnostics\s*=\s*\{\s*path\s*=\s*"\.\./nord_diagnostics"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_diagnostics.'
}
if ($appManifest -notmatch '(?m)^nord_crash\s*=\s*\{\s*path\s*=\s*"\.\./nord_crash"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_crash.'
}
if ($appManifest -notmatch '(?m)^nord_memory\s*=\s*\{\s*path\s*=\s*"\.\./nord_memory"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_memory.'
}
if ($appManifest -notmatch '(?m)^nord_hash\s*=\s*\{\s*path\s*=\s*"\.\./nord_hash"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_hash.'
}
if ($appManifest -notmatch '(?m)^nord_time\s*=\s*\{\s*path\s*=\s*"\.\./nord_time"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_time.'
}
if ($appManifest -notmatch '(?m)^nord_config\s*=\s*\{\s*path\s*=\s*"\.\./nord_config"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_config.'
}
if ($appManifest -notmatch '(?m)^nord_threading\s*=\s*\{\s*path\s*=\s*"\.\./nord_threading"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_threading.'
}
if ($appManifest -notmatch '(?m)^nord_jobs\s*=\s*\{\s*path\s*=\s*"\.\./nord_jobs"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_jobs.'
}
if ($appManifest -notmatch '(?m)^nord_scheduler\s*=\s*\{\s*path\s*=\s*"\.\./nord_scheduler"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must own the facade dependency on nord_scheduler.'
}

$rootManifest = Get-Content (Join-Path $script:NordRoot 'Cargo.toml') -Raw
if ($rootManifest -match '(?m)^nord_dependency\s*=') {
    $violations += 'The game package must not depend directly on nord_dependency.'
}
if ($rootManifest -match '(?m)^nord_platform\s*=') {
    $violations += 'The game package must not depend directly on nord_platform.'
}
if ($rootManifest -match '(?m)^nord_events\s*=|^nord_inputs\s*=') {
    $violations += 'The game package must use the nord_app event and input facades.'
}
if ($rootManifest -match '(?m)^nord_math\s*=|^glam\s*=') {
    $violations += 'The game package must use the nord_app math facade.'
}
if ($rootManifest -match '(?m)^nord_log\s*=') {
    $violations += 'The game package must not depend directly on nord_log.'
}
if ($rootManifest -match '(?m)^nord_diagnostics\s*=') {
    $violations += 'The game package must not depend directly on nord_diagnostics.'
}
if ($rootManifest -match '(?m)^nord_crash\s*=') {
    $violations += 'The game package must not depend directly on nord_crash.'
}
if ($rootManifest -match '(?m)^nord_memory\s*=') {
    $violations += 'The game package must not depend directly on nord_memory.'
}
if ($rootManifest -match '(?m)^nord_hash\s*=') {
    $violations += 'The game package must not depend directly on nord_hash.'
}
if ($rootManifest -match '(?m)^nord_time\s*=') {
    $violations += 'The game package must not depend directly on nord_time.'
}
if ($rootManifest -match '(?m)^nord_config\s*=') {
    $violations += 'The game package must not depend directly on nord_config.'
}
if ($rootManifest -match '(?m)^nord_threading\s*=|^nord_jobs\s*=|^nord_scheduler\s*=|^rayon\s*=') {
    $violations += 'The game package must use the nord_app threading/job facade.'
}

$engineManifests = Get-ChildItem (Join-Path $script:NordRoot 'crates') -Filter 'Cargo.toml' -Recurse -File |
    Where-Object { $_.Directory.Name -ne 'nord_app' }
foreach ($manifest in $engineManifests) {
    if ($manifest.Directory.Name -notin @('nord_render', 'nord_physics', 'nord_world', 'nord_generation', 'nord_camera', 'nord_mesh') -and
        (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_math\s*=')) {
        $violations += "$($manifest.FullName) must keep nord_math behind nord_app or a math-consuming engine crate."
    }
    if ($manifest.Directory.Name -ne 'nord_math' -and
        (Select-String -Path $manifest.FullName -Pattern '(?m)^glam\s*=')) {
        $violations += "$($manifest.FullName) must use nord_math instead of depending directly on glam."
    }
    if ($manifest.Directory.Name -notin @('nord_platform', 'nord_inputs') -and
        (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_events\s*=')) {
        $violations += "$($manifest.FullName) must keep nord_events behind its platform or application facade."
    }
    if ($manifest.Directory.Name -ne 'nord_platform' -and
        (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_inputs\s*=')) {
        $violations += "$($manifest.FullName) must keep nord_inputs behind nord_platform or nord_app."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_dependency\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_dependency; use the nord_app adapter."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_platform\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_platform; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_log\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_log; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_diagnostics\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_diagnostics; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_crash\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_crash; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_memory\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_memory; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_hash\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_hash; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_time\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_time; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_config\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_config; use the nord_app facade."
    }
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_scheduler\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_scheduler; use the nord_app facade."
    }
    if ($manifest.Directory.Name -notin @('nord_threading', 'nord_jobs') -and
        (Select-String -Path $manifest.FullName -Pattern '(?m)^rayon\s*=')) {
        $violations += "$($manifest.FullName) must not depend directly on Rayon; use nord_jobs or nord_app."
    }
}

if ($violations.Count -gt 0) {
    Write-Host 'Architecture boundary violation: keep internal engine APIs behind crates/nord_app.' -ForegroundColor Red
    $violations | ForEach-Object { Write-Host $_ -ForegroundColor Red }
    exit 1
}

Write-Host 'Application facade boundaries are intact.' -ForegroundColor Green
