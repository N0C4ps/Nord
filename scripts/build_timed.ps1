[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$Fast
)

. "$PSScriptRoot/_common.ps1"
$cargoArgs = @('build', '--workspace', '--locked', '--timings')
if ($Fast) {
    $cargoArgs += @('--profile', 'release-fast')
} elseif ($Release) {
    $cargoArgs += '--release'
}
exit (Invoke-NordCargo -CargoArgs $cargoArgs)
