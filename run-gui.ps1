# PowerShell script to run chirp-rs with Qt DLLs in PATH

# Add Qt's bin directories to PATH so DLLs can be found
$env:PATH = "C:\Qt\Tools\mingw1310_64\bin;C:\Qt\6.10.2\mingw_64\bin;" + $env:PATH

Write-Host "Running chirp-rs with Qt libraries in PATH..."
Write-Host ""

# Run the GUI binary
& .\target\debug\chirp-rs.exe
