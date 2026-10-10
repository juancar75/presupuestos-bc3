// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Presupuesto en PDF»: portada (logo, cliente, fecha, revisión),
//! firma, porcentajes, observaciones y plantilla. Los datos de la empresa,
//! la firma, el logo y las observaciones se recuerdan entre sesiones.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::formato::{leer, num};
use ppto_pdf::OpcionesPdf;
use std::path::PathBuf;

/// Estado de la ventana mientras está abierta.
pub(super) struct VentanaPdf {
    pub o: OpcionesPdf,
    pub observaciones: String,
    pub gg: String,
    pub bi: String,
    pub iva: String,
}

/// Fichero de preferencias: `%APPDATA%\presupuestos-bc3\pdf.json` en Windows,
/// `~/.config/presupuestos-bc3/pdf.json` en Linux.
fn ruta_preferencias() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("presupuestos-bc3").join("pdf.json"))
}

fn cargar_preferencias() -> OpcionesPdf {
    ruta_preferencias()
        .and_then(|r| std::fs::read(r).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn guardar_preferencias(o: &OpcionesPdf) {
    // Solo lo que no cambia de una obra a otra
    let fijo = OpcionesPdf {
        tipo_documento: o.tipo_documento.clone(),
        lugar: o.lugar.clone(),
        empresa: o.empresa.clone(),
        empresa_datos: o.empresa_datos.clone(),
        autor: o.autor.clone(),
        autor_datos: o.autor_datos.clone(),
        logo: o.logo.clone(),
        observaciones: o.observaciones.clone(),
        mostrar_mediciones: o.mostrar_mediciones,
        mostrar_textos: o.mostrar_textos,
        plantilla: o.plantilla.clone(),
        ..OpcionesPdf::default()
    };
    if let Some(r) = ruta_preferencias()
        && let Some(dir) = r.parent()
        && std::fs::create_dir_all(dir).is_ok()
        && let Ok(json) = serde_json::to_vec_pretty(&fijo)
    {
        let _ = std::fs::write(r, json);
    }
}

impl Aplicacion {
    pub(super) fn abrir_pdf(&mut self) {
        let mut o = cargar_preferencias();
        o.fecha = ppto_pdf::fecha_hoy();
        o.revision = match self.etiqueta_actual().as_str() {
            "sin guardar" => "R0".into(),
            e => e.to_owned(),
        };
        o.gastos_generales = self.gg;
        o.beneficio_industrial = self.bi;
        o.iva = self.iva;
        self.ventana_pdf = Some(VentanaPdf {
            observaciones: o.observaciones.join("\n"),
            gg: num(o.gastos_generales, 2),
            bi: num(o.beneficio_industrial, 2),
            iva: num(o.iva, 2),
            o,
        });
    }

    pub(super) fn ventana_pdf(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_pdf.take() else {
            return;
        };
        let mut abierta = true;
        let mut generar = false;
        let titulo_obra = self.p.conceptos[&self.p.raiz].resumen.clone();
        egui::Window::new("Presupuesto en PDF")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(620.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                    let o = &mut v.o;
                    ui.label(RichText::new("Portada").strong());
                    egui::Grid::new("pdf_portada")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            fila(ui, "Tipo de documento", &mut o.tipo_documento, "Presupuesto");
                            fila(ui, "Título", &mut o.titulo, &titulo_obra);
                            fila(ui, "Dirección de la obra", &mut o.direccion, "");
                            fila(ui, "Cliente", &mut o.cliente, "");
                            fila(ui, "Referencia", &mut o.referencia, &self.p.raiz);
                            fila(ui, "Revisión", &mut o.revision, "R0");
                            fila(ui, "Fecha", &mut o.fecha, "");
                            ui.label("Logo");
                            ui.horizontal(|ui| {
                                let txt = o
                                    .logo
                                    .as_ref()
                                    .and_then(|l| l.file_name())
                                    .map_or("(sin logo)".to_owned(), |n| n.to_string_lossy().into_owned());
                                ui.label(txt);
                                if ui.button("Elegir…").clicked()
                                    && let Some(r) = rfd::FileDialog::new()
                                        .add_filter("Imagen", &["png", "jpg", "jpeg", "svg", "gif"])
                                        .pick_file()
                                {
                                    o.logo = Some(r);
                                }
                                if o.logo.is_some() && ui.button("Quitar").clicked() {
                                    o.logo = None;
                                }
                            });
                            ui.end_row();
                        });
                    ui.separator();
                    ui.label(RichText::new("Empresa y firma (se recuerdan)").strong());
                    egui::Grid::new("pdf_firma")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            fila(ui, "Empresa", &mut o.empresa, "");
                            fila(
                                ui,
                                "Datos de la empresa",
                                &mut o.empresa_datos,
                                "CIF · dirección · teléfono",
                            );
                            fila(ui, "Firma", &mut o.autor, "Nombre y apellidos");
                            fila(
                                ui,
                                "Datos de la firma",
                                &mut o.autor_datos,
                                "Titulación · n.º de colegiado",
                            );
                            fila(ui, "Lugar", &mut o.lugar, "Madrid");
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("GG %");
                        ui.add(egui::TextEdit::singleline(&mut v.gg).desired_width(50.0));
                        ui.label("BI %");
                        ui.add(egui::TextEdit::singleline(&mut v.bi).desired_width(50.0));
                        ui.label("IVA %");
                        ui.add(egui::TextEdit::singleline(&mut v.iva).desired_width(50.0));
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut o.mostrar_textos, "Textos de las partidas");
                        ui.checkbox(&mut o.mostrar_mediciones, "Líneas de medición");
                    });
                    ui.separator();
                    ui.label(RichText::new("Observaciones al presupuesto (una por línea; se recuerdan)").strong());
                    ui.add(
                        egui::TextEdit::multiline(&mut v.observaciones)
                            .desired_width(f32::INFINITY)
                            .desired_rows(6),
                    );
                    if ui.small_button("Restaurar observaciones habituales").clicked() {
                        v.observaciones = ppto_pdf::OBSERVACIONES_HABITUALES.join("\n");
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Plantilla:");
                        let txt = o
                            .plantilla
                            .as_ref()
                            .map_or("la incluida en el programa".to_owned(), |p| p.display().to_string());
                        ui.label(RichText::new(txt).weak());
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button("Guardar copia de la plantilla…")
                            .on_hover_text("Para editarla (Typst) y usarla después con «Usar plantilla…»")
                            .clicked()
                            && let Some(r) = rfd::FileDialog::new()
                                .add_filter("Typst", &["typ"])
                                .set_file_name("presupuesto.typ")
                                .save_file()
                        {
                            let _ = std::fs::write(&r, ppto_pdf::PLANTILLA);
                            o.plantilla = Some(r);
                        }
                        if ui.button("Usar plantilla…").clicked()
                            && let Some(r) = rfd::FileDialog::new().add_filter("Typst", &["typ"]).pick_file()
                        {
                            o.plantilla = Some(r);
                        }
                        if o.plantilla.is_some() && ui.button("Usar la incluida").clicked() {
                            o.plantilla = None;
                        }
                    });
                });
                ui.separator();
                generar = ui.button(RichText::new("Generar PDF…").strong()).clicked();
            });
        if generar {
            match (leer(&v.gg), leer(&v.bi), leer(&v.iva)) {
                (Some(gg), Some(bi), Some(iva)) => {
                    v.o.gastos_generales = gg;
                    v.o.beneficio_industrial = bi;
                    v.o.iva = iva;
                    v.o.observaciones = v
                        .observaciones
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_owned)
                        .collect();
                    if self.generar_pdf(&v.o) {
                        guardar_preferencias(&v.o);
                        abierta = false;
                    }
                }
                _ => self.error("GG, BI o IVA no son números válidos (use coma decimal: 13,00)."),
            }
        }
        if abierta {
            self.ventana_pdf = Some(v);
        }
    }

    /// Pide la ruta y genera el PDF. Devuelve `true` si se ha guardado.
    fn generar_pdf(&mut self, o: &OpcionesPdf) -> bool {
        let base = if o.referencia.is_empty() {
            &self.p.nombre
        } else {
            &o.referencia
        };
        let nombre =
            format!("{} {}.pdf", base, o.revision).replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(nombre)
            .save_file()
        else {
            return false;
        };
        match ppto_pdf::guardar_pdf(&self.p, o, &ruta) {
            Ok(()) => {
                self.info(format!("PDF guardado en {}", ruta.display()));
                true
            }
            Err(e) => {
                self.error(format!("No se pudo generar el PDF: {e}"));
                false
            }
        }
    }
}

fn fila(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String, pista: &str) {
    ui.label(etiqueta);
    ui.add(egui::TextEdit::singleline(valor).desired_width(400.0).hint_text(pista));
    ui.end_row();
}
