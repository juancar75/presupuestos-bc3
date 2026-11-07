// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Datos del edificio»: la ficha de la obra (bloques, plantas,
//! plazas, superficies, escaleras…) con los campos que pide su
//! especialidad. Se guarda con la obra y es la base de las reglas de
//! medición. Se abre sola al crear una obra con «Nuevo…» y desde
//! Obra → Datos del edificio.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::ficha;

impl Aplicacion {
    pub(super) fn ventana_ficha(&mut self, ctx: &egui::Context) {
        if !self.ver_ficha {
            return;
        }
        let mut abierta = true;
        let campos = ficha::campos(self.especialidad);
        let titulo = match self.especialidad {
            Some(e) => format!("Datos del edificio · {}", e.nombre()),
            None => "Datos del edificio".to_owned(),
        };
        let mut cambios = Vec::new();
        egui::Window::new(titulo)
            .id(egui::Id::new("ficha_edificio"))
            .open(&mut abierta)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Rellena lo que sepas (puedes dejar campos vacíos y completarlos después). \
                         Con estos datos se proponen y comprueban las mediciones.",
                    )
                    .small()
                    .weak(),
                );
                let mut grupo = "";
                egui::Grid::new("ficha_grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for c in &campos {
                            if c.grupo != grupo {
                                grupo = c.grupo;
                                ui.label(RichText::new(grupo).strong());
                                ui.end_row();
                            }
                            ui.label(c.etiqueta);
                            let valor = self.ficha.get(c.clave).copied();
                            if let Some(n) = self.edicion.opcional(ui, &format!("ficha|{}", c.clave), valor, 2, 80.0) {
                                cambios.push((c.clave, n));
                            }
                            ui.label(RichText::new(c.unidad).weak());
                            ui.end_row();
                        }
                    });
            });
        for (k, v) in cambios {
            match v {
                Some(v) => self.ficha.insert(k.to_owned(), v),
                None => self.ficha.remove(k),
            };
            self.cambios = true;
        }
        self.ver_ficha = abierta;
    }
}
