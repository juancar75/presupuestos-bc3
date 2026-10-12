// SPDX-License-Identifier: GPL-3.0-or-later
//! Coeficiente de dificultad: más horas de mano de obra propia en toda la
//! obra, un capítulo o una partida. Subcontratas y materiales no cambian.

use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::formato::{eur, leer, num};
use ppto_core::{Decimal, Naturaleza};

#[derive(Default)]
pub(super) struct FormDificultad {
    pub ambito: String,
    pub coeficiente: String,
    pub motivo: String,
    /// Alta de un descuento de fabricante (pestaña Recursos).
    pub nuevo_fab: String,
    pub nuevo_dto: String,
}

/// Texto para Deshacer y para la auditoría de la revisión.
pub(super) fn describir(ambito: &str, coef: Decimal, motivo: &str) -> String {
    let m = motivo.trim();
    format!(
        "dificultad ×{} en {ambito}{}",
        num(coef, 2),
        if m.is_empty() { String::new() } else { format!(" ({m})") }
    )
}

impl Aplicacion {
    pub(super) fn form_dificultad(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let mut f = std::mem::take(&mut self.dificultad);
        if f.ambito.is_empty() || !self.p.conceptos.contains_key(&f.ambito) {
            f.ambito = self.p.raiz.clone();
        }
        if f.coeficiente.is_empty() {
            f.coeficiente = "1,20".into();
        }
        ui.label(
            RichText::new(
                "Multiplica las horas de mano de obra propia (no el precio de la hora). Subcontratas, materiales \
                 y maquinaria no cambian; los % de medios auxiliares se recalculan. Queda anotado al guardar la revisión.",
            )
            .weak(),
        );
        let nombre = |c: &str| {
            if c == self.p.raiz {
                "Toda la obra".to_owned()
            } else {
                format!("{}  {}", c, super::corto(&self.p.conceptos[c].resumen, 40))
            }
        };
        egui::Grid::new("dificultad").num_columns(2).show(ui, |ui| {
            ui.label("Ámbito");
            egui::ComboBox::from_id_salt("dif_ambito")
                .selected_text(nombre(&f.ambito))
                .width(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.ambito, self.p.raiz.clone(), "Toda la obra");
                    for c in self
                        .p
                        .conceptos
                        .values()
                        .filter(|c| c.naturaleza == Naturaleza::Capitulo)
                    {
                        if c.codigo != self.p.raiz {
                            ui.selectable_value(&mut f.ambito, c.codigo.clone(), nombre(&c.codigo));
                        }
                    }
                    if let Some((_, h)) = &self.sel
                        && self
                            .p
                            .conceptos
                            .get(h)
                            .is_some_and(|c| c.naturaleza == Naturaleza::Partida)
                    {
                        ui.selectable_value(&mut f.ambito, h.clone(), format!("Partida {}", nombre(h)));
                    }
                });
            ui.end_row();
            ui.label("Coeficiente");
            ui.add(egui::TextEdit::singleline(&mut f.coeficiente).desired_width(70.0))
                .on_hover_text("1,20 = 20 % más de horas; 0,90 = 10 % menos");
            ui.end_row();
            ui.label("Motivo");
            ui.add(
                egui::TextEdit::singleline(&mut f.motivo)
                    .desired_width(320.0)
                    .hint_text("obra en altura, horario nocturno, accesos difíciles…"),
            );
            ui.end_row();
        });
        match leer(&f.coeficiente) {
            None => {
                ui.label(
                    RichText::new("Coeficiente no válido (use coma decimal: 1,20).")
                        .color(Color32::from_rgb(220, 80, 60)),
                );
            }
            Some(k) => match self.p.clone().aplicar_dificultad(&f.ambito, k) {
                Err(e) => {
                    ui.label(RichText::new(e.to_string()).color(Color32::from_rgb(220, 150, 40)));
                }
                Ok(inf) => {
                    egui::Grid::new("dif_previa").num_columns(3).show(ui, |ui| {
                        ui.label("");
                        ui.label(RichText::new("Antes").strong());
                        ui.label(RichText::new("Después").strong());
                        ui.end_row();
                        for (r, h) in &inf.horas_antes {
                            let d = inf.horas_despues.get(r).copied().unwrap_or_default();
                            if d != *h {
                                ui.label(format!("{r} (h)"));
                                ui.label(num(*h, 2));
                                ui.label(RichText::new(num(d, 2)).strong());
                                ui.end_row();
                            }
                        }
                        ui.label("Coste directo");
                        ui.label(format!("{} €", eur(inf.coste_antes)));
                        ui.label(
                            RichText::new(format!(
                                "{} € ({}{} €)",
                                eur(inf.coste_despues),
                                if inf.coste_despues >= inf.coste_antes {
                                    "+"
                                } else {
                                    "−"
                                },
                                eur((inf.coste_despues - inf.coste_antes).abs())
                            ))
                            .strong(),
                        );
                        ui.end_row();
                    });
                    ui.label(RichText::new(format!("{} partidas o auxiliares cambian.", inf.modificadas.len())).weak());
                    if !inf.omitidas.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "No se tocan por estar también fuera del ámbito: {}",
                                inf.omitidas.join(", ")
                            ))
                            .color(Color32::from_rgb(220, 150, 40)),
                        );
                    }
                    if ui
                        .button(RichText::new("Aplicar coeficiente").strong())
                        .on_hover_text("Cambia los rendimientos; Deshacer (Ctrl+Z) lo revierte")
                        .clicked()
                    {
                        acciones.push(Accion::Dificultad {
                            ambito: f.ambito.clone(),
                            coeficiente: k,
                            motivo: f.motivo.trim().to_owned(),
                        });
                    }
                }
            },
        }
        self.dificultad = f;
    }
}

impl Aplicacion {
    /// Tabla de descuentos por fabricante (pestaña Recursos).
    pub(super) fn form_descuentos(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        ui.label(
            RichText::new(
                "Los recursos con PVP de tarifa calculan su neto con el descuento de su fabricante. \
                 Cambie aquí el descuento y se recalculan todos de una vez.",
            )
            .weak(),
        );
        let fabs = self.p.fabricantes();
        egui::Grid::new("descuentos")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for t in ["Fabricante", "Descuento %", "Recursos"] {
                    ui.label(RichText::new(t).weak());
                }
                ui.end_row();
                for f in &fabs {
                    ui.label(&f.nombre);
                    if let Some(v) = self
                        .edicion
                        .decimal(ui, &format!("dto|{}", f.nombre), f.descuento, 2, 70.0)
                    {
                        acciones.push(Accion::Descuento(f.nombre.clone(), v));
                    }
                    ui.label(f.recursos.to_string());
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            ui.label("Añadir fabricante");
            ui.add(
                egui::TextEdit::singleline(&mut self.dificultad.nuevo_fab)
                    .desired_width(140.0)
                    .hint_text("KOOLAIR"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.dificultad.nuevo_dto)
                    .desired_width(60.0)
                    .hint_text("45"),
            );
            let v = leer(&self.dificultad.nuevo_dto);
            if ui
                .add_enabled(
                    v.is_some() && !self.dificultad.nuevo_fab.trim().is_empty(),
                    egui::Button::new("Añadir"),
                )
                .clicked()
                && let Some(v) = v
            {
                acciones.push(Accion::Descuento(self.dificultad.nuevo_fab.trim().to_owned(), v));
                self.dificultad.nuevo_fab.clear();
                self.dificultad.nuevo_dto.clear();
            }
        });
    }
}
