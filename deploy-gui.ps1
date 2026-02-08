# PowerShell script to deploy Qt DLLs alongside the executable
# This copies all necessary Qt libraries so the exe can run standalone

$env:PATH = "C:\Qt\Tools\mingw1310_64\bin;C:\Qt\6.10.2\mingw_64\bin;" + $env:PATH

Write-Host "Deploying Qt libraries to target\debug..."
Write-Host ""

# Use windeployqt to copy all necessary Qt DLLs
& C:\Qt\6.10.2\mingw_64\bin\windeployqt.exe --no-translations .\target\debug\chirp-rs.exe

Write-Host ""
Write-Host "Deployment complete! You can now run .\target\debug\chirp-rs.exe directly."
