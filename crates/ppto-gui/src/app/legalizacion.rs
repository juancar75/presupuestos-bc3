// SPDX-License-Identifier: GPL-3.0-or-later
//! «Legalización…»: abre la aplicación independiente `legalizacion` en su
//! propia ventana y, cuando el usuario crea la partida, la aplica a la obra.
//!
//! Intercambio por dos JSON en una carpeta temporal (ver
//! `ppto_legalizacion::intercambio`): la solicitud lleva la obra y su margen;
//! la respuesta, la partida calculada. Mientras la otra ventana está abierta
//! el presupuestador sigue funcionando; la partida se aplica sobre la obra tal
//! como esté en ese momento y se puede deshacer.

use super::Aplicacion;
use eframe::egui;
use ppto_core::formato::eur;
use ppto_legalizacion::{Respuesta, Solicitud, aplicar, codigo_partida};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

/// Nombre del ejecutable, junto a `ppto-gui`.
const EJECUTABLE: &str = "legalizacion";

/// La aplicación de legalización abierta desde esta obra.
pub(super) struct Lanzada {
    hijo: Child,
    dir: PathBuf,
    respuesta: PathBuf,
}

/// Ruta de `legalizacion(.exe)` en la carpeta del programa.
fn ejecutable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let r = exe
        .parent()?
        .join(format!("{EJECUTABLE}{}", std::env::consts::EXE_SUFFIX));
    r.is_file().then_some(r)
}

fn carpeta_temporal() -> PathBuf {
    let marca = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    std::env::temp_dir().join(format!("presupuestos-bc3-legalizacion-{}-{marca}", std::process::id()))
}

fn limpiar(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

impl Aplicacion {
    /// Menú «Legalización…».
    pub(super) fn abrir_legalizacion(&mut self) {
        if self.ventana_legalizacion.is_some() {
            self.info("La ventana de legalización ya está abierta.");
            return;
        }
        let Some(exe) = ejecutable() else {
            self.error(format!(
                "No se encuentra «{EJECUTABLE}{}» junto al programa. Reinstale o compile con «cargo build -p ppto-legalizacion-app».",
                std::env::consts::EXE_SUFFIX
            ));
            return;
        };
        let dir = carpeta_temporal();
        let solicitud = dir.join("solicitud.json");
        let respuesta = dir.join("respuesta.json");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.error(format!("No se pudo preparar la legalización: {e}"));
            return;
        }
        if let Err(e) = Solicitud::nueva(self.p.clone(), self.gg + self.bi).guardar(&solicitud) {
            limpiar(&dir);
            self.error(format!("No se pudo preparar la legalización: {e}"));
            return;
        }
        match Command::new(&exe)
            .arg("--solicitud")
            .arg(&solicitud)
            .arg("--respuesta")
            .arg(&respuesta)
            .spawn()
        {
            Ok(hijo) => {
                self.ventana_legalizacion = Some(Lanzada { hijo, dir, respuesta });
                self.info("Legalización abierta en otra ventana.");
            }
            Err(e) => {
                limpiar(&dir);
                self.error(format!("No se pudo abrir la legalización: {e}"));
            }
        }
    }

    /// Cada fotograma: si la ventana de legalización se ha cerrado, recoge la
    /// partida (si la hay) y la aplica.
    pub(super) fn ventana_legalizacion(&mut self, ctx: &egui::Context) {
        let Some(l) = self.ventana_legalizacion.as_mut() else {
            return;
        };
        // Sigue abierta: volver a mirar dentro de medio segundo.
        if let Ok(None) = l.hijo.try_wait() {
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        }
        let Some(l) = self.ventana_legalizacion.take() else {
            return;
        };
        if l.respuesta.is_file() {
            match Respuesta::cargar(&l.respuesta) {
                Ok(r) => self.aplicar_legalizacion(r),
                Err(e) => self.error(format!("No se pudo leer la partida de legalización: {e}")),
            }
        } else {
            self.info("Legalización cerrada sin crear partida.");
        }
        limpiar(&l.dir);
    }

    /// Aplica la partida devuelta (con deshacer).
    pub(super) fn aplicar_legalizacion(&mut self, r: Respuesta) {
        let codigo = codigo_partida(r.disciplina);
        self.historial
            .antes(&self.p, format!("legalización {}", r.disciplina.sigla()), None);
        match aplicar(&mut self.p, r.disciplina, &r.calculo) {
            Ok(()) => {
                self.marcar_cambio();
                self.info(format!("Partida {codigo} creada: {} €.", eur(r.calculo.total())));
            }
            Err(e) => {
                self.historial.deshacer.pop();
                self.error(format!("No se pudo crear la partida {codigo}: {e}"));
            }
        }
    }
}
