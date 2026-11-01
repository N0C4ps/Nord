[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$ProgramArgs = @())

. "$PSScriptRoot/_common.ps1"
$cargoArgs = @('run', '--package', 'nord', '--release', '--locked', '--') + $ProgramArgs
exit (Invoke-NordRun -CargoArgs $cargoArgs)
