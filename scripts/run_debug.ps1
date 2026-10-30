[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$ProgramArgs = @())

. "$PSScriptRoot/_common.ps1"
$cargoArgs = @('run', '--package', 'nord', '--locked', '--') + $ProgramArgs
exit (Invoke-NordCargo -CargoArgs $cargoArgs)
