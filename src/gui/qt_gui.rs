//! Qt-based GUI for CHIRP-RS
//! Provides a traditional desktop application experience using Qt Widgets

use crate::core::Memory;
use crate::drivers::{get_driver, init_drivers, list_drivers};
use crate::formats::{load_img, save_img};
use cpp::cpp;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

cpp! {{
    #include <QtWidgets/QApplication>
    #include <QtWidgets/QMainWindow>
    #include <QtWidgets/QMenuBar>
    #include <QtWidgets/QMenu>
    #include <QtGui/QAction>
    #include <QtWidgets/QTableWidget>
    #include <QtWidgets/QTableWidgetItem>
    #include <QtWidgets/QHeaderView>
    #include <QtWidgets/QVBoxLayout>
    #include <QtWidgets/QHBoxLayout>
    #include <QtWidgets/QWidget>
    #include <QtWidgets/QMessageBox>
    #include <QtWidgets/QPushButton>
    #include <QtWidgets/QFileDialog>
    #include <QtWidgets/QDialog>
    #include <QtWidgets/QDialogButtonBox>
    #include <QtWidgets/QFormLayout>
    #include <QtWidgets/QLineEdit>
    #include <QtWidgets/QComboBox>
    #include <QtWidgets/QProgressDialog>
    #include <QtCore/QString>
    #include <QtCore/QStringList>
    #include <QtCore/QTimer>

    // C-compatible row data structure
    struct RowData {
        const char* loc;
        const char* freq;
        const char* name;
        const char* duplex;
        const char* offset;
        const char* mode;
        const char* tmode;
        const char* tone;
        const char* power;
        const char* urcall;
        const char* rpt1;
        const char* rpt2;
    };

    // Forward declare Memory struct (we only need pointer to it)
    struct Memory;

    // Declare Rust FFI functions
    extern "C" {
        size_t get_memory_count();
        RowData get_memory_row(size_t row);
        const char* load_file(const char* path);
        const char* save_file(const char* path);
        void new_file();
        const char* get_current_filename();
        void free_error_message(const char* msg);
        const Memory* get_memory_by_row(size_t row);
        const char* update_memory(size_t row, uint64_t freq, const char* name,
                                 const char* duplex, uint64_t offset, const char* mode,
                                 const char* tmode, float rtone, float ctone);
        const char* get_vendors();
        const char* get_models_for_vendor(const char* vendor);
        const char* get_serial_ports();
        const char* get_ctcss_tones();
        const char* download_from_radio(const char* vendor, const char* model, const char* port);
        void start_download_async(const char* vendor, const char* model, const char* port);
        int get_download_progress(int* out_current, int* out_total, const char** out_message);
        int is_download_complete();
        const char* get_download_result();
    }

    // Helper function to refresh table from Rust data
    void refreshTable(QTableWidget* table) {
        size_t row_count = get_memory_count();
        table->setRowCount(row_count);

        for (size_t row = 0; row < row_count; ++row) {
            RowData data = get_memory_row(row);
            table->setItem(row, 0, new QTableWidgetItem(QString::fromUtf8(data.loc)));
            table->setItem(row, 1, new QTableWidgetItem(QString::fromUtf8(data.freq)));
            table->setItem(row, 2, new QTableWidgetItem(QString::fromUtf8(data.name)));
            table->setItem(row, 3, new QTableWidgetItem(QString::fromUtf8(data.duplex)));
            table->setItem(row, 4, new QTableWidgetItem(QString::fromUtf8(data.offset)));
            table->setItem(row, 5, new QTableWidgetItem(QString::fromUtf8(data.mode)));
            table->setItem(row, 6, new QTableWidgetItem(QString::fromUtf8(data.tmode)));
            table->setItem(row, 7, new QTableWidgetItem(QString::fromUtf8(data.tone)));
            table->setItem(row, 8, new QTableWidgetItem(QString::fromUtf8(data.power)));
            table->setItem(row, 9, new QTableWidgetItem(QString::fromUtf8(data.urcall)));
            table->setItem(row, 10, new QTableWidgetItem(QString::fromUtf8(data.rpt1)));
            table->setItem(row, 11, new QTableWidgetItem(QString::fromUtf8(data.rpt2)));
        }

        // Force table to update display
        table->viewport()->update();
    }

    // Helper function to show download dialog
    void showDownloadDialog(QWidget* parent, QTableWidget* table) {
        QDialog dialog(parent);
        dialog.setWindowTitle("Download from Radio");
        QFormLayout* layout = new QFormLayout(&dialog);

        // Get vendors
        QString vendorsStr = QString::fromUtf8(get_vendors());
        QStringList vendors = vendorsStr.split(",", Qt::SkipEmptyParts);

        // Create vendor dropdown
        QComboBox* vendorCombo = new QComboBox();
        vendorCombo->addItems(vendors);

        // Create model dropdown (populated when vendor changes)
        QComboBox* modelCombo = new QComboBox();

        // Create port dropdown
        QComboBox* portCombo = new QComboBox();
        QString portsStr = QString::fromUtf8(get_serial_ports());
        QStringList ports = portsStr.split(",", Qt::SkipEmptyParts);
        if (ports.isEmpty()) {
            portCombo->addItem("(No ports found)");
        } else {
            portCombo->addItems(ports);
        }

        // Refresh ports button
        QPushButton* refreshBtn = new QPushButton("Refresh");
        QObject::connect(refreshBtn, &QPushButton::clicked, [portCombo, parent]() {
            QString portsStr = QString::fromUtf8(get_serial_ports());
            QStringList ports = portsStr.split(",", Qt::SkipEmptyParts);
            portCombo->clear();
            if (ports.isEmpty()) {
                portCombo->addItem("(No ports found)");
                QMessageBox::warning(parent, "No Serial Ports Found",
                    "No serial ports were detected.\n\n"
                    "Please check:\n"
                    "• Radio is connected via USB\n"
                    "• USB drivers are installed\n"
                    "• Radio is powered on");
            } else {
                portCombo->addItems(ports);
            }
        });

        // Update models when vendor changes
        QObject::connect(vendorCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            [vendorCombo, modelCombo]() {
                QString vendor = vendorCombo->currentText();
                QString modelsStr = QString::fromUtf8(get_models_for_vendor(vendor.toUtf8().constData()));
                QStringList models = modelsStr.split(",", Qt::SkipEmptyParts);
                modelCombo->clear();
                modelCombo->addItems(models);
            });

        // Trigger initial model population
        if (vendors.count() > 0) {
            vendorCombo->setCurrentIndex(0);
            QString vendor = vendorCombo->currentText();
            QString modelsStr = QString::fromUtf8(get_models_for_vendor(vendor.toUtf8().constData()));
            QStringList models = modelsStr.split(",", Qt::SkipEmptyParts);
            modelCombo->addItems(models);
        }

        // Add fields to form
        layout->addRow("Vendor:", vendorCombo);
        layout->addRow("Model:", modelCombo);

        QHBoxLayout* portLayout = new QHBoxLayout();
        portLayout->addWidget(portCombo);
        portLayout->addWidget(refreshBtn);
        layout->addRow("Port:", portLayout);

        // Add buttons
        QDialogButtonBox* buttons = new QDialogButtonBox(
            QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
        buttons->button(QDialogButtonBox::Ok)->setText("Download");
        QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        layout->addRow(buttons);

        // Show dialog
        if (dialog.exec() == QDialog::Accepted) {
            QString vendor = vendorCombo->currentText();
            QString model = modelCombo->currentText();
            QString port = portCombo->currentText();

            if (vendor.isEmpty() || model.isEmpty() || port.isEmpty()) {
                QMessageBox::warning(parent, "Invalid Selection",
                    "Please select vendor, model, and port");
                return;
            }

            // Create progress dialog
            QProgressDialog* progressDlg = new QProgressDialog(
                "Initializing...", "Cancel", 0, 100, parent);
            progressDlg->setWindowTitle("Downloading from Radio");
            progressDlg->setWindowModality(Qt::WindowModal);
            progressDlg->setMinimumDuration(0);  // Show immediately
            progressDlg->setAutoClose(false);     // Don't auto-close
            progressDlg->setAutoReset(false);     // Don't auto-reset
            progressDlg->setValue(0);
            progressDlg->show();
            progressDlg->raise();  // Bring to front
            progressDlg->activateWindow();  // Activate window
            QApplication::processEvents();  // Force immediate render

            // Start async download
            start_download_async(
                vendor.toUtf8().constData(),
                model.toUtf8().constData(),
                port.toUtf8().constData()
            );

            // Create timer to poll progress (give it parent so it stays alive)
            QTimer* timer = new QTimer(parent);
            timer->setInterval(100); // Poll every 100ms

            QObject::connect(timer, &QTimer::timeout, [=]() mutable {
                // Check if user cancelled
                if (progressDlg->wasCanceled()) {
                    timer->stop();
                    progressDlg->deleteLater();
                    timer->deleteLater();
                    // TODO: Add ability to cancel download on Rust side
                    return;
                }

                // Get current progress
                int current = 0;
                int total = 100;
                const char* message = nullptr;
                int percentage = get_download_progress(&current, &total, &message);

                if (percentage >= 0) {
                    // Still in progress
                    progressDlg->setMaximum(total);
                    progressDlg->setValue(current);
                    if (message) {
                        progressDlg->setLabelText(QString::fromUtf8(message));
                    }
                    progressDlg->show();  // Ensure it stays visible
                }

                // Check if complete
                int complete = is_download_complete();
                if (complete == 1) {
                    timer->stop();
                    progressDlg->close();

                    // Get result
                    const char* error = get_download_result();
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(parent, "Download Failed",
                            QString("Failed to download memories from radio.\n\n"
                                   "Error: %1\n\n"
                                   "Please check:\n"
                                   "• Radio is connected and powered on\n"
                                   "• Correct serial port is selected\n"
                                   "• No other program is using the radio")
                            .arg(errorMsg));
                        free_error_message(error);
                    } else {
                        refreshTable(table);
                        QMessageBox::information(parent, "Download Complete",
                            QString("Successfully downloaded %1 memories from radio")
                                .arg(get_memory_count()));
                    }

                    progressDlg->deleteLater();
                    timer->deleteLater();
                }
            });

            timer->start();
        }
    }

    // Helper function to show edit dialog for a memory
    void showEditDialog(QWidget* parent, QTableWidget* table, int row) {
        // Get current row data
        RowData data = get_memory_row(row);

        // Create dialog
        QDialog dialog(parent);
        dialog.setWindowTitle(QString("Edit Memory %1").arg(QString::fromUtf8(data.loc)));
        QFormLayout* layout = new QFormLayout(&dialog);

        // Create input fields
        QLineEdit* freqEdit = new QLineEdit(QString::fromUtf8(data.freq));
        QLineEdit* nameEdit = new QLineEdit(QString::fromUtf8(data.name));

        QComboBox* duplexCombo = new QComboBox();
        duplexCombo->addItems({"", "+", "-", "split", "off"});
        duplexCombo->setCurrentText(QString::fromUtf8(data.duplex));

        QLineEdit* offsetEdit = new QLineEdit(QString::fromUtf8(data.offset));

        QComboBox* modeCombo = new QComboBox();
        modeCombo->addItems({"FM", "NFM", "AM", "DV", "USB", "LSB"});
        modeCombo->setCurrentText(QString::fromUtf8(data.mode));

        QComboBox* tmodeCombo = new QComboBox();
        tmodeCombo->addItems({"", "Tone", "TSQL", "DTCS", "Cross"});
        tmodeCombo->setCurrentText(QString::fromUtf8(data.tmode));

        // Get standard CTCSS tones
        QString tonesStr = QString::fromUtf8(get_ctcss_tones());
        QStringList tones = tonesStr.split(",", Qt::SkipEmptyParts);

        QComboBox* rtoneCombo = new QComboBox();
        rtoneCombo->addItems(tones);
        rtoneCombo->setEditable(false);

        QComboBox* ctoneCombo = new QComboBox();
        ctoneCombo->addItems(tones);
        ctoneCombo->setEditable(false);

        // Parse current tone value and select in dropdown
        QString toneStr = QString::fromUtf8(data.tone);
        if (!toneStr.isEmpty()) {
            rtoneCombo->setCurrentText(toneStr);
            ctoneCombo->setCurrentText(toneStr);
        } else {
            // Default to 88.5 Hz
            rtoneCombo->setCurrentText("88.5");
            ctoneCombo->setCurrentText("88.5");
        }

        // Add fields to form
        layout->addRow("Frequency (MHz):", freqEdit);
        layout->addRow("Name:", nameEdit);
        layout->addRow("Duplex:", duplexCombo);
        layout->addRow("Offset (MHz):", offsetEdit);
        layout->addRow("Mode:", modeCombo);
        layout->addRow("Tone Mode:", tmodeCombo);
        layout->addRow("TX Tone (Hz):", rtoneCombo);
        layout->addRow("RX Tone (Hz):", ctoneCombo);

        // Add buttons
        QDialogButtonBox* buttons = new QDialogButtonBox(
            QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
        QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        layout->addRow(buttons);

        // Show dialog
        if (dialog.exec() == QDialog::Accepted) {
            // Parse frequency (convert MHz string to Hz)
            QString freqStr = freqEdit->text().trimmed();
            bool freqOk = false;
            double freqMHz = freqStr.toDouble(&freqOk);

            if (!freqOk || freqMHz <= 0.0) {
                QMessageBox::warning(parent, "Invalid Frequency",
                    QString("Invalid frequency value: '%1'\n\nPlease enter a valid frequency in MHz (e.g., 146.520)")
                    .arg(freqStr));
                return;
            }

            // Validate frequency range (30 MHz to 3 GHz)
            if (freqMHz < 30.0 || freqMHz > 3000.0) {
                QMessageBox::warning(parent, "Frequency Out of Range",
                    QString("Frequency %1 MHz is out of valid range.\n\nValid range: 30-3000 MHz")
                    .arg(freqMHz, 0, 'f', 3));
                return;
            }

            uint64_t freqHz = static_cast<uint64_t>(freqMHz * 1000000.0);

            // Parse offset
            QString offsetStr = offsetEdit->text().trimmed();
            bool offsetOk = false;
            double offsetMHz = offsetStr.isEmpty() ? 0.0 : offsetStr.toDouble(&offsetOk);

            if (!offsetStr.isEmpty() && !offsetOk) {
                QMessageBox::warning(parent, "Invalid Offset",
                    QString("Invalid offset value: '%1'\n\nPlease enter a valid offset in MHz (e.g., 0.6)")
                    .arg(offsetStr));
                return;
            }

            uint64_t offsetHz = static_cast<uint64_t>(offsetMHz * 1000000.0);

            // Get tones from dropdowns (no validation needed - all values are valid)
            float rtone = rtoneCombo->currentText().toFloat();
            float ctone = ctoneCombo->currentText().toFloat();

            // Call Rust to update memory
            const char* error = update_memory(
                row,
                freqHz,
                nameEdit->text().toUtf8().constData(),
                duplexCombo->currentText().toUtf8().constData(),
                offsetHz,
                modeCombo->currentText().toUtf8().constData(),
                tmodeCombo->currentText().toUtf8().constData(),
                rtone,
                ctone
            );

            if (error) {
                QMessageBox::critical(parent, "Failed to Update Memory",
                    QString("Could not update memory:\n\n%1").arg(QString::fromUtf8(error)));
                free_error_message(error);
            } else {
                // Refresh the table
                refreshTable(table);
            }
        }
    }
}}

/// C-compatible row data structure (must match C++ definition)
#[repr(C)]
pub struct RowData {
    loc: *const c_char,
    freq: *const c_char,
    name: *const c_char,
    duplex: *const c_char,
    offset: *const c_char,
    mode: *const c_char,
    tmode: *const c_char,
    tone: *const c_char,
    power: *const c_char,
    urcall: *const c_char,
    rpt1: *const c_char,
    rpt2: *const c_char,
}

/// Application state
struct AppState {
    memories: Vec<Memory>,
    cstrings: Vec<Vec<CString>>,
    current_file: Option<PathBuf>,
    is_modified: bool,
}

/// Global storage for memory data and C strings
/// This keeps data alive while Qt is displaying it
static MEMORY_DATA: Mutex<Option<AppState>> = Mutex::new(None);

/// Download progress state
#[derive(Clone)]
struct DownloadProgress {
    current: usize,
    total: usize,
    message: String,
}

/// Download state machine
enum DownloadState {
    Idle,
    InProgress(DownloadProgress),
    Complete(Result<Vec<Memory>, String>),
}

/// Global storage for async download state
static DOWNLOAD_STATE: Mutex<DownloadState> = Mutex::new(DownloadState::Idle);

/// Convert Memory to row data strings
fn memory_to_row_strings(mem: &Memory) -> Vec<String> {
    let freq_str = Memory::format_freq(mem.freq);
    let offset_str = if mem.offset > 0 {
        Memory::format_freq(mem.offset)
    } else {
        String::new()
    };

    let (tmode, tone_str, urcall, rpt1, rpt2) = if mem.mode == "DV" {
        // D-STAR mode: show D-STAR fields
        (
            String::new(),
            String::new(),
            mem.dv_urcall.clone(),
            mem.dv_rpt1call.clone(),
            mem.dv_rpt2call.clone(),
        )
    } else {
        // FM/other modes: show tone fields
        let tone_val = if mem.tmode.contains("TSQL") || mem.tmode == "Cross" {
            mem.ctone
        } else {
            mem.rtone
        };
        let tone_str = if !mem.tmode.is_empty() && tone_val > 0.0 {
            format!("{:.1}", tone_val)
        } else {
            String::new()
        };
        (mem.tmode.clone(), tone_str, String::new(), String::new(), String::new())
    };

    let power_str = mem.power.as_ref().map(|p| p.label().to_string()).unwrap_or_default();

    vec![
        mem.number.to_string(),
        freq_str,
        mem.name.clone(),
        mem.duplex.clone(),
        offset_str,
        mem.mode.clone(),
        tmode,
        tone_str,
        power_str,
        urcall,
        rpt1,
        rpt2,
    ]
}

/// FFI: Get the number of memories
#[no_mangle]
pub extern "C" fn get_memory_count() -> usize {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref().map(|state| state.memories.len()).unwrap_or(0)
}

/// FFI: Get data for a specific row
#[no_mangle]
pub extern "C" fn get_memory_row(row: usize) -> RowData {
    let data = MEMORY_DATA.lock().unwrap();

    if let Some(state) = data.as_ref() {
        if row < state.cstrings.len() {
            let row_cstrings = &state.cstrings[row];
            return RowData {
                loc: row_cstrings[0].as_ptr(),
                freq: row_cstrings[1].as_ptr(),
                name: row_cstrings[2].as_ptr(),
                duplex: row_cstrings[3].as_ptr(),
                offset: row_cstrings[4].as_ptr(),
                mode: row_cstrings[5].as_ptr(),
                tmode: row_cstrings[6].as_ptr(),
                tone: row_cstrings[7].as_ptr(),
                power: row_cstrings[8].as_ptr(),
                urcall: row_cstrings[9].as_ptr(),
                rpt1: row_cstrings[10].as_ptr(),
                rpt2: row_cstrings[11].as_ptr(),
            };
        }
    }

    // Return empty row if out of bounds
    static EMPTY: &[u8] = b"\0";
    RowData {
        loc: EMPTY.as_ptr() as *const c_char,
        freq: EMPTY.as_ptr() as *const c_char,
        name: EMPTY.as_ptr() as *const c_char,
        duplex: EMPTY.as_ptr() as *const c_char,
        offset: EMPTY.as_ptr() as *const c_char,
        mode: EMPTY.as_ptr() as *const c_char,
        tmode: EMPTY.as_ptr() as *const c_char,
        tone: EMPTY.as_ptr() as *const c_char,
        power: EMPTY.as_ptr() as *const c_char,
        urcall: EMPTY.as_ptr() as *const c_char,
        rpt1: EMPTY.as_ptr() as *const c_char,
        rpt2: EMPTY.as_ptr() as *const c_char,
    }
}

/// Initialize memory data for display
fn set_memory_data(memories: Vec<Memory>) {
    // Convert all memories to CStrings and store them
    let mut all_cstrings = Vec::new();

    for mem in &memories {
        let strings = memory_to_row_strings(mem);
        let cstrings: Vec<CString> = strings
            .into_iter()
            .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
            .collect();
        all_cstrings.push(cstrings);
    }

    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories,
        cstrings: all_cstrings,
        current_file: None,
        is_modified: false,
    });
}

/// Clear all memory data
fn clear_memory_data() {
    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories: Vec::new(),
        cstrings: Vec::new(),
        current_file: None,
        is_modified: false,
    });
}

/// FFI: Load a file and populate memory data
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn load_file(path: *const c_char) -> *const c_char {
    // Convert C string to Rust PathBuf
    let c_str = CStr::from_ptr(path);
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("Invalid file path encoding").unwrap().into_raw(),
    };
    let path = PathBuf::from(path_str);

    // Load the .img file
    let (mmap, metadata) = match load_img(&path) {
        Ok(data) => data,
        Err(e) => {
            let err_msg = format!("Failed to load file: {}", e);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Determine which driver to use from metadata
    let vendor = &metadata.vendor;
    let model = &metadata.model;

    // Get the appropriate driver
    let driver_info = match get_driver(vendor, model) {
        Some(info) => info,
        None => {
            let err_msg = format!("Unknown radio: {} {}", vendor, model);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Parse memories from the memmap
    let memories = if driver_info.model == "TH-D75" || driver_info.model == "TH-D74" {
        use crate::drivers::thd75::THD75Radio;
        let mut radio = THD75Radio::new();
        radio.mmap = Some(mmap);
        match radio.get_memories() {
            Ok(mems) => mems,
            Err(e) => {
                let err_msg = format!("Failed to parse memories: {}", e);
                return CString::new(err_msg).unwrap().into_raw();
            }
        }
    } else {
        let err_msg = format!("Unsupported radio model: {}", driver_info.model);
        return CString::new(err_msg).unwrap().into_raw();
    };

    // Filter out empty memories
    let non_empty_memories: Vec<Memory> = memories.into_iter().filter(|m| !m.empty).collect();

    // Convert to CStrings
    let mut all_cstrings = Vec::new();
    for mem in &non_empty_memories {
        let strings = memory_to_row_strings(mem);
        let cstrings: Vec<CString> = strings
            .into_iter()
            .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
            .collect();
        all_cstrings.push(cstrings);
    }

    // Update global state
    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories: non_empty_memories,
        cstrings: all_cstrings,
        current_file: Some(path),
        is_modified: false,
    });

    // Return NULL to indicate success
    std::ptr::null()
}

/// FFI: Save current memories to a file
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn save_file(path: *const c_char) -> *const c_char {
    // Convert C string to Rust PathBuf
    let c_str = CStr::from_ptr(path);
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("Invalid file path encoding").unwrap().into_raw(),
    };
    let path = PathBuf::from(path_str);

    let mut data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_mut() {
        Some(s) => s,
        None => return CString::new("No data to save").unwrap().into_raw(),
    };

    // For now, we only support TH-D75
    // TODO: Detect radio type from current_file or add a way to track it
    use crate::drivers::thd75::THD75Radio;
    let mut radio = THD75Radio::new();

    // Convert memories back to MemoryMap
    // This requires encoding the memories into the raw format
    // For now, return an error as encoding is not yet implemented
    let err_msg = "Save functionality not yet implemented - encoding memories to .img format is TODO";
    return CString::new(err_msg).unwrap().into_raw();

    // TODO: Implement this:
    // 1. Create a new MemoryMap
    // 2. Encode each memory using THD75Radio::set_memory()
    // 3. Save the MemoryMap using save_img()
    // 4. Update state.current_file and set is_modified = false
}

/// FFI: Create a new empty file
#[no_mangle]
pub extern "C" fn new_file() {
    clear_memory_data();
}

/// FFI: Get the current filename for display
/// Returns pointer to static string (caller must NOT free)
#[no_mangle]
pub extern "C" fn get_current_filename() -> *const c_char {
    static mut FILENAME_BUF: Option<CString> = None;

    let data = MEMORY_DATA.lock().unwrap();
    let filename = if let Some(state) = data.as_ref() {
        if let Some(path) = &state.current_file {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled")
        } else {
            "Untitled"
        }
    } else {
        "Untitled"
    };

    unsafe {
        FILENAME_BUF = Some(CString::new(filename).unwrap());
        FILENAME_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Free an error message string returned by load_file or save_file
#[no_mangle]
pub unsafe extern "C" fn free_error_message(ptr: *const c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr as *mut c_char);
    }
}

/// FFI: Get a memory by row index for editing
/// Returns a copy of the Memory that can be edited
#[no_mangle]
pub extern "C" fn get_memory_by_row(row: usize) -> *const Memory {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if row < state.memories.len() {
            // Return pointer to the memory (it's safe because MEMORY_DATA lives forever)
            return &state.memories[row] as *const Memory;
        }
    }
    std::ptr::null()
}

/// FFI: Get list of available vendors (comma-separated)
#[no_mangle]
pub extern "C" fn get_vendors() -> *const c_char {
    static mut VENDORS_BUF: Option<CString> = None;

    let data = MEMORY_DATA.lock().unwrap();
    let vendors = if let Some(state) = data.as_ref() {
        // We need to get vendors from somewhere - let's get them from drivers
        let drivers = list_drivers();
        let mut vendors: Vec<String> = drivers.iter().map(|d| d.vendor.clone()).collect();
        vendors.sort();
        vendors.dedup();
        vendors.join(",")
    } else {
        let drivers = list_drivers();
        let mut vendors: Vec<String> = drivers.iter().map(|d| d.vendor.clone()).collect();
        vendors.sort();
        vendors.dedup();
        vendors.join(",")
    };

    unsafe {
        VENDORS_BUF = Some(CString::new(vendors).unwrap());
        VENDORS_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Get list of models for a vendor (comma-separated)
#[no_mangle]
pub unsafe extern "C" fn get_models_for_vendor(vendor: *const c_char) -> *const c_char {
    static mut MODELS_BUF: Option<CString> = None;

    let c_str = CStr::from_ptr(vendor);
    let vendor_str = c_str.to_str().unwrap_or("");

    let drivers = list_drivers();
    let models: Vec<String> = drivers
        .iter()
        .filter(|d| d.vendor == vendor_str)
        .map(|d| d.model.clone())
        .collect();

    let models_str = models.join(",");

    unsafe {
        MODELS_BUF = Some(CString::new(models_str).unwrap());
        MODELS_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Get list of available serial ports (comma-separated)
#[no_mangle]
pub extern "C" fn get_serial_ports() -> *const c_char {
    static mut PORTS_BUF: Option<CString> = None;

    let ports = match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect::<Vec<_>>().join(","),
        Err(_) => String::new(),
    };

    unsafe {
        PORTS_BUF = Some(CString::new(ports).unwrap());
        PORTS_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Get list of standard CTCSS tones (comma-separated)
#[no_mangle]
pub extern "C" fn get_ctcss_tones() -> *const c_char {
    static mut TONES_BUF: Option<CString> = None;

    use crate::core::constants::TONES;
    let tones_str = TONES.iter()
        .map(|t| format!("{:.1}", t))
        .collect::<Vec<_>>()
        .join(",");

    unsafe {
        TONES_BUF = Some(CString::new(tones_str).unwrap());
        TONES_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Download memories from radio (blocking operation)
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn download_from_radio(
    vendor: *const c_char,
    model: *const c_char,
    port: *const c_char,
) -> *const c_char {
    // Convert C strings to Rust
    let vendor_str = CStr::from_ptr(vendor).to_str().unwrap_or("").to_string();
    let model_str = CStr::from_ptr(model).to_str().unwrap_or("").to_string();
    let port_str = CStr::from_ptr(port).to_str().unwrap_or("").to_string();

    // Create a tokio runtime for the async operation
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            let err_msg = format!("Failed to create async runtime: {}", e);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Run the download operation
    let result = runtime.block_on(async {
        // Progress callback (for now, just ignore progress updates)
        let progress_fn = std::sync::Arc::new(|_current: usize, _total: usize, _msg: String| {
            // TODO: Update progress bar
        });

        crate::gui::radio_ops::download_from_radio(
            port_str,
            vendor_str,
            model_str,
            progress_fn,
        )
        .await
    });

    match result {
        Ok(memories) => {
            // Filter out empty memories
            let non_empty: Vec<Memory> = memories.into_iter().filter(|m| !m.empty).collect();

            // Convert to CStrings
            let mut all_cstrings = Vec::new();
            for mem in &non_empty {
                let strings = memory_to_row_strings(mem);
                let cstrings: Vec<CString> = strings
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
                    .collect();
                all_cstrings.push(cstrings);
            }

            // Update global state
            let mut data = MEMORY_DATA.lock().unwrap();
            *data = Some(AppState {
                memories: non_empty,
                cstrings: all_cstrings,
                current_file: None,
                is_modified: false,
            });

            // Return NULL to indicate success
            std::ptr::null()
        }
        Err(e) => {
            let err_msg = format!("Download failed: {}", e);
            CString::new(err_msg).unwrap().into_raw()
        }
    }
}

/// FFI: Start async download from radio (non-blocking)
/// Returns immediately, use get_download_progress/is_download_complete to poll
#[no_mangle]
pub unsafe extern "C" fn start_download_async(
    vendor: *const c_char,
    model: *const c_char,
    port: *const c_char,
) {
    // Convert C strings to Rust
    let vendor_str = CStr::from_ptr(vendor).to_str().unwrap_or("").to_string();
    let model_str = CStr::from_ptr(model).to_str().unwrap_or("").to_string();
    let port_str = CStr::from_ptr(port).to_str().unwrap_or("").to_string();

    // Reset state to InProgress
    {
        let mut state = DOWNLOAD_STATE.lock().unwrap();
        *state = DownloadState::InProgress(DownloadProgress {
            current: 0,
            total: 100,
            message: "Initializing...".to_string(),
        });
    }

    // Spawn background thread to do the download
    thread::spawn(move || {
        // Create tokio runtime
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let mut state = DOWNLOAD_STATE.lock().unwrap();
                *state = DownloadState::Complete(Err(format!("Failed to create async runtime: {}", e)));
                return;
            }
        };

        // Run the download
        let result = runtime.block_on(async {
            // Progress callback that updates global state
            let progress_fn = Arc::new(|current: usize, total: usize, msg: String| {
                let mut state = DOWNLOAD_STATE.lock().unwrap();
                *state = DownloadState::InProgress(DownloadProgress {
                    current,
                    total,
                    message: msg,
                });
            });

            crate::gui::radio_ops::download_from_radio(
                port_str,
                vendor_str,
                model_str,
                progress_fn,
            )
            .await
        });

        // Store result
        let mut state = DOWNLOAD_STATE.lock().unwrap();
        *state = DownloadState::Complete(result);
    });
}

/// FFI: Get current download progress (returns current, total, message)
/// Returns current progress as percentage (0-100), or -1 if not in progress
#[no_mangle]
pub extern "C" fn get_download_progress(
    out_current: *mut i32,
    out_total: *mut i32,
    out_message: *mut *const c_char,
) -> i32 {
    static mut MESSAGE_BUF: Option<CString> = None;

    let state = DOWNLOAD_STATE.lock().unwrap();
    match &*state {
        DownloadState::InProgress(progress) => {
            unsafe {
                if !out_current.is_null() {
                    *out_current = progress.current as i32;
                }
                if !out_total.is_null() {
                    *out_total = progress.total as i32;
                }
                if !out_message.is_null() {
                    MESSAGE_BUF = Some(CString::new(progress.message.clone()).unwrap());
                    *out_message = MESSAGE_BUF.as_ref().unwrap().as_ptr();
                }
            }
            if progress.total > 0 {
                ((progress.current as f64 / progress.total as f64) * 100.0) as i32
            } else {
                0
            }
        }
        _ => -1, // Not in progress
    }
}

/// FFI: Check if download is complete
/// Returns 1 if complete, 0 if still in progress, -1 if idle
#[no_mangle]
pub extern "C" fn is_download_complete() -> i32 {
    let state = DOWNLOAD_STATE.lock().unwrap();
    match &*state {
        DownloadState::Idle => -1,
        DownloadState::InProgress(_) => 0,
        DownloadState::Complete(_) => 1,
    }
}

/// FFI: Get download result and reset state
/// Returns NULL on success, or error message on failure
/// After calling this, state returns to Idle
#[no_mangle]
pub extern "C" fn get_download_result() -> *const c_char {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    let result = std::mem::replace(&mut *state, DownloadState::Idle);

    match result {
        DownloadState::Complete(Ok(memories)) => {
            // Filter out empty memories
            let non_empty: Vec<Memory> = memories.into_iter().filter(|m| !m.empty).collect();

            // Convert to CStrings
            let mut all_cstrings = Vec::new();
            for mem in &non_empty {
                let strings = memory_to_row_strings(mem);
                let cstrings: Vec<CString> = strings
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
                    .collect();
                all_cstrings.push(cstrings);
            }

            // Update global state
            let mut data = MEMORY_DATA.lock().unwrap();
            *data = Some(AppState {
                memories: non_empty,
                cstrings: all_cstrings,
                current_file: None,
                is_modified: false,
            });

            // Return NULL to indicate success
            std::ptr::null()
        }
        DownloadState::Complete(Err(e)) => {
            let err_msg = format!("Download failed: {}", e);
            CString::new(err_msg).unwrap().into_raw()
        }
        _ => {
            let err_msg = "Download not complete";
            CString::new(err_msg).unwrap().into_raw()
        }
    }
}

/// FFI: Update a memory at a specific row
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn update_memory(
    row: usize,
    freq: u64,
    name: *const c_char,
    duplex: *const c_char,
    offset: u64,
    mode: *const c_char,
    tmode: *const c_char,
    rtone: f32,
    ctone: f32,
) -> *const c_char {
    let mut data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_mut() {
        Some(s) => s,
        None => return CString::new("No data loaded").unwrap().into_raw(),
    };

    if row >= state.memories.len() {
        return CString::new("Invalid row index").unwrap().into_raw();
    }

    // Convert C strings to Rust
    let name_str = CStr::from_ptr(name).to_str().unwrap_or("").to_string();
    let duplex_str = CStr::from_ptr(duplex).to_str().unwrap_or("").to_string();
    let mode_str = CStr::from_ptr(mode).to_str().unwrap_or("FM").to_string();
    let tmode_str = CStr::from_ptr(tmode).to_str().unwrap_or("").to_string();

    // Update the memory
    let mem = &mut state.memories[row];
    mem.freq = freq;
    mem.name = name_str;
    mem.duplex = duplex_str;
    mem.offset = offset;
    mem.mode = mode_str;
    mem.tmode = tmode_str;
    mem.rtone = rtone;
    mem.ctone = ctone;

    // Regenerate CStrings for this row
    let strings = memory_to_row_strings(mem);
    let cstrings: Vec<CString> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    state.cstrings[row] = cstrings;

    // Mark as modified
    state.is_modified = true;

    // Return NULL to indicate success
    std::ptr::null()
}

/// Create sample test memories
fn create_test_memories() -> Vec<Memory> {
    vec![
        {
            let mut mem = Memory::new(1);
            mem.freq = 146_520_000;
            mem.name = "Simplex".to_string();
            mem.mode = "FM".to_string();
            mem
        },
        {
            let mut mem = Memory::new(2);
            mem.freq = 146_940_000;
            mem.name = "W6CX Rpt".to_string();
            mem.mode = "FM".to_string();
            mem.duplex = "-".to_string();
            mem.offset = 600_000;
            mem.tmode = "Tone".to_string();
            mem.rtone = 100.0;
            mem
        },
        {
            let mut mem = Memory::new(3);
            mem.freq = 147_330_000;
            mem.name = "N6NFI Rpt".to_string();
            mem.mode = "FM".to_string();
            mem.duplex = "+".to_string();
            mem.offset = 600_000;
            mem.tmode = "TSQL".to_string();
            mem.ctone = 88.5;
            mem
        },
        {
            let mut mem = Memory::new(4);
            mem.freq = 145_230_000;
            mem.name = "DV Memory".to_string();
            mem.mode = "DV".to_string();
            mem.dv_urcall = "CQCQCQ".to_string();
            mem.dv_rpt1call = "W3POG  B".to_string();
            mem.dv_rpt2call = "W3POG  G".to_string();
            mem
        },
    ]
}

/// Run the Qt application
pub fn run_qt_app() -> i32 {
    // Initialize drivers
    init_drivers();
    let _drivers = list_drivers();

    // Create and store test memories
    let test_memories = create_test_memories();
    set_memory_data(test_memories);

    // Create Qt application
    let mut argc = 0;
    let argv: *mut *mut i8 = std::ptr::null_mut();

    unsafe {
        cpp!([mut argc as "int", argv as "char**"] -> i32 as "int" {
            QApplication app(argc, argv);
            app.setApplicationName("CHIRP-RS");
            app.setOrganizationName("CHIRP");

            // Create main window
            QMainWindow* window = new QMainWindow();
            window->setWindowTitle("CHIRP-RS");
            window->resize(1200, 600);

            // Create central widget with table FIRST (so menus can reference it)
            QWidget* centralWidget = new QWidget(window);
            QVBoxLayout* layout = new QVBoxLayout(centralWidget);

            QTableWidget* table = new QTableWidget(centralWidget);
            table->setColumnCount(12);
            QStringList headers;
            headers << "Loc" << "Frequency" << "Name" << "Duplex" << "Offset"
                    << "Mode" << "ToneMode" << "Tone" << "Power"
                    << "URCALL" << "RPT1" << "RPT2";
            table->setHorizontalHeaderLabels(headers);
            table->horizontalHeader()->setStretchLastSection(true);
            table->setAlternatingRowColors(true);
            table->setSelectionBehavior(QTableWidget::SelectRows);
            table->setEditTriggers(QTableWidget::NoEditTriggers);
            table->verticalHeader()->setVisible(false);

            // Set column widths
            table->setColumnWidth(0, 50);   // Loc
            table->setColumnWidth(1, 110);  // Frequency
            table->setColumnWidth(2, 150);  // Name
            table->setColumnWidth(3, 70);   // Duplex
            table->setColumnWidth(4, 90);   // Offset
            table->setColumnWidth(5, 70);   // Mode
            table->setColumnWidth(6, 90);   // ToneMode
            table->setColumnWidth(7, 70);   // Tone
            table->setColumnWidth(8, 70);   // Power
            table->setColumnWidth(9, 100);  // URCALL
            table->setColumnWidth(10, 100); // RPT1
            table->setColumnWidth(11, 100); // RPT2

            layout->addWidget(table);
            centralWidget->setLayout(layout);
            window->setCentralWidget(centralWidget);

            // Create menu bar (after table, so menus can reference it)
            QMenuBar* menuBar = window->menuBar();

            // File menu
            QMenu* fileMenu = menuBar->addMenu("&File");

            fileMenu->addAction("&New", [=]() {
                new_file();
                table->setRowCount(0);
                window->setWindowTitle("CHIRP-RS - Untitled");
            });

            fileMenu->addAction("&Open...", [=]() {
                QString fileName = QFileDialog::getOpenFileName(window,
                    "Open CHIRP Image", "", "CHIRP Image (*.img)");
                if (!fileName.isEmpty()) {
                    const char* error = load_file(fileName.toUtf8().constData());
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Failed to Open File",
                            QString("Could not open file:\n%1\n\nError: %2\n\n"
                                   "Please ensure:\n"
                                   "• File is a valid CHIRP image (.img)\n"
                                   "• File is not corrupted\n"
                                   "• You have permission to read the file")
                            .arg(fileName).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        refreshTable(table);
                        const char* filename = get_current_filename();
                        window->setWindowTitle(QString("CHIRP-RS - %1").arg(QString::fromUtf8(filename)));
                    }
                }
            });

            fileMenu->addAction("&Save", [=]() {
                QMessageBox::information(window, "Save",
                    "Save functionality not yet implemented.\n\n"
                    "Memory encoding to .img format is TODO.");
            });

            fileMenu->addAction("Save &As...", [=]() {
                QString fileName = QFileDialog::getSaveFileName(window,
                    "Save CHIRP Image", "", "CHIRP Image (*.img)");
                if (!fileName.isEmpty()) {
                    QMessageBox::information(window, "Save As",
                        "Save functionality not yet implemented.\n\n"
                        "Memory encoding to .img format is TODO.");
                }
            });

            fileMenu->addSeparator();
            fileMenu->addAction("E&xit", &app, &QApplication::quit);

            // Radio menu
            QMenu* radioMenu = menuBar->addMenu("&Radio");
            radioMenu->addAction("&Download from Radio", [=]() {
                showDownloadDialog(window, table);
            });
            radioMenu->addAction("&Upload to Radio", [window]() {
                // TODO: Show upload dialog
                QMessageBox::information(window, "Upload",
                    "Upload to radio functionality coming soon");
            });

            // Connect double-click event to edit dialog
            QObject::connect(table, &QTableWidget::cellDoubleClicked,
                [=](int row, int column) {
                    showEditDialog(window, table, row);
                });

            // Populate table with initial data
            refreshTable(table);

            // Show window
            window->show();

            return app.exec();
        })
    }
}

