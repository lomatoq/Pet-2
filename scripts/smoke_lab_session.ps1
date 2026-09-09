param(
    [string]$PetExecutable = '',
    [string]$DataDirectory = ''
)
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
if (-not $PetExecutable) {
    $PetExecutable = Join-Path $workspace 'dist/Pet2-windows-x64/Pet2.exe'
}
$PetExecutable = [IO.Path]::GetFullPath($PetExecutable)
if (-not (Test-Path -LiteralPath $PetExecutable -PathType Leaf)) {
    throw "Pet executable not found: $PetExecutable"
}
if (-not $DataDirectory) {
    $DataDirectory = Join-Path $workspace 'artifacts/r13-lab-session-smoke'
}
$DataDirectory = [IO.Path]::GetFullPath($DataDirectory)
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $workspace 'artifacts'))
$artifactPrefix = $artifactRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not $DataDirectory.StartsWith($artifactPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "smoke data directory must stay inside $artifactRoot"
}
New-Item -ItemType Directory -Force -Path $DataDirectory | Out-Null
$token = [Guid]::NewGuid().ToString('N').ToLowerInvariant()
$controlPath = Join-Path $DataDirectory 'lab-control.json'
$telemetryPath = Join-Path $DataDirectory 'telemetry.jsonl'
$ackPath = Join-Path $DataDirectory 'runtime-load-ack.json'
$utf8WithoutBom = New-Object System.Text.UTF8Encoding($false)
$lastCommandId = 0L
$process = $null

function Write-Control([hashtable]$command, [bool]$withToken = $true, [int]$expiryMs = 5000) {
    $issued = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    if ($issued -le $script:lastCommandId) { $issued = $script:lastCommandId + 1 }
    $script:lastCommandId = $issued
    $envelope = [ordered]@{
        schema_version = 4
        command_id = $issued
        issued_unix_ms = $issued
        expires_after_ms = $expiryMs
    }
    if ($withToken) { $envelope.session_token = $script:token }
    $envelope.command = $command
    $json = $envelope | ConvertTo-Json -Depth 5 -Compress
    $temporary = "$script:controlPath.$issued.tmp"
    [IO.File]::WriteAllText($temporary, $json, $script:utf8WithoutBom)
    Move-Item -LiteralPath $temporary -Destination $script:controlPath -Force
    return $issued
}

function Wait-Until([scriptblock]$condition, [int]$timeoutSeconds, [string]$failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds($timeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $condition) { return }
        Start-Sleep -Milliseconds 100
    }
    throw $failure
}

function Read-Telemetry {
    if (-not (Test-Path -LiteralPath $script:telemetryPath)) { return @() }
    $frames = @()
    foreach ($line in Get-Content -LiteralPath $script:telemetryPath) {
        if ($line.Trim().Length -eq 0) { continue }
        try {
            $frame = $line | ConvertFrom-Json
            if ($frame.kind -eq 'debug_state') { $frames += $frame }
        } catch {}
    }
    return @($frames)
}

try {
    $petArguments = "--data-dir `"$DataDirectory`" --reset-pet --no-audio"
    $process = Start-Process -FilePath $PetExecutable `
        -ArgumentList $petArguments `
        -WorkingDirectory (Split-Path -Parent $PetExecutable) `
        -WindowStyle Hidden -PassThru
    Wait-Until {
        if (-not (Test-Path -LiteralPath $ackPath)) { return $false }
        try {
            $ack = Get-Content -LiteralPath $ackPath -Raw | ConvertFrom-Json
            return $ack.status -eq 'running' -and $ack.pid -eq $process.Id
        } catch { return $false }
    } 15 'normal release did not publish a fresh runtime heartbeat'

    $openId = Write-Control ([ordered]@{
        type = 'open_session'
        protocol_version = 2
        lease_seconds = 10
    })
    Wait-Until {
        $frames = @(Read-Telemetry)
        return $frames.Count -ge 2 -and $frames[-1].details.lab_session.active -eq $true `
            -and $frames[-1].details.lab_session.protocol_version -eq 2
    } 8 'OpenSession did not produce live 5 Hz session telemetry'

    $framesAfterOpen = @(Read-Telemetry)
    $firstSequence = [uint64]$framesAfterOpen[-2].details.sequence
    $secondSequence = [uint64]$framesAfterOpen[-1].details.sequence
    if ($secondSequence -le $firstSequence) { throw 'telemetry sequence did not advance' }

    $motorId = Write-Control ([ordered]@{
        type = 'run_motor_program'
        program = 'move_orient_reflex'
    })
    Wait-Until {
        $frames = @(Read-Telemetry)
        if ($frames.Count -eq 0) { return $false }
        $packet = $frames[-1].details.motor.packet
        return $frames[-1].details.motor.lab_program_override -eq 'move_orient_reflex' `
            -or $packet.program -eq 'move_orient_reflex'
    } 5 'one of the 64 motor programs was not acknowledged by the release runtime'

    $closeId = Write-Control ([ordered]@{ type = 'close_session' })
    Start-Sleep -Milliseconds 750
    $countAfterClose = @(Read-Telemetry).Count
    Start-Sleep -Milliseconds 900
    $countAfterQuiet = @(Read-Telemetry).Count
    if ($countAfterQuiet -gt $countAfterClose + 1) {
        throw '5 Hz telemetry remained active after Disconnect'
    }

    $shutdownId = Write-Control ([ordered]@{ type = 'shutdown_for_promotion' }) $false 15000
    Wait-Until { $process.HasExited } 10 'release did not exit through the bounded shutdown command'

    [ordered]@{
        result = 'pass'
        pid = $process.Id
        open_command_id = $openId
        motor_command_id = $motorId
        close_command_id = $closeId
        shutdown_command_id = $shutdownId
        telemetry_frames = $countAfterQuiet
        sequence_first = $firstSequence
        sequence_last = $secondSequence
        motor_program = 'move_orient_reflex'
        lease_seconds = 10
        renewal_seconds = 3
    } | ConvertTo-Json -Depth 3
} finally {
    if ($process -and -not $process.HasExited) {
        try { Write-Control ([ordered]@{ type = 'shutdown_for_promotion' }) $false 15000 | Out-Null } catch {}
        try { $process.WaitForExit(3000) | Out-Null } catch {}
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}
