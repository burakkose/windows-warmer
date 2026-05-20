$ErrorActionPreference = 'Stop'

$installDirectory = Join-Path $env:LOCALAPPDATA 'Warmer'
$installedExe = Join-Path $installDirectory 'Warmer.exe'
$currentIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$currentSid = $currentIdentity.User.Value
$safeSid = $currentSid -replace '[^A-Za-z0-9_-]', '_'
$taskPath = '\Warmer\'
$taskName = "Warmer-$safeSid"

function Stop-WarmerProcesses {
    Get-CimInstance Win32_Process -Filter "Name = 'Warmer.exe'" -ErrorAction SilentlyContinue |
        ForEach-Object {
            $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid -ErrorAction SilentlyContinue
            if ($owner.Sid -eq $currentSid -or $_.ExecutablePath -eq $installedExe) {
                Stop-Process -Id $_.ProcessId -Force
            }
        }
}

function Unregister-WarmerTasks {
    Unregister-ScheduledTask -TaskPath $taskPath -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue

    $legacyTask = Get-ScheduledTask -TaskName 'Warmer' -ErrorAction SilentlyContinue
    if ($legacyTask -and $legacyTask.Actions.Execute -eq $installedExe) {
        Unregister-ScheduledTask -TaskName 'Warmer' -Confirm:$false -ErrorAction SilentlyContinue
    }
}

Unregister-WarmerTasks
Stop-WarmerProcesses

if (Test-Path $installedExe) {
    Start-Process -FilePath $installedExe -ArgumentList '--reset' -Wait -WindowStyle Hidden
}

if (Test-Path $installDirectory) {
    Remove-Item -Path $installDirectory -Recurse -Force
}

Write-Host 'Warmer uninstalled.'

