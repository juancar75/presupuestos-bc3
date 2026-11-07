// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Nuevo presupuesto»: pregunta qué se va a presupuestar
//! (especialidad) y de dónde se parte (ciego del cliente, BC3 o plantilla),
//! y deja la obra orientada a esa especialidad: plantilla y bases de precios
//! de la especialidad. «Presupuesto en blanco» es el camino de siempre.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::especialidad::Especialidad;

/// Estado de la ventana: `None` en el primer paso; `Some` cuando ya se ha
/// elegido la especialidad.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct AsistenteNuevo {
    pub especialidad: Option<Especialidad>,
}

/// Lo que se elige en la ventana.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Inicio {
    EnBlanco,
    Plantilla(Especialidad),
    Ciego(Especialidad),
    Bc3(Especialidad),
}

impl Aplicacion {
    /// Abre la ventana «Nuevo presupuesto» (Archivo → Nuevo, Ctrl+N).
    pub(super) fn pedir_nuevo(&mut self) {
        self.asistente_nuevo = Some(AsistenteNuevo::default());
    }

    /// Crea la obra según lo elegido y la deja con su especialidad.
    pub(super) fn empezar(&mut self, inicio: Inicio) {
        match inicio {
            Inicio::EnBlanco => self.nuevo_presupuesto(),
            Inicio::Plantilla(e) => {
                match e.plantilla() {
                    Some(t) => self.nuevo_desde_plantilla(t),
                    None => self.nuevo_presupuesto(),
                }
                self.especialidad = Some(e);
                self.info(format!(
                    "Obra de {}: {}la Ventana → Bases de precios muestra las bases de esta especialidad.",
                    e.nombre(),
                    if e.plantilla().is_some() {
                        "plantilla cargada (partidas sin precio); "
                    } else {
                        "presupuesto vacío (esta especialidad aún no tiene plantilla); "
                    }
                ));
            }
            Inicio::Ciego(e) => {
                if let Some(r) = rfd::FileDialog::new()
                    .add_filter("Excel", &["xlsx", "xls"])
                    .set_title("Presupuesto ciego del cliente (Excel)")
                    .pick_file()
                {
                    self.importar_ciego(r);
                    self.especialidad = Some(e);
                }
            }
            Inicio::Bc3(e) => {
                if let Some(r) = rfd::FileDialog::new()
                    .add_filter("BC3", &["bc3", "BC3"])
                    .set_title("Presupuesto del cliente (BC3)")
                    .pick_file()
                {
                    self.importar_bc3(r);
                    self.especialidad = Some(e);
                }
            }
        }
    }

    pub(super) fn ventana_nuevo(&mut self, ctx: &egui::Context) {
        let Some(mut estado) = self.asistente_nuevo else {
            return;
        };
        let mut abierta = true;
        let mut elegido: Option<Inicio> = None;
        egui::Window::new("Nuevo presupuesto")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| match estado.especialidad {
                None => {
                    ui.label(RichText::new("¿Qué vas a presupuestar?").heading());
                    ui.add_space(6.0);
                    for e in Especialidad::TODAS {
                        let b = egui::Button::new(RichText::new(format!("{}\n", e.nombre())).strong().size(15.0))
                            .min_size(egui::vec2(460.0, 0.0));
                        if ui.add(b).on_hover_text(e.descripcion()).clicked() {
                            estado.especialidad = Some(e);
                        }
                        ui.label(RichText::new(e.descripcion()).small().weak());
                        ui.add_space(4.0);
                    }
                    ui.separator();
                    if ui
                        .button("Presupuesto en blanco")
                        .on_hover_text("Sin especialidad ni plantilla, como siempre")
                        .clicked()
                    {
                        elegido = Some(Inicio::EnBlanco);
                    }
                }
                Some(e) => {
                    ui.label(RichText::new(e.nombre()).heading());
                    ui.label(RichText::new("¿De dónde partes?").strong());
                    ui.add_space(6.0);
                    let ancho = egui::vec2(460.0, 0.0);
                    if ui
                        .add(egui::Button::new("Tengo el presupuesto ciego del cliente (Excel)").min_size(ancho))
                        .clicked()
                    {
                        elegido = Some(Inicio::Ciego(e));
                    }
                    if ui
                        .add(egui::Button::new("Tengo el presupuesto del cliente en BC3").min_size(ancho))
                        .clicked()
                    {
                        elegido = Some(Inicio::Bc3(e));
                    }
                    let desde = if e.plantilla().is_some() {
                        "Empiezo desde cero (plantilla de la especialidad)"
                    } else {
                        "Empiezo desde cero (presupuesto vacío)"
                    };
                    if ui.add(egui::Button::new(desde).min_size(ancho)).clicked() {
                        elegido = Some(Inicio::Plantilla(e));
                    }
                    ui.add_space(6.0);
                    if ui.small_button("← Otra especialidad").clicked() {
                        estado.especialidad = None;
                    }
                }
            });
        self.asistente_nuevo = (abierta && elegido.is_none()).then_some(estado);
        if let Some(i) = elegido {
            self.empezar(i);
        }
    }
}
