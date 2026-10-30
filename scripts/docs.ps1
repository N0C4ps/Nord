[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"
exit (Invoke-NordCargo -CargoArgs @('doc', '--workspace', '--no-deps', '--all-features', '--locked'))
