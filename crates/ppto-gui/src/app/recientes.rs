// SPDX-License-Identifier: GPL-3.0-or-later
//! Últimos presupuestos abiertos o guardados, para volver a ellos desde
//! Archivo → Abrir reciente y desde la ventana «Nuevo presupuesto». Se
//! guardan en `recientes.json` de la carpeta de configuración (solo rutas).

use std::path::{Path, PathBuf};

/// Cuántos se recuerdan.
pub(super) const MAXIMO: usize = 10;

#[derive(Debug, Default, Clone)]
pub(super) struct Recientes {
    /// Del más reciente al más antiguo.
    pub rutas: Vec<PathBuf>,
    /// Fichero donde se guardan (`None` en las pruebas: no se toca el disco).
    fichero: Option<PathBuf>,
}

impl Recientes {
    pub fn en_disco() -> Self {
        let fichero = super::pdf::carpeta_config().map(|c| c.join("recientes.json"));
        let rutas = fichero
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<Vec<PathBuf>>(&t).ok())
            .unwrap_or_default();
        Self { rutas, fichero }
    }

    /// Pone `ruta` la primera (sin repetirla) y guarda la lista.
    pub fn anotar(&mut self, ruta: &Path) {
        let ruta = std::fs::canonicalize(ruta).unwrap_or_else(|_| ruta.to_path_buf());
        let ruta = limpiar_unc(ruta);
        self.rutas.retain(|r| r != &ruta);
        self.rutas.insert(0, ruta);
        self.rutas.truncate(MAXIMO);
        self.guardar();
    }

    pub fn quitar(&mut self, ruta: &Path) {
        self.rutas.retain(|r| r != ruta);
        self.guardar();
    }

    pub fn vaciar(&mut self) {
        self.rutas.clear();
        self.guardar();
    }

    /// Carpeta del último, para empezar ahí el diálogo «Abrir…».
    pub fn carpeta(&self) -> Option<PathBuf> {
        self.rutas
            .iter()
            .filter_map(|r| r.parent())
            .find(|c| c.is_dir())
            .map(Path::to_path_buf)
    }

    fn guardar(&self) {
        let Some(f) = &self.fichero else { return };
        if let Some(c) = f.parent() {
            let _ = std::fs::create_dir_all(c);
        }
        if let Ok(t) = serde_json::to_string_pretty(&self.rutas) {
            let _ = std::fs::write(f, t);
        }
    }
}

/// En Windows `canonicalize` devuelve `\\?\C:\…`; se quita el prefijo para
/// que la ruta se lea bien en el menú.
fn limpiar_unc(r: PathBuf) -> PathBuf {
    match r.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(s) if !s.starts_with("UNC") => PathBuf::from(s),
        _ => r,
    }
}

/// Texto del menú: nombre del fichero y, en pequeño, la carpeta.
pub(super) fn etiqueta(r: &Path) -> String {
    let nombre = r
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| r.display().to_string());
    match r.parent().and_then(|c| c.file_name()) {
        Some(c) => format!("{nombre}   ({})", c.to_string_lossy()),
        None => nombre,
    }
}

impl super::Aplicacion {
    /// Submenú Archivo → Abrir reciente.
    pub(super) fn menu_recientes(&mut self, ui: &mut eframe::egui::Ui) {
        let rutas = self.recientes.rutas.clone();
        ui.add_enabled_ui(!rutas.is_empty(), |ui| {
            ui.menu_button("🕘 Abrir reciente", |ui| {
                let mut abrir = None;
                lista(ui, &rutas, &mut abrir);
                ui.separator();
                if ui.button("Vaciar la lista").clicked() {
                    ui.close();
                    self.recientes.vaciar();
                }
                if let Some(r) = abrir {
                    ui.close();
                    self.abrir(r);
                }
            });
        });
    }
}

/// Botones de la lista de recientes; el que ya no existe sale en gris.
pub(super) fn lista(ui: &mut eframe::egui::Ui, rutas: &[PathBuf], abrir: &mut Option<PathBuf>) {
    for r in rutas {
        let existe = r.exists();
        let b = ui
            .add_enabled(existe, eframe::egui::Button::new(etiqueta(r)))
            .on_hover_text(r.display().to_string())
            .on_disabled_hover_text(format!("Ya no está: {}", r.display()));
        if b.clicked() {
            *abrir = Some(r.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_ultimo_va_primero_sin_repetir_y_con_maximo() {
        let mut r = Recientes::default();
        for i in 0..12 {
            r.anotar(Path::new(&format!("/no/existe/obra{i}.sqlite")));
        }
        assert_eq!(r.rutas.len(), MAXIMO);
        assert_eq!(r.rutas[0], PathBuf::from("/no/existe/obra11.sqlite"));
        r.anotar(Path::new("/no/existe/obra5.sqlite"));
        assert_eq!(r.rutas[0], PathBuf::from("/no/existe/obra5.sqlite"));
        assert_eq!(r.rutas.iter().filter(|x| x.ends_with("obra5.sqlite")).count(), 1);
        r.quitar(Path::new("/no/existe/obra5.sqlite"));
        assert!(!r.rutas.iter().any(|x| x.ends_with("obra5.sqlite")));
    }

    #[test]
    fn etiqueta_con_carpeta() {
        assert_eq!(etiqueta(Path::new("/obras/Uber/medicion.sqlite")), "medicion   (Uber)");
    }

    #[test]
    fn quita_prefijo_unc_de_windows() {
        assert_eq!(
            limpiar_unc(PathBuf::from(r"\\?\C:\obras\a.sqlite")),
            PathBuf::from(r"C:\obras\a.sqlite")
        );
    }
}
