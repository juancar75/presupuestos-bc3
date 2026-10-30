// SPDX-License-Identifier: GPL-3.0-or-later
//! Guardado automático, como en Word: cada minuto, si hay cambios sin
//! guardar, se escribe una **copia de recuperación** de la obra en
//! `%APPDATA%\presupuestos-bc3\recuperacion\` (no crea revisiones en el
//! fichero de la obra). Al guardar, o si ya no hay cambios, la copia se
//! borra. Si el programa se cierra de golpe, al volver a abrirlo se ofrece
//! recuperar lo que no se guardó.
//!
//! Cada ventana tiene su copia (`obra-<proceso>.json`); al arrancar solo se
//! ofrecen las de otras sesiones que llevan un rato sin actualizarse.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::Presupuesto;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cada cuánto se guarda la copia si hay cambios (segundos).
pub(super) const INTERVALO: f64 = 60.0;
/// Una copia de otra ventana que no se ha tocado en este tiempo se da por
/// abandonada (segundos).
const ABANDONADA: u64 = 180;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Copia {
    /// Segundos desde 1970.
    pub guardada: u64,
    /// Fichero de la obra, si ya se había guardado alguna vez.
    pub fichero: Option<String>,
    pub presupuesto_id: Option<i64>,
    /// GG, BI, IVA y ciego (como en el fichero de la obra).
    pub datos: BTreeMap<String, String>,
    pub presupuesto: Presupuesto,
}

pub(super) struct Recuperacion {
    carpeta: Option<PathBuf>,
    propia: Option<PathBuf>,
    /// Momento (reloj de egui) de la última copia.
    ultima: f64,
    /// Copias de sesiones anteriores que se pueden recuperar.
    pub pendientes: Vec<(PathBuf, Copia)>,
}

fn ahora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

impl Default for Recuperacion {
    /// Sin disco (pruebas): no guarda nada.
    fn default() -> Self {
        Self {
            carpeta: None,
            propia: None,
            ultima: 0.0,
            pendientes: Vec::new(),
        }
    }
}

impl Recuperacion {
    pub fn en_disco() -> Self {
        match super::pdf::carpeta_config() {
            Some(c) => Self::en(&c.join("recuperacion"), ABANDONADA),
            None => Self::default(),
        }
    }

    /// En una carpeta concreta; `edad`: segundos sin tocar para ofrecer una
    /// copia ajena.
    pub fn en(carpeta: &Path, edad: u64) -> Self {
        let propia = carpeta.join(format!("obra-{}.json", std::process::id()));
        let mut pendientes = Vec::new();
        if let Ok(rd) = std::fs::read_dir(carpeta) {
            for e in rd.flatten() {
                let r = e.path();
                if r == propia || r.extension().is_none_or(|x| x != "json") {
                    continue;
                }
                let vieja = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d.as_secs() >= edad);
                if !vieja {
                    continue;
                }
                if let Some(c) = std::fs::read_to_string(&r)
                    .ok()
                    .and_then(|t| serde_json::from_str::<Copia>(&t).ok())
                {
                    pendientes.push((r, c));
                }
            }
        }
        pendientes.sort_by_key(|(_, c)| std::cmp::Reverse(c.guardada));
        Self {
            carpeta: Some(carpeta.to_owned()),
            propia: Some(propia),
            ultima: 0.0,
            pendientes,
        }
    }

    pub fn escribir(&self, c: &Copia) -> std::io::Result<()> {
        let (Some(dir), Some(r)) = (&self.carpeta, &self.propia) else {
            return Ok(());
        };
        std::fs::create_dir_all(dir)?;
        let t = serde_json::to_string(c).map_err(std::io::Error::other)?;
        ppto_propuestas::escribir(r, &t)
    }

    /// Borra la copia de esta ventana (ya no hace falta).
    pub fn borrar_propia(&self) {
        if let Some(r) = &self.propia
            && r.exists()
        {
            let _ = std::fs::remove_file(r);
        }
    }
}

impl Aplicacion {
    /// Copia de recuperación si toca (llamar en cada fotograma).
    pub(super) fn autoguardar(&mut self, ctx: &egui::Context) {
        let t = ctx.input(|i| i.time);
        if !self.cambios {
            if self.recuperacion.ultima != 0.0 {
                self.recuperacion.borrar_propia();
                self.recuperacion.ultima = 0.0;
            }
            return;
        }
        // Que vuelva a pintar aunque no se toque nada, para guardar a su hora
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(INTERVALO));
        // Al cerrar la ventana con cambios, copia en el acto
        let cerrando = ctx.input(|i| i.viewport().close_requested());
        if !cerrando {
            if self.recuperacion.ultima == 0.0 {
                // Primer cambio: la primera copia, al cabo de un intervalo
                self.recuperacion.ultima = t.max(f64::MIN_POSITIVE);
                return;
            }
            if t - self.recuperacion.ultima < INTERVALO {
                return;
            }
        }
        self.recuperacion.ultima = t;
        let copia = self.copia_recuperacion();
        if let Err(e) = self.recuperacion.escribir(&copia) {
            self.error(format!("No se pudo guardar la copia de recuperación: {e}"));
        }
    }

    pub(super) fn copia_recuperacion(&self) -> Copia {
        Copia {
            guardada: ahora(),
            fichero: self.fichero.as_ref().map(|f| f.display().to_string()),
            presupuesto_id: self.presupuesto_id,
            datos: self.datos_obra(),
            presupuesto: self.p.clone(),
        }
    }

    /// Pone en pantalla una copia recuperada (queda con cambios sin guardar).
    pub(super) fn recuperar(&mut self, c: Copia) {
        self.historial.limpiar();
        self.p = c.presupuesto;
        self.aplicar_datos_obra(&c.datos);
        self.fichero = c.fichero.map(PathBuf::from).filter(|f| f.exists());
        self.presupuesto_id = c.presupuesto_id.filter(|_| self.fichero.is_some());
        self.revisiones = self
            .fichero
            .as_ref()
            .and_then(|f| ppto_db::Almacen::abrir(f).ok())
            .and_then(|a| a.listar_revisiones().ok())
            .unwrap_or_default();
        self.revision_actual = None;
        self.cambios = true;
        self.sel = None;
        self.recalcular();
        self.sel = self.primera_partida(&self.p.raiz.clone());
        self.info("Trabajo recuperado. Guárdelo (Ctrl+S) para no perderlo.");
    }

    pub(super) fn ventana_recuperacion(&mut self, ctx: &egui::Context) {
        if self.recuperacion.pendientes.is_empty() {
            return;
        }
        let mut accion: Option<(usize, bool)> = None;
        egui::Window::new("Recuperar trabajo sin guardar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("El programa se cerró sin guardar estas obras. ¿Las recupera?");
                ui.add_space(4.0);
                egui::Grid::new("recuperar")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for (i, (_, c)) in self.recuperacion.pendientes.iter().enumerate() {
                            ui.label(RichText::new(&c.presupuesto.nombre).strong());
                            ui.label(
                                RichText::new(
                                    c.fichero
                                        .as_deref()
                                        .and_then(|f| Path::new(f).file_name())
                                        .map_or("(sin guardar nunca)".to_owned(), |f| f.to_string_lossy().into_owned()),
                                )
                                .weak(),
                            );
                            ui.label(RichText::new(hace(ahora().saturating_sub(c.guardada))).weak());
                            ui.horizontal(|ui| {
                                if ui.button("Recuperar").clicked() {
                                    accion = Some((i, true));
                                }
                                if ui
                                    .button("Descartar")
                                    .on_hover_text("Borra la copia: se pierde lo no guardado")
                                    .clicked()
                                {
                                    accion = Some((i, false));
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some((i, recuperar)) = accion {
            let (r, c) = self.recuperacion.pendientes.remove(i);
            let _ = std::fs::remove_file(&r);
            if recuperar {
                self.recuperar(c);
                // A partir de ahora se protege con la copia de esta ventana
                self.recuperacion.ultima = 0.0;
            }
        }
    }
}

/// «hace 5 min», «hace 2 h», «hace 3 días».
fn hace(s: u64) -> String {
    match s {
        0..=59 => "hace un momento".into(),
        60..=3599 => format!("hace {} min", s / 60),
        3600..=86_399 => format!("hace {} h", s / 3600),
        _ => format!("hace {} días", s / 86_400),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn copia_y_recuperacion() {
        let dir = std::env::temp_dir().join(format!("ppto-recup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = Recuperacion::en(&dir, 0);
        assert!(r.pendientes.is_empty());
        let p = ppto_core::ejemplos::aleatorio(7, true);
        let c = Copia {
            guardada: ahora(),
            fichero: Some(r"D:\Obras\obra.sqlite".into()),
            presupuesto_id: Some(3),
            datos: BTreeMap::from([("gg".into(), "15".into())]),
            presupuesto: p.clone(),
        };
        r.escribir(&c).unwrap();
        // La propia no se ofrece; la de «otra sesión» sí
        assert!(Recuperacion::en(&dir, 0).pendientes.is_empty());
        std::fs::rename(
            dir.join(format!("obra-{}.json", std::process::id())),
            dir.join("obra-1.json"),
        )
        .unwrap();
        let r2 = Recuperacion::en(&dir, 0);
        assert_eq!(r2.pendientes.len(), 1);
        assert_eq!(r2.pendientes[0].1, c);
        // Una copia reciente de otra ventana abierta no se ofrece
        assert!(Recuperacion::en(&dir, 3600).pendientes.is_empty());
        // Ficheros dañados se ignoran
        std::fs::write(dir.join("obra-2.json"), "{roto").unwrap();
        assert_eq!(Recuperacion::en(&dir, 0).pendientes.len(), 1);
        r.borrar_propia();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(hace(30), "hace un momento");
        assert_eq!(hace(7200), "hace 2 h");
    }
}
