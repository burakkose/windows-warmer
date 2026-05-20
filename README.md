# Warmer

Warmer is a small Windows tray app for people whose **Night Light** or **f.lux** does not actually warm the screen.

I made it after running into this on a Qualcomm/Adreno laptop: Windows and f.lux both looked enabled, but the display stayed cold. Warmer uses a different Windows color path, so it can still apply a warm tint when the usual gamma-ramp approach is ignored by the driver.

By default it starts at **3500K**.

## What it does

- Runs quietly as `Warmer.exe` in the tray.
- Lets you turn warming on/off from the tray menu.
- Includes presets from 2700K to 6500K.
- Has strength presets if 3500K feels too strong.
- Starts automatically when you sign in.
- Does not need admin rights.

Warmer does **not** change your monitor brightness or laptop backlight. It changes the color mix, so whites may look a little dimmer because blue/green output is reduced.

## Install

Open PowerShell in this folder and run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass -Force
.\install.ps1
```

The installer checks for Rust and Microsoft C++ Build Tools. If either is missing, it installs the missing pieces with `winget`, builds the app, copies it to `%LOCALAPPDATA%\Warmer`, creates a `Warmer` scheduled task, and starts it.

First install can take a while because the Microsoft build tools are not tiny. After that, reinstalling is quick.

## Use it

- Click the tray icon to open the menu.
- Use **Enabled** to toggle it on/off.
- Pick a temperature like **3500K**.
- Use **Strength** if the full effect feels too much.
- Choose **Exit** to close it and reset colors.

If you do not see the icon, check the tray overflow area.

## Build only

Run this from a Visual Studio Developer PowerShell so `link.exe` and `rc.exe` are on `PATH`:

```powershell
cargo build --release
```

This uses Rust's normal Windows MSVC target, so it builds ARM64 on Windows ARM laptops and x64 on x64 Windows.

## Uninstall

```powershell
.\uninstall.ps1
```

This stops Warmer, resets the color transform, removes the scheduled task, and deletes the installed copy.

## Notes

- It affects the whole desktop.
- HDR video, exclusive fullscreen games, or display reconnects may temporarily override it; Warmer reapplies the color every few seconds.
- Screenshots or recordings may include the warmer tint depending on how they are captured.
- Keep Windows Night Light and f.lux off so they do not fight with Warmer.

