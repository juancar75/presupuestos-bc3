// SPDX-License-Identifier: GPL-3.0-or-later
//! Últimos presupuestos abiertos o guardados, para volver a ellos desde
//! Archivo → Abrir reciente y desde la ventana «Nuevo presupuesto». Se
//! guardan en `recientes.json` de la carpeta de configuración: las rutas, un
//! resumen de cada obra para la pantalla de inicio (nombre, cliente, estado
//! y fecha) y si se abre sola la última al arrancar.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Cuántos se recuerdan.
pub(super) const MAXIMO: usize = 10;

/// Resumen de una obra reciente.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct InfoReciente {
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub cliente: String,
    /// Estado de la oferta (ofertada, adjudicada, perdida).
    #[serde(default)]
    pub estado: String,
    /// Última vez que se abrió o se guardó (dd/mm/aaaa).
    #[serde(default)]
    pub fecha: String,
}

/// Lo que se guarda en `recientes.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct EnDisco {
    #[serde(default)]
    rutas: Vec<PathBuf>,
    #[serde(default)]
    info: BTreeMap<PathBuf, InfoReciente>,
    #[serde(default)]
    abrir_ultima: bool,
}

#[derive(Debug, Default, Clone)]
pub(super) struct Recientes {
    /// Del más reciente al más antiguo.
    pub rutas: Vec<PathBuf>,
    pub info: BTreeMap<PathBuf, InfoReciente>,
    /// Al arrancar se abre la última obra (si no, la pantalla de inicio).
    pub abrir_ultima: bool,
    /// Fichero donde se guardan (`None` en las pruebas: no se toca el disco).
    fichero: Option<PathBuf>,
}

impl Recientes {
    pub fn en_disco() -> Self {
        let fichero = super::pdf::carpeta_config().map(|c| c.join("recientes.json"));
        let t = fichero.as_ref().and_then(|f| std::fs::read_to_string(f).ok());
        let mut r = t.as_deref().map(Self::leer).unwrap_or_default();
        r.fichero = fichero;
        r
    }

    /// Lee el formato actual o el antiguo (solo la lista de rutas).
    fn leer(t: &str) -> Self {
        let d = serde_json::from_str::<EnDisco>(t)
            .or_else(|_| {
                serde_json::from_str::<Vec<PathBuf>>(t).map(|rutas| EnDisco {
                    rutas,
                    ..Default::default()
                })
            })
            .unwrap_or_default();
        Self {
            rutas: d.rutas,
            info: d.info,
            abrir_ultima: d.abrir_ultima,
            fichero: None,
        }
    }

    /// Pone `ruta` la primera (sin repetirla) y guarda la lista.
    #[cfg(test)]
    pub fn anotar(&mut self, ruta: &Path) {
        self.anotar_con(ruta, None);
    }

    /// Como [`Self::anotar`], con el resumen de la obra.
    pub fn anotar_con(&mut self, ruta: &Path, info: Option<InfoReciente>) {
        let ruta = std::fs::canonicalize(ruta).unwrap_or_else(|_| ruta.to_path_buf());
        let ruta = limpiar_unc(ruta);
        self.rutas.retain(|r| r != &ruta);
        self.rutas.insert(0, ruta.clone());
        self.rutas.truncate(MAXIMO);
        if let Some(i) = info {
            self.info.insert(ruta, i);
        }
        self.info.retain(|k, _| self.rutas.contains(k));
        self.guardar();
    }

    pub fn quitar(&mut self, ruta: &Path) {
        self.rutas.retain(|r| r != ruta);
        self.info.remove(ruta);
        self.guardar();
    }

    pub fn vaciar(&mut self) {
        self.rutas.clear();
        self.info.clear();
        self.guardar();
    }

    pub fn fijar_abrir_ultima(&mut self, si: bool) {
        self.abrir_ultima = si;
        self.guardar();
    }

    /// La obra que se abre sola al arrancar, si está marcado y sigue ahí.
    pub fn para_arrancar(&self) -> Option<&PathBuf> {
        self.rutas.first().filter(|r| self.abrir_ultima && r.is_file())
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
        let d = EnDisco {
            rutas: self.rutas.clone(),
            info: self.info.clone(),
            abrir_ultima: self.abrir_ultima,
        };
        if let Ok(t) = serde_json::to_string_pretty(&d) {
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
    fn lee_el_formato_antiguo_y_el_nuevo() {
        let r = Recientes::leer(r#"["/a/obra.sqlite"]"#);
        assert_eq!(r.rutas, vec![PathBuf::from("/a/obra.sqlite")]);
        assert!(!r.abrir_ultima);
        let mut r = Recientes::default();
        r.anotar_con(
            Path::new("/no/existe/x.sqlite"),
            Some(InfoReciente {
                nombre: "Garaje".into(),
                cliente: "Constructora".into(),
                ..Default::default()
            }),
        );
        r.abrir_ultima = true;
        // No existe: no se abre sola
        assert!(r.para_arrancar().is_none());
        let d = EnDisco {
            rutas: r.rutas.clone(),
            info: r.info.clone(),
            abrir_ultima: true,
        };
        let r2 = Recientes::leer(&serde_json::to_string(&d).unwrap());
        assert!(r2.abrir_ultima);
        assert_eq!(r2.info[&r2.rutas[0]].cliente, "Constructora");
    }

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
