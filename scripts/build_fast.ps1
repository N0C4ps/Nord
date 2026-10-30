[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"
exit (Invoke-NordCargo -CargoArgs @('build', '--workspace', '--profile', 'release-fast', '--locked'))
