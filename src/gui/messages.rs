// Message types for iced MVU (Model-View-Update) architecture

use crate::core::Memory;
use std::path::PathBuf;

/// Messages that can be sent in the application
#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    NewFile,
    OpenFile,
    FileOpened(Result<(PathBuf, Vec<Memory>), String>),
    SaveFile,
    SaveFileAs,
    FileSaved(Result<(), String>),

    // Radio operations
    DownloadFromRadio,
    UploadToRadio,
    DownloadProgress(usize, usize, String),
    DownloadComplete(Result<Vec<Memory>, String>),
    UploadProgress(usize, usize, String),
    UploadComplete(Result<(), String>),

    // Radio/port selection
    RadioVendorSelected(String),
    RadioModelSelected(String),
    SerialPortSelected(String),
    RefreshSerialPorts,
    PortsRefreshed(Vec<String>),

    // Memory editing
    MemorySelected(u32),
    MemoryFrequencyChanged(u32, String),
    MemoryNameChanged(u32, String),
    MemoryModeChanged(u32, String),
    MemoryDuplexChanged(u32, String),
    MemoryOffsetChanged(u32, String),
    MemoryToneModeChanged(u32, String),
    MemoryRToneChanged(u32, f32),
    MemoryCToneChanged(u32, f32),
    MemoryPowerChanged(u32, String),
    MemoryDelete(u32),

    // Dialog control
    CloseDialog,

    // Error handling
    ShowError(String),
    DismissError,
}
