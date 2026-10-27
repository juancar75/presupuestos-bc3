// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Comparar»: la obra abierta frente a otra revisión (o a un BC3):
//! partidas nuevas, quitadas y cambiadas, con la diferencia de importe y del
//! PEM. Doble uso: «¿qué ha cambiado desde la R1?» y revisar la versión del
//! cliente.

use super::{Accion, Aplicacion, corto};
use eframe::egui::{self, Color32, RichText};
use ppto_core::comparar::{Comparacion, Estado, comparar};
use ppto_core::formato::{cant, eur, num};
use ppto_db::Almacen;
use std::path::PathBuf;

pub(super) struct VentanaComparar {
    /// Qué se compara con la obra abierta (p. ej. «revisión R1»).
    pub origen: String,
    pub resultado: Comparacion,
}

impl Aplicacion {
    /// Compara la obra abierta con una revisión del mismo fichero.
    pub(super) fn comparar_con_revision(&mut self, id: i64) {
        let Some(ruta) = self.fichero.clone() else { return };
        let etiqueta = self
            .revisiones
            .iter()
            .find(|r| r.id == id)
            .map_or_else(|| id.to_string(), |r| r.etiqueta.clone());
        match Almacen::abrir(&ruta).and_then(|a| a.cargar_revision(id)) {
            Ok(antes) => self.abrir_comparacion(&antes, format!("revisión {etiqueta}")),
            Err(e) => self.error(format!("No se pudo leer la revisión {etiqueta}: {e}")),
        }
    }

    /// Compara la obra abierta con un BC3 (versión del cliente, de Presto…).
    pub(super) fn comparar_con_bc3(&mut self) {
        let Some(ruta): Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3", "BC3"])
            .pick_file()
        else {
            return;
        };
        match ppto_bc3::importar_fichero(&ruta) {
            Ok(imp) => {
                let nombre = ruta
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.abrir_comparacion(&imp.presupuesto, nombre);
            }
            Err(e) => self.error(format!("No se pudo leer el BC3: {e}")),
        }
    }

    pub(super) fn abrir_comparacion(&mut self, antes: &ppto_core::Presupuesto, origen: String) {
        match comparar(antes, &self.p) {
            Ok(resultado) => {
                self.ventana_comparar = Some(VentanaComparar { origen, resultado });
            }
            Err(e) => self.error(format!("No se pudo comparar: {e}")),
        }
    }

    pub(super) fn ventana_comparar(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(v) = self.ventana_comparar.take() else { return };
        let mut abierta = true;
        let d = self.p.decimales;
        egui::Window::new(format!("Comparar: {} → obra abierta", v.origen))
            .open(&mut abierta)
            .default_width(900.0)
            .default_height(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                let r = &v.resultado;
                ui.horizontal(|ui| {
                    ui.label(format!("PEM {}: {} €", v.origen, eur(r.pem_antes)));
                    ui.label("→");
                    ui.label(format!("ahora: {} €", eur(r.pem_despues)));
                    ui.separator();
                    let dif = r.diferencia();
                    let t = format!(
                        "{}{} €{}",
                        if dif.is_sign_positive() && !dif.is_zero() {
                            "+"
                        } else {
                            ""
                        },
                        eur(dif),
                        r.variacion_pct()
                            .map(|p| format!(" ({} %)", num(p, 2)))
                            .unwrap_or_default()
                    );
                    ui.label(RichText::new(t).strong().color(color(dif)));
                });
                let (n, q, c) = r.cambios.iter().fold((0, 0, 0), |(n, q, c), x| match x.estado {
                    Estado::Nueva => (n + 1, q, c),
                    Estado::Quitada => (n, q + 1, c),
                    Estado::Cambiada => (n, q, c + 1),
                });
                ui.label(
                    RichText::new(format!(
                        "{n} nuevas · {q} quitadas · {c} cambiadas · {} iguales",
                        r.iguales
                    ))
                    .weak(),
                );
                if r.cambios.is_empty() {
                    ui.label("No hay diferencias de partidas, cantidades ni precios.");
                    return;
                }
                ui.separator();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("comparar_grid")
                        .striped(true)
                        .num_columns(9)
                        .show(ui, |ui| {
                            for t in [
                                "",
                                "Capítulo",
                                "Código",
                                "Resumen",
                                "Cantidad antes",
                                "Cantidad ahora",
                                "Precio antes",
                                "Precio ahora",
                                "Diferencia",
                            ] {
                                ui.label(RichText::new(t).strong());
                            }
                            ui.end_row();
                            for x in &r.cambios {
                                let (txt, col) = match x.estado {
                                    Estado::Nueva => ("nueva", Color32::from_rgb(80, 175, 110)),
                                    Estado::Quitada => ("quitada", Color32::from_rgb(220, 90, 80)),
                                    Estado::Cambiada => ("cambiada", Color32::from_rgb(230, 150, 60)),
                                };
                                ui.label(RichText::new(txt).color(col).small());
                                ui.label(&x.capitulo);
                                if x.despues.is_some() {
                                    if ui.link(&x.codigo).on_hover_text("Abrir la ficha").clicked() {
                                        acciones.push(Accion::Seleccionar(x.capitulo.clone(), x.codigo.clone()));
                                    }
                                } else {
                                    ui.label(&x.codigo);
                                }
                                ui.label(corto(&x.resumen, 46)).on_hover_text(&x.resumen);
                                let celda = |v: Option<ppto_core::Decimal>, dec: u32| {
                                    v.map(|v| cant(v, dec)).unwrap_or_default()
                                };
                                let a = x.antes;
                                let b = x.despues;
                                let marca = |ui: &mut egui::Ui, t: String, cambio: bool| {
                                    if cambio {
                                        ui.label(RichText::new(t).strong());
                                    } else {
                                        ui.label(t);
                                    }
                                };
                                let cambia_c = a.zip(b).is_some_and(|(a, b)| a.cantidad != b.cantidad);
                                let cambia_p = a.zip(b).is_some_and(|(a, b)| a.precio != b.precio);
                                ui.label(celda(a.map(|v| v.cantidad), d.medicion));
                                marca(ui, celda(b.map(|v| v.cantidad), d.medicion), cambia_c);
                                ui.label(a.map(|v| eur(v.precio)).unwrap_or_default());
                                marca(ui, b.map(|v| eur(v.precio)).unwrap_or_default(), cambia_p);
                                let dif = x.diferencia();
                                ui.label(RichText::new(format!("{} €", eur(dif))).color(color(dif)));
                                ui.end_row();
                            }
                        });
                });
            });
        if abierta {
            self.ventana_comparar = Some(v);
        }
    }
}

fn color(d: ppto_core::Decimal) -> Color32 {
    if d.is_zero() {
        Color32::GRAY
    } else if d.is_sign_positive() {
        Color32::from_rgb(230, 150, 60)
    } else {
        Color32::from_rgb(80, 175, 110)
    }
}
