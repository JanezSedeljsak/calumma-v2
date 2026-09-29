//! Exporting: the composite, PSD, SVG, PDF and single layers, saved through a native dialog and
//! reported with a toast.

use super::*;

impl AppController {
    pub fn export_basename(&self) -> String {
        let name = self
            .engine
            .borrow()
            .project_name()
            .unwrap_or_else(|| "export".to_string());
        project_basename(&name)
    }

    pub fn save_export_bytes(&self, bytes: &[u8], extension: &str) -> bool {
        let suggested = format!("{}.{}", self.export_basename(), extension);
        save_bytes(bytes, &suggested, extension)
    }

    pub fn save_export_text(&self, text: &str, extension: &str) -> bool {
        let suggested = format!("{}.{}", self.export_basename(), extension);
        save_text(text, &suggested, extension)
    }

    pub fn export_composite(&self, format: RasterFormat) -> Result<Vec<u8>> {
        self.engine.borrow().export_raster(format)
    }

    pub fn export_psd(&self) -> Result<Vec<u8>> {
        self.engine.borrow().export_psd_bytes()
    }

    pub fn export_svg(&self) -> Result<String> {
        self.engine.borrow().export_svg_string()
    }

    pub fn export_pdf(&self) -> Result<Vec<u8>> {
        self.engine.borrow().export_pdf_bytes()
    }

    pub fn export_layer_at(&self, index: usize) -> Result<(bool, Vec<u8>, Option<String>)> {
        let engine = self.engine.borrow();
        if engine.layer_is_vector(index) {
            let svg = engine.export_layer_svg(index)?;
            return Ok((true, Vec::new(), Some(svg)));
        }
        let bytes = engine.export_layer_raster(index, RasterFormat::Png)?;
        Ok((false, bytes, None))
    }

    pub fn try_save_composite(&self, format: RasterFormat, ext: &str) -> Result<bool> {
        let bytes = self.export_composite(format)?;
        Ok(self.save_export_bytes(&bytes, ext))
    }

    pub fn try_save_psd(&self) -> Result<bool> {
        let bytes = self.export_psd()?;
        Ok(self.save_export_bytes(&bytes, "psd"))
    }

    pub fn try_save_svg(&self) -> Result<bool> {
        let text = self.export_svg()?;
        Ok(self.save_export_text(&text, "svg"))
    }

    pub fn try_save_pdf(&self) -> Result<bool> {
        let bytes = self.export_pdf()?;
        Ok(self.save_export_bytes(&bytes, "pdf"))
    }

    pub fn try_save_layer_export(&self, index: usize) -> Result<bool> {
        let (is_svg, bytes, svg) = self.export_layer_at(index)?;
        if is_svg {
            Ok(self.save_export_text(&svg.unwrap_or_default(), "svg"))
        } else {
            Ok(self.save_export_bytes(&bytes, "png"))
        }
    }

    pub fn notify_export(&mut self, result: Result<bool>) {
        if result.is_err() {
            self.show_toast_key("exportFailed", true);
        }
    }

    pub fn notify_layer_export(&mut self, result: Result<bool>) {
        if result.is_err() {
            self.show_toast_key("exportLayerFailed", true);
        }
    }
}
