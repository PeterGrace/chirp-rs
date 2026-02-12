all: clean build

windows: build-gui-win

clean:
  cargo clean

build:
  cargo build --features=gui

check:
  cargo check --features=gui -q

build-gui-win:
  powershell.exe .\build-gui.ps1
