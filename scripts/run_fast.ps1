[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$ProgramArgs = @())

. "$PSScriptRoot/_common.ps1"
$cargoArgs = @('run', '--package', 'nord', '--profile', 'release-fast', '--locked', '--') + $ProgramArgs
exit (Invoke-NordRun -CargoArgs $cargoArgs)
