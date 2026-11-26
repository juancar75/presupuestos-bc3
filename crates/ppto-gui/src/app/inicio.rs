// SPDX-License-Identifier: GPL-3.0-or-later
//! Pantalla de inicio: al arrancar sin obra se ve esto (obras recientes,
//! nuevo, abrir, importar ciego) con un presupuesto vacío detrás, nunca el
//! ejemplo. El ejemplo sintético solo se abre a propósito (Ayuda) y lleva una
//! franja que lo avisa.

use super::recientes::{InfoReciente, etiqueta};
use super::*;

/// Lo que se elige en la pantalla de inicio.
enum Eleccion {
    Nuevo,
    Abrir,
    Ciego,
    Reciente(std::path::PathBuf),
    Ejemplo,
}

impl Aplicacion {
    /// Arranque sin obra: presupuesto vacío y pantalla de inicio.
    pub(super) fn arrancar_vacio(&mut self) {
        self.nuevo_presupuesto();
        self.mensaje = None;
        self.inicio = true;
    }

    /// Resumen de la obra abierta para la lista de recientes.
    pub(super) fn info_reciente(&self) -> InfoReciente {
        let texto = |k: &str| {
            self.obra_textos
                .get(k)
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
        };
        InfoReciente {
            nombre: texto("titulo").unwrap_or_else(|| self.p.nombre.clone()),
            cliente: texto("cliente").unwrap_or_default(),
            estado: self.estado_oferta().nombre().to_owned(),
            fecha: ppto_pdf::fecha_hoy(),
        }
    }

    /// Franja naranja mientras está abierto el ejemplo.
    pub(super) fn franja_ejemplo(&mut self, ui: &mut Ui) {
        if !self.es_ejemplo {
            return;
        }
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(230, 140, 40))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("EJEMPLO — datos inventados para practicar. No es una obra real.")
                            .strong()
                            .color(egui::Color32::BLACK),
                    );
                    if ui.small_button("Cerrar el ejemplo").clicked() {
                        self.arrancar_vacio();
                    }
                });
            });
    }

    pub(super) fn pantalla_inicio(&mut self, ctx: &egui::Context) {
        if !self.inicio {
            return;
        }
        // Si se abre otra ventana encima (nuevo, ciego…), esta se aparta
        if self.asistente_nuevo.is_some() {
            return;
        }
        let mut abierta = true;
        let mut eleccion = None;
        let mut abrir_ultima = self.recientes.abrir_ultima;
        let rutas = self.recientes.rutas.clone();
        egui::Window::new("presupuestos-bc3")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(RichText::new("¿Con qué obra trabajamos?").heading());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let grande = |t: &str| egui::Button::new(RichText::new(t).strong().size(15.0));
                    if ui.add(grande("＋ Nuevo presupuesto…")).clicked() {
                        eleccion = Some(Eleccion::Nuevo);
                    }
                    if ui.add(grande("📂 Abrir…")).clicked() {
                        eleccion = Some(Eleccion::Abrir);
                    }
                    if ui
                        .add(grande("📥 Importar ciego del cliente…"))
                        .on_hover_text("BC3 o Excel sin precios")
                        .clicked()
                    {
                        eleccion = Some(Eleccion::Ciego);
                    }
                });
                ui.add_space(8.0);
                if rutas.is_empty() {
                    ui.label(RichText::new("Todavía no hay obras recientes.").weak());
                } else {
                    ui.label(RichText::new("Obras recientes").strong());
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        egui::Grid::new("inicio_recientes")
                            .striped(true)
                            .num_columns(4)
                            .spacing([12.0, 4.0])
                            .show(ui, |ui| {
                                for r in &rutas {
                                    let info = self.recientes.info.get(r).cloned().unwrap_or_default();
                                    let existe = r.is_file();
                                    let nombre = if info.nombre.is_empty() {
                                        etiqueta(r)
                                    } else {
                                        info.nombre.clone()
                                    };
                                    let b = ui
                                        .add_enabled(existe, egui::Button::new(nombre).frame(false))
                                        .on_hover_text(r.display().to_string())
                                        .on_disabled_hover_text(format!("Ya no está: {}", r.display()));
                                    if b.clicked() {
                                        eleccion = Some(Eleccion::Reciente(r.clone()));
                                    }
                                    ui.label(RichText::new(&info.cliente).weak());
                                    ui.label(RichText::new(&info.estado).weak());
                                    ui.label(RichText::new(&info.fecha).weak());
                                    ui.end_row();
                                }
                            });
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut abrir_ultima, "Abrir siempre la última obra al arrancar")
                        .on_hover_text("Si se marca, el programa empieza en la obra de la última vez");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Abrir el ejemplo para practicar")
                            .on_hover_text("Obra inventada de suelo radiante")
                            .clicked()
                        {
                            eleccion = Some(Eleccion::Ejemplo);
                        }
                    });
                });
            });
        if abrir_ultima != self.recientes.abrir_ultima {
            self.recientes.fijar_abrir_ultima(abrir_ultima);
        }
        if !abierta {
            self.inicio = false;
        }
        match eleccion {
            Some(Eleccion::Nuevo) => self.pedir_nuevo(),
            Some(Eleccion::Abrir) => self.dialogo_abrir(),
            Some(Eleccion::Ciego) => self.dialogo_ciego(),
            Some(Eleccion::Reciente(r)) => self.abrir(r),
            Some(Eleccion::Ejemplo) => self.cargar_ejemplo(),
            None => {}
        }
    }
}
