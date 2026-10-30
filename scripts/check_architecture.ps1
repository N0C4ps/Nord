[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"

$sourceFiles = Get-ChildItem (Join-Path $script:NordRoot 'src') -Filter '*.rs' -Recurse -File
$violations = @()
foreach ($file in $sourceFiles) {
    $matches = Select-String -Path $file.FullName -Pattern '\bnord_dependency\b|\bDependencyBuilder\b|\bDependencyContainer\b|\bDependencyError\b'
    if ($matches) {
        $violations += $matches | ForEach-Object { "$($_.Path):$($_.LineNumber): $($_.Line.Trim())" }
    }
}

$appManifest = Get-Content (Join-Path $script:NordRoot 'crates/nord_app/Cargo.toml') -Raw
if ($appManifest -notmatch '(?m)^nord_dependency\s*=\s*\{\s*path\s*=\s*"\.\./nord_dependency"\s*\}') {
    $violations += 'crates/nord_app/Cargo.toml must be the sole adapter dependency on nord_dependency.'
}

$rootManifest = Get-Content (Join-Path $script:NordRoot 'Cargo.toml') -Raw
if ($rootManifest -match '(?m)^nord_dependency\s*=') {
    $violations += 'The game package must not depend directly on nord_dependency.'
}

$engineManifests = Get-ChildItem (Join-Path $script:NordRoot 'crates') -Filter 'Cargo.toml' -Recurse -File |
    Where-Object { $_.Directory.Name -ne 'nord_app' }
foreach ($manifest in $engineManifests) {
    if (Select-String -Path $manifest.FullName -Pattern '(?m)^nord_dependency\s*=') {
        $violations += "$($manifest.FullName) must not depend directly on nord_dependency; use the nord_app adapter."
    }
}

if ($violations.Count -gt 0) {
    Write-Host 'Architecture boundary violation: keep the dependency manager behind crates/nord_app.' -ForegroundColor Red
    $violations | ForEach-Object { Write-Host $_ -ForegroundColor Red }
    exit 1
}

Write-Host 'Dependency manager boundary is intact.' -ForegroundColor Green
