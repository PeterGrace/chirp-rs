# Change Log

## 2024 - GUI Framework Decision: iced

### Change Summary
**Before:** egui (immediate mode GUI)
**After:** iced (Elm-inspired declarative GUI)

### Rationale

#### Why iced is Better for CHIRP-RS

1. **Architecture Match**
   - iced uses Elm's Model-View-Update (MVU) pattern
   - Clean separation: State → View → Messages → Update
   - Better fits radio programming workflow (state machine for download/upload)
   - More maintainable for complex async operations

2. **Async Integration**
   - Built-in `Command` system for async operations
   - Natural integration with tokio (already using for serial I/O)
   - Download/upload with progress updates is straightforward
   - Example:
     ```rust
     Command::perform(
         download_from_radio(port),
         Message::DownloadComplete
     )
     ```

3. **Type Safety**
   - All UI interactions are strongly-typed messages
   - Compiler helps catch UI logic errors
   - Better refactoring support

4. **State Management**
   - Explicit Model struct with all application state
   - No hidden state in closures or callbacks
   - Easier to serialize/restore app state
   - Better for debugging

5. **Windows Distribution**
   - Single executable (same as egui)
   - Uses wgpu for rendering (GPU-accelerated)
   - Good DPI scaling support
   - No external dependencies

#### Trade-offs

| Aspect | egui | iced |
|--------|------|------|
| **Ease of Use** | Easier - immediate mode is intuitive | Medium - MVU pattern has learning curve |
| **Boilerplate** | Less - inline everything | More - messages for all actions |
| **Async** | Manual integration | Built-in Command system ✓ |
| **State** | Implicit in app | Explicit in Model ✓ |
| **Flexibility** | Very flexible | Structured (good for teams) ✓ |
| **Table Widget** | egui_extras::Table (mature) | Custom or iced_table (newer) |
| **Maturity** | Very mature | Mature (v0.12) |
| **Learning Resources** | Extensive | Growing |

### Implementation Impact

#### No Impact on Previous Phases
- Phases 1-6 are **not affected**
- Core, memmap, formats, bitwise, serial, drivers remain the same
- iced is only used in Phase 7 (GUI)

#### Cargo Configuration
```toml
[features]
default = []
gui = ["dep:iced", "dep:rfd"]
```

**Build Options:**
- `cargo build` - CLI-only (no GUI dependencies)
- `cargo build --features gui` - Full GUI application
- Enables optional builds for server/automation use cases

#### Phase 7 Changes

**Old Plan (egui):**
```rust
// Immediate mode
fn ui(&mut self, ui: &mut egui::Ui) {
    if ui.button("Download").clicked() {
        // Do download inline
        self.download();
    }
}
```

**New Plan (iced):**
```rust
// Elm architecture
enum Message {
    DownloadClicked,
    DownloadProgress(f32),
    DownloadComplete(Result<MemoryMap>),
}

fn update(&mut self, message: Message) -> Command<Message> {
    match message {
        Message::DownloadClicked => {
            Command::perform(
                download_async(),
                Message::DownloadComplete
            )
        }
        Message::DownloadProgress(p) => {
            self.progress = p;
            Command::none()
        }
        Message::DownloadComplete(Ok(mmap)) => {
            self.mmap = mmap;
            Command::none()
        }
    }
}

fn view(&self) -> Element<Message> {
    button("Download")
        .on_press(Message::DownloadClicked)
        .into()
}
```

### Benefits for CHIRP-RS Specifically

1. **Radio Download/Upload Flow**
   - Download: User clicks → Show dialog → Select radio/port → Spawn async download → Update progress → Complete
   - This is naturally modeled as a sequence of messages in iced
   - egui would require manual state machine tracking

2. **Memory Grid Editing**
   - Each cell edit is a typed message
   - Validation happens in update() function
   - View is pure function of state
   - Easier to add undo/redo later

3. **Multi-Step Wizards**
   - Radio selection wizard naturally fits MVU
   - State: WizardStep enum
   - Messages: NextStep, PreviousStep, SelectRadio(String)

4. **Testing**
   - Update logic can be unit tested independently
   - Mock messages to test state transitions
   - View is pure, always produces same output for same state

5. **Future Features**
   - Multiple tabs (multiple files open) - easy with iced
   - Settings panel - separate Model/View/Update
   - Bank editor - another View component

### Timeline Impact

**No change to timeline.**

Phase 7 is still estimated at 2-3 weeks. While iced has more boilerplate than egui, the built-in async support saves time on download/upload implementation, and the structure reduces debugging time.

### Next Steps

1. **Phase 3-6**: Proceed as planned (no GUI dependencies)
2. **Phase 7**:
   - Start with iced tutorials/examples
   - Build simple memory grid
   - Add file operations
   - Implement download/upload dialogs
3. **Phase 8**: Integration and polish

### Resources

- **iced GitHub**: https://github.com/iced-rs/iced
- **iced Book**: https://book.iced.rs/
- **Examples**: https://github.com/iced-rs/iced/tree/master/examples
- **Awesome iced**: https://github.com/iced-rs/awesome-iced

### Decision: APPROVED ✓

Using iced provides a more robust, maintainable foundation for CHIRP-RS's GUI that better handles async operations and complex state management.
