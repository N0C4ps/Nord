Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$global:LASTEXITCODE = 0

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

function Invoke-NordRun {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string[]]$CargoArgs
    )

    $shortTemp = Join-Path (Join-Path $script:NordRoot 'target') 't'
    New-Item -ItemType Directory -Path $shortTemp -Force | Out-Null
    $previousTemp = $env:TEMP
    $previousTmp = $env:TMP
    try {
        $env:TEMP = $shortTemp
        $env:TMP = $shortTemp
        Invoke-NordCargo -CargoArgs $CargoArgs
    } finally {
        $env:TEMP = $previousTemp
        $env:TMP = $previousTmp
    }
}
