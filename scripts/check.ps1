[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"
exit (Invoke-NordCargo -CargoArgs @('check', '--workspace', '--all-targets', '--all-features', '--locked'))
