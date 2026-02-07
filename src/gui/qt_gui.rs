//! Qt-based GUI for CHIRP-RS
//! Provides a traditional desktop application experience using Qt Widgets

use crate::core::Memory;
use crate::drivers::{init_drivers, list_drivers};
use cpp::cpp;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::Mutex;

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
    #include <QtWidgets/QWidget>
    #include <QtWidgets/QMessageBox>
    #include <QtWidgets/QFileDialog>
    #include <QtCore/QString>
    #include <QtCore/QStringList>

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

    // Declare Rust FFI functions
    extern "C" {
        size_t get_memory_count();
        RowData get_memory_row(size_t row);
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

/// Global storage for memory data and C strings
/// This keeps data alive while Qt is displaying it
static MEMORY_DATA: Mutex<Option<(Vec<Memory>, Vec<Vec<CString>>)>> = Mutex::new(None);

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
    data.as_ref().map(|(mems, _)| mems.len()).unwrap_or(0)
}

/// FFI: Get data for a specific row
#[no_mangle]
pub extern "C" fn get_memory_row(row: usize) -> RowData {
    let data = MEMORY_DATA.lock().unwrap();

    if let Some((_, cstrings)) = data.as_ref() {
        if row < cstrings.len() {
            let row_cstrings = &cstrings[row];
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
    *data = Some((memories, all_cstrings));
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

            // Create menu bar
            QMenuBar* menuBar = window->menuBar();

            // File menu
            QMenu* fileMenu = menuBar->addMenu("&File");
            fileMenu->addAction("&New", [window]() {
                // TODO: Implement new file
            });
            fileMenu->addAction("&Open...", [window]() {
                QString fileName = QFileDialog::getOpenFileName(window,
                    "Open CHIRP Image", "", "CHIRP Image (*.img)");
                if (!fileName.isEmpty()) {
                    // TODO: Load file
                }
            });
            fileMenu->addAction("&Save", [window]() {
                // TODO: Implement save
            });
            fileMenu->addAction("Save &As...", [window]() {
                QString fileName = QFileDialog::getSaveFileName(window,
                    "Save CHIRP Image", "", "CHIRP Image (*.img)");
                if (!fileName.isEmpty()) {
                    // TODO: Save file
                }
            });
            fileMenu->addSeparator();
            fileMenu->addAction("E&xit", &app, &QApplication::quit);

            // Radio menu
            QMenu* radioMenu = menuBar->addMenu("&Radio");
            radioMenu->addAction("&Download from Radio", [window]() {
                // TODO: Show download dialog
                QMessageBox::information(window, "Download",
                    "Download from radio functionality coming soon");
            });
            radioMenu->addAction("&Upload to Radio", [window]() {
                // TODO: Show upload dialog
                QMessageBox::information(window, "Upload",
                    "Upload to radio functionality coming soon");
            });

            // Create central widget with table
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

            // Populate table by calling back into Rust
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

            layout->addWidget(table);
            centralWidget->setLayout(layout);
            window->setCentralWidget(centralWidget);

            // Show window
            window->show();

            return app.exec();
        })
    }
}

