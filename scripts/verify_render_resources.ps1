[CmdletBinding()]
param([switch]$Release)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/_common.ps1"

if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows) -eq $false) {
    throw 'The D3D12 render-resource smoke check requires Windows.'
}

$dxc = Get-Command 'dxc.exe' -ErrorAction SilentlyContinue
if ($null -eq $dxc) {
    throw 'DXC was not found. Install it or add dxc.exe to PATH before running this check.'
}

$previousDxcPath = $env:NORD_DXC_PATH
$env:NORD_DXC_PATH = $dxc.Source
$exitCode = 1
try {
    $cargoArgs = @('run')
    if ($Release) {
        $cargoArgs += '--release'
    }
    $cargoArgs += @('--package', 'nord_app', '--example', 'render_resource_smoke', '--locked')
    $exitCode = Invoke-NordRun -CargoArgs $cargoArgs
}
finally {
    $env:NORD_DXC_PATH = $previousDxcPath
}
exit $exitCode
