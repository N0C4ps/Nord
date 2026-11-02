[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [Parameter(Mandatory)]
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifestPath = Join-Path $projectRoot 'Cargo.toml'
$changelogPath = Join-Path $projectRoot 'CHANGELOG.md'
$executablePath = Join-Path $projectRoot 'target\release\nord.exe'

$manifest = Get-Content -LiteralPath $manifestPath -Raw
$packageMatch = [regex]::Match(
    $manifest,
    '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)'
)
if (-not $packageMatch.Success) {
    throw 'Could not find the root [package] section in Cargo.toml.'
}
$versionMatch = [regex]::Match($packageMatch.Groups[1].Value, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success -or $versionMatch.Groups[1].Value -ne $Version) {
    $manifestVersion = if ($versionMatch.Success) { $versionMatch.Groups[1].Value } else { '<missing>' }
    throw "Release version $Version does not match Cargo.toml ($manifestVersion)."
}

$lines = @(Get-Content -LiteralPath $changelogPath)
$escapedVersion = [regex]::Escape($Version)
$sectionStart = -1
for ($index = 0; $index -lt $lines.Count; $index++) {
    if ($lines[$index] -match "^## \[$escapedVersion\](?:\s|$)") {
        $sectionStart = $index
        break
    }
}
if ($sectionStart -lt 0) {
    throw "CHANGELOG.md has no section for version $Version."
}

$sectionEnd = $lines.Count
for ($index = $sectionStart + 1; $index -lt $lines.Count; $index++) {
    if ($lines[$index] -match '^## ') {
        $sectionEnd = $index
        break
    }
}
$notes = ($lines[($sectionStart + 1)..($sectionEnd - 1)] -join [Environment]::NewLine).Trim()
if ([string]::IsNullOrWhiteSpace($notes)) {
    throw "CHANGELOG.md section for $Version is empty."
}
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "Release executable was not built: $executablePath"
}

$outputPath = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
} else {
    Join-Path $projectRoot $OutputDirectory
}
$destination = [System.IO.Path]::GetFullPath($outputPath)
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $projectRoot 'target'))
if (-not $destination.StartsWith(
        $targetRoot + [System.IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw "Release package output must be inside target: $destination"
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null

$releaseExecutable = Join-Path $destination 'Nord.exe'
$checksumPath = Join-Path $destination 'Nord.exe.sha256'
$notesPath = Join-Path $destination 'RELEASE_NOTES.md'
Copy-Item -LiteralPath $executablePath -Destination $releaseExecutable -Force
$hash = (Get-FileHash -LiteralPath $releaseExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath $checksumPath -Value "$hash  Nord.exe" -Encoding ascii
Set-Content -LiteralPath $notesPath -Value $notes -Encoding utf8

Write-Host "Release package created for $Version at $destination"
