param(
    [int]$Temperature = 3500,
    [int]$Strength = 100,
    [switch]$SkipPrerequisiteInstall,
    [switch]$NoStart
)

$ErrorActionPreference = 'Stop'

$projectRoot = $PSScriptRoot
$installDirectory = Join-Path $env:LOCALAPPDATA 'Warmer'
$installedExe = Join-Path $installDirectory 'Warmer.exe'
$settingsPath = Join-Path $installDirectory 'settings.ini'
$currentIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$currentUser = $currentIdentity.Name
$currentSid = $currentIdentity.User.Value
$safeSid = $currentSid -replace '[^A-Za-z0-9_-]', '_'
$taskPath = '\Warmer\'
$taskName = "Warmer-$safeSid"
$temperatureWasProvided = $PSBoundParameters.ContainsKey('Temperature')
$strengthWasProvided = $PSBoundParameters.ContainsKey('Strength')

function Add-CargoToPath {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path $cargoBin) {
        $env:PATH = "$cargoBin;$env:PATH"
    }
}

function Get-Winget {
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if (-not $winget) {
        throw 'winget is required to install missing prerequisites automatically.'
    }

    return $winget.Source
}

function Install-WingetPackage {
    param(
        [string]$Id,
        [string[]]$ExtraArgs = @()
    )

    if ($SkipPrerequisiteInstall) {
        throw "Missing prerequisite: $Id. Install it manually, then rerun install.ps1."
    }

    $winget = Get-Winget
    Write-Host "Installing missing prerequisite: $Id"
    & $winget install --id $Id --exact --accept-package-agreements --accept-source-agreements @ExtraArgs
    if ($LASTEXITCODE -ne 0) {
        throw "winget failed to install $Id with exit code $LASTEXITCODE"
    }
}

function Ensure-Rust {
    Add-CargoToPath

    if (-not (Get-Command rustup.exe -ErrorAction SilentlyContinue)) {
        Install-WingetPackage -Id 'Rustlang.Rustup' -ExtraArgs @('--silent')
        Add-CargoToPath
    }

    if (-not (Get-Command rustup.exe -ErrorAction SilentlyContinue)) {
        throw 'rustup is still not available after installation.'
    }

    rustup default stable
    if ($LASTEXITCODE -ne 0) {
        throw "rustup default stable failed with exit code $LASTEXITCODE"
    }

    if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
        throw 'cargo is still not available after installing Rust.'
    }
}

function Get-VisualStudioBuildToolsPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        return $null
    }

    $installationPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Workload.VCTools -property installationPath
    if ([string]::IsNullOrWhiteSpace($installationPath)) {
        return $null
    }

    return $installationPath
}

function Ensure-VisualStudioBuildTools {
    if ((Get-Command link.exe -ErrorAction SilentlyContinue) -and
        (Get-Command rc.exe -ErrorAction SilentlyContinue)) {
        return
    }

    if (Get-VisualStudioBuildToolsPath) {
        return
    }

    $overrideParts = @(
        '--quiet'
        '--wait'
        '--norestart'
        '--add Microsoft.VisualStudio.Workload.VCTools'
        '--includeRecommended'
        '--add Microsoft.VisualStudio.Component.Windows11SDK.26100'
    )

    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    if ($arch -eq 'Arm64') {
        $overrideParts += '--add Microsoft.VisualStudio.Component.VC.Tools.ARM64'
    }

    Install-WingetPackage -Id 'Microsoft.VisualStudio.2022.BuildTools' -ExtraArgs @('--override', ($overrideParts -join ' '))
}

function Import-VisualStudioEnvironment {
    if ((Get-Command link.exe -ErrorAction SilentlyContinue) -and
        (Get-Command rc.exe -ErrorAction SilentlyContinue)) {
        return
    }

    $installationPath = Get-VisualStudioBuildToolsPath
    if ([string]::IsNullOrWhiteSpace($installationPath)) {
        throw 'Could not find Microsoft C++ Build Tools with the C++ workload installed.'
    }

    $vsDevCmd = Join-Path $installationPath 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path $vsDevCmd)) {
        throw "Could not find VsDevCmd.bat at $vsDevCmd"
    }

    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    $toolArch = if ($arch -eq 'Arm64') { 'arm64' } else { 'x64' }
    $environment = cmd /s /c "`"$vsDevCmd`" -arch=$toolArch -host_arch=$toolArch >nul && set"

    foreach ($line in $environment) {
        $name, $value = $line -split '=', 2
        if ($name) {
            Set-Item -Path "env:$name" -Value $value
        }
    }

    if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        throw 'link.exe is still not available after loading the Visual Studio environment.'
    }
    if (-not (Get-Command rc.exe -ErrorAction SilentlyContinue)) {
        throw 'rc.exe is still not available after loading the Visual Studio environment.'
    }
}

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

function Read-WarmerSettings {
    $settings = @{
        Enabled = 'True'
        Temperature = [string][Math]::Max(1000, [Math]::Min(10000, $Temperature))
        Strength = [string][Math]::Max(0, [Math]::Min(100, $Strength))
    }

    if (Test-Path $settingsPath) {
        Get-Content $settingsPath -ErrorAction SilentlyContinue |
            ForEach-Object {
                $name, $value = $_ -split '=', 2
                if ($name -and $settings.ContainsKey($name.Trim())) {
                    $settings[$name.Trim()] = $value.Trim()
                }
            }
    }

    if ($temperatureWasProvided) {
        $settings['Temperature'] = [string][Math]::Max(1000, [Math]::Min(10000, $Temperature))
    }
    if ($strengthWasProvided) {
        $settings['Strength'] = [string][Math]::Max(0, [Math]::Min(100, $Strength))
    }

    return $settings
}

function Write-WarmerSettings {
    $settings = Read-WarmerSettings
    @(
        "Enabled=$($settings['Enabled'])"
        "Temperature=$($settings['Temperature'])"
        "Strength=$($settings['Strength'])"
    ) | Set-Content -Path $settingsPath -Encoding ASCII
}

Ensure-Rust
Ensure-VisualStudioBuildTools
Import-VisualStudioEnvironment

Set-Location $projectRoot
cargo build --release
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE"
}

$builtExe = Join-Path $projectRoot 'target\release\warmer.exe'
if (-not (Test-Path $builtExe)) {
    throw "Build did not produce $builtExe"
}

New-Item -ItemType Directory -Path $installDirectory -Force | Out-Null

Unregister-WarmerTasks
Stop-WarmerProcesses

if (Test-Path $installedExe) {
    Start-Process -FilePath $installedExe -ArgumentList '--reset' -Wait -WindowStyle Hidden
}

Copy-Item -Path $builtExe -Destination $installedExe -Force

Write-WarmerSettings

$action = New-ScheduledTaskAction -Execute $installedExe
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $currentUser
$taskSettings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskPath $taskPath -TaskName $taskName -Action $action -Trigger $trigger -Settings $taskSettings -Description 'Warmer background tray color temperature controller.' -Force | Out-Null

if (-not $NoStart) {
    Start-Process -FilePath $installedExe
}

Write-Host "Installed Warmer to $installDirectory"
Write-Host "Scheduled task: $taskPath$taskName"

