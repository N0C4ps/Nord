[CmdletBinding()]
param()

. "$PSScriptRoot/_common.ps1"
exit (Invoke-NordCargo -CargoArgs @('doc', '--target-dir', 'target/doc-build', '--workspace', '--no-deps', '--all-features', '--locked'))
