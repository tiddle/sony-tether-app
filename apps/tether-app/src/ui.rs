use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use backend_sdk::{SDK_CONFIGURED, SdkBackend};
use camera_core::{
    BackendEvent, ButtonState, CameraBackend, CameraCapabilities, CameraInfo, CapturedImage,
    FeatureSupport, FrameRef, Session,
};
use eframe::egui;

use crate::review::PhotoView;

const LOG_LINES_SHOWN: usize = 8;
const LOG_LINES_KEPT: usize = 200;

/// Where the app thinks the connection stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Disconnected,
    Connecting,
    Live,
    Failed(String),
}

/// Which view fills the central area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Live,
    Review,
}

impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Live => "Live view",
            Self::Review => "Last capture",
        }
    }
}

/// One capture the camera reported, plus the file it reached on disk once saved.
struct CaptureRow {
    image: CapturedImage,
    saved: Option<PathBuf>,
}

pub struct TetherApp {
    backend: Box<dyn CameraBackend>,
    events_tx: Sender<BackendEvent>,
    events_rx: Receiver<BackendEvent>,

    status: Status,
    info: Option<CameraInfo>,
    capabilities: CameraCapabilities,

    session: Session,
    folder_text: String,

    last_frame: Option<FrameRef>,
    captures: Vec<CaptureRow>,
    view: View,
    photo: PhotoView,
    log: Vec<String>,
}

impl TetherApp {
    pub fn new() -> Self {
        let (events_tx, events_rx) = channel();

        let capabilities = match CameraCapabilities::embedded() {
            Ok(caps) => caps,
            Err(err) => {
                eprintln!("capability matrix failed to load: {err}");
                CameraCapabilities::empty()
            }
        };

        let root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("captures");

        let newest = camera_core::session::newest_viewable(&root);
        let mut photo = PhotoView::new();
        photo.show(newest.clone());
        let view = if newest.is_some() {
            View::Review
        } else {
            View::Live
        };

        let mut app = Self {
            backend: Box::new(SdkBackend::new()),
            events_tx,
            events_rx,
            status: Status::Disconnected,
            info: None,
            capabilities,
            session: Session::new(&root, "capture"),
            folder_text: root.display().to_string(),
            last_frame: None,
            captures: Vec::new(),
            view,
            photo,
            log: Vec::new(),
        };

        app.push_log(format!("backend: {}", app.backend.name()));
        if !SDK_CONFIGURED {
            app.push_log("Sony SDK not configured in this build (SONY_SDK_ROOT unset)".into());
        }
        if let Some(path) = &newest {
            app.push_log(format!("showing the newest capture: {}", path.display()));
        }
        app
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > LOG_LINES_KEPT {
            self.log.remove(0);
        }
    }

    fn is_connected(&self) -> bool {
        matches!(self.status, Status::Live)
    }

    fn connect(&mut self) {
        self.status = Status::Connecting;
        let result = self.backend.connect(self.events_tx.clone());
        match result {
            Ok(()) => self.push_log("connect: session requested".into()),
            Err(err) => {
                self.push_log(format!("connect failed: {err}"));
                self.status = Status::Failed(err.to_string());
            }
        }
    }

    fn disconnect(&mut self) {
        if let Err(err) = self.backend.disconnect() {
            self.push_log(format!("disconnect failed: {err}"));
        }
        self.status = Status::Disconnected;
        self.info = None;
        self.last_frame = None;
    }

    fn shutter(&mut self, state: ButtonState) {
        if let Err(err) = self.backend.shutter(state) {
            self.push_log(format!("shutter {state:?} failed: {err}"));
        }
    }

    /// Save the most recent capture into the session folder.
    ///
    /// Blocking on purpose for now: the download worker thread arrives in M2.
    fn save_last_capture(&mut self) {
        let Some(row) = self.captures.last() else {
            self.push_log("nothing to save: no capture received yet".into());
            return;
        };
        let image = row.image.clone();

        let extension = image
            .filename_hint
            .rsplit('.')
            .next()
            .filter(|ext| !ext.is_empty() && *ext != image.filename_hint.as_str())
            .unwrap_or("ARW")
            .to_uppercase();

        let path = match self.session.peek_path(&extension) {
            Ok(path) => path,
            Err(err) => {
                self.push_log(format!("cannot build a filename: {err}"));
                return;
            }
        };

        if let Err(err) = self.session.ensure_root() {
            self.push_log(format!(
                "cannot create {}: {err}",
                self.session.root().display()
            ));
            return;
        }

        match self.backend.download(&image, &path) {
            Ok(()) => {
                self.session.reserve_through(1);
                if let Some(row) = self.captures.last_mut() {
                    row.saved = Some(path.clone());
                }
                self.photo.show(Some(path.clone()));
                self.view = View::Review;
                self.push_log(format!("saved {}", path.display()));
            }
            Err(err) => self.push_log(format!("download failed: {err}")),
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                BackendEvent::Connected(info) => {
                    self.push_log(format!(
                        "connected: {} ({}) firmware {} over {:?}",
                        info.model,
                        info.id,
                        info.firmware.as_deref().unwrap_or("unknown"),
                        info.transport
                    ));
                    self.info = Some(info);
                    self.status = Status::Live;
                    self.view = View::Live;
                }
                BackendEvent::Disconnected => {
                    self.push_log("camera disconnected".into());
                    self.info = None;
                    self.last_frame = None;
                    self.status = Status::Disconnected;
                }
                BackendEvent::Frame(frame) => self.last_frame = Some(frame),
                BackendEvent::Captured(image) => {
                    self.push_log(format!(
                        "captured {} ({} bytes)",
                        image.filename_hint, image.size_bytes
                    ));
                    self.captures.push(CaptureRow { image, saved: None });
                }
                BackendEvent::DownloadProgress {
                    received, total, ..
                } => {
                    if total > 0 && received >= total {
                        self.push_log(format!("download complete ({total} bytes)"));
                    }
                }
                BackendEvent::Log(line) => self.push_log(line),
                BackendEvent::Error(message) => {
                    self.push_log(format!("error: {message}"));
                    self.status = Status::Failed(message);
                }
            }
        }
    }

    fn status_line(&self) -> (String, egui::Color32) {
        match &self.status {
            Status::Disconnected => ("disconnected".to_owned(), egui::Color32::GRAY),
            Status::Connecting => ("connecting...".to_owned(), egui::Color32::YELLOW),
            Status::Live => match &self.info {
                Some(info) => (
                    format!("{} connected over USB", info.model),
                    egui::Color32::LIGHT_GREEN,
                ),
                None => ("connected".to_owned(), egui::Color32::LIGHT_GREEN),
            },
            Status::Failed(message) => (format!("failed: {message}"), egui::Color32::LIGHT_RED),
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Sony tether");
            ui.label(egui::RichText::new("pre-alpha").weak());
            ui.separator();

            let connected = self.is_connected();
            if ui
                .add_enabled(!connected, egui::Button::new("Connect"))
                .clicked()
            {
                self.connect();
            }
            if ui
                .add_enabled(connected, egui::Button::new("Disconnect"))
                .clicked()
            {
                self.disconnect();
            }

            ui.separator();
            let (text, colour) = self.status_line();
            ui.colored_label(colour, text);
        });

        if !SDK_CONFIGURED {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "This build has no Sony SDK. Set SONY_SDK_ROOT and rebuild to talk to a camera.",
                );
            });
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Session");
        ui.label("Download folder");
        ui.text_edit_singleline(&mut self.folder_text);
        ui.label(
            egui::RichText::new(format!("files land in {}", self.session.root().display())).weak(),
        );
        ui.add_space(8.0);

        ui.separator();
        ui.heading("Captures");
        let mut open: Option<PathBuf> = None;
        if self.captures.is_empty() {
            ui.label(egui::RichText::new("none yet").weak());
        } else {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for row in self.captures.iter().rev() {
                        let label = format!(
                            "{}  {:.1} MB",
                            row.image.filename_hint,
                            row.image.size_bytes as f64 / 1_048_576.0
                        );
                        let saved = row.saved.clone();
                        let clicked = ui
                            .add_enabled_ui(saved.is_some(), |ui| ui.selectable_label(false, label))
                            .inner
                            .clicked();
                        if clicked {
                            open = saved;
                        }
                    }
                });
        }
        if let Some(path) = open {
            self.photo.show(Some(path));
            self.view = View::Review;
        }
        ui.add_enabled_ui(!self.captures.is_empty(), |ui| {
            if ui.button("Save last capture").clicked() {
                self.save_last_capture();
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Capabilities");
        match &self.info {
            None => {
                ui.label(egui::RichText::new("connect a camera to see its row").weak());
            }
            Some(info) => {
                if let Some(entry) = self.capabilities.find(info.id.as_str()) {
                    ui.label(format!("{} ({})", entry.name, entry.id));
                    ui.label(
                        egui::RichText::new(format!(
                            "tested firmware: {}",
                            if entry.tested_firmware.is_empty() {
                                "none recorded".to_owned()
                            } else {
                                entry.tested_firmware.join(", ")
                            }
                        ))
                        .weak(),
                    );
                    for feature in [
                        "live_view",
                        "capture",
                        "download_jpeg",
                        "download_raw",
                        "exposure_control",
                    ] {
                        let support = self.capabilities.support(info.id.as_str(), feature);
                        ui.label(format!("{feature}: {}", support.as_str()));
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "unknown body: every feature is unverified",
                    );
                }
            }
        }
    }

    /// The central area: two views, chosen by a tab strip.
    fn central(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Live, View::Live.label());
            ui.selectable_value(&mut self.view, View::Review, View::Review.label());
            ui.separator();
            if self.view == View::Live {
                let hint = self
                    .photo
                    .path()
                    .and_then(|path| {
                        path.file_name()
                            .map(|name| name.to_string_lossy().to_string())
                    })
                    .map(|name| format!("last capture: {name}"))
                    .unwrap_or_else(|| "no capture yet".to_owned());
                ui.label(egui::RichText::new(hint).weak());
            }
        });
        ui.separator();

        match self.view {
            View::Live => self.live_view(ui),
            View::Review => self.photo.ui(ui),
        }
    }

    fn live_view(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(20));

        let centre = rect.center();
        match &self.last_frame {
            Some(frame) => {
                // M2 draws the decoded frame here. Until then we report what arrived.
                painter.text(
                    centre,
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "frame {}x{}, {} bytes\n(decoding lands in M2)",
                        frame.width,
                        frame.height,
                        frame.jpeg.len()
                    ),
                    egui::FontId::proportional(15.0),
                    egui::Color32::LIGHT_GRAY,
                );
            }
            None => {
                painter.text(
                    centre,
                    egui::Align2::CENTER_CENTER,
                    "no live view",
                    egui::FontId::proportional(18.0),
                    egui::Color32::DARK_GRAY,
                );
            }
        }
    }

    fn shutter_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can_shoot = self.is_connected()
                && self.info.as_ref().is_some_and(|info| {
                    self.capabilities
                        .support(info.id.as_str(), "capture")
                        .allows_ui()
                });

            if ui
                .add_enabled(can_shoot, egui::Button::new("Focus (half press)"))
                .clicked()
            {
                self.shutter(ButtonState::HalfPress);
            }
            if ui
                .add_enabled(can_shoot, egui::Button::new("Shoot"))
                .clicked()
            {
                self.shutter(ButtonState::FullPress);
                self.shutter(ButtonState::Released);
            }

            ui.separator();
            ui.label(egui::RichText::new(self.capture_gate_text()).weak());
        });
    }

    /// Says out loud why the shutter is greyed out. Silent disabled buttons are a
    /// support ticket waiting to happen.
    fn capture_gate_text(&self) -> String {
        match &self.info {
            None => "no camera connected".to_owned(),
            Some(info) => {
                let support = self.capabilities.support(info.id.as_str(), "capture");
                match support {
                    FeatureSupport::Verified => "ready".to_owned(),
                    other => format!("shutter disabled: capture is {}", other.as_str()),
                }
            }
        }
    }

    fn log_bar(&self, ui: &mut egui::Ui) {
        for line in self.log.iter().rev().take(LOG_LINES_SHOWN) {
            ui.label(egui::RichText::new(line).monospace().weak());
        }
    }
}

impl eframe::App for TetherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("log").show(ui, |ui| self.log_bar(ui));
        egui::Panel::right("side")
            .default_size(300.0)
            .show(ui, |ui| self.side_panel(ui));
        egui::Panel::bottom("shutter").show(ui, |ui| self.shutter_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));

        if self.is_connected() {
            // Live view arrives on a backend thread; keep the UI polling.
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}
