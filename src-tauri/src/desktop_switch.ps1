$ErrorActionPreference = 'Stop'
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$package = Get-AppxPackage -Name OpenAI.Codex | Select-Object -First 1
$exe = $null
$appId = $null
if ($package) {
    $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
    $application = $manifest.Package.Applications.Application | Select-Object -First 1
    $exe = Join-Path $package.InstallLocation $application.Executable
    $appId = $package.PackageFamilyName + '!' + $application.Id
}
if (-not $exe) {
    foreach ($candidate in @((Join-Path $env:LOCALAPPDATA 'Programs\Codex\Codex.exe'), (Join-Path $env:LOCALAPPDATA 'Codex\Codex.exe'))) {
        if (Test-Path -LiteralPath $candidate) { $exe = $candidate; break }
    }
}
if (-not $exe) { throw 'Codex Desktop was not found. Install the official Windows desktop app.' }
if ($request.action -eq 'locate') {
    @{ exe = $exe; appId = $appId } | ConvertTo-Json -Compress
    exit
}
if ($exe -ne $request.exe) { throw 'Codex installation changed; retry switching.' }
if ($request.action -eq 'close') {
    $processes = @(Get-Process | Where-Object { $_.Path -eq $exe })
    foreach ($process in $processes) {
        if ($process.MainWindowHandle -ne 0) { $null = $process.CloseMainWindow() }
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (@(Get-Process | Where-Object { $_.Path -eq $exe }).Count -gt 0) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Codex did not close normally. Finish or stop its active work and retry. Credentials were not changed.' }
        Start-Sleep -Milliseconds 250
    }
} elseif ($request.action -eq 'open') {
    if ($appId) {
        # Activate the Store application through its registered shell identity.
        Start-Process explorer.exe -ArgumentList ('shell:AppsFolder\' + $appId) -WindowStyle Hidden
    } else {
        Start-Process -FilePath $exe -WindowStyle Hidden
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (@(Get-Process | Where-Object { $_.Path -eq $exe }).Count -eq 0) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Codex did not relaunch in time.' }
        Start-Sleep -Milliseconds 250
    }
}
