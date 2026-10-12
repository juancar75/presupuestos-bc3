// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Presupuesto en PDF».
//!
//! - **Perfil de empresa** (desplegable): logo, empresa, firma, observaciones
//!   y tipo de resumen (con o sin PEM + GG + BI). Se guardan en
//!   `%APPDATA%\presupuestos-bc3\perfiles.json` (Linux: `~/.config/...`).
//! - **Datos de la obra**: título, cliente, referencia, revisión y fecha.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::formato::{leer, num};
use ppto_pdf::{OpcionesPdf, Perfiles};
use std::path::PathBuf;

pub(super) struct VentanaPdf {
    perfiles: Perfiles,
    /// Datos de la obra (los del perfil se aplican al generar).
    obra: OpcionesPdf,
    /// Campos de texto del perfil activo mientras se editan.
    observaciones: String,
    gg: String,
    bi: String,
    iva: String,
}

/// Carpeta de configuración del programa.
fn carpeta_config() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("presupuestos-bc3"))
}

pub(super) fn ruta_perfiles() -> Option<PathBuf> {
    carpeta_config().map(|c| c.join("perfiles.json"))
}

impl VentanaPdf {
    /// Copia al perfil activo lo escrito en los campos de texto.
    fn volcar(&mut self) -> Result<(), String> {
        let (Some(gg), Some(bi), Some(iva)) = (leer(&self.gg), leer(&self.bi), leer(&self.iva)) else {
            return Err("GG, BI o IVA no son números válidos (use coma decimal: 13,00).".into());
        };
        let p = self.perfiles.activo_mut();
        p.gastos_generales = gg;
        p.beneficio_industrial = bi;
        p.iva = iva;
        p.observaciones = self
            .observaciones
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(())
    }

    /// Rellena los campos de texto con el perfil activo.
    fn cargar_textos(&mut self) {
        let p = self.perfiles.activo();
        self.observaciones = p.observaciones.join("\n");
        self.gg = num(p.gastos_generales, 2);
        self.bi = num(p.beneficio_industrial, 2);
        self.iva = num(p.iva, 2);
    }
}

impl Aplicacion {
    pub(super) fn abrir_pdf(&mut self) {
        let perfiles = ruta_perfiles().map(|r| Perfiles::cargar(&r)).unwrap_or_default();
        let obra = OpcionesPdf {
            fecha: ppto_pdf::fecha_hoy(),
            revision: match self.etiqueta_actual().as_str() {
                "sin guardar" => "R0".into(),
                e => e.to_owned(),
            },
            ..OpcionesPdf::default()
        };
        let mut v = VentanaPdf {
            perfiles,
            obra,
            observaciones: String::new(),
            gg: String::new(),
            bi: String::new(),
            iva: String::new(),
        };
        v.cargar_textos();
        self.ventana_pdf = Some(v);
    }

    pub(super) fn ventana_pdf(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_pdf.take() else {
            return;
        };
        let mut abierta = true;
        let mut generar = false;
        let titulo_obra = self.p.conceptos[&self.p.raiz].resumen.clone();
        let raiz = self.p.raiz.clone();
        let mut mensaje: Option<(String, bool)> = None;
        egui::Window::new("Presupuesto en PDF")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .show(ctx, |ui| {
                // ---------------------------------------------------- perfil
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Perfil de empresa").strong());
                    let antes = v.perfiles.activo;
                    let mut elegido = antes;
                    egui::ComboBox::from_id_salt("perfil_pdf")
                        .width(260.0)
                        .selected_text(v.perfiles.activo().nombre.clone())
                        .show_ui(ui, |ui| {
                            for (i, p) in v.perfiles.lista.iter().enumerate() {
                                ui.selectable_value(&mut elegido, i, &p.nombre);
                            }
                        });
                    if elegido != antes {
                        if let Err(e) = v.volcar() {
                            mensaje = Some((e, true));
                        }
                        v.perfiles.activo = elegido;
                        v.cargar_textos();
                    }
                    if ui
                        .button("Duplicar")
                        .on_hover_text("Crear otro perfil a partir de este")
                        .clicked()
                    {
                        let _ = v.volcar();
                        v.perfiles.duplicar();
                        v.cargar_textos();
                    }
                    if v.perfiles.lista.len() > 1 && ui.button("Borrar").clicked() {
                        v.perfiles.borrar_activo();
                        v.cargar_textos();
                    }
                });
                egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                    let mut subir_logo = None;
                    {
                        let p = v.perfiles.activo_mut();
                        egui::Grid::new("pdf_perfil")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                fila(ui, "Nombre del perfil", &mut p.nombre, "");
                                fila(ui, "Tipo de documento", &mut p.tipo_documento, "Oferta / Presupuesto");
                                ui.label("Logo");
                                ui.horizontal(|ui| {
                                    let txt = p
                                        .logo
                                        .as_ref()
                                        .and_then(|l| l.file_name())
                                        .map_or("(sin logo)".to_owned(), |n| n.to_string_lossy().into_owned());
                                    ui.label(txt);
                                    if ui
                                        .button("Subir logo…")
                                        .on_hover_text("PNG, JPG o SVG. Se guarda una copia con el perfil.")
                                        .clicked()
                                        && let Some(origen) = rfd::FileDialog::new()
                                            .add_filter("Imagen", &["png", "jpg", "jpeg", "svg", "gif"])
                                            .pick_file()
                                    {
                                        subir_logo = Some(origen);
                                    }
                                });
                                ui.end_row();
                            });
                        if let Some(origen) = subir_logo {
                            mensaje = Some(match carpeta_config() {
                                Some(dir) => match v.perfiles.importar_logo(&origen, &dir) {
                                    Ok(_) => ("Logo guardado en el perfil.".into(), false),
                                    Err(e) => (e.to_string(), true),
                                },
                                None => ("No se encuentra la carpeta de configuración.".into(), true),
                            });
                        }
                        let p = v.perfiles.activo_mut();
                        if p.logo.is_some() && ui.small_button("Quitar logo").clicked() {
                            p.logo = None;
                        }
                        egui::Grid::new("pdf_empresa")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                fila(ui, "Empresa", &mut p.empresa, "");
                                fila(
                                    ui,
                                    "Datos de la empresa",
                                    &mut p.empresa_datos,
                                    "CIF · dirección · teléfono",
                                );
                                fila(ui, "Firma", &mut p.autor, "Nombre y apellidos");
                                fila(
                                    ui,
                                    "Datos de la firma",
                                    &mut p.autor_datos,
                                    "Cargo · titulación · n.º de colegiado",
                                );
                                fila(ui, "Lugar", &mut p.lugar, "Madrid");
                            });
                        ui.add_space(4.0);
                        ui.checkbox(
                            &mut p.desglose_contrata,
                            "Resumen con PEM, gastos generales y beneficio industrial",
                        )
                        .on_hover_text(
                            "Desmarcado (instaladora): suma de capítulos, IVA y total. GG y BI no se aplican.",
                        );
                    }
                    ui.horizontal(|ui| {
                        let desglose = v.perfiles.activo().desglose_contrata;
                        ui.add_enabled_ui(desglose, |ui| {
                            ui.label("GG %");
                            ui.add(egui::TextEdit::singleline(&mut v.gg).desired_width(50.0));
                            ui.label("BI %");
                            ui.add(egui::TextEdit::singleline(&mut v.bi).desired_width(50.0));
                        });
                        ui.label("IVA %");
                        ui.add(egui::TextEdit::singleline(&mut v.iva).desired_width(50.0));
                    });
                    {
                        let p = v.perfiles.activo_mut();
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut p.mostrar_textos, "Textos de las partidas");
                            ui.checkbox(&mut p.mostrar_mediciones, "Líneas de medición");
                        });
                    }
                    ui.label(RichText::new("Observaciones de este perfil (una por línea)").strong());
                    ui.add(
                        egui::TextEdit::multiline(&mut v.observaciones)
                            .desired_width(f32::INFINITY)
                            .desired_rows(6),
                    );
                    ui.horizontal(|ui| {
                        if ui.small_button("Poner las de instaladora").clicked() {
                            v.observaciones = ppto_pdf::perfiles::OBSERVACIONES_INSTALADORA.join("\n");
                        }
                        if ui.small_button("Poner las de ingeniería").clicked() {
                            v.observaciones = ppto_pdf::OBSERVACIONES_HABITUALES.join("\n");
                        }
                    });
                    plantilla(ui, v.perfiles.activo_mut());

                    // ------------------------------------------------ obra
                    ui.separator();
                    ui.label(RichText::new("Datos de esta obra").strong());
                    let o = &mut v.obra;
                    egui::Grid::new("pdf_obra")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            fila(ui, "Título", &mut o.titulo, &titulo_obra);
                            fila(ui, "Dirección de la obra", &mut o.direccion, "");
                            fila(ui, "Cliente", &mut o.cliente, "");
                            fila(ui, "Referencia", &mut o.referencia, &raiz);
                            fila(ui, "Revisión", &mut o.revision, "R0");
                            fila(ui, "Fecha", &mut o.fecha, "");
                        });
                });
                ui.separator();
                generar = ui.button(RichText::new("Generar PDF…").strong()).clicked();
            });
        if generar {
            match v.volcar() {
                Ok(()) => {
                    let mut o = v.obra.clone();
                    v.perfiles.activo().aplicar(&mut o);
                    if self.generar_pdf(&o) {
                        abierta = false;
                    }
                }
                Err(e) => mensaje = Some((e, true)),
            }
        }
        // Los perfiles se guardan siempre al cerrar o generar
        if !abierta {
            let _ = v.volcar();
            if let Some(r) = ruta_perfiles()
                && let Err(e) = v.perfiles.guardar(&r)
            {
                mensaje = Some((format!("No se pudieron guardar los perfiles: {e}"), true));
            }
        }
        match mensaje {
            Some((m, true)) => self.error(m),
            Some((m, false)) => self.info(m),
            None => {}
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

fn plantilla(ui: &mut egui::Ui, p: &mut ppto_pdf::Perfil) {
    ui.horizontal(|ui| {
        ui.label("Plantilla:");
        let txt = p
            .plantilla
            .as_ref()
            .map_or("la incluida en el programa".to_owned(), |r| r.display().to_string());
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
            && std::fs::write(&r, ppto_pdf::PLANTILLA).is_ok()
        {
            p.plantilla = Some(r);
        }
        if ui.button("Usar plantilla…").clicked()
            && let Some(r) = rfd::FileDialog::new().add_filter("Typst", &["typ"]).pick_file()
        {
            p.plantilla = Some(r);
        }
        if p.plantilla.is_some() && ui.button("Usar la incluida").clicked() {
            p.plantilla = None;
        }
    });
}

fn fila(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String, pista: &str) {
    ui.label(etiqueta);
    ui.add(egui::TextEdit::singleline(valor).desired_width(400.0).hint_text(pista));
    ui.end_row();
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use eframe::egui::{Context, RawInput};

    #[test]
    fn ventana_con_perfiles() {
        let mut app = Aplicacion::sin_ventana();
        app.abrir_pdf();
        let ctx = Context::default();
        for _ in 0..2 {
            let mut s = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_pdf(&c);
            });
            s.textures_delta.clear();
        }
        let v = app.ventana_pdf.as_mut().expect("la ventana sigue abierta");
        assert!(!v.perfiles.lista.is_empty());
        // Cambiar de perfil conserva lo escrito en el anterior
        v.perfiles.lista = ppto_pdf::Perfiles::default().lista;
        v.perfiles.activo = 0;
        v.cargar_textos();
        v.observaciones = "Solo esta observación".into();
        v.volcar().unwrap();
        v.perfiles.activo = 1;
        v.cargar_textos();
        assert_eq!(v.gg, "13,00");
        assert_eq!(v.perfiles.lista[0].observaciones, ["Solo esta observación"]);
        v.gg = "abc".into();
        assert!(v.volcar().is_err());
    }
}
