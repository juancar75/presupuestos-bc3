// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Datos de la obra»: dirección, cliente, carpeta de documentos y la
//! ficha del edificio (bloques, plantas, plazas, superficies…) con los
//! campos que pide su especialidad. Se guarda con la obra y es la base de
//! las reglas de medición. Claude puede proponer los datos leyendo los
//! documentos de la obra; aquí se aceptan o se descartan.
//! Se abre sola al crear una obra con «Nuevo…» y desde Obra → Datos de la obra.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::Decimal;
use ppto_core::ficha;
use ppto_propuestas::{DatoPropuesto, FichaPropuesta};
use std::time::{Duration, Instant};

enum Cambio {
    Num(&'static str, Option<Decimal>),
    Texto(&'static str, String),
    Aceptar(Vec<DatoPropuesto>),
    Descartar(Vec<String>),
    PedirAClaude,
}

/// Encargo para que la IA rellene los datos de la obra con sus documentos.
pub(super) const ENCARGO: &str = "Rellena los datos de la obra con sus documentos: ver_datos_obra, documentos_obra, \
lee la memoria, el pliego y los planos en PDF con leer_documento y propón con proponer_datos_obra los datos \
que falten, cada uno con su documento y página. Dime después qué marcas, modelos y alcance has visto \
que afecten a la valoración.";

impl Aplicacion {
    /// Relee las propuestas de ficha de la IA (como mucho cada 2 s).
    fn leer_ficha_propuesta(&mut self) {
        if self.ficha_leida.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return;
        }
        self.ficha_leida = Some(Instant::now());
        let obra = ppto_propuestas::clave(&self.p);
        self.ficha_propuesta = self
            .enlace
            .ruta_ficha
            .as_ref()
            .map(|r| FichaPropuesta::cargar(r))
            .and_then(|mut f| f.obras.remove(&obra))
            .unwrap_or_default();
    }

    /// Pone en la obra los datos propuestos (los numéricos que no se leen,
    /// se ignoran) y los quita de la lista de la IA.
    pub(super) fn aceptar_ficha(&mut self, datos: &[DatoPropuesto]) -> usize {
        let mut n = 0;
        for d in datos {
            if ficha::es_numerica(&d.clave) {
                if let Ok(v) = d.valor.trim().replace(',', ".").parse::<Decimal>() {
                    self.ficha.insert(d.clave.clone(), v);
                    n += 1;
                }
            } else if ficha::TEXTOS.iter().any(|c| c.clave == d.clave) {
                self.obra_textos.insert(d.clave.clone(), d.valor.trim().to_owned());
                n += 1;
            }
        }
        self.descartar_ficha(&datos.iter().map(|d| d.clave.clone()).collect::<Vec<_>>());
        if n > 0 {
            self.cambios = true;
            self.enlace.sucia = true;
        }
        n
    }

    fn descartar_ficha(&mut self, claves: &[String]) {
        let obra = ppto_propuestas::clave(&self.p);
        if let Some(r) = &self.enlace.ruta_ficha {
            let _ = FichaPropuesta::modificar(r, |f| {
                for k in claves {
                    f.quitar(&obra, k);
                }
            });
        }
        self.ficha_propuesta.retain(|d| !claves.contains(&d.clave));
    }

    pub(super) fn ventana_ficha(&mut self, ctx: &egui::Context) {
        if !self.ver_ficha {
            return;
        }
        self.leer_ficha_propuesta();
        let mut abierta = true;
        let campos = ficha::campos_de(&self.especialidades);
        let titulo = if self.especialidades.is_empty() {
            "Datos de la obra".to_owned()
        } else {
            let n: Vec<&str> = self.especialidades.iter().map(|e| e.nombre()).collect();
            format!("Datos de la obra · {}", n.join(" + "))
        };
        let propuesta = self.ficha_propuesta.clone();
        let de = |k: &str| propuesta.iter().find(|d| d.clave == k);
        let mut cambios = Vec::new();
        egui::Window::new(titulo)
            .id(egui::Id::new("ficha_edificio"))
            .open(&mut abierta)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Rellena lo que sepas (puedes dejar campos vacíos y completarlos después). \
                         Con estos datos se proponen y comprueban las mediciones. \
                         Claude puede rellenarlos leyendo la carpeta de documentos: \
                         pídele «rellena los datos de la obra con sus documentos».",
                    )
                    .small()
                    .weak(),
                );
                let hay_docs = self.obra_textos.get("documentos").is_some_and(|d| !d.trim().is_empty());
                if hay_docs
                    && ui
                        .button("✨ Que Claude los rellene con los documentos")
                        .on_hover_text(
                            "Deja el encargo en el panel del asistente: Claude lee la carpeta de documentos \
                             (memoria, pliego, planos en PDF) y propone los datos que falten, con su fuente",
                        )
                        .clicked()
                {
                    cambios.push(Cambio::PedirAClaude);
                }
                if !propuesta.is_empty() {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(70, 130, 200),
                            format!("Claude propone {} datos (en azul, con su fuente).", propuesta.len()),
                        );
                        if ui.button("Aceptar todos").clicked() {
                            cambios.push(Cambio::Aceptar(propuesta.clone()));
                        }
                        if ui.button("Descartar todos").clicked() {
                            cambios.push(Cambio::Descartar(propuesta.iter().map(|d| d.clave.clone()).collect()));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Estado de la oferta:");
                    let actual = self.estado_oferta();
                    if self.p.seguimiento.contrato.is_some() {
                        ui.label(RichText::new("Adjudicada (contratada)").strong());
                    } else {
                        for e in ppto_biblioteca::huellas::Estado::TODOS {
                            if ui.selectable_label(actual == e, e.nombre()).clicked() && actual != e {
                                cambios.push(Cambio::Texto("estado", e.nombre().to_owned()));
                            }
                        }
                    }
                })
                .response
                .on_hover_text("Para comparar con obras parecidas: las adjudicadas pesan más");
                ui.separator();
                let propuesto = |ui: &mut egui::Ui, cambios: &mut Vec<Cambio>, d: Option<&DatoPropuesto>| {
                    if let Some(d) = d {
                        ui.colored_label(egui::Color32::from_rgb(70, 130, 200), format!("→ {}", d.valor))
                            .on_hover_text(format!("Propuesto por Claude. Fuente: {}", d.fuente));
                        if ui.small_button("✔").on_hover_text("Aceptar").clicked() {
                            cambios.push(Cambio::Aceptar(vec![d.clone()]));
                        }
                        if ui.small_button("✖").on_hover_text("Descartar").clicked() {
                            cambios.push(Cambio::Descartar(vec![d.clave.clone()]));
                        }
                    }
                };
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    egui::Grid::new("ficha_textos")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Obra").strong());
                            ui.end_row();
                            for c in ficha::TEXTOS {
                                ui.label(c.etiqueta);
                                let mut t = self.obra_textos.get(c.clave).cloned().unwrap_or_default();
                                ui.horizontal(|ui| {
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut t)
                                            .hint_text(c.pista)
                                            .desired_width(300.0),
                                    );
                                    if r.changed() {
                                        cambios.push(Cambio::Texto(c.clave, t.clone()));
                                    }
                                    if c.clave == "documentos" {
                                        if ui.small_button("📁").on_hover_text("Elegir la carpeta").clicked()
                                            && let Some(d) = rfd::FileDialog::new()
                                                .set_title("Carpeta de documentos de la obra")
                                                .pick_folder()
                                        {
                                            cambios.push(Cambio::Texto(c.clave, d.display().to_string()));
                                        }
                                        if !t.trim().is_empty()
                                            && ui.small_button("Abrir").on_hover_text("Abrir la carpeta").clicked()
                                        {
                                            super::asistente::abrir_en_sistema(std::path::Path::new(t.trim()));
                                        }
                                    }
                                });
                                ui.horizontal(|ui| propuesto(ui, &mut cambios, de(c.clave)));
                                ui.end_row();
                            }
                        });
                    ui.add_space(6.0);
                    let mut grupo = "";
                    egui::Grid::new("ficha_grid")
                        .num_columns(4)
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
                                if let Some(n) =
                                    self.edicion.opcional(ui, &format!("ficha|{}", c.clave), valor, 2, 80.0)
                                {
                                    cambios.push(Cambio::Num(c.clave, n));
                                }
                                ui.label(RichText::new(c.unidad).weak());
                                ui.horizontal(|ui| propuesto(ui, &mut cambios, de(c.clave)));
                                ui.end_row();
                            }
                        });
                    // Lo que propone la IA y no está entre los campos de estas especialidades
                    let fuera: Vec<&DatoPropuesto> = propuesta
                        .iter()
                        .filter(|d| {
                            !campos.iter().any(|c| c.clave == d.clave)
                                && !ficha::TEXTOS.iter().any(|c| c.clave == d.clave)
                        })
                        .collect();
                    if !fuera.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Otros datos propuestos").strong());
                        for d in fuera {
                            ui.horizontal(|ui| {
                                let (et, ud) = ficha::describir(&d.clave).unwrap_or((d.clave.as_str(), ""));
                                ui.label(format!("{et}: {} {ud}", d.valor)).on_hover_text(&d.fuente);
                                if ficha::describir(&d.clave).is_some() && ui.small_button("✔").clicked() {
                                    cambios.push(Cambio::Aceptar(vec![d.clone()]));
                                }
                                if ui.small_button("✖").clicked() {
                                    cambios.push(Cambio::Descartar(vec![d.clave.clone()]));
                                }
                            });
                        }
                    }
                });
            });
        for c in cambios {
            match c {
                Cambio::Num(k, Some(v)) => {
                    self.ficha.insert(k.to_owned(), v);
                }
                Cambio::Num(k, None) => {
                    self.ficha.remove(k);
                }
                Cambio::Texto(k, t) => {
                    if t.trim().is_empty() {
                        self.obra_textos.remove(k);
                    } else {
                        self.obra_textos.insert(k.to_owned(), t);
                    }
                }
                Cambio::Aceptar(d) => {
                    let n = self.aceptar_ficha(&d);
                    self.info(format!("{n} datos de la obra aceptados."));
                }
                Cambio::Descartar(k) => self.descartar_ficha(&k),
                Cambio::PedirAClaude => {
                    self.asistente.entrada = ENCARGO.to_owned();
                    self.asistente.abierto = true;
                    match self.asistente.enviar(&self.p) {
                        Ok(()) => self.info(
                            "Encargo dejado en el panel del asistente. Con Claude Desktop: pídale «atiende el asistente».",
                        ),
                        Err(e) => self.error(e),
                    }
                    continue;
                }
            }
            self.cambios = true;
            self.enlace.sucia = true;
        }
        self.ver_ficha = abierta;
    }
}
