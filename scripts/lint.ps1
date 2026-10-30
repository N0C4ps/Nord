[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"
exit (Invoke-NordCargo -CargoArgs @('clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'))
