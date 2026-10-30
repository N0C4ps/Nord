[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"

$steps = @(
    @{ Name = 'Formatting'; Args = @('fmt', '--all', '--', '--check') },
    @{ Name = 'Clippy'; Args = @('clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings') },
    @{ Name = 'Check'; Args = @('check', '--workspace', '--all-targets', '--all-features', '--locked') }
)

foreach ($step in $steps) {
    Write-Host "`n== $($step.Name) ==" -ForegroundColor Cyan
    $exitCode = Invoke-NordCargo -CargoArgs $step.Args
    if ($exitCode -ne 0) {
        Write-Host "Validation step '$($step.Name)' failed with exit code $exitCode." -ForegroundColor Red
        exit $exitCode
    }
}

Write-Host "`nAll validation steps passed." -ForegroundColor Green
