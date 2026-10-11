// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Tarifas de subcontratas»: importar una tarifa de Excel, revisar
//! las unidades dudosas y guardarla en
//! `%APPDATA%\presupuestos-bc3\tarifas.json`. Las líneas se usan desde
//! «+ Añadir línea… → De tarifa».

use super::Aplicacion;
use eframe::egui::{self, Color32, RichText};
use ppto_core::formato::{eur, leer};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct VentanaTarifas {
    sel: usize,
    /// Precios mientras se editan (texto), por línea de la tarifa elegida.
    precios: Vec<String>,
    cambios: bool,
}

pub(super) fn ruta_tarifas() -> Option<PathBuf> {
    super::pdf::carpeta_config().map(|c| c.join("tarifas.json"))
}

impl VentanaTarifas {
    fn cargar_precios(&mut self, b: &ppto_tarifas::Biblioteca) {
        self.precios = b
            .tarifas
            .get(self.sel)
            .map(|t| t.lineas.iter().map(|l| eur(l.precio)).collect())
            .unwrap_or_default();
    }
}

impl Aplicacion {
    pub(super) fn abrir_tarifas(&mut self) {
        let mut v = VentanaTarifas::default();
        v.cargar_precios(&self.tarifas);
        self.ventana_tarifas = Some(v);
    }

    fn guardar_tarifas(&mut self) {
        match ruta_tarifas().map(|r| self.tarifas.guardar(&r)) {
            Some(Ok(())) => self.info("Tarifas guardadas."),
            Some(Err(e)) => self.error(e.to_string()),
            None => self.error("No se encuentra la carpeta de configuración."),
        }
    }

    fn importar_tarifa(&mut self, v: &mut VentanaTarifas) {
        let Some(r) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx", "xls", "xlsm", "ods"])
            .set_title("Tarifa de la subcontrata (columnas Referencia, Descripción, Precio y, si la hay, Unidad)")
            .pick_file()
        else {
            return;
        };
        match ppto_tarifas::leer_excel(&r, "", "") {
            Ok(mut t) => {
                if t.proveedor.is_empty() {
                    t.proveedor = r
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
                let n = t.lineas.len();
                let revisar = t.por_revisar();
                let nombre = t.nombre();
                let sustituye = self.tarifas.anadir(t);
                v.sel = self
                    .tarifas
                    .tarifas
                    .iter()
                    .position(|x| x.nombre() == nombre)
                    .unwrap_or(0);
                v.cargar_precios(&self.tarifas);
                v.cambios = true;
                self.info(format!(
                    "Tarifa {nombre}: {n} líneas{}{}. Revise proveedor, año y unidades y pulse «Guardar».",
                    if sustituye { " (sustituye a la anterior)" } else { "" },
                    if revisar > 0 {
                        format!(", {revisar} con la unidad deducida (en naranja)")
                    } else {
                        String::new()
                    }
                ));
            }
            Err(e) => self.error(format!("No se pudo importar la tarifa: {e}")),
        }
    }

    pub(super) fn ventana_tarifas(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_tarifas.take() else {
            return;
        };
        let mut abierta = true;
        let mut importar = false;
        let mut guardar = false;
        egui::Window::new("Tarifas de subcontratas")
            .open(&mut abierta)
            .default_width(820.0)
            .default_height(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Precios netos sin IVA por unidad de obra. Cada línea entra en los descompuestos como recurso \
                         de subcontrata con código PROVEEDOR.REFERENCIA. Se guardan solo en este equipo.",
                    )
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("📥 Importar Excel…").clicked() {
                        importar = true;
                    }
                    if ui
                        .add_enabled(v.cambios, egui::Button::new(RichText::new("💾 Guardar").strong()))
                        .clicked()
                    {
                        guardar = true;
                    }
                    if !self.tarifas.tarifas.is_empty()
                        && ui
                            .button("🗑 Borrar tarifa")
                            .on_hover_text("Los presupuestos que ya la usan no cambian")
                            .clicked()
                    {
                        self.tarifas.tarifas.remove(v.sel.min(self.tarifas.tarifas.len() - 1));
                        v.sel = 0;
                        v.cargar_precios(&self.tarifas);
                        v.cambios = true;
                    }
                });
                if self.tarifas.tarifas.is_empty() {
                    ui.label("Todavía no hay tarifas. Importe la primera desde Excel.");
                    return;
                }
                v.sel = v.sel.min(self.tarifas.tarifas.len() - 1);
                let antes = v.sel;
                egui::ComboBox::from_label("Tarifa")
                    .selected_text(self.tarifas.tarifas[v.sel].nombre())
                    .show_ui(ui, |ui| {
                        for (i, t) in self.tarifas.tarifas.iter().enumerate() {
                            ui.selectable_value(&mut v.sel, i, format!("{} · {}", t.nombre(), t.especialidad));
                        }
                    });
                if v.sel != antes {
                    v.cargar_precios(&self.tarifas);
                }
                let t = &mut self.tarifas.tarifas[v.sel];
                egui::Grid::new("tarifa_datos").num_columns(4).show(ui, |ui| {
                    ui.label("Proveedor");
                    v.cambios |= ui.text_edit_singleline(&mut t.proveedor).changed();
                    ui.label("Año / vigencia");
                    v.cambios |= ui.text_edit_singleline(&mut t.vigencia).changed();
                    ui.end_row();
                    ui.label("Especialidad");
                    v.cambios |= ui.text_edit_singleline(&mut t.especialidad).changed();
                    ui.label("Incluye");
                    v.cambios |= ui
                        .add(egui::TextEdit::singleline(&mut t.notas).hint_text("cable, tubo, pequeño material…"))
                        .changed();
                    ui.end_row();
                });
                let revisar = t.por_revisar();
                if revisar > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{revisar} unidades deducidas de la descripción (en naranja): corríjalas si hace falta \
                             y marque «ok»."
                        ))
                        .color(Color32::from_rgb(220, 150, 40)),
                    );
                }
                ui.separator();
                let prefijo = t.prefijo();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui::Grid::new("tarifa_lineas")
                        .num_columns(5)
                        .striped(true)
                        .spacing([10.0, 3.0])
                        .show(ui, |ui| {
                            for h in ["Código", "Descripción", "Ud", "Precio €", ""] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for (i, l) in t.lineas.iter_mut().enumerate() {
                                ui.label(format!("{prefijo}.{}", l.referencia));
                                ui.label(super::corto(&l.descripcion, 70)).on_hover_text(&l.descripcion);
                                let mut ud = egui::TextEdit::singleline(&mut l.unidad).desired_width(40.0);
                                if l.revisar_unidad {
                                    ud = ud.background_color(Color32::from_rgb(120, 80, 20));
                                }
                                if ui.add(ud).changed() {
                                    v.cambios = true;
                                }
                                if let Some(txt) = v.precios.get_mut(i) {
                                    let r = ui.add(egui::TextEdit::singleline(txt).desired_width(70.0));
                                    if r.changed()
                                        && let Some(p) = leer(txt)
                                        && p >= ppto_core::Decimal::ZERO
                                    {
                                        l.precio = p;
                                        v.cambios = true;
                                    }
                                } else {
                                    ui.label(eur(l.precio));
                                }
                                if l.revisar_unidad {
                                    if ui.small_button("ok").on_hover_text("Unidad revisada").clicked() {
                                        l.revisar_unidad = false;
                                        v.cambios = true;
                                    }
                                } else {
                                    ui.label("");
                                }
                                ui.end_row();
                            }
                        });
                });
            });
        if importar {
            self.importar_tarifa(&mut v);
        }
        if guardar {
            self.guardar_tarifas();
            v.cambios = false;
        }
        if abierta {
            self.ventana_tarifas = Some(v);
        } else if v.cambios {
            // Al cerrar no se pierde lo editado
            self.guardar_tarifas();
        }
    }
}
