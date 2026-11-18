// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Comprobar antes de valorar»: lo primero, leer la obra y ver si
//! están todos los recursos para sacar los precios.

use super::*;
use ppto_biblioteca::comprobar::{Nivel, comprobar_con_carpeta};

impl Aplicacion {
    pub(super) fn abrir_comprobar(&mut self) {
        let b = self
            .ruta_biblioteca
            .as_deref()
            .map(ppto_biblioteca::Biblioteca::cargar)
            .unwrap_or_default();
        self.comprobacion = Some(comprobar_con_carpeta(&self.p, &b));
    }

    pub(super) fn ventana_comprobar(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.comprobacion else { return };
        let mut abierta = true;
        let mut otra_vez = false;
        let mut copiar = None;
        let mut ayuda = false;
        egui::Window::new("Comprobar antes de valorar")
            .open(&mut abierta)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} partidas, {} sin precio. Antes de valorar: ¿están las tarifas y los descuentos?",
                    c.partidas, c.sin_precio
                ));
                ui.separator();
                let avisos = c.avisos();
                if avisos.is_empty() {
                    ui.label("Nada que comprobar: la obra no tiene materiales de fabricante ni marcas conocidas.");
                }
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    for (n, a) in &avisos {
                        let (icono, color) = match n {
                            Nivel::Falta => ("✖", egui::Color32::from_rgb(210, 60, 60)),
                            Nivel::Revisar => ("⚠", egui::Color32::from_rgb(210, 150, 30)),
                            Nivel::Bien => ("✔", egui::Color32::from_rgb(60, 160, 90)),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(color, icono);
                            ui.label(a);
                        });
                    }
                });
                ui.separator();
                ui.label(
                    RichText::new(
                        "Fabricantes: tarifa en la carpeta de tarifas o en la biblioteca, y descuento en la obra. \
                         Use el mismo nombre de fabricante en la tarifa, el descuento y los materiales.",
                    )
                    .weak()
                    .small(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("?")
                        .on_hover_text("Ayuda: comprobar antes de valorar (F1)")
                        .clicked()
                    {
                        ayuda = true;
                    }
                    if ui.button("Volver a comprobar").clicked() {
                        otra_vez = true;
                    }
                    if ui
                        .button("Copiar")
                        .on_hover_text("Copiar la lista para pegarla en un correo o en el chat")
                        .clicked()
                    {
                        copiar = Some(c.texto());
                    }
                });
            });
        if let Some(t) = copiar {
            ctx.copy_text(t);
        }
        if ayuda {
            self.abrir_ayuda(Some("comprobar"));
        }
        if !abierta {
            self.comprobacion = None;
        } else if otra_vez {
            self.abrir_comprobar();
        }
    }
}
