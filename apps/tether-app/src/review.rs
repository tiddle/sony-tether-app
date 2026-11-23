//! The "last capture" view: decode one file from disk and draw it.
//!
//! Decoding happens once per file, on the frame where the path changes, and the
//! result is kept in a texture. Redrawing a frame costs one textured quad.

use std::path::{Path, PathBuf};

use camera_core::session;
use eframe::egui;

pub struct PhotoView {
    /// What the caller wants shown. `None` means nothing yet.
    requested: Option<PathBuf>,
    /// What has actually been decoded, so a change can be detected.
    loaded: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
    /// Size of the decoded image in pixels, used to keep the aspect ratio.
    pixels: egui::Vec2,
    /// One line under nothing: file name, dimensions, size on disk.
    caption: String,
    /// Why nothing is drawn, when nothing is drawn.
    reason: String,
}

impl Default for PhotoView {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoView {
    pub fn new() -> Self {
        Self {
            requested: None,
            loaded: None,
            texture: None,
            pixels: egui::Vec2::ZERO,
            caption: String::new(),
            reason: String::new(),
        }
    }

    /// Ask for a file, or for nothing. Cheap: the work happens in `ui`.
    pub fn show(&mut self, path: Option<PathBuf>) {
        if path != self.requested {
            self.requested = path;
            self.loaded = None;
            self.texture = None;
            self.pixels = egui::Vec2::ZERO;
            self.caption.clear();
            self.reason.clear();
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.requested.as_deref()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.requested.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("nothing captured yet").weak());
            });
            return;
        };

        if self.loaded.as_deref() != Some(path.as_path()) {
            self.load(ui.ctx(), &path);
        }

        if !self.caption.is_empty() {
            ui.label(egui::RichText::new(&self.caption).weak());
        }

        let Some(texture) = &self.texture else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(&self.reason).weak());
            });
            return;
        };

        // Fit inside the remaining space without cropping or distorting. Never
        // enlarge past 1:1, because a 33 MP file blown up looks like a bug.
        let available = ui.available_size();
        let scale = (available.x / self.pixels.x)
            .min(available.y / self.pixels.y)
            .min(1.0)
            .max(0.05);
        let target = self.pixels * scale;

        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(
                    texture.id(),
                    target,
                )));
            },
        );
    }

    fn load(&mut self, ctx: &egui::Context, path: &Path) {
        self.loaded = Some(path.to_path_buf());
        self.texture = None;
        self.pixels = egui::Vec2::ZERO;
        self.reason.clear();

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let bytes = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        let size = file_size_text(bytes);
        let extension = session::extension_of(path).unwrap_or_default();

        if session::is_raw_extension(&extension) {
            self.caption = format!("{name}  {size}");
            self.reason = format!(
                ".{extension} holds a JPEG preview inside the file. Extracting it is planned \
                 with RAW support; until then this view shows JPEG and PNG only."
            );
            return;
        }

        if !session::is_viewable_extension(&extension) {
            self.caption = name;
            self.reason = format!("cannot display .{extension}");
            return;
        }

        match image::open(path) {
            Ok(decoded) => {
                let rgba = decoded.to_rgba8();
                let (width, height) = rgba.dimensions();
                self.pixels = egui::vec2(width as f32, height as f32);
                let pixels = egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, height as usize],
                    rgba.as_raw(),
                );
                self.texture = Some(ctx.load_texture(
                    format!("capture:{name}"),
                    pixels,
                    egui::TextureOptions::LINEAR,
                ));
                self.caption = format!("{name}  {width} x {height}  {size}");
            }
            Err(err) => {
                self.caption = name;
                self.reason = format!("cannot decode: {err}");
            }
        }
    }
}

/// Sizes as a person reads them: kilobytes for small files, megabytes for
/// photographs. A 14 kB file shown as "0.0 MB" looks like a bug.
fn file_size_text(bytes: u64) -> String {
    const MIB: f64 = 1_048_576.0;
    let value = bytes as f64;
    if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else {
        format!("{:.0} kB", value / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::file_size_text;

    #[test]
    fn sizes_read_sensibly() {
        assert_eq!(file_size_text(0), "0 kB");
        assert_eq!(file_size_text(14_484), "14 kB");
        assert_eq!(file_size_text(1_048_576), "1.0 MB");
        assert_eq!(file_size_text(33_554_432), "32.0 MB");
    }
}
