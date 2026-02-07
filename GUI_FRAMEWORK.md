# GUI Framework: iced

## Decision: Using iced instead of egui

### Why iced?

**iced** is a cross-platform GUI library for Rust inspired by Elm. It provides:

1. **Elm Architecture (MVU Pattern)**
   - Model: Application state
   - View: Pure function that renders the model
   - Update: Handles messages and updates the model
   - Clean separation of concerns

2. **Excellent Windows Support**
   - Native rendering via wgpu
   - Good performance
   - Proper DPI scaling

3. **Built-in Widgets**
   - `iced::widget::scrollable` - Scrollable containers
   - `iced::widget::column/row` - Layout primitives
   - `iced::widget::text_input` - Text editing
   - `iced::widget::button` - Buttons
   - `iced::widget::pick_list` - Dropdowns (perfect for radio/port selection)
   - `iced::widget::progress_bar` - Progress indicators
   - Custom widgets can be built

4. **Table/Grid Support**
   - Can build custom table widget
   - `iced_table` crate available
   - Community has examples of data grids

5. **Async Support**
   - `Command` system for async operations
   - Natural integration with tokio for serial I/O
   - Can run async tasks and update UI via messages

6. **Single Binary Distribution**
   - Easy to package for Windows
   - No external GUI dependencies

### iced vs egui Comparison

| Feature | iced | egui |
|---------|------|------|
| Architecture | Elm/MVU (structured) | Immediate mode (flexible) |
| Async | Built-in Command system | Manual integration |
| State Management | Explicit Model | Implicit in app |
| Learning Curve | Medium (MVU pattern) | Easy (imperative) |
| Table Widgets | Custom/iced_table | egui_extras::Table |
| Windows Support | ✅ Excellent | ✅ Excellent |
| Maturity | Stable | Very Stable |
| Community | Growing | Large |

### Implementation Approach

#### 1. Application Structure

```rust
use iced::{Application, Command, Element, Settings};

struct ChirpApp {
    // Model
    memories: Vec<Memory>,
    selected_radio: Option<String>,
    serial_ports: Vec<String>,
    status: String,
    // ... other state
}

#[derive(Debug, Clone)]
enum Message {
    // File operations
    OpenFile,
    SaveFile,
    FileLoaded(Result<(MemoryMap, Metadata), String>),

    // Radio operations
    DownloadFromRadio,
    UploadToRadio,
    RadioSelected(String),
    PortSelected(String),
    DownloadProgress(f32),
    DownloadComplete(Result<MemoryMap, String>),

    // Memory editing
    MemorySelected(usize),
    FrequencyChanged(String),
    NameChanged(String),
    // ... other fields

    // UI events
    MenuAction(MenuAction),
}

impl Application for ChirpApp {
    type Message = Message;
    type Executor = iced::executor::Default;
    type Flags = ();
    type Theme = iced::Theme;

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        // Initialize app state
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("CHIRP-RS")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::OpenFile => {
                // Spawn async file picker
                Command::perform(
                    pick_file(),
                    Message::FileLoaded
                )
            }
            Message::FileLoaded(Ok((mmap, metadata))) => {
                // Load memories from mmap
                // Update UI state
                Command::none()
            }
            Message::DownloadFromRadio => {
                // Spawn async download task
                Command::perform(
                    download_from_radio(self.selected_radio.clone()),
                    Message::DownloadComplete
                )
            }
            // ... handle other messages
            _ => Command::none()
        }
    }

    fn view(&self) -> Element<Message> {
        // Build UI from current state
        let menu = menu_bar();
        let memory_grid = memory_grid_view(&self.memories);
        let status_bar = status_bar(&self.status);

        column![menu, memory_grid, status_bar].into()
    }
}
```

#### 2. Memory Grid Widget

Options:
1. **Custom Table Widget**: Build our own using `column` + `row` + `scrollable`
2. **iced_table crate**: Community-maintained table widget
3. **iced_aw (Awesome Widgets)**: Additional widget library

For MVP, we'll start with a simple custom table:

```rust
fn memory_grid_view(memories: &[Memory]) -> Element<Message> {
    let header = row![
        text("Num").width(50),
        text("Frequency").width(100),
        text("Name").width(150),
        text("Duplex").width(60),
        text("Offset").width(100),
        text("Mode").width(60),
        // ... more columns
    ];

    let rows = memories.iter().enumerate().map(|(idx, mem)| {
        memory_row(idx, mem)
    }).collect();

    scrollable(
        column![header]
            .push(rows)
    ).into()
}

fn memory_row(idx: usize, mem: &Memory) -> Element<Message> {
    row![
        text(mem.number).width(50),
        text(Memory::format_freq(mem.freq)).width(100),
        text_input("", &mem.name)
            .on_input(move |s| Message::NameChanged(idx, s))
            .width(150),
        pick_list(DUPLEXES, Some(&mem.duplex), move |d| {
            Message::DuplexChanged(idx, d)
        }).width(60),
        // ... more columns
    ].into()
}
```

#### 3. Async Operations

```rust
async fn download_from_radio(
    radio_name: String,
    port: String,
) -> Result<MemoryMap, String> {
    // Open serial port
    let port = serialport::new(&port, 9600)
        .open_async()
        .map_err(|e| e.to_string())?;

    // Get driver
    let driver = get_driver(&radio_name)?;

    // Perform download with progress updates
    driver.sync_in(port).await
}

// In update():
Message::DownloadFromRadio => {
    Command::perform(
        download_from_radio(
            self.selected_radio.clone().unwrap(),
            self.selected_port.clone().unwrap(),
        ),
        Message::DownloadComplete,
    )
}
```

#### 4. File Dialogs

```rust
async fn pick_file() -> Result<(MemoryMap, Metadata), String> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("CHIRP Image", &["img"])
        .pick_file()
        .await
        .ok_or("No file selected")?;

    let path = file.path();
    load_img(path).map_err(|e| e.to_string())
}
```

### Dependencies for Phase 7

```toml
[dependencies]
iced = { version = "0.12", features = ["tokio", "advanced"] }
rfd = "0.14"  # Native file dialogs
# iced_table = "0.1"  # Optional if we use community table
# iced_aw = "0.9"  # Optional for additional widgets
```

### MVP GUI Features

#### Must Have:
- [x] Memory grid (display all channels)
- [x] Edit frequency, name, tone modes, duplex, offset, power
- [x] File menu: New, Open, Save, Save As, Quit
- [x] Radio menu: Download from Radio, Upload to Radio
- [x] Download/Upload dialog with progress bar
- [x] Radio vendor/model selection
- [x] Serial port selection
- [x] Basic error dialogs

#### Nice to Have (Post-MVP):
- [ ] Cell editing with validation feedback
- [ ] Keyboard navigation (arrow keys, tab)
- [ ] Copy/paste rows
- [ ] Undo/redo
- [ ] Search/filter
- [ ] Column sorting
- [ ] Resizable columns

### Implementation Order (Phase 7)

1. **Basic iced app skeleton** (1-2 days)
   - Window creation
   - Menu bar
   - Status bar
   - Basic layout

2. **Memory grid display** (2-3 days)
   - Read-only table showing memories
   - Scrolling
   - Column headers
   - Basic formatting

3. **Memory editing** (2-3 days)
   - Text inputs for frequency, name
   - Dropdowns for mode, duplex, tone mode
   - Number inputs for tones, offset
   - Validation on change

4. **File operations** (1-2 days)
   - Open file dialog
   - Save file dialog
   - Load/save integration with formats module

5. **Radio dialogs** (3-4 days)
   - Download dialog
   - Upload dialog
   - Radio selection (vendor → model)
   - Port selection (auto-detect)
   - Progress bar
   - Cancel button

6. **Error handling** (1 day)
   - Modal error dialogs
   - Status messages
   - Validation errors

7. **Integration & polish** (2-3 days)
   - Wire everything together
   - Test end-to-end workflows
   - UX improvements

**Total Phase 7 Estimate:** 12-18 days (2-3 weeks)

### Resources

- **iced docs**: https://docs.rs/iced/
- **iced examples**: https://github.com/iced-rs/iced/tree/master/examples
- **iced_table**: https://github.com/iced-rs/iced_table
- **iced_aw**: https://github.com/iced-rs/iced_aw

### Advantages of iced for CHIRP

1. **Type-Safe Messages**: All interactions are strongly typed
2. **Testable**: View is a pure function, update logic is isolated
3. **Async-Native**: Commands integrate naturally with tokio
4. **Memory-Efficient**: Updates only what changed
5. **Windows-Ready**: Single executable, no DLL dependencies
6. **Maintainable**: Clear MVU pattern scales well

The iced framework will provide a clean, maintainable GUI foundation that integrates well with our async serial communication and file I/O systems.
