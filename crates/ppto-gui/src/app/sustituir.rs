// SPDX-License-Identifier: GPL-3.0-or-later
//! «Sustituir en todo el presupuesto por…»: cambia un recurso (o una
//! partida) por otro del mismo tipo en todas las partidas donde está.

use super::{Accion, Aplicacion, corto};
use eframe::egui::{self, RichText};
use ppto_core::Naturaleza;
use ppto_core::formato::eur;

pub(super) struct FormSustituir {
    pub viejo: String,
    pub filtro: String,
}

impl FormSustituir {
    pub fn new(viejo: String) -> Self {
        Self {
            viejo,
            filtro: String::new(),
        }
    }
}

fn es_recurso(n: Naturaleza) -> bool {
    !matches!(n, Naturaleza::Capitulo | Naturaleza::Partida)
}

impl Aplicacion {
    pub(super) fn ventana_sustituir(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.sustituir.take() else { return };
        let Some(viejo) = self.p.conceptos.get(&v.viejo) else {
            return;
        };
        let recurso = es_recurso(viejo.naturaleza);
        let usos = self.p.padres(&v.viejo);
        let mut abierta = true;
        let mut hecho = false;
        egui::Window::new(format!("Sustituir {} en todo el presupuesto", v.viejo))
            .open(&mut abierta)
            .default_width(600.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{}  {} ({} €) está en {} {}. Elija por cuál sustituirlo: se pone el nuevo con la misma cantidad \
                     en todas (si ya estaba, se suman). Se puede deshacer.",
                    viejo.codigo,
                    corto(&viejo.resumen, 50),
                    eur(self.calc.precios.get(&v.viejo).copied().unwrap_or(viejo.precio)),
                    usos.len(),
                    if usos.len() == 1 { "concepto" } else { "conceptos" }
                ));
                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.filtro)
                            .hint_text("código, texto o fabricante")
                            .desired_width(300.0),
                    )
                    .request_focus();
                });
                let f = super::valorar::plano(&v.filtro);
                let candidatos: Vec<_> = self
                    .p
                    .conceptos
                    .values()
                    .filter(|c| c.codigo != v.viejo && c.naturaleza != Naturaleza::Capitulo)
                    .filter(|c| es_recurso(c.naturaleza) == recurso)
                    .filter(|c| {
                        f.is_empty()
                            || super::valorar::plano(&format!(
                                "{} {} {}",
                                c.codigo,
                                c.resumen,
                                c.tarifa.as_ref().map(|t| t.fabricante.as_str()).unwrap_or("")
                            ))
                            .contains(&f)
                    })
                    .take(200)
                    .collect();
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for c in &candidatos {
                        let precio = self.calc.precios.get(&c.codigo).copied().unwrap_or(c.precio);
                        let t = format!(
                            "{}  {}  ·  {} {} €",
                            c.codigo,
                            corto(&c.resumen, 60),
                            c.unidad,
                            eur(precio)
                        );
                        if ui
                            .button(t)
                            .on_hover_text(format!("Sustituir {} por {}", v.viejo, c.codigo))
                            .clicked()
                        {
                            acciones.push(Accion::Sustituir(v.viejo.clone(), c.codigo.clone()));
                            hecho = true;
                        }
                    }
                    if candidatos.is_empty() {
                        ui.label(RichText::new("No hay nada del mismo tipo con ese texto.").weak());
                    }
                });
            });
        if abierta && !hecho {
            self.sustituir = Some(v);
        }
    }
}
