// Main CHIRP-RS application state and update logic
// Uses iced's Elm-like MVU (Model-View-Update) architecture

use crate::core::Memory;
use crate::drivers::{list_drivers, DriverInfo};
use crate::formats::{load_img, save_img, Metadata};
use crate::gui::{dialogs, messages::Message, radio_ops};
use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::{Command, Element, Length, Theme};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Application state
pub struct ChirpApp {
    // File state
    current_file: Option<PathBuf>,
    memories: Vec<Memory>,
    is_modified: bool,

    // Radio state
    available_vendors: Vec<String>,
    available_models: HashMap<String, Vec<DriverInfo>>,
    selected_vendor: Option<String>,
    selected_model: Option<String>,
    available_ports: Vec<String>,
    selected_port: Option<String>,

    // UI state
    selected_memory: Option<u32>,
    show_download_dialog: bool,
    show_upload_dialog: bool,
    show_error_dialog: bool,
    error_message: String,
    operation_progress: Option<(usize, usize, String)>,

    // Editing state
    editing_cell: Option<(u32, String)>, // (memory_number, field_name)
}

impl Default for ChirpApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ChirpApp {
    /// Create sample memories for testing
    fn create_sample_memories() -> Vec<Memory> {
        vec![
            {
                let mut mem = Memory::new(1);
                mem.freq = 146_520_000; // 146.520 MHz
                mem.name = "Simplex".to_string();
                mem.mode = "FM".to_string();
                mem
            },
            {
                let mut mem = Memory::new(2);
                mem.freq = 146_940_000; // 146.940 MHz
                mem.name = "W6CX Rpt".to_string();
                mem.mode = "FM".to_string();
                mem.duplex = "-".to_string();
                mem.offset = 600_000; // 600 kHz
                mem.rtone = 100.0;
                mem
            },
            {
                let mut mem = Memory::new(3);
                mem.freq = 147_330_000; // 147.330 MHz
                mem.name = "N6NFI Rpt".to_string();
                mem.mode = "FM".to_string();
                mem.duplex = "+".to_string();
                mem.offset = 600_000;
                mem.ctone = 88.5;
                mem
            },
        ]
    }
}

impl ChirpApp {
    /// Create a new CHIRP application
    pub fn new() -> Self {
        // Initialize driver registry (must be called once at startup)
        crate::drivers::init_drivers();

        // Get available radio drivers
        let drivers = list_drivers();
        tracing::debug!("Found {} drivers after init", drivers.len());
        for driver in &drivers {
            tracing::debug!("  - {} {}", driver.vendor, driver.model);
        }

        let mut vendors_map: HashMap<String, Vec<DriverInfo>> = HashMap::new();

        for driver in drivers {
            vendors_map
                .entry(driver.vendor.clone())
                .or_insert_with(Vec::new)
                .push(driver);
        }

        let vendors: Vec<String> = vendors_map.keys().cloned().collect();
        tracing::debug!("Vendors list: {:?}", vendors);

        Self {
            current_file: None,
            memories: Vec::new(),
            is_modified: false,
            available_vendors: vendors,
            available_models: vendors_map,
            selected_vendor: None,
            selected_model: None,
            available_ports: Vec::new(),
            selected_port: None,
            selected_memory: None,
            show_download_dialog: false,
            show_upload_dialog: false,
            show_error_dialog: false,
            error_message: String::new(),
            operation_progress: None,
            editing_cell: None,
        }
    }

    /// Get the window title
    pub fn title(&self) -> String {
        let filename = self
            .current_file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled");

        let modified = if self.is_modified { "*" } else { "" };

        format!("CHIRP-RS - {}{}", filename, modified)
    }

    /// Update application state based on messages
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::NewFile => {
                self.current_file = None;
                self.memories.clear();
                self.is_modified = false;
                self.selected_memory = None;
                Command::none()
            }

            Message::OpenFile => {
                // Spawn file dialog task
                Command::perform(
                    async {
                        let file = rfd::AsyncFileDialog::new()
                            .add_filter("CHIRP Image", &["img"])
                            .pick_file()
                            .await;

                        if let Some(file) = file {
                            let path = file.path().to_path_buf();

                            // Load the .img file
                            match load_img(&path) {
                                Ok((mmap, metadata)) => {
                                    // TODO: Parse memories from memmap
                                    // For now, return empty list with path
                                    Ok((path, Vec::new()))
                                }
                                Err(e) => Err(format!("Failed to load file: {}", e)),
                            }
                        } else {
                            Err("No file selected".to_string())
                        }
                    },
                    Message::FileOpened,
                )
            }

            Message::FileOpened(result) => {
                match result {
                    Ok((path, _memories)) => {
                        self.current_file = Some(path);
                        // TODO: Parse actual memories from memmap
                        // For now, use sample memories for testing
                        self.memories = Self::create_sample_memories();
                        self.is_modified = false;
                    }
                    Err(err) => {
                        self.error_message = format!("Failed to open file: {}", err);
                        self.show_error_dialog = true;
                    }
                }
                Command::none()
            }

            Message::SaveFile => {
                if let Some(path) = &self.current_file {
                    let path = path.clone();
                    let memories = self.memories.clone();

                    Command::perform(
                        async move {
                            // TODO: Convert memories to memmap and save
                            // For now, return success
                            Ok(())
                        },
                        Message::FileSaved,
                    )
                } else {
                    // No current file, show save dialog
                    self.update(Message::SaveFileAs)
                }
            }

            Message::SaveFileAs => {
                let memories = self.memories.clone();

                Command::perform(
                    async move {
                        let file = rfd::AsyncFileDialog::new()
                            .add_filter("CHIRP Image", &["img"])
                            .save_file()
                            .await;

                        if let Some(file) = file {
                            let path = file.path().to_path_buf();
                            // TODO: Convert memories to memmap and save
                            // For now, return success
                            Ok(())
                        } else {
                            Err("No file selected".to_string())
                        }
                    },
                    Message::FileSaved,
                )
            }

            Message::FileSaved(result) => {
                match result {
                    Ok(()) => {
                        self.is_modified = false;
                    }
                    Err(err) => {
                        self.error_message = format!("Failed to save file: {}", err);
                        self.show_error_dialog = true;
                    }
                }
                Command::none()
            }

            Message::DownloadFromRadio => {
                // If dialog is already open and all selections are made, start download
                if self.show_download_dialog
                    && self.selected_vendor.is_some()
                    && self.selected_model.is_some()
                    && self.selected_port.is_some()
                {
                    tracing::info!("USER CLICKED DOWNLOAD - Starting radio communication");
                    let port = self.selected_port.clone().unwrap();
                    let vendor = self.selected_vendor.clone().unwrap();
                    let model = self.selected_model.clone().unwrap();

                    // Start download operation
                    Command::perform(
                        async move {
                            let progress_fn: Arc<dyn Fn(usize, usize, String) + Send + Sync> =
                                Arc::new(|_current, _total, _msg| {
                                    // Progress updates are sent via callback in real implementation
                                    // For now, we can't easily send messages from here
                                });

                            radio_ops::download_from_radio(port, vendor, model, progress_fn).await
                        },
                        Message::DownloadComplete,
                    )
                } else {
                    // Show dialog and refresh ports
                    self.show_download_dialog = true;
                    self.update(Message::RefreshSerialPorts)
                }
            }

            Message::UploadToRadio => {
                // If dialog is already open and port is selected, start upload
                if self.show_upload_dialog && self.selected_port.is_some() && !self.memories.is_empty() {
                    let port = self.selected_port.clone().unwrap();
                    let memories = self.memories.clone();
                    let vendor = self.selected_vendor.clone().unwrap_or_else(|| "Unknown".to_string());
                    let model = self.selected_model.clone().unwrap_or_else(|| "Unknown".to_string());

                    // Start upload operation
                    Command::perform(
                        async move {
                            let progress_fn: Arc<dyn Fn(usize, usize, String) + Send + Sync> =
                                Arc::new(|_current, _total, _msg| {
                                    // Progress updates in real implementation
                                });

                            radio_ops::upload_to_radio(port, memories, vendor, model, progress_fn).await
                        },
                        Message::UploadComplete,
                    )
                } else {
                    // Show dialog and refresh ports
                    self.show_upload_dialog = true;
                    self.update(Message::RefreshSerialPorts)
                }
            }

            Message::DownloadProgress(current, total, msg) => {
                self.operation_progress = Some((current, total, msg));
                Command::none()
            }

            Message::DownloadComplete(result) => {
                self.operation_progress = None;
                self.show_download_dialog = false;
                match result {
                    Ok(memories) => {
                        tracing::debug!("Download complete, got {} memories", memories.len());
                        // Use actual downloaded memories
                        self.memories = memories;
                        self.is_modified = false;
                        self.current_file = None; // Clear file path since this is from radio
                    }
                    Err(err) => {
                        tracing::debug!("Download failed: {}", err);
                        self.error_message = format!("Download failed: {}", err);
                        self.show_error_dialog = true;
                    }
                }
                Command::none()
            }

            Message::UploadProgress(current, total, msg) => {
                self.operation_progress = Some((current, total, msg));
                Command::none()
            }

            Message::UploadComplete(result) => {
                self.operation_progress = None;
                self.show_upload_dialog = false;
                match result {
                    Ok(()) => {
                        // Success
                    }
                    Err(err) => {
                        self.error_message = format!("Upload failed: {}", err);
                        self.show_error_dialog = true;
                    }
                }
                Command::none()
            }

            Message::RadioVendorSelected(vendor) => {
                self.selected_vendor = Some(vendor.clone());
                self.selected_model = None;
                Command::none()
            }

            Message::RadioModelSelected(model) => {
                self.selected_model = Some(model);
                Command::none()
            }

            Message::SerialPortSelected(port) => {
                self.selected_port = Some(port);
                Command::none()
            }

            Message::RefreshSerialPorts => {
                Command::perform(
                    async {
                        // Get actual serial ports from system
                        match serialport::available_ports() {
                            Ok(ports) => ports
                                .into_iter()
                                .map(|p| p.port_name)
                                .collect(),
                            Err(_) => Vec::new(),
                        }
                    },
                    Message::PortsRefreshed,
                )
            }

            Message::PortsRefreshed(ports) => {
                self.available_ports = ports;
                Command::none()
            }

            Message::MemorySelected(number) => {
                self.selected_memory = Some(number);
                Command::none()
            }

            Message::MemoryFrequencyChanged(number, freq_str) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    if let Ok(freq) = Memory::parse_freq(&freq_str) {
                        mem.freq = freq;
                        self.is_modified = true;
                    }
                }
                Command::none()
            }

            Message::MemoryNameChanged(number, name) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.name = name;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryModeChanged(number, mode) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.mode = mode;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryDuplexChanged(number, duplex) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.duplex = duplex;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryOffsetChanged(number, offset_str) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    if let Ok(offset) = Memory::parse_freq(&offset_str) {
                        mem.offset = offset;
                        self.is_modified = true;
                    }
                }
                Command::none()
            }

            Message::MemoryToneModeChanged(number, tmode) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.tmode = tmode;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryRToneChanged(number, rtone) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.rtone = rtone;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryCToneChanged(number, ctone) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.ctone = ctone;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryPowerChanged(number, power_str) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    // TODO: Parse power level
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::MemoryDelete(number) => {
                if let Some(mem) = self.memories.iter_mut().find(|m| m.number == number) {
                    mem.empty = true;
                    self.is_modified = true;
                }
                Command::none()
            }

            Message::CloseDialog => {
                self.show_download_dialog = false;
                self.show_upload_dialog = false;
                self.operation_progress = None;
                Command::none()
            }

            Message::ShowError(msg) => {
                self.error_message = msg;
                self.show_error_dialog = true;
                Command::none()
            }

            Message::DismissError => {
                self.show_error_dialog = false;
                self.error_message.clear();
                Command::none()
            }
        }
    }

    /// Create the view (UI layout)
    pub fn view(&self) -> Element<Message> {
        // Main content area
        let content = if self.memories.is_empty() {
            // Show welcome message when no file is loaded
            container(
                column![
                    text("Welcome to CHIRP-RS").size(32),
                    text("Open a file or download from radio to get started"),
                    button("Open File").on_press(Message::OpenFile),
                    button("Download from Radio").on_press(Message::DownloadFromRadio),
                ]
                .spacing(20)
                .align_items(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
        } else {
            // Show memory grid
            self.view_memory_grid()
        };

        // Wrap in container with menu bar
        let main_view = column![self.view_menu_bar(), content]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);

        // Add overlays for dialogs
        if self.show_download_dialog {
            self.view_download_dialog(main_view.into())
        } else if self.show_upload_dialog {
            self.view_upload_dialog(main_view.into())
        } else if self.show_error_dialog {
            self.view_error_dialog(main_view.into())
        } else {
            main_view.into()
        }
    }

    /// Create menu bar
    fn view_menu_bar(&self) -> Element<Message> {
        row![
            button("File").on_press(Message::NewFile),
            button("Open").on_press(Message::OpenFile),
            button("Save").on_press(Message::SaveFile),
            button("Download").on_press(Message::DownloadFromRadio),
            button("Upload").on_press(Message::UploadToRadio),
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    /// Create memory grid view
    fn view_memory_grid(&self) -> Element<Message> {
        let mut grid = Column::new().spacing(5).padding(10);

        // Header row
        let header = row![
            text("Loc").width(60),
            text("Frequency").width(120),
            text("Name").width(150),
            text("Duplex").width(80),
            text("Offset").width(100),
            text("Mode").width(80),
            text("ToneMode").width(80),
            text("Tone").width(80),
            text("Power").width(80),
            text("URCALL").width(100),
            text("RPT1").width(100),
            text("RPT2").width(100),
        ]
        .spacing(5);

        grid = grid.push(header);

        // Memory rows
        for mem in &self.memories {
            if !mem.empty {
                // Show tone or D-STAR fields depending on mode
                let (tone_mode, tone, urcall, rpt1, rpt2) = if mem.mode == "DV" {
                    // For DV mode, show D-STAR fields
                    (String::new(), String::new(), mem.dv_urcall.clone(), mem.dv_rpt1call.clone(), mem.dv_rpt2call.clone())
                } else {
                    // For non-DV modes, show tone info
                    (mem.tmode.clone(), format!("{:.1}", mem.rtone), String::new(), String::new(), String::new())
                };

                let row_elem = row![
                    text(mem.number.to_string()).width(60),
                    text(Memory::format_freq(mem.freq)).width(120),
                    text(&mem.name).width(150),
                    text(&mem.duplex).width(80),
                    text(Memory::format_freq(mem.offset)).width(100),
                    text(&mem.mode).width(80),
                    text(&tone_mode).width(80),
                    text(&tone).width(80),
                    text(mem.power.as_ref().map(|p| p.label()).unwrap_or("")).width(80),
                    text(&urcall).width(100),
                    text(&rpt1).width(100),
                    text(&rpt2).width(100),
                ]
                .spacing(5);

                grid = grid.push(row_elem);
            }
        }

        scrollable(grid).height(Length::Fill).into()
    }

    /// Create download dialog
    fn view_download_dialog<'a>(&self, background: Element<'a, Message>) -> Element<'a, Message> {
        tracing::debug!("view_download_dialog called");
        tracing::debug!("  available_vendors: {:?}", self.available_vendors);
        tracing::debug!("  selected_vendor: {:?}", self.selected_vendor);

        let models: Vec<String> = if let Some(vendor) = &self.selected_vendor {
            self.available_models
                .get(vendor)
                .map(|drivers| drivers.iter().map(|d| d.model.clone()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        tracing::debug!("  models: {:?}", models);

        let progress = self.operation_progress.as_ref().map(|(c, t, m)| (*c, *t, m.clone()));

        dialogs::modal(
            background,
            dialogs::download_dialog(
                self.available_vendors.clone(),
                models,
                self.available_ports.clone(),
                self.selected_vendor.clone(),
                self.selected_model.clone(),
                self.selected_port.clone(),
                progress,
            ),
        )
    }

    /// Create upload dialog
    fn view_upload_dialog<'a>(&self, background: Element<'a, Message>) -> Element<'a, Message> {
        let progress = self.operation_progress.as_ref().map(|(c, t, m)| (*c, *t, m.clone()));

        dialogs::modal(
            background,
            dialogs::upload_dialog(
                self.available_ports.clone(),
                self.selected_port.clone(),
                progress,
            ),
        )
    }

    /// Create error dialog
    fn view_error_dialog<'a>(&self, background: Element<'a, Message>) -> Element<'a, Message> {
        dialogs::modal(background, dialogs::error_dialog(self.error_message.clone()))
    }
}

impl iced::Application for ChirpApp {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Self::new(), Command::none())
    }

    fn title(&self) -> String {
        self.title()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        self.update(message)
    }

    fn view(&self) -> Element<Self::Message> {
        self.view()
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}
