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
    #include <QtWidgets/QTreeWidget>
    #include <QtWidgets/QTreeWidgetItem>
    #include <QtWidgets/QHeaderView>
    #include <QtWidgets/QVBoxLayout>
    #include <QtWidgets/QHBoxLayout>
    #include <QtWidgets/QSplitter>
    #include <QtWidgets/QWidget>
    #include <QtWidgets/QMessageBox>
    #include <QtWidgets/QPushButton>
    #include <QtWidgets/QFileDialog>
    #include <QtWidgets/QDialog>
    #include <QtWidgets/QDialogButtonBox>
    #include <QtWidgets/QFormLayout>
    #include <QtWidgets/QLineEdit>
    #include <QtWidgets/QComboBox>
    #include <QtWidgets/QSpinBox>
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
        const char* bank;
    };

    // Forward declare Memory struct (we only need pointer to it)
    struct Memory;

    // Declare Rust FFI functions
    extern "C" {
        size_t get_memory_count();
        RowData get_memory_row(size_t row);
        const char* load_file(const char* path);
        const char* save_file(const char* path);
        const char* export_to_csv(const char* path);
        const char* import_from_csv(const char* path);
        void new_file();
        const char* get_current_filename();
        const char* get_current_filepath();
        void free_error_message(const char* msg);
        const Memory* get_memory_by_row(size_t row);
        const char* update_memory(size_t row, uint64_t freq, const char* name,
                                 const char* duplex, uint64_t offset, const char* mode,
                                 const char* tmode, float rtone, float ctone, uint8_t bank,
                                 const char* urcall, const char* rpt1call, const char* rpt2call);
        const char* get_vendors();
        const char* get_models_for_vendor(const char* vendor);
        const char* get_serial_ports();
        const char* get_ctcss_tones();
        const char* get_bank_names();
        const char* download_from_radio(const char* vendor, const char* model, const char* port);
        void start_download_async(const char* vendor, const char* model, const char* port);
        int get_download_progress(int* out_current, int* out_total, const char** out_message);
        int is_download_complete();
        const char* get_download_result();
        void start_upload_async(const char* vendor, const char* model, const char* port);
        int get_upload_progress(int* out_current, int* out_total, const char** out_message);
        int is_upload_complete();
        const char* get_upload_result();
        const char* delete_memory_at(size_t row);
        void copy_memory_at(size_t row);
        const char* paste_memory_at(size_t row);
        int has_clipboard_memory();

        // Multi-band support
        bool has_band_organization();
        size_t get_band_count();
        uint8_t get_band_number_by_index(size_t index);
        const char* get_band_name(uint8_t band_num);
        size_t get_band_memory_count(uint8_t band_num);
        RowData get_memory_by_band_row(uint8_t band_num, size_t row);
        intptr_t get_global_index_from_band_row(uint8_t band_num, size_t row);

        // Multi-bank/group support
        bool has_bank_organization();
        size_t get_bank_count();
        uint8_t get_bank_number_by_index(size_t index);
        const char* get_bank_name_by_number(uint8_t bank_num);
        size_t get_bank_memory_count(uint8_t bank_num);
        RowData get_memory_by_bank_row(uint8_t bank_num, size_t row);
        intptr_t get_global_index_from_bank_row(uint8_t bank_num, size_t row);
    }

    // Forward declarations for helper refresh functions
    void refreshTable(QTableWidget* table);
    void refreshTreeWithBands(QTreeWidget* tree);
    void refreshTableForBand(QTableWidget* table, uint8_t band_num);
    void refreshTreeWithBanks(QTreeWidget* tree);
    void refreshTableForBank(QTableWidget* table, uint8_t bank_num);

    // Helper function to refresh whichever view is currently visible
    // Used by dialogs that don't have direct access to both widgets
    void refreshCurrentView(QTableWidget* table, QTreeWidget* tree) {
        if (has_band_organization()) {
            // Multi-band radio: show tree and table in split view
            tree->show();
            table->show();
            refreshTreeWithBands(tree);
            // Refresh table with first band
            if (tree->topLevelItemCount() > 0) {
                QTreeWidgetItem* firstItem = tree->topLevelItem(0);
                uint8_t band_num = firstItem->data(0, Qt::UserRole).toUInt();
                refreshTableForBand(table, band_num);
            }
        } else if (has_bank_organization()) {
            // Radio with banks/groups: show tree and table in split view
            tree->show();
            table->show();
            refreshTreeWithBanks(tree);
            // Table will be refreshed via tree selection (defaults to "All Memories")
        } else {
            // Single-band, single-bank radio: hide tree, show table full width
            tree->hide();
            table->show();
            refreshTable(table);
        }
    }

    // Helper function to refresh just the current band/bank's table
    // Used after edit/paste/cut/clear operations to avoid resetting tree selection
    void refreshCurrentBandTable(QTableWidget* table, QTreeWidget* tree) {
        if (has_band_organization() && tree->currentItem()) {
            // Multi-band mode: refresh table for currently selected band
            uint8_t band_num = tree->currentItem()->data(0, Qt::UserRole).toUInt();
            refreshTableForBand(table, band_num);
        } else if (has_bank_organization() && tree->currentItem()) {
            // Bank/group mode: refresh table for currently selected bank
            bool is_bank_filter = tree->currentItem()->data(0, Qt::UserRole + 1).toBool();
            if (is_bank_filter) {
                uint8_t bank_num = tree->currentItem()->data(0, Qt::UserRole).toUInt();
                refreshTableForBank(table, bank_num);
            } else {
                // "All Memories" selected
                refreshTable(table);
            }
        } else {
            // Single-band, single-bank mode: refresh entire table
            refreshTable(table);
        }
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
            // Bank column removed - now shown in tree view instead
        }

        // Force table to update display
        table->viewport()->update();
    }

    // Helper function to refresh tree widget with band list (no children)
    void refreshTreeWithBands(QTreeWidget* tree) {
        tree->clear();

        size_t band_count = get_band_count();
        if (band_count == 0) {
            // Fallback: no bands, shouldn't happen but handle gracefully
            return;
        }

        // Create an item for each band (no children - memories shown in table)
        for (size_t band_idx = 0; band_idx < band_count; ++band_idx) {
            uint8_t band_num = get_band_number_by_index(band_idx);
            const char* band_name_cstr = get_band_name(band_num);
            QString band_name = QString::fromUtf8(band_name_cstr);
            free_error_message(band_name_cstr);

            size_t mem_count = get_band_memory_count(band_num);

            // Create item with band name and memory count
            QTreeWidgetItem* band_item = new QTreeWidgetItem(tree);
            band_item->setText(0, QString("%1 (%2 memories)").arg(band_name).arg(mem_count));

            // Store band number in item data for later retrieval
            band_item->setData(0, Qt::UserRole, band_num);
        }

        // Select first band by default
        if (tree->topLevelItemCount() > 0) {
            tree->setCurrentItem(tree->topLevelItem(0));
        }

        // Force tree to update display
        tree->viewport()->update();
    }

    // Helper function to refresh table with memories from a specific band
    void refreshTableForBand(QTableWidget* table, uint8_t band_num) {
        size_t mem_count = get_band_memory_count(band_num);
        table->setRowCount(mem_count);

        for (size_t row = 0; row < mem_count; ++row) {
            RowData data = get_memory_by_band_row(band_num, row);
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
            // Bank column removed - now shown in tree view instead
        }

        // Force table to update display
        table->viewport()->update();
    }

    // Helper function to refresh tree with bank/group names
    void refreshTreeWithBanks(QTreeWidget* tree) {
        tree->clear();

        size_t bank_count = get_bank_count();
        if (bank_count == 0) {
            // Fallback: no banks, shouldn't happen but handle gracefully
            return;
        }

        // Create an "All Memories" item at the top
        QTreeWidgetItem* all_item = new QTreeWidgetItem(tree);
        all_item->setText(0, QString("All Memories (%1 memories)").arg(get_memory_count()));
        all_item->setData(0, Qt::UserRole, 255);  // Special value for "all"
        all_item->setData(0, Qt::UserRole + 1, true);  // Is a bank filter (shows all banks)

        // Create an item for each bank/group
        for (size_t bank_idx = 0; bank_idx < bank_count; ++bank_idx) {
            uint8_t bank_num = get_bank_number_by_index(bank_idx);
            const char* bank_name_cstr = get_bank_name_by_number(bank_num);
            QString bank_name = QString::fromUtf8(bank_name_cstr);
            free_error_message(bank_name_cstr);

            size_t mem_count = get_bank_memory_count(bank_num);

            // Create item with bank name and memory count
            QTreeWidgetItem* bank_item = new QTreeWidgetItem(tree);
            bank_item->setText(0, QString("%1 (%2 memories)").arg(bank_name).arg(mem_count));

            // Store bank number in item data for later retrieval
            bank_item->setData(0, Qt::UserRole, bank_num);
            bank_item->setData(0, Qt::UserRole + 1, true);  // Is a bank filter
        }

        // Select "All Memories" by default
        if (tree->topLevelItemCount() > 0) {
            tree->setCurrentItem(tree->topLevelItem(0));
        }

        // Force tree to update display
        tree->viewport()->update();
    }

    // Helper function to refresh table with memories from a specific bank
    void refreshTableForBank(QTableWidget* table, uint8_t bank_num) {
        size_t mem_count = get_bank_memory_count(bank_num);
        table->setRowCount(mem_count);

        for (size_t row = 0; row < mem_count; ++row) {
            RowData data = get_memory_by_bank_row(bank_num, row);
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
            // Note: Bank column removed
        }

        // Force table to update display
        table->viewport()->update();
    }

    // Helper function to show download dialog
    void showDownloadDialog(QWidget* parent, QTableWidget* table, QTreeWidget* tree) {
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
                        refreshCurrentView(table, tree);
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

    // Helper function to show upload dialog
    void showUploadDialog(QWidget* parent, QTableWidget* table, QTreeWidget* tree) {
        QDialog dialog(parent);
        dialog.setWindowTitle("Upload to Radio");
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
        buttons->button(QDialogButtonBox::Ok)->setText("Upload");
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
            progressDlg->setWindowTitle("Uploading to Radio");
            progressDlg->setWindowModality(Qt::WindowModal);
            progressDlg->setMinimumDuration(0);
            progressDlg->setAutoClose(false);
            progressDlg->setAutoReset(false);
            progressDlg->setValue(0);
            progressDlg->show();
            progressDlg->raise();
            progressDlg->activateWindow();
            QApplication::processEvents();

            // Start async upload
            start_upload_async(
                vendor.toUtf8().constData(),
                model.toUtf8().constData(),
                port.toUtf8().constData()
            );

            // Create timer to poll progress
            QTimer* timer = new QTimer(parent);
            timer->setInterval(100); // Poll every 100ms

            QObject::connect(timer, &QTimer::timeout, [=]() mutable {
                // Check if user cancelled
                if (progressDlg->wasCanceled()) {
                    timer->stop();
                    progressDlg->deleteLater();
                    timer->deleteLater();
                    return;
                }

                // Get current progress
                int current = 0;
                int total = 100;
                const char* message = nullptr;
                int percentage = get_upload_progress(&current, &total, &message);

                if (percentage >= 0) {
                    // Still in progress
                    progressDlg->setMaximum(total);
                    progressDlg->setValue(current);
                    if (message) {
                        progressDlg->setLabelText(QString::fromUtf8(message));
                    }
                    progressDlg->show();
                }

                // Check if complete
                int complete = is_upload_complete();
                if (complete == 1) {
                    timer->stop();
                    progressDlg->close();

                    // Get result
                    const char* error = get_upload_result();
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(parent, "Upload Failed",
                            QString("Failed to upload memories to radio.\n\n"
                                   "Error: %1\n\n"
                                   "Please check:\n"
                                   "• Radio is connected and powered on\n"
                                   "• Correct serial port is selected\n"
                                   "• No other program is using the radio")
                            .arg(errorMsg));
                        free_error_message(error);
                    } else {
                        QMessageBox::information(parent, "Upload Complete",
                            "Successfully uploaded memories to radio!");
                    }

                    progressDlg->deleteLater();
                    timer->deleteLater();
                }
            });

            timer->start();
        }
    }

    // Helper function to show edit dialog for a memory
    void showEditDialog(QWidget* parent, QTableWidget* table, QTreeWidget* tree, int row) {
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

        // Bank selection (0-9) - use combo box with actual bank names from radio
        QComboBox* bankCombo = new QComboBox();
        QString bankNamesStr = QString::fromUtf8(get_bank_names());
        QStringList bankNames = bankNamesStr.split(",", Qt::SkipEmptyParts);

        for (int i = 0; i < bankNames.size() && i < 10; i++) {
            bankCombo->addItem(bankNames[i], i);
        }

        // Set current bank from data
        int currentBank = QString::fromUtf8(data.bank).toInt();
        bankCombo->setCurrentIndex(currentBank);

        // D-STAR fields (for DV mode)
        QLineEdit* urcallEdit = new QLineEdit(QString::fromUtf8(data.urcall));
        QLineEdit* rpt1Edit = new QLineEdit(QString::fromUtf8(data.rpt1));
        QLineEdit* rpt2Edit = new QLineEdit(QString::fromUtf8(data.rpt2));

        urcallEdit->setMaxLength(8);  // D-STAR call signs are max 8 characters
        rpt1Edit->setMaxLength(8);
        rpt2Edit->setMaxLength(8);

        // Add fields to form
        layout->addRow("Frequency (MHz):", freqEdit);
        layout->addRow("Name:", nameEdit);
        layout->addRow("Duplex:", duplexCombo);
        layout->addRow("Offset (MHz):", offsetEdit);
        layout->addRow("Mode:", modeCombo);

        // Tone fields (hidden for DV mode)
        QWidget* toneWidget = new QWidget();
        QFormLayout* toneLayout = new QFormLayout(toneWidget);
        toneLayout->setContentsMargins(0, 0, 0, 0);
        toneLayout->addRow("Tone Mode:", tmodeCombo);
        toneLayout->addRow("TX Tone (Hz):", rtoneCombo);
        toneLayout->addRow("RX Tone (Hz):", ctoneCombo);
        layout->addRow(toneWidget);

        // D-STAR fields (hidden for non-DV modes)
        QWidget* dstarWidget = new QWidget();
        QFormLayout* dstarLayout = new QFormLayout(dstarWidget);
        dstarLayout->setContentsMargins(0, 0, 0, 0);
        dstarLayout->addRow("URCALL:", urcallEdit);
        dstarLayout->addRow("RPT1CALL:", rpt1Edit);
        dstarLayout->addRow("RPT2CALL:", rpt2Edit);
        layout->addRow(dstarWidget);

        layout->addRow("Bank:", bankCombo);

        // Show/hide fields based on mode
        auto updateFieldVisibility = [=]() {
            bool isDV = modeCombo->currentText() == "DV";
            toneWidget->setVisible(!isDV);
            dstarWidget->setVisible(isDV);
        };

        // Initial visibility
        updateFieldVisibility();

        // Update visibility when mode changes
        QObject::connect(modeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
                        updateFieldVisibility);

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
                ctone,
                static_cast<uint8_t>(bankCombo->currentData().toInt()),
                urcallEdit->text().toUtf8().constData(),
                rpt1Edit->text().toUtf8().constData(),
                rpt2Edit->text().toUtf8().constData()
            );

            if (error) {
                QMessageBox::critical(parent, "Failed to Update Memory",
                    QString("Could not update memory:\n\n%1").arg(QString::fromUtf8(error)));
                free_error_message(error);
            } else {
                // Refresh the entire view (tree + table) since bank may have changed
                if (has_bank_organization()) {
                    refreshTreeWithBanks(tree);
                    // After tree refresh, select "All Memories" or preserve selection
                    if (tree->topLevelItemCount() > 0) {
                        tree->setCurrentItem(tree->topLevelItem(0));
                    }
                } else if (has_band_organization()) {
                    refreshTreeWithBands(tree);
                } else {
                    refreshTable(table);
                }
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
    bank: *const c_char,
}

/// Application state
struct AppState {
    memories: Vec<Memory>,
    cstrings: Vec<Vec<CString>>,
    current_file: Option<PathBuf>,
    is_modified: bool,
    mmap: Option<crate::memmap::MemoryMap>,
    bank_names: Vec<String>,
    clipboard: Option<Memory>,
    /// Band organization for multi-band radios (band_num -> memory indices)
    band_groups: std::collections::HashMap<u8, Vec<usize>>,
    /// Band display names (band_num -> display name like "VHF (144 MHz)")
    band_display_names: std::collections::HashMap<u8, String>,
    /// Bank/Group organization for radios with banks/groups (bank_num -> memory indices)
    bank_groups: std::collections::HashMap<u8, Vec<usize>>,
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
    Complete(Result<(Vec<Memory>, crate::memmap::MemoryMap), String>),
}

/// Global storage for async download state
static DOWNLOAD_STATE: Mutex<DownloadState> = Mutex::new(DownloadState::Idle);

/// Upload progress tracking
#[derive(Clone)]
struct UploadProgress {
    current: usize,
    total: usize,
    message: String,
}

/// Upload state machine
enum UploadState {
    Idle,
    InProgress(UploadProgress),
    Complete(Result<(), String>),
}

/// Global storage for async upload state
static UPLOAD_STATE: Mutex<UploadState> = Mutex::new(UploadState::Idle);

/// Convert Memory to row data strings
fn memory_to_row_strings(mem: &Memory, bank_names: &[String]) -> Vec<String> {
    // If memory is empty, show placeholder values
    if mem.empty {
        return vec![
            mem.number.to_string(),
            String::new(),
            String::from("(Empty)"),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
    }

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
        (
            mem.tmode.clone(),
            tone_str,
            String::new(),
            String::new(),
            String::new(),
        )
    };

    let power_str = mem
        .power
        .as_ref()
        .map(|p| p.label().to_string())
        .unwrap_or_default();

    // Get bank name, fallback to number if out of range
    let bank_str = bank_names
        .get(mem.bank as usize)
        .cloned()
        .unwrap_or_else(|| format!("Bank {}", mem.bank));

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
        mem.bank.to_string(), // Store bank NUMBER (not name) for edit dialog
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
                bank: row_cstrings[12].as_ptr(),
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
        bank: EMPTY.as_ptr() as *const c_char,
    }
}

/// FFI: Check if memories have band organization (multi-band radio)
#[no_mangle]
pub extern "C" fn has_band_organization() -> bool {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .map(|state| !state.band_groups.is_empty())
        .unwrap_or(false)
}

/// FFI: Get the number of bands
#[no_mangle]
pub extern "C" fn get_band_count() -> usize {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .map(|state| state.band_groups.len())
        .unwrap_or(0)
}

/// FFI: Get band number by index (for iteration)
/// Returns 0 if index is out of bounds
#[no_mangle]
pub extern "C" fn get_band_number_by_index(index: usize) -> u8 {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        let mut bands: Vec<u8> = state.band_groups.keys().copied().collect();
        bands.sort();
        if index < bands.len() {
            return bands[index];
        }
    }
    0
}

/// FFI: Get band display name
/// Caller must free the returned string with free_error_message()
#[no_mangle]
pub unsafe extern "C" fn get_band_name(band_num: u8) -> *const c_char {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if let Some(name) = state.band_display_names.get(&band_num) {
            return CString::new(name.as_str())
                .unwrap_or_else(|_| CString::new("").unwrap())
                .into_raw();
        }
    }
    CString::new("Unknown Band").unwrap().into_raw()
}

/// FFI: Get number of memories in a specific band
#[no_mangle]
pub extern "C" fn get_band_memory_count(band_num: u8) -> usize {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .and_then(|state| state.band_groups.get(&band_num))
        .map(|indices| indices.len())
        .unwrap_or(0)
}

/// FFI: Get memory data by band and row within that band
#[no_mangle]
pub extern "C" fn get_memory_by_band_row(band_num: u8, row: usize) -> RowData {
    let data = MEMORY_DATA.lock().unwrap();

    if let Some(state) = data.as_ref() {
        if let Some(indices) = state.band_groups.get(&band_num) {
            if row < indices.len() {
                let mem_idx = indices[row];
                if mem_idx < state.cstrings.len() {
                    let row_cstrings = &state.cstrings[mem_idx];
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
                        bank: row_cstrings[12].as_ptr(),
                    };
                }
            }
        }
    }

    // Return empty row if not found
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
        bank: EMPTY.as_ptr() as *const c_char,
    }
}

/// FFI: Convert band+row to global memory index
/// Returns the global index, or -1 if invalid
#[no_mangle]
pub extern "C" fn get_global_index_from_band_row(band_num: u8, row: usize) -> isize {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if let Some(indices) = state.band_groups.get(&band_num) {
            if row < indices.len() {
                return indices[row] as isize;
            }
        }
    }
    -1
}

/// FFI: Check if memories have bank/group organization
/// Returns true if there are memories organized into banks/groups
#[no_mangle]
pub extern "C" fn has_bank_organization() -> bool {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .map(|state| !state.bank_groups.is_empty() && state.bank_groups.len() > 1)
        .unwrap_or(false)
}

/// FFI: Get the number of unique banks/groups
#[no_mangle]
pub extern "C" fn get_bank_count() -> usize {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .map(|state| state.bank_groups.len())
        .unwrap_or(0)
}

/// FFI: Get bank number by index (for iteration)
/// Returns 0 if index is out of bounds
#[no_mangle]
pub extern "C" fn get_bank_number_by_index(index: usize) -> u8 {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        let mut banks: Vec<u8> = state.bank_groups.keys().copied().collect();
        banks.sort();
        if index < banks.len() {
            return banks[index];
        }
    }
    0
}

/// FFI: Get bank name by bank number
/// Returns allocated C string that must be freed with free_error_message()
#[no_mangle]
pub unsafe extern "C" fn get_bank_name_by_number(bank_num: u8) -> *const c_char {
    let data = MEMORY_DATA.lock().unwrap();

    let bank_name = if let Some(state) = data.as_ref() {
        if bank_num < state.bank_names.len() as u8 {
            state.bank_names[bank_num as usize].clone()
        } else {
            format!("Bank {}", bank_num)
        }
    } else {
        format!("Bank {}", bank_num)
    };

    CString::new(bank_name).unwrap().into_raw()
}

/// FFI: Get number of memories in a specific bank
#[no_mangle]
pub extern "C" fn get_bank_memory_count(bank_num: u8) -> usize {
    let data = MEMORY_DATA.lock().unwrap();
    data.as_ref()
        .and_then(|state| state.bank_groups.get(&bank_num))
        .map(|indices| indices.len())
        .unwrap_or(0)
}

/// FFI: Get memory data by bank and row within that bank
#[no_mangle]
pub extern "C" fn get_memory_by_bank_row(bank_num: u8, row: usize) -> RowData {
    let data = MEMORY_DATA.lock().unwrap();

    if let Some(state) = data.as_ref() {
        if let Some(indices) = state.bank_groups.get(&bank_num) {
            if row < indices.len() {
                let mem_idx = indices[row];
                if mem_idx < state.cstrings.len() {
                    let row_cstrings = &state.cstrings[mem_idx];
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
                        bank: row_cstrings[12].as_ptr(),
                    };
                }
            }
        }
    }

    // Return empty row if not found
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
        bank: EMPTY.as_ptr() as *const c_char,
    }
}

/// FFI: Convert bank+row to global memory index
/// Returns the global index, or -1 if invalid
#[no_mangle]
pub extern "C" fn get_global_index_from_bank_row(bank_num: u8, row: usize) -> isize {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if let Some(indices) = state.bank_groups.get(&bank_num) {
            if row < indices.len() {
                return indices[row] as isize;
            }
        }
    }
    -1
}

/// Initialize memory data for display
/// Build band groups and display names from memories
fn build_band_info(memories: &[Memory]) -> (std::collections::HashMap<u8, Vec<usize>>, std::collections::HashMap<u8, String>) {
    use std::collections::HashMap;

    let mut band_groups: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut band_display_names: HashMap<u8, String> = HashMap::new();

    // Group memories by band
    for (idx, mem) in memories.iter().enumerate() {
        if let Some(band_num) = mem.band {
            band_groups.entry(band_num).or_default().push(idx);

            // Set display name for this band if not already set
            if !band_display_names.contains_key(&band_num) {
                let display_name = match band_num {
                    1 => "VHF (144 MHz)",
                    2 => "UHF (430 MHz)",
                    3 => "1.2 GHz (1240 MHz)",
                    _ => "Unknown Band",
                };
                band_display_names.insert(band_num, display_name.to_string());
            }
        }
    }

    (band_groups, band_display_names)
}

fn set_memory_data(memories: Vec<Memory>, bank_names: Vec<String>) {
    // Convert all memories to CStrings and store them
    let mut all_cstrings = Vec::new();

    for mem in &memories {
        let strings = memory_to_row_strings(mem, &bank_names);
        let cstrings: Vec<CString> = strings
            .into_iter()
            .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
            .collect();
        all_cstrings.push(cstrings);
    }

    // Build band organization
    let (band_groups, band_display_names) = build_band_info(&memories);

    // Build bank/group organization
    let bank_groups = build_bank_info(&memories);

    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories,
        cstrings: all_cstrings,
        current_file: None,
        is_modified: false,
        mmap: None,
        bank_names,
        clipboard: None,
        band_groups,
        band_display_names,
        bank_groups,
    });
}

/// Build bank/group information from memories
/// Returns a HashMap mapping bank numbers to vectors of memory indices
fn build_bank_info(memories: &[Memory]) -> std::collections::HashMap<u8, Vec<usize>> {
    let mut bank_groups = std::collections::HashMap::new();

    // Group memories by bank number (skip empty memories)
    for (idx, mem) in memories.iter().enumerate() {
        if !mem.empty {
            bank_groups.entry(mem.bank).or_insert_with(Vec::new).push(idx);
        }
    }

    bank_groups
}

/// Clear all memory data
fn clear_memory_data() {
    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories: Vec::new(),
        cstrings: Vec::new(),
        current_file: None,
        is_modified: false,
        mmap: None,
        bank_names: (0..10).map(|i| format!("Bank {}", i)).collect(),
        clipboard: None,
        band_groups: std::collections::HashMap::new(),
        band_display_names: std::collections::HashMap::new(),
        bank_groups: std::collections::HashMap::new(),
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
        Err(e) => {
            let err_msg = format!("Invalid file path encoding: {}", e);
            tracing::error!("load_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };
    let path = PathBuf::from(path_str);

    tracing::debug!("load_file called: {}", path.display());

    // Load the .img file
    let (mmap, metadata) = match load_img(&path) {
        Ok(data) => data,
        Err(e) => {
            let err_msg = format!("Failed to load file: {}", e);
            tracing::error!("load_file: {}", err_msg);
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
            tracing::error!("load_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Parse memories and bank names from the memmap
    let (memories, bank_names) = if driver_info.model == "TH-D75" || driver_info.model == "TH-D74" {
        use crate::drivers::thd75::THD75Radio;
        let mut radio = THD75Radio::new();
        radio.mmap = Some(mmap.clone());

        let mems = match radio.get_memories() {
            Ok(mems) => mems,
            Err(e) => {
                let err_msg = format!("Failed to parse memories: {}", e);
                tracing::error!("load_file: {}", err_msg);
                return CString::new(err_msg).unwrap().into_raw();
            }
        };

        let names = radio
            .get_bank_names()
            .unwrap_or_else(|_| (0..10).map(|i| format!("Bank {}", i)).collect());

        (mems, names)
    } else {
        let err_msg = format!("Unsupported radio model: {}", driver_info.model);
        tracing::error!("load_file: {}", err_msg);
        return CString::new(err_msg).unwrap().into_raw();
    };

    tracing::info!(
        "File loaded successfully: {} memories from {} {}",
        memories.len(),
        vendor,
        model
    );

    // Convert to CStrings (include all memories, even empty ones)
    let mut all_cstrings = Vec::new();
    for mem in &memories {
        let strings = memory_to_row_strings(mem, &bank_names);
        let cstrings: Vec<CString> = strings
            .into_iter()
            .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
            .collect();
        all_cstrings.push(cstrings);
    }

    // Build band organization
    let (band_groups, band_display_names) = build_band_info(&memories);

    // Build bank/group organization
    let bank_groups = build_bank_info(&memories);

    // Update global state
    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories,
        cstrings: all_cstrings,
        current_file: Some(path),
        is_modified: false,
        mmap: Some(mmap),
        bank_names,
        clipboard: None,
        band_groups,
        band_display_names,
        bank_groups,
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
        Err(e) => {
            let err_msg = format!("Invalid file path encoding: {}", e);
            tracing::error!("save_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };
    let path = PathBuf::from(path_str);

    tracing::debug!("save_file called: {}", path.display());

    let mut data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_mut() {
        Some(s) => s,
        None => {
            let err_msg = "No data to save";
            tracing::error!("save_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Check if this is a multi-band radio (IC-9700)
    let has_bands = !state.band_groups.is_empty();
    if has_bands {
        let err_msg = "Saving .img files is not supported for multi-band radios like IC-9700. Use CSV export instead or upload directly to radio.";
        tracing::error!("save_file: {}", err_msg);
        return CString::new(err_msg).unwrap().into_raw();
    }

    use crate::drivers::thd75::THD75Radio;
    use crate::drivers::{CloneModeRadio, Radio};
    use crate::formats::{save_img, Metadata};

    // Get the mmap - must have been loaded from file or download
    let base_mmap = match &state.mmap {
        Some(m) => m.clone(),
        None => {
            let err_msg =
                "No memory map available. Please load from file or download from radio first.";
            tracing::error!("save_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Create radio and load the base mmap
    let mut radio = THD75Radio::new();
    if let Err(e) = radio.process_mmap(&base_mmap) {
        let err_msg = format!("Failed to process memory map: {}", e);
        tracing::error!("save_file: {}", err_msg);
        return CString::new(err_msg).unwrap().into_raw();
    }

    // Update only non-empty memories
    // Empty memories should be left as-is in the original mmap
    for mem in &state.memories {
        if !mem.empty {
            if let Err(e) = radio.set_memory(mem) {
                let err_msg = format!("Failed to update memory #{}: {}", mem.number, e);
                tracing::error!("save_file: {}", err_msg);
                return CString::new(err_msg).unwrap().into_raw();
            }
        }
    }

    // Get the updated mmap (preserves bank names and all other data)
    let mmap = match radio.mmap.clone() {
        Some(m) => m,
        None => {
            let err_msg = "Memory map not available after update";
            tracing::error!("save_file: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Create metadata
    let metadata = Metadata::new("Kenwood", "TH-D75");

    // Save to file
    if let Err(e) = save_img(&path, &mmap, &metadata) {
        let err_msg = format!("Failed to save file: {}", e);
        tracing::error!("save_file: {}", err_msg);
        return CString::new(err_msg).unwrap().into_raw();
    }

    tracing::info!("File saved successfully: {}", path.display());

    // Update state
    state.current_file = Some(path);
    state.is_modified = false;

    // Return NULL to indicate success
    std::ptr::null()
}

/// FFI: Export memories to CSV file
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn export_to_csv(path: *const c_char) -> *const c_char {
    // Convert C string to Rust PathBuf
    let c_str = CStr::from_ptr(path);
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("Invalid file path encoding: {}", e);
            tracing::error!("export_to_csv: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };
    let path = PathBuf::from(path_str);

    tracing::debug!("export_to_csv called: {}", path.display());

    let data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_ref() {
        Some(s) => s,
        None => {
            let err_msg = "No data to export";
            tracing::error!("export_to_csv: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Export all memories (including empty ones if desired, or filter them)
    use crate::formats::export_csv;
    if let Err(e) = export_csv(&path, &state.memories) {
        let err_msg = format!("Failed to export CSV: {}", e);
        tracing::error!("export_to_csv: {}", err_msg);
        return CString::new(err_msg).unwrap().into_raw();
    }

    tracing::info!("CSV exported successfully: {}", path.display());

    // Return NULL to indicate success
    std::ptr::null()
}

/// FFI: Import memories from CSV file
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub unsafe extern "C" fn import_from_csv(path: *const c_char) -> *const c_char {
    // Convert C string to Rust PathBuf
    let c_str = CStr::from_ptr(path);
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("Invalid file path encoding: {}", e);
            tracing::error!("import_from_csv: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };
    let path = PathBuf::from(path_str);

    tracing::debug!("import_from_csv called: {}", path.display());

    // Import memories from CSV
    use crate::formats::import_csv;
    let memories = match import_csv(&path) {
        Ok(mems) => mems,
        Err(e) => {
            let err_msg = format!("Failed to import CSV: {}", e);
            tracing::error!("import_from_csv: {}", err_msg);
            return CString::new(err_msg).unwrap().into_raw();
        }
    };

    // Use default bank names since CSV doesn't contain memory map
    let bank_names: Vec<String> = (0..10).map(|i| format!("Bank {}", i)).collect();

    // Convert to CStrings
    let mut all_cstrings = Vec::new();
    for mem in &memories {
        let strings = memory_to_row_strings(mem, &bank_names);
        let cstrings: Vec<CString> = strings
            .into_iter()
            .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
            .collect();
        all_cstrings.push(cstrings);
    }

    // Build band organization
    let (band_groups, band_display_names) = build_band_info(&memories);

    tracing::info!("CSV imported successfully: {} memories", memories.len());

    // Build bank/group organization
    let bank_groups = build_bank_info(&memories);

    // Update global state
    let mut data = MEMORY_DATA.lock().unwrap();
    *data = Some(AppState {
        memories,
        cstrings: all_cstrings,
        current_file: None,
        is_modified: true, // Mark as modified since imported from CSV
        mmap: None,        // No memory map from CSV import
        bank_names,
        clipboard: None,
        band_groups,
        band_display_names,
        bank_groups,
    });

    // Return NULL to indicate success
    std::ptr::null()
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

/// FFI: Get the current file path
/// Returns pointer to static string (caller must NOT free), or NULL if no file
#[no_mangle]
pub extern "C" fn get_current_filepath() -> *const c_char {
    static mut FILEPATH_BUF: Option<CString> = None;

    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if let Some(path) = &state.current_file {
            if let Some(path_str) = path.to_str() {
                unsafe {
                    FILEPATH_BUF = Some(CString::new(path_str).unwrap());
                    return FILEPATH_BUF.as_ref().unwrap().as_ptr();
                }
            }
        }
    }
    std::ptr::null()
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
        Ok(ports) => ports
            .into_iter()
            .map(|p| p.port_name)
            .collect::<Vec<_>>()
            .join(","),
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
    let tones_str = TONES
        .iter()
        .map(|t| format!("{:.1}", t))
        .collect::<Vec<_>>()
        .join(",");

    unsafe {
        TONES_BUF = Some(CString::new(tones_str).unwrap());
        TONES_BUF.as_ref().unwrap().as_ptr()
    }
}

/// FFI: Get bank names (comma-separated)
/// Returns bank names from loaded memory map, or default names if no data loaded
#[no_mangle]
pub extern "C" fn get_bank_names() -> *const c_char {
    static mut BANK_NAMES_BUF: Option<CString> = None;

    let data = MEMORY_DATA.lock().unwrap();

    let bank_names = if let Some(state) = data.as_ref() {
        // Try to read bank names from memory map
        if let Some(ref mmap) = state.mmap {
            use crate::drivers::thd75::THD75Radio;
            let mut radio = THD75Radio::new();
            radio.mmap = Some(mmap.clone());

            match radio.get_bank_names() {
                Ok(names) => names.join(","),
                Err(_) => {
                    // Fallback to default names on error
                    (0..10)
                        .map(|i| format!("Bank {}", i))
                        .collect::<Vec<_>>()
                        .join(",")
                }
            }
        } else {
            // No mmap, return default names
            (0..10)
                .map(|i| format!("Bank {}", i))
                .collect::<Vec<_>>()
                .join(",")
        }
    } else {
        // No data loaded, return default names
        (0..10)
            .map(|i| format!("Bank {}", i))
            .collect::<Vec<_>>()
            .join(",")
    };

    unsafe {
        BANK_NAMES_BUF = Some(CString::new(bank_names).unwrap());
        BANK_NAMES_BUF.as_ref().unwrap().as_ptr()
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

        crate::gui::radio_ops::download_from_radio(port_str, vendor_str, model_str, progress_fn)
            .await
    });

    match result {
        Ok((memories, mmap)) => {
            // Get bank names from the downloaded mmap
            let bank_names = {
                use crate::drivers::thd75::THD75Radio;
                use crate::drivers::{CloneModeRadio, Radio};
                let mut radio = THD75Radio::new();
                radio.process_mmap(&mmap).ok();
                radio.get_bank_names().unwrap_or_else(|_| {
                    (0..30).map(|i| format!("Bank {}", i)).collect()
                })
            };

            // Convert to CStrings (include all memories, even empty ones)
            let mut all_cstrings = Vec::new();
            for mem in &memories {
                let strings = memory_to_row_strings(mem, &bank_names);
                let cstrings: Vec<CString> = strings
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
                    .collect();
                all_cstrings.push(cstrings);
            }

            // Build band organization
            let (band_groups, band_display_names) = build_band_info(&memories);

            // Build bank/group organization
            let bank_groups_map = build_bank_info(&memories);

            // Update global state
            let mut data = MEMORY_DATA.lock().unwrap();
            *data = Some(AppState {
                memories,
                cstrings: all_cstrings,
                current_file: None,
                is_modified: false,
                mmap: Some(mmap), // Store mmap from radio download
                bank_names,
                clipboard: None,
                band_groups,
                band_display_names,
                bank_groups: bank_groups_map,
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
                *state =
                    DownloadState::Complete(Err(format!("Failed to create async runtime: {}", e)));
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

            crate::gui::radio_ops::download_from_radio(port_str, vendor_str, model_str, progress_fn)
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
        DownloadState::Complete(Ok((memories, mmap))) => {
            // Get bank names from the downloaded mmap
            let bank_names = {
                use crate::drivers::thd75::THD75Radio;
                use crate::drivers::{CloneModeRadio, Radio};
                let mut radio = THD75Radio::new();
                radio.process_mmap(&mmap).ok();
                radio.get_bank_names().unwrap_or_else(|_| {
                    (0..30).map(|i| format!("Bank {}", i)).collect()
                })
            };

            // Convert to CStrings (include all memories, even empty ones)
            let mut all_cstrings = Vec::new();
            for mem in &memories {
                let strings = memory_to_row_strings(mem, &bank_names);
                let cstrings: Vec<CString> = strings
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
                    .collect();
                all_cstrings.push(cstrings);
            }

            // Build band organization
            let (band_groups, band_display_names) = build_band_info(&memories);

            // Build bank/group organization
            let bank_groups_map = build_bank_info(&memories);

            // Update global state
            let mut data = MEMORY_DATA.lock().unwrap();
            *data = Some(AppState {
                memories,
                cstrings: all_cstrings,
                current_file: None,
                is_modified: false,
                mmap: Some(mmap), // Store mmap from radio download
                bank_names,
                clipboard: None,
                band_groups,
                band_display_names,
                bank_groups: bank_groups_map,
            });

            // Return NULL to indicate success
            std::ptr::null()
        }
        DownloadState::Complete(Err(e)) => {
            let err_msg = format!("Download failed: {}", e);
            tracing::error!("{}", err_msg); // Log GUI errors to console
            CString::new(err_msg).unwrap().into_raw()
        }
        _ => {
            let err_msg = "Download not complete";
            CString::new(err_msg).unwrap().into_raw()
        }
    }
}

/// FFI: Start async upload to radio
#[no_mangle]
pub unsafe extern "C" fn start_upload_async(
    vendor: *const c_char,
    model: *const c_char,
    port: *const c_char,
) {
    // Convert C strings to Rust
    let vendor_str = CStr::from_ptr(vendor).to_str().unwrap_or("").to_string();
    let model_str = CStr::from_ptr(model).to_str().unwrap_or("").to_string();
    let port_str = CStr::from_ptr(port).to_str().unwrap_or("").to_string();

    // Get memories and mmap from current state
    let (memories, mmap) = {
        let data = MEMORY_DATA.lock().unwrap();
        match data.as_ref() {
            Some(state) => {
                let mmap = match &state.mmap {
                    Some(m) => m.clone(),
                    None => {
                        let mut upload_state = UPLOAD_STATE.lock().unwrap();
                        *upload_state = UploadState::Complete(Err(
                            "No memory map available. Please download from radio first."
                                .to_string(),
                        ));
                        return;
                    }
                };
                (state.memories.clone(), mmap)
            }
            None => {
                let mut state = UPLOAD_STATE.lock().unwrap();
                *state = UploadState::Complete(Err("No data loaded".to_string()));
                return;
            }
        }
    };

    // Reset state to InProgress
    {
        let mut state = UPLOAD_STATE.lock().unwrap();
        *state = UploadState::InProgress(UploadProgress {
            current: 0,
            total: 100,
            message: "Initializing...".to_string(),
        });
    }

    // Spawn background thread to do the upload
    thread::spawn(move || {
        // Create tokio runtime
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let mut state = UPLOAD_STATE.lock().unwrap();
                *state =
                    UploadState::Complete(Err(format!("Failed to create async runtime: {}", e)));
                return;
            }
        };

        // Run the upload
        let result = runtime.block_on(async {
            // Progress callback that updates global state
            let progress_fn = Arc::new(|current: usize, total: usize, msg: String| {
                let mut state = UPLOAD_STATE.lock().unwrap();
                *state = UploadState::InProgress(UploadProgress {
                    current,
                    total,
                    message: msg,
                });
            });

            crate::gui::radio_ops::upload_to_radio(
                port_str,
                mmap,
                memories,
                vendor_str,
                model_str,
                progress_fn,
            )
            .await
        });

        // Store result
        let mut state = UPLOAD_STATE.lock().unwrap();
        *state = UploadState::Complete(result);
    });
}

/// FFI: Get upload progress
/// Returns percentage (0-100), or -1 if not started
#[no_mangle]
pub extern "C" fn get_upload_progress(
    out_current: *mut i32,
    out_total: *mut i32,
    out_message: *mut *const c_char,
) -> i32 {
    static mut MESSAGE_BUF: Option<CString> = None;

    let state = UPLOAD_STATE.lock().unwrap();

    match &*state {
        UploadState::InProgress(progress) => {
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

            // Calculate percentage
            if progress.total > 0 {
                ((progress.current as f64 / progress.total as f64) * 100.0) as i32
            } else {
                0
            }
        }
        _ => -1,
    }
}

/// FFI: Check if upload is complete
/// Returns 1 if complete, 0 otherwise
#[no_mangle]
pub extern "C" fn is_upload_complete() -> i32 {
    let state = UPLOAD_STATE.lock().unwrap();
    match &*state {
        UploadState::Complete(_) => 1,
        _ => 0,
    }
}

/// FFI: Get upload result
/// Returns NULL on success, or error message on failure
#[no_mangle]
pub extern "C" fn get_upload_result() -> *const c_char {
    let mut state = UPLOAD_STATE.lock().unwrap();
    let result = std::mem::replace(&mut *state, UploadState::Idle);

    match result {
        UploadState::Complete(Ok(())) => {
            // Success - return NULL
            std::ptr::null()
        }
        UploadState::Complete(Err(e)) => {
            let err_msg = format!("Upload failed: {}", e);
            tracing::error!("{}", err_msg); // Log GUI errors to console
            CString::new(err_msg).unwrap().into_raw()
        }
        _ => {
            let err_msg = "Upload not complete";
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
    bank: u8,
    urcall: *const c_char,
    rpt1call: *const c_char,
    rpt2call: *const c_char,
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
    let urcall_str = CStr::from_ptr(urcall).to_str().unwrap_or("").to_string();
    let rpt1call_str = CStr::from_ptr(rpt1call).to_str().unwrap_or("").to_string();
    let rpt2call_str = CStr::from_ptr(rpt2call).to_str().unwrap_or("").to_string();

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
    mem.bank = bank;
    mem.dv_urcall = urcall_str;
    mem.dv_rpt1call = rpt1call_str;
    mem.dv_rpt2call = rpt2call_str;
    mem.modified = true; // Mark memory as modified for efficient upload

    // Regenerate CStrings for this row
    let bank_names = &state.bank_names;
    let strings = memory_to_row_strings(mem, bank_names);
    let cstrings: Vec<CString> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    state.cstrings[row] = cstrings;

    // Rebuild bank organization since the memory may have moved to a different bank
    state.bank_groups = build_bank_info(&state.memories);

    // Mark as modified
    state.is_modified = true;

    // Return NULL to indicate success
    std::ptr::null()
}

/// Delete memory at the given row (marks it as empty)
#[no_mangle]
pub extern "C" fn delete_memory_at(row: usize) -> *const c_char {
    let mut data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_mut() {
        Some(s) => s,
        None => return CString::new("No data loaded").unwrap().into_raw(),
    };

    if row >= state.memories.len() {
        return CString::new("Invalid row index").unwrap().into_raw();
    }

    // Mark the memory as empty instead of removing it
    let mem = &mut state.memories[row];
    mem.empty = true;
    mem.freq = 0;
    mem.name = String::new();
    mem.duplex = String::new();
    mem.offset = 0;
    mem.mode = String::new();
    mem.tmode = String::new();
    mem.rtone = 0.0;
    mem.ctone = 0.0;
    mem.dv_urcall = String::new();
    mem.dv_rpt1call = String::new();
    mem.dv_rpt2call = String::new();
    mem.modified = true; // Mark as modified for efficient upload (will erase on radio)

    // Regenerate CStrings for this row
    let bank_names = &state.bank_names;
    let strings = memory_to_row_strings(mem, bank_names);
    let cstrings: Vec<CString> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    state.cstrings[row] = cstrings;

    // Mark as modified
    state.is_modified = true;

    std::ptr::null()
}

/// Copy memory at the given row to clipboard
#[no_mangle]
pub extern "C" fn copy_memory_at(row: usize) {
    let mut data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_mut() {
        if row < state.memories.len() {
            state.clipboard = Some(state.memories[row].clone());
        }
    }
}

/// Paste clipboard memory at the given row (replaces existing)
#[no_mangle]
pub extern "C" fn paste_memory_at(row: usize) -> *const c_char {
    let mut data = MEMORY_DATA.lock().unwrap();
    let state = match data.as_mut() {
        Some(s) => s,
        None => return CString::new("No data loaded").unwrap().into_raw(),
    };

    if row >= state.memories.len() {
        return CString::new("Invalid row index").unwrap().into_raw();
    }

    let clipboard_mem = match &state.clipboard {
        Some(mem) => mem.clone(),
        None => return CString::new("No memory in clipboard").unwrap().into_raw(),
    };

    // Preserve the target memory's channel number and band
    let target_number = state.memories[row].number;
    let target_band = state.memories[row].band;

    // Update the memory at the row
    let mut new_mem = clipboard_mem;
    new_mem.number = target_number;  // Keep the target slot's channel number
    new_mem.band = target_band;      // Keep the target slot's band assignment
    new_mem.modified = true;         // Mark as modified for efficient upload
    state.memories[row] = new_mem.clone();

    // Regenerate CStrings for this row
    let bank_names = &state.bank_names;
    let strings = memory_to_row_strings(&new_mem, bank_names);
    let cstrings: Vec<CString> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    state.cstrings[row] = cstrings;

    // Mark as modified
    state.is_modified = true;

    std::ptr::null()
}

/// Check if there's a memory in the clipboard
#[no_mangle]
pub extern "C" fn has_clipboard_memory() -> i32 {
    let data = MEMORY_DATA.lock().unwrap();
    if let Some(state) = data.as_ref() {
        if state.clipboard.is_some() {
            return 1;
        }
    }
    0
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
    let default_bank_names: Vec<String> = (0..10).map(|i| format!("Bank {}", i)).collect();
    set_memory_data(test_memories, default_bank_names);

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

            // Create central widget with BOTH table and tree (so menus can reference them)
            QWidget* centralWidget = new QWidget(window);
            QVBoxLayout* layout = new QVBoxLayout(centralWidget);

            // Create splitter for split view (tree on left, table on right)
            QSplitter* splitter = new QSplitter(Qt::Horizontal, centralWidget);

            // Create tree widget for band selection (left side)
            QTreeWidget* tree = new QTreeWidget(splitter);
            tree->setColumnCount(1);
            tree->setHeaderLabel("Memory Bands");
            tree->setAlternatingRowColors(true);
            tree->setSelectionBehavior(QTreeWidget::SelectRows);
            tree->setMaximumWidth(250);  // Limit tree width
            tree->setMinimumWidth(150);
            tree->hide();  // Initially hidden for single-band radios

            // Create table widget for memory display (right side)
            QTableWidget* table = new QTableWidget(splitter);
            table->setColumnCount(12);  // Removed "Bank" column
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

            // Add splitter to layout
            layout->addWidget(splitter);
            centralWidget->setLayout(layout);
            window->setCentralWidget(centralWidget);

            // Connect tree selection to table update
            QObject::connect(tree, &QTreeWidget::currentItemChanged,
                [=](QTreeWidgetItem* current, QTreeWidgetItem* previous) {
                    if (current) {
                        // Check if this is a band or bank filter
                        bool is_bank_filter = current->data(0, Qt::UserRole + 1).toBool();
                        uint8_t num = current->data(0, Qt::UserRole).toUInt();

                        if (is_bank_filter && num == 255) {
                            // "All Memories" selected
                            refreshTable(table);
                        } else if (is_bank_filter) {
                            // Bank/group selected
                            refreshTableForBank(table, num);
                        } else {
                            // Band selected (or old-style tree item without bank flag)
                            refreshTableForBand(table, num);
                        }
                    }
                });

            // Unified refresh function that checks for band/bank organization
            auto refreshMemoryView = [=]() {
                if (has_band_organization()) {
                    // Multi-band radio: show tree and table in split view
                    tree->show();
                    refreshTreeWithBands(tree);
                    // Table will be updated via tree selection signal
                } else if (has_bank_organization()) {
                    // Radio with banks/groups: show tree and table in split view
                    tree->show();
                    refreshTreeWithBanks(tree);
                    // Table will be updated via tree selection signal (defaults to "All")
                } else {
                    // Single-band, single-bank radio: hide tree, show full-width table
                    tree->hide();
                    refreshTable(table);
                }
            };

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
                        refreshMemoryView();
                        const char* filename = get_current_filename();
                        window->setWindowTitle(QString("CHIRP-RS - %1").arg(QString::fromUtf8(filename)));
                    }
                }
            });

            fileMenu->addAction("&Save", [=]() {
                const char* filepath = get_current_filepath();
                if (!filepath) {
                    // No current file, show Save As dialog
                    QString fileName = QFileDialog::getSaveFileName(window,
                        "Save CHIRP Image", "", "CHIRP Image (*.img)");
                    if (fileName.isEmpty()) {
                        return;
                    }
                    const char* error = save_file(fileName.toUtf8().constData());
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Failed to Save File",
                            QString("Could not save file:\n%1\n\nError: %2")
                            .arg(fileName).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        // Update window title with new filename
                        const char* filename = get_current_filename();
                        window->setWindowTitle(QString("CHIRP-RS - %1").arg(QString::fromUtf8(filename)));
                        QMessageBox::information(window, "Save Successful",
                            QString("File saved successfully:\n%1").arg(fileName));
                    }
                } else {
                    // Save to current file
                    const char* error = save_file(filepath);
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Failed to Save File",
                            QString("Could not save file:\n%1\n\nError: %2")
                            .arg(QString::fromUtf8(filepath)).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        QMessageBox::information(window, "Save Successful",
                            QString("File saved successfully:\n%1")
                            .arg(QString::fromUtf8(filepath)));
                    }
                }
            });

            fileMenu->addAction("Save &As...", [=]() {
                QString fileName = QFileDialog::getSaveFileName(window,
                    "Save CHIRP Image", "", "CHIRP Image (*.img)");
                if (!fileName.isEmpty()) {
                    const char* error = save_file(fileName.toUtf8().constData());
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Failed to Save File",
                            QString("Could not save file:\n%1\n\nError: %2")
                            .arg(fileName).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        // Update window title with new filename
                        const char* filename = get_current_filename();
                        window->setWindowTitle(QString("CHIRP-RS - %1").arg(QString::fromUtf8(filename)));
                        QMessageBox::information(window, "Save Successful",
                            QString("File saved successfully:\n%1").arg(fileName));
                    }
                }
            });

            fileMenu->addSeparator();

            fileMenu->addAction("&Import from CSV...", [=]() {
                QString fileName = QFileDialog::getOpenFileName(window,
                    "Import from CSV", "", "CSV Files (*.csv)");
                if (!fileName.isEmpty()) {
                    const char* error = import_from_csv(fileName.toUtf8().constData());
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Import Failed",
                            QString("Could not import CSV file:\n%1\n\nError: %2\n\n"
                                   "Please ensure:\n"
                                   "• File is a valid CSV file\n"
                                   "• File has correct column headers\n"
                                   "• File is not corrupted")
                            .arg(fileName).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        refreshMemoryView();
                        window->setWindowTitle("CHIRP-RS - Imported from CSV");
                        QMessageBox::information(window, "Import Successful",
                            QString("Successfully imported %1 memories from CSV")
                                .arg(get_memory_count()));
                    }
                }
            });

            fileMenu->addAction("&Export to CSV...", [=]() {
                QString fileName = QFileDialog::getSaveFileName(window,
                    "Export to CSV", "", "CSV Files (*.csv)");
                if (!fileName.isEmpty()) {
                    const char* error = export_to_csv(fileName.toUtf8().constData());
                    if (error) {
                        QString errorMsg = QString::fromUtf8(error);
                        QMessageBox::critical(window, "Export Failed",
                            QString("Could not export to CSV:\n%1\n\nError: %2")
                            .arg(fileName).arg(errorMsg));
                        free_error_message(error);
                    } else {
                        QMessageBox::information(window, "Export Successful",
                            QString("Successfully exported %1 memories to CSV:\n%2")
                                .arg(get_memory_count()).arg(fileName));
                    }
                }
            });

            fileMenu->addSeparator();
            fileMenu->addAction("E&xit", &app, &QApplication::quit);

            // Radio menu
            QMenu* radioMenu = menuBar->addMenu("&Radio");
            radioMenu->addAction("&Download from Radio", [=]() {
                showDownloadDialog(window, table, tree);
            });
            radioMenu->addAction("&Upload to Radio", [=]() {
                showUploadDialog(window, table, tree);
            });

            // Enable context menu on table
            table->setContextMenuPolicy(Qt::CustomContextMenu);

            // Connect context menu request
            QObject::connect(table, &QTableWidget::customContextMenuRequested,
                [=](const QPoint& pos) {
                    QTableWidgetItem* item = table->itemAt(pos);
                    if (!item) return;

                    int row = item->row();

                    // If multi-band mode, convert band+row to global index
                    int globalRow = row;
                    if (has_band_organization() && tree->currentItem()) {
                        uint8_t band_num = tree->currentItem()->data(0, Qt::UserRole).toUInt();
                        intptr_t globalIndex = get_global_index_from_band_row(band_num, row);
                        if (globalIndex >= 0) {
                            globalRow = globalIndex;
                        }
                    }

                    QMenu contextMenu;

                    // Cut, Copy, Paste, Clear operations
                    QAction* cutAction = contextMenu.addAction("Cut");
                    QAction* copyAction = contextMenu.addAction("Copy");
                    QAction* pasteAction = contextMenu.addAction("Paste");
                    contextMenu.addSeparator();
                    QAction* clearAction = contextMenu.addAction("Clear");

                    // Show menu and handle selection
                    QAction* selectedAction = contextMenu.exec(table->viewport()->mapToGlobal(pos));

                    if (selectedAction == cutAction) {
                        // Copy then clear
                        copy_memory_at(globalRow);
                        const char* error = delete_memory_at(globalRow);
                        if (error) {
                            QMessageBox::warning(window, "Error", QString::fromUtf8(error));
                            free_error_message(error);
                        } else {
                            refreshCurrentBandTable(table, tree);
                        }
                    } else if (selectedAction == copyAction) {
                        copy_memory_at(globalRow);
                    } else if (selectedAction == pasteAction) {
                        const char* error = paste_memory_at(globalRow);
                        if (error) {
                            QMessageBox::warning(window, "Error", QString::fromUtf8(error));
                            free_error_message(error);
                        } else {
                            refreshCurrentBandTable(table, tree);
                        }
                    } else if (selectedAction == clearAction) {
                        const char* error = delete_memory_at(globalRow);
                        if (error) {
                            QMessageBox::warning(window, "Error", QString::fromUtf8(error));
                            free_error_message(error);
                        } else {
                            refreshCurrentBandTable(table, tree);
                        }
                    }
                });

            // Connect double-click event to edit dialog
            // Connect table double-click to edit dialog
            QObject::connect(table, &QTableWidget::cellDoubleClicked,
                [=](int row, int column) {
                    // If multi-band mode, convert band+row to global index
                    int globalRow = row;
                    if (has_band_organization() && tree->currentItem()) {
                        uint8_t band_num = tree->currentItem()->data(0, Qt::UserRole).toUInt();
                        intptr_t globalIndex = get_global_index_from_band_row(band_num, row);
                        if (globalIndex >= 0) {
                            globalRow = globalIndex;
                        }
                    }
                    // If bank mode, convert bank+row to global index
                    else if (has_bank_organization() && tree->currentItem()) {
                        uint8_t bank_num = tree->currentItem()->data(0, Qt::UserRole).toUInt();
                        intptr_t globalIndex = get_global_index_from_bank_row(bank_num, row);
                        if (globalIndex >= 0) {
                            globalRow = globalIndex;
                        }
                    }
                    showEditDialog(window, table, tree, globalRow);
                });

            // Populate view with initial data (table or tree based on band organization)
            refreshMemoryView();

            // Show window
            window->show();

            return app.exec();
        })
    }
}
