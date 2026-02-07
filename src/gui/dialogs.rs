// Dialog widgets for CHIRP-RS GUI

use crate::gui::messages::Message;
use iced::widget::{button, column, container, pick_list, progress_bar, row, text};
use iced::{Alignment, Element, Length};

/// Create a simple error dialog
pub fn error_dialog(message: String) -> Element<'static, Message> {
    container(
        column![
            text("Error").size(24),
            text(message).size(16),
            button("OK").on_press(Message::DismissError),
        ]
        .spacing(20)
        .padding(30)
        .align_items(Alignment::Center),
    )
    .padding(20)
    .width(Length::Fixed(400.0))
    .into()
}

/// Create a download dialog
pub fn download_dialog(
    vendors: Vec<String>,
    models: Vec<String>,
    ports: Vec<String>,
    selected_vendor: Option<String>,
    selected_model: Option<String>,
    selected_port: Option<String>,
    progress: Option<(usize, usize, String)>,
) -> Element<'static, Message> {
    tracing::debug!("download_dialog called");
    tracing::debug!("  vendors: {:?}", vendors);
    tracing::debug!("  models: {:?}", models);
    tracing::debug!("  ports: {:?}", ports);

    let mut content = column![
        text("Download from Radio").size(24),
        text("Select radio model and serial port").size(14),
    ]
    .spacing(15)
    .padding(30)
    .align_items(Alignment::Start);

    // Radio vendor selection
    tracing::debug!("vendors.is_empty() = {}", vendors.is_empty());
    if !vendors.is_empty() {
        tracing::debug!("Adding vendor picker");

        let vendor_list = pick_list(
            vendors,
            selected_vendor.clone(),
            Message::RadioVendorSelected,
        )
        .width(Length::Fixed(200.0));

        content = content.push(
            row![text("Vendor:").width(Length::Fixed(80.0)), vendor_list,]
                .spacing(10)
                .align_items(Alignment::Center),
        );
    }

    // Radio model selection (only if vendor is selected)
    if selected_vendor.is_some() && !models.is_empty() {
        let model_list = pick_list(
            models,
            selected_model.clone(),
            Message::RadioModelSelected,
        )
        .width(Length::Fixed(200.0));

        content = content.push(
            row![text("Model:").width(Length::Fixed(80.0)), model_list,]
                .spacing(10)
                .align_items(Alignment::Center),
        );
    }

    // Serial port selection
    if !ports.is_empty() {
        let port_list = pick_list(
            ports,
            selected_port.clone(),
            Message::SerialPortSelected,
        )
        .width(Length::Fixed(200.0));

        content = content.push(
            row![
                text("Port:").width(Length::Fixed(80.0)),
                port_list,
                button("Refresh").on_press(Message::RefreshSerialPorts),
            ]
            .spacing(10)
            .align_items(Alignment::Center),
        );
    }

    // Progress bar (if operation is in progress)
    if let Some((current, total, ref msg)) = progress {
        let percent = if total > 0 {
            (current as f32 / total as f32)
        } else {
            0.0
        };

        content = content.push(
            column![
                text(msg).size(14),
                progress_bar(0.0..=1.0, percent),
                text(format!("{} / {}", current, total)).size(12),
            ]
            .spacing(5),
        );
    }

    // Buttons
    let can_start = selected_vendor.is_some()
        && selected_model.is_some()
        && selected_port.is_some()
        && progress.is_none();

    let buttons = if progress.is_some() {
        row![button("Cancel").on_press(Message::CloseDialog),].spacing(10)
    } else if can_start {
        row![
            button("Cancel").on_press(Message::CloseDialog),
            button("Download").on_press(Message::DownloadFromRadio),
        ]
        .spacing(10)
    } else {
        row![
            button("Cancel").on_press(Message::CloseDialog),
            button("Download"),
        ]
        .spacing(10)
    };

    content = content.push(buttons);

    container(content)
        .padding(20)
        .width(Length::Fixed(500.0))
        .into()
}

/// Create an upload dialog
pub fn upload_dialog(
    ports: Vec<String>,
    selected_port: Option<String>,
    progress: Option<(usize, usize, String)>,
) -> Element<'static, Message> {
    let mut content = column![
        text("Upload to Radio").size(24),
        text("Select serial port").size(14),
    ]
    .spacing(15)
    .padding(30)
    .align_items(Alignment::Start);

    // Serial port selection
    if !ports.is_empty() {
        let port_list = pick_list(
            ports,
            selected_port.clone(),
            Message::SerialPortSelected,
        )
        .width(Length::Fixed(200.0));

        content = content.push(
            row![
                text("Port:").width(Length::Fixed(80.0)),
                port_list,
                button("Refresh").on_press(Message::RefreshSerialPorts),
            ]
            .spacing(10)
            .align_items(Alignment::Center),
        );
    }

    // Progress bar (if operation is in progress)
    if let Some((current, total, ref msg)) = progress {
        let percent = if total > 0 {
            (current as f32 / total as f32)
        } else {
            0.0
        };

        content = content.push(
            column![
                text(msg).size(14),
                progress_bar(0.0..=1.0, percent),
                text(format!("{} / {}", current, total)).size(12),
            ]
            .spacing(5),
        );
    }

    // Buttons
    let can_start = selected_port.is_some() && progress.is_none();

    let buttons = if progress.is_some() {
        row![button("Cancel").on_press(Message::CloseDialog),].spacing(10)
    } else if can_start {
        row![
            button("Cancel").on_press(Message::CloseDialog),
            button("Upload").on_press(Message::UploadToRadio),
        ]
        .spacing(10)
    } else {
        row![
            button("Cancel").on_press(Message::CloseDialog),
            button("Upload"),
        ]
        .spacing(10)
    };

    content = content.push(buttons);

    container(content)
        .padding(20)
        .width(Length::Fixed(500.0))
        .into()
}

/// Create a modal overlay - simplified version
pub fn modal<'a>(
    background: impl Into<Element<'a, Message>>,
    dialog: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    // For now, just show the dialog - modal overlay requires more complex layout
    // In a real implementation, this would use iced's Stack or custom overlay
    dialog.into()
}
