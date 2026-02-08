# PowerShell script to build chirp-rs with GUI feature in release mode
# Sets necessary Qt environment variables and uses Qt's MinGW toolchain
# Automatically deploys Qt DLLs after successful build

$env:QTDIR = "C:\Qt\6.10.2\mingw_64"
# Prepend Qt's MinGW to PATH (must come before system MinGW for ABI compatibility)
$env:PATH = "C:\Qt\Tools\mingw1310_64\bin;C:\Qt\6.10.2\mingw_64\bin;" + $env:PATH

Write-Host "Building with GUI feature (release mode)..."
Write-Host "QTDIR: $env:QTDIR"
Write-Host "qmake: C:\Qt\6.10.2\mingw_64\bin\qmake.exe"
Write-Host "g++: C:\Qt\Tools\mingw1310_64\bin\g++.exe"
Write-Host ""

# Build the project in release mode
cargo build --release --features gui

# Check if build succeeded
if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "Build successful! Deploying Qt DLLs..."
    Write-Host ""

    # Deploy Qt DLLs to the release executable
    & C:\Qt\6.10.2\mingw_64\bin\windeployqt.exe --no-translations --release .\target\release\chirp-rs.exe

    Write-Host ""
    Write-Host "Done! You can now run: .\target\release\chirp-rs.exe"
} else {
    Write-Host ""
    Write-Host "Build failed. Skipping DLL deployment."
    exit $LASTEXITCODE
}
