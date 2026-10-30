Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:NordRoot = Split-Path -Parent $PSScriptRoot

function Invoke-NordCargo {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string[]]$CargoArgs
    )

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'Cargo was not found. Install Rust with rustup and ensure cargo is on PATH.'
    }

    Push-Location $script:NordRoot
    try {
        & cargo @CargoArgs | Out-Host
        return $LASTEXITCODE
    } finally {
        Pop-Location
    }
}
