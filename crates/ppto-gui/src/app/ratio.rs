// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulario «Subcontrata a ratio»: varias partidas, un medidor (máquinas,
//! m²…) y un precio por unidad; se aplica de verdad al presupuesto.

use super::{Accion, Aplicacion, corto, nombre_naturaleza};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::formato::{eur, leer, num};
use ppto_core::ratio::PaqueteRatio;
use ppto_core::{Decimal, Naturaleza};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct FormRatio {
    codigo: String,
    descripcion: String,
    unidad: String,
    precio: String,
    buscar: String,
    /// Partida → (cuenta para el medidor, recursos que pasan a la subcontrata)
    partidas: BTreeMap<String, (bool, BTreeSet<String>)>,
}

impl FormRatio {
    fn paquete(&self) -> Result<PaqueteRatio, String> {
        let precio = leer(&self.precio).ok_or("ratio no válido (use coma decimal: 650,00)")?;
        Ok(PaqueteRatio {
            codigo: self.codigo.trim().to_owned(),
            descripcion: self.descripcion.trim().to_owned(),
            unidad: self.unidad.trim().to_owned(),
            precio,
            medidor: self
                .partidas
                .iter()
                .filter(|(_, (m, _))| *m)
                .map(|(p, _)| p.clone())
                .collect(),
            asignaciones: self
                .partidas
                .iter()
                .map(|(p, (_, r))| (p.clone(), r.iter().cloned().collect()))
                .collect(),
        })
    }
}

impl Aplicacion {
    pub(super) fn form_ratio(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        ui.label(
            RichText::new(
                "Ejemplo: montaje VRF a 650 € por máquina. Añada las partidas, marque en cada una lo que pone la \
                 subcontrata (mano de obra, tubería, cable…) y qué partidas cuentan como «máquina». Los medios \
                 auxiliares se quedan en las partidas. Se aplica al presupuesto (Deshacer lo revierte).",
            )
            .weak(),
        );
        let mut f = std::mem::take(&mut self.ratio);
        if f.codigo.is_empty() {
            f.codigo = self.p.codigo_libre("SUB.", 2);
            f.unidad = "máquina".into();
        }
        egui::Grid::new("ratio_datos")
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Código");
                ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(&mut f.codigo));
                ui.label("Descripción");
                ui.add(
                    egui::TextEdit::singleline(&mut f.descripcion)
                        .desired_width(300.0)
                        .hint_text("Montaje VRF"),
                );
                ui.end_row();
                ui.label("Unidad");
                ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(&mut f.unidad));
                ui.label("Ratio € / unidad");
                ui.add(
                    egui::TextEdit::singleline(&mut f.precio)
                        .desired_width(90.0)
                        .hint_text("650,00"),
                );
                ui.end_row();
            });

        // ---- añadir partidas
        ui.horizontal(|ui| {
            ui.label("Añadir partida:");
            ui.add(
                egui::TextEdit::singleline(&mut f.buscar)
                    .desired_width(260.0)
                    .hint_text("código o texto"),
            );
        });
        let buscar = f.buscar.trim().to_lowercase();
        if !buscar.is_empty() {
            let mut vistas = BTreeSet::new();
            egui::ScrollArea::vertical()
                .id_salt("ratio_buscar")
                .max_height(140.0)
                .show(ui, |ui| {
                    for (_, h) in self.orden_partidas() {
                        if !vistas.insert(h.clone()) || f.partidas.contains_key(&h) {
                            continue;
                        }
                        let c = &self.p.conceptos[&h];
                        if c.descomposicion.is_empty()
                            || !format!("{} {}", c.codigo, c.resumen).to_lowercase().contains(&buscar)
                        {
                            continue;
                        }
                        if ui
                            .button(format!("+ {}  {}", c.codigo, corto(&c.resumen, 50)))
                            .clicked()
                        {
                            let mo = c
                                .descomposicion
                                .iter()
                                .filter(|l| {
                                    self.p
                                        .conceptos
                                        .get(&l.hijo)
                                        .is_some_and(|x| x.naturaleza == Naturaleza::ManoObra)
                                })
                                .map(|l| l.hijo.clone())
                                .collect();
                            f.partidas.insert(h.clone(), (false, mo));
                        }
                    }
                });
        }

        // ---- partidas del paquete
        let mut quitar = None;
        for (partida, (medidor, recursos)) in f.partidas.iter_mut() {
            let Some(c) = self.p.conceptos.get(partida) else {
                continue;
            };
            let q = self.calc.cantidades.get(partida).copied().unwrap_or_default();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  {}", c.codigo, corto(&c.resumen, 40))).strong());
                    ui.label(RichText::new(format!("{} {}", num(q, 2), c.unidad)).weak());
                    ui.checkbox(medidor, format!("cuenta como «{}»", f.unidad));
                    if ui.small_button("x").on_hover_text("Quitar del paquete").clicked() {
                        quitar = Some(partida.clone());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Pone la subcontrata:").weak());
                    for l in &c.descomposicion {
                        let Some(h) = self.p.conceptos.get(&l.hijo) else {
                            continue;
                        };
                        if h.naturaleza == Naturaleza::Porcentaje {
                            ui.label(RichText::new(format!("{} (se queda)", h.codigo)).weak());
                            continue;
                        }
                        let mut marcado = recursos.contains(&h.codigo);
                        if ui
                            .checkbox(&mut marcado, &h.codigo)
                            .on_hover_text(format!("{} · {}", nombre_naturaleza(h.naturaleza), h.resumen))
                            .changed()
                        {
                            if marcado {
                                recursos.insert(h.codigo.clone());
                            } else {
                                recursos.remove(&h.codigo);
                            }
                        }
                    }
                });
            });
        }
        if let Some(q) = quitar {
            f.partidas.remove(&q);
        }

        // ---- vista previa (sobre una copia)
        if !f.partidas.is_empty() {
            match f.paquete() {
                Err(e) => {
                    ui.label(RichText::new(e).color(Color32::from_rgb(220, 150, 40)));
                }
                Ok(paq) => match self.p.clone().aplicar_ratio(&paq) {
                    Err(e) => {
                        ui.label(RichText::new(e.to_string()).color(Color32::from_rgb(220, 150, 40)));
                    }
                    Ok(inf) => {
                        let horas: Decimal = inf.horas_liberadas.values().copied().sum();
                        egui::Grid::new("ratio_previa").num_columns(2).show(ui, |ui| {
                            let fila = |ui: &mut Ui, a: &str, b: String| {
                                ui.label(a);
                                ui.label(RichText::new(b).strong());
                                ui.end_row();
                            };
                            fila(ui, "Medidor", format!("{} {}", num(inf.medidor, 2), paq.unidad));
                            fila(ui, "Subcontrata", format!("{} €", eur(inf.importe)));
                            fila(ui, "Coste propio que se quita", format!("{} €", eur(inf.retirado)));
                            fila(ui, "Coste directo antes", format!("{} €", eur(inf.coste_antes)));
                            fila(ui, "Coste directo después", format!("{} €", eur(inf.coste_despues)));
                            let a = inf.ahorro();
                            ui.label(if a >= Decimal::ZERO { "Ahorro" } else { "Sobrecoste" });
                            ui.label(RichText::new(format!("{} €", eur(a.abs()))).strong().color(
                                if a >= Decimal::ZERO {
                                    Color32::from_rgb(80, 175, 110)
                                } else {
                                    Color32::from_rgb(220, 80, 60)
                                },
                            ));
                            ui.end_row();
                            fila(ui, "Horas propias liberadas", num(horas, 2));
                        });
                        if ui
                            .button(RichText::new("Aplicar al presupuesto").strong())
                            .on_hover_text("Cambia los descompuestos; Deshacer (Ctrl+Z) lo revierte")
                            .clicked()
                        {
                            acciones.push(Accion::AplicarRatio(paq));
                            f = FormRatio::default();
                        }
                    }
                },
            }
        }
        self.ratio = f;
    }
}
