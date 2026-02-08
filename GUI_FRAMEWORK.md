# GUI Framework: Qt (via qmetaobject-rs)

## Decision: Using Qt instead of iced/egui

### Why Qt?

**Qt** is a mature, cross-platform GUI framework with excellent Rust bindings via `qmetaobject-rs`. It provides:

1. **Native Look and Feel**
   - Uses platform-native widgets on Windows/Linux/macOS
   - Familiar UI patterns for users
   - Proper theme integration (dark mode, etc.)
   - Better accessibility support

2. **Excellent Table Widget**
   - `QTableWidget` is battle-tested and feature-complete
   - Built-in sorting, scrolling, cell editing
   - Row/column selection
   - Excellent keyboard navigation
   - Perfect for memory grid display

3. **Built-in Dialogs**
   - Native file dialogs (`QFileDialog`)
   - Message boxes (`QMessageBox`)
   - Input dialogs
   - Custom modal dialogs
   - Better UX than custom implementations

4. **Mature Ecosystem**
   - Decades of development
   - Extensive documentation
   - Known best practices
   - Large community

5. **Rust Bindings**
   - `qmetaobject` crate provides safe Rust API
   - `cpp!` macro for inline C++ when needed
   - Good FFI boundary management
   - Active maintenance

6. **Single Binary Distribution**
   - Can statically link Qt (with proper licensing)
   - Or distribute Qt DLLs alongside
   - Smaller binary than Electron alternatives

### Qt vs iced/egui Comparison

| Feature | Qt | iced | egui |
|---------|-----|------|------|
| **Native Look** | ✅ Native widgets | ⚠️ Custom rendering | ⚠️ Custom rendering |
| **Table Widget** | ✅ QTableWidget (excellent) | ⬜ Custom implementation | ⬜ egui_extras::Table |
| **File Dialogs** | ✅ Native dialogs | ⚠️ rfd (separate crate) | ⚠️ rfd (separate crate) |
| **Maturity** | ✅ Very mature | ⚠️ Growing | ✅ Mature |
| **Learning Curve** | Medium (Qt API) | Medium (MVU pattern) | Easy (immediate mode) |
| **Async Support** | Manual (via tokio) | Built-in Commands | Manual integration |
| **Windows Support** | ✅ Excellent | ✅ Good | ✅ Good |
| **Binary Size** | Large (with Qt) | Medium | Small |
| **Rust Integration** | Good (qmetaobject) | Native Rust | Native Rust |
| **Documentation** | ✅ Extensive | Growing | ✅ Good |

### Architecture

#### Rust-Qt Integration

```rust
// FFI boundary: Rust functions callable from C++
#[no_mangle]
pub extern "C" fn load_file(path: *const c_char) -> *const c_char {
    // Rust implementation
}

// Qt C++ side (in cpp! macro)
cpp! {{
    extern "C" const char* load_file(const char* path);

    // Call from Qt slots
    fileMenu->addAction("&Open", [=]() {
        QString fileName = QFileDialog::getOpenFileName(...);
        const char* error = load_file(fileName.toUtf8().constData());
        // Handle result
    });
}}
```

#### Application Structure

```
src/gui/
├── qt_gui.rs          # Main Qt application (1500+ lines)
│   ├── C++ code (cpp! macros)
│   │   ├── QApplication setup
│   │   ├── Menu bar creation
│   │   ├── Table widget setup
│   │   └── Event handlers (slots)
│   └── Rust FFI functions
│       ├── Memory data management
│       ├── File operations
│       ├── Radio operations
│       └── Data conversion
└── radio_ops.rs       # Async radio operations
    ├── download_from_radio()
    ├── upload_to_radio()
    └── Progress callbacks
```

#### Data Flow

```
User Action (Qt)
    ↓
Qt Signal/Slot
    ↓
FFI Call (extern "C" fn)
    ↓
Rust Implementation
    ↓  (async operations via tokio)
Radio Driver / File Format Handler
    ↓
Rust Result
    ↓
FFI Return (success/error)
    ↓
Qt UI Update (QMessageBox, table refresh, etc.)
```

### Key Components

#### 1. Table Display (`QTableWidget`)

**Features:**
- 13 columns: Location, Frequency, Name, Duplex, Offset, Mode, ToneMode, Tone, Power, URCALL, RPT1, RPT2, Bank
- Alternating row colors for readability
- Sortable columns
- Row selection
- Custom column widths
- Conditional display (D-STAR fields only for DV memories)

**Data Binding:**
```rust
#[repr(C)]
pub struct RowData {
    loc: *const c_char,
    freq: *const c_char,
    name: *const c_char,
    // ... (13 fields total)
}

#[no_mangle]
pub extern "C" fn get_memory_row(row: usize) -> RowData {
    // Convert Memory struct to C-compatible strings
}
```

#### 2. File Operations

**Open File:**
- Native `QFileDialog::getOpenFileName()`
- Filters: "CHIRP Image (*.img)"
- Async load via FFI call
- Error handling with `QMessageBox::critical()`

**Save File:**
- `QFileDialog::getSaveFileName()` for Save As
- Use current file path for Save
- Encode memories to MemoryMap
- Create metadata
- Call `save_img()` from formats module
- Success/error dialogs

#### 3. Radio Operations

**Download Dialog:**
- Vendor selection (`QComboBox`)
- Model selection (filtered by vendor)
- Port selection (auto-detected)
- Progress bar with current/total/message
- Cancel button
- Async execution with progress callbacks

**Upload Dialog:**
- Port selection
- Progress bar
- Confirmation before upload
- Async execution

#### 4. Memory Management

**Global State:**
```rust
struct AppState {
    memories: Vec<Memory>,           // All loaded memories
    cstrings: Vec<Vec<CString>>,    // Cached C strings for display
    current_file: Option<PathBuf>,   // Current file path
    is_modified: bool,               // Has data changed?
}

static MEMORY_DATA: Mutex<Option<AppState>> = Mutex::new(None);
```

**Thread Safety:**
- `Mutex` for state synchronization
- FFI calls lock mutex briefly
- No long-running operations under lock
- Async tasks spawn separately

### Building

#### Dependencies

```toml
[dependencies]
qmetaobject = { version = "0.2", optional = true }
cpp = { version = "0.5", optional = true }

[build-dependencies]
cpp_build = "0.5"

[features]
gui = ["dep:qmetaobject", "dep:cpp"]
```

#### Build Process

1. `build.rs` runs `cpp_build` to parse `cpp!` macros
2. Generates C++ glue code
3. Compiles with Qt's moc (Meta-Object Compiler)
4. Links Qt libraries
5. Produces binary with embedded Qt code

#### Platform Requirements

**Linux:**
```bash
# Debian/Ubuntu
sudo apt install qtbase5-dev qt5-qmake build-essential

# Fedora
sudo dnf install qt5-qtbase-devel gcc-c++
```

**Windows:**
- Install Qt 5.x from qt.io
- Set `QTDIR` environment variable
- Use MSVC or MinGW toolchain

**macOS:**
```bash
brew install qt@5
```

### Best Practices

#### 1. FFI Safety
- Always validate C pointers before dereferencing
- Use `CStr::from_ptr()` for C strings
- Return owned `CString` for Rust → C
- Free error messages with `free_error_message()`

#### 2. Memory Management
- Cache `CString` conversions (they're expensive)
- Store in `AppState.cstrings` field
- Clear cache when data changes
- Return pointers to cached strings

#### 3. Error Handling
- Return `*const c_char` for errors (NULL = success)
- Descriptive error messages
- Qt displays in `QMessageBox`
- User-friendly language

#### 4. Async Operations
- Spawn tokio tasks for I/O
- Use `Arc<Mutex<>>` for progress callbacks
- Update Qt UI from main thread only
- Handle cancellation gracefully

#### 5. Table Performance
- Update only changed rows
- Use `setItem()` instead of recreating table
- Disable sorting during bulk updates
- Cache formatted strings

### Testing

#### Unit Tests (Rust)
```bash
# Test core functionality
cargo test --lib
```

#### Integration Tests
```bash
# Test with real data
cargo test --lib thd75
```

#### Manual GUI Testing
```bash
# Run application
cargo run --features gui

# Test checklist:
# - Load .img file
# - Edit memory
# - Save file
# - Download from radio (if connected)
# - Upload to radio (if connected)
```

### Troubleshooting

#### Build Issues

**"Qt not found":**
```bash
# Set QTDIR
export QTDIR=/usr/lib/qt5  # Linux
export QTDIR=/usr/local/opt/qt@5  # macOS
set QTDIR=C:\Qt\5.15.2\msvc2019_64  # Windows
```

**"moc not found":**
```bash
# Add Qt bin to PATH
export PATH=$QTDIR/bin:$PATH
```

**`cpp!` macro errors:**
```bash
# Clean build
cargo clean
cargo build --features gui
```

#### Runtime Issues

**"Cannot load library":**
- Linux: Install `libqt5gui5`, `libqt5widgets5`
- Windows: Copy Qt DLLs to binary directory
- macOS: Use `macdeployqt` to bundle Qt frameworks

**Crash on startup:**
- Check Qt version (5.12+ recommended)
- Verify platform plugins installed
- Run with `RUST_BACKTRACE=1` for details

### Future Enhancements

#### Short Term
- ⬜ In-cell editing (double-click to edit)
- ⬜ Context menu (right-click options)
- ⬜ Keyboard shortcuts (Ctrl+S, Ctrl+O, etc.)
- ⬜ Toolbar with common actions

#### Medium Term
- ⬜ Multiple tabs (open multiple files)
- ⬜ Split view (compare memories)
- ⬜ Search/filter toolbar
- ⬜ Memory sorting by column

#### Long Term
- ⬜ Custom themes
- ⬜ Keyboard shortcut customization
- ⬜ Plugin system
- ⬜ Memory templates

### Resources

- **qmetaobject-rs**: https://github.com/woboq/qmetaobject-rs
- **Qt Documentation**: https://doc.qt.io/qt-5/
- **cpp crate**: https://github.com/mystor/rust-cpp
- **CHIRP-RS Examples**: See `src/gui/qt_gui.rs` for working code

### Conclusion

Qt provides the best balance of features, maturity, and user experience for CHIRP-RS. While it requires more setup than pure-Rust solutions, the result is a professional, native-feeling application that users expect from desktop software.

The `qmetaobject-rs` bindings make it practical to use Qt from Rust while maintaining safety and good performance. The FFI boundary is clean and well-defined, making it easy to maintain and extend.
