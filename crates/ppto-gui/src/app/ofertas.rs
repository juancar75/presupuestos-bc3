// SPDX-License-Identifier: GPL-3.0-or-later
//! Origen de los precios: vincular el precio de un recurso a la **oferta**
//! que lo justifica (fabricante, número, fecha, validez, artículo y neto) y
//! a su **documento**.
//!
//! El documento se guarda con la ruta que se elija en cada caso (carpeta de
//! la obra, unidad de red, OneDrive…); no se copia. Si deja de encontrarse,
//! el 🖹 se pone en naranja y se puede volver a vincular.

use super::asistente::abrir_en_sistema;
use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_biblioteca::{Biblioteca, hoy};
use ppto_core::formato::{eur, leer};
use ppto_core::{Decimal, OrigenPrecio};
use std::collections::HashMap;
use std::path::Path;

pub(super) const NARANJA: Color32 = Color32::from_rgb(230, 150, 60);

/// Cada cuánto se vuelve a comprobar si un documento sigue en su sitio (s).
const REFRESCO_DOC: f64 = 20.0;

/// Elige el documento de una oferta, en cualquier carpeta.
pub(super) fn elegir_documento(actual: Option<&str>) -> Option<String> {
    let mut d = rfd::FileDialog::new()
        .set_title("Documento de la oferta (en la carpeta donde esté)")
        .add_filter("PDF", &["pdf", "PDF"])
        .add_filter("Correo", &["msg", "eml"])
        .add_filter("Todos los ficheros", &["*"]);
    if let Some(dir) = actual.map(Path::new).and_then(Path::parent).filter(|p| p.is_dir()) {
        d = d.set_directory(dir);
    }
    d.pick_file().map(|p| p.display().to_string())
}

/// Lo que se pide con el botón del documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpDoc {
    Abrir,
    Vincular,
    Quitar,
}

/// Botón 🖹 de un documento: abrir, vincular, o volver a vincular si no se
/// encuentra (en naranja). Clic derecho: cambiar o quitar.
pub(super) fn boton_documento(ui: &mut Ui, ruta: Option<&str>, existe: bool) -> Option<OpDoc> {
    let mut op = None;
    let r = match ruta {
        None => ui
            .small_button(RichText::new("🖹").weak())
            .on_hover_text("Vincular el documento de la oferta…"),
        Some(r) if existe => ui.small_button("🖹").on_hover_text(format!("Abrir {r}")),
        Some(r) => ui
            .small_button(RichText::new("🖹 ?").color(NARANJA))
            .on_hover_text(format!(
                "No se encuentra el documento:\n{r}\n\nPulse para volver a vincularlo (puede estar en otra carpeta)."
            )),
    };
    if r.clicked() {
        op = Some(if ruta.is_some() && existe {
            OpDoc::Abrir
        } else {
            OpDoc::Vincular
        });
    }
    if ruta.is_some() {
        r.context_menu(|ui| {
            if ui.button("Cambiar documento…").clicked() {
                op = Some(OpDoc::Vincular);
                ui.close();
            }
            if ui.button("Quitar documento").clicked() {
                op = Some(OpDoc::Quitar);
                ui.close();
            }
        });
    }
    op
}

/// Comprueba (con caché) si existen los documentos: una ruta de red caída no
/// debe frenar cada fotograma.
#[derive(Default)]
pub(super) struct CacheDocumentos(HashMap<String, (bool, f64)>);

impl CacheDocumentos {
    pub fn existe(&mut self, ruta: &str, ahora: f64) -> bool {
        if let Some((e, t)) = self.0.get(ruta)
            && ahora - t < REFRESCO_DOC
        {
            return *e;
        }
        let e = Path::new(ruta).is_file();
        self.0.insert(ruta.to_owned(), (e, ahora));
        e
    }

    /// Olvida lo comprobado (tras vincular un documento).
    pub fn olvidar(&mut self) {
        self.0.clear();
    }
}

/// Formulario «Origen del precio» de un recurso.
pub(super) struct FormOrigen {
    pub codigo: String,
    resumen: String,
    precio: Decimal,
    fabricante: String,
    oferta: String,
    fecha: String,
    validez: String,
    referencia: String,
    neto: String,
    documento: String,
    aplicar: bool,
    /// Ofertas de la biblioteca: (fabricante, número).
    bib: Biblioteca,
    elegida: Option<(String, String)>,
    error: Option<String>,
}

impl FormOrigen {
    pub fn nuevo(codigo: &str, resumen: &str, precio: Decimal, o: Option<&OrigenPrecio>) -> Self {
        let bib = ppto_biblioteca::ruta()
            .as_deref()
            .map(Biblioteca::cargar)
            .unwrap_or_default();
        Self::con_biblioteca(codigo, resumen, precio, o, bib)
    }

    pub fn con_biblioteca(
        codigo: &str,
        resumen: &str,
        precio: Decimal,
        o: Option<&OrigenPrecio>,
        bib: Biblioteca,
    ) -> Self {
        let o = o.cloned().unwrap_or_default();
        Self {
            codigo: codigo.to_owned(),
            resumen: resumen.to_owned(),
            precio,
            fabricante: o.fabricante,
            oferta: o.oferta,
            fecha: if o.fecha.is_empty() { hoy() } else { o.fecha },
            validez: o.validez.unwrap_or_default(),
            referencia: o.referencia.unwrap_or_default(),
            neto: o.neto.map(|n| n.to_string()).unwrap_or_default(),
            documento: o.documento.unwrap_or_default(),
            aplicar: false,
            bib,
            elegida: None,
            error: None,
        }
    }

    /// Rellena con una oferta (y, si se indica, una línea) de la biblioteca.
    pub fn desde_biblioteca(&mut self, fabricante: &str, numero: &str, referencia: Option<&str>) {
        let Some(of) = self
            .bib
            .fabricante(fabricante)
            .and_then(|f| f.ofertas.iter().find(|o| o.numero == numero))
        else {
            return;
        };
        let nombre = self
            .bib
            .fabricante(fabricante)
            .map_or(fabricante, |f| f.nombre.as_str());
        let o = of.origen(nombre, referencia);
        self.fabricante = o.fabricante;
        self.oferta = o.oferta;
        self.fecha = o.fecha;
        self.validez = o.validez.unwrap_or_default();
        if let Some(r) = o.referencia {
            self.referencia = r;
        }
        if let Some(n) = o.neto {
            self.neto = n.to_string();
            self.aplicar = n != self.precio;
        }
        if let Some(d) = o.documento {
            self.documento = d;
        }
        self.elegida = Some((fabricante.to_owned(), numero.to_owned()));
    }

    /// El origen tal como está en el formulario.
    pub fn origen(&self) -> Result<OrigenPrecio, String> {
        let fabricante = self.fabricante.trim().to_owned();
        if fabricante.is_empty() {
            return Err("Indique el fabricante o proveedor.".into());
        }
        let neto = match self.neto.trim() {
            "" => None,
            t => Some(leer(t).ok_or("El neto no es un número.")?),
        };
        let o = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        Ok(OrigenPrecio {
            fabricante,
            oferta: self.oferta.trim().to_owned(),
            fecha: self.fecha.trim().to_owned(),
            validez: o(&self.validez),
            referencia: o(&self.referencia),
            neto,
            documento: o(self.documento.trim().trim_matches('"')),
        })
    }
}

impl Aplicacion {
    pub(super) fn abrir_origen(&mut self, codigo: &str) {
        if let Some(c) = self.p.conceptos.get(codigo) {
            self.form_origen = Some(FormOrigen::nuevo(codigo, &c.resumen, c.precio, c.origen.as_ref()));
        }
    }

    /// Celda «Origen» de un recurso (tabla de Recursos).
    pub(super) fn celda_origen(&mut self, ui: &mut Ui, codigo: &str, acciones: &mut Vec<Accion>) {
        let Some(c) = self.p.conceptos.get(codigo) else { return };
        let precio = c.precio;
        let Some(o) = c.origen.clone() else {
            if ui
                .small_button(RichText::new("+ oferta").weak())
                .on_hover_text("Vincular el precio a la oferta que lo justifica")
                .clicked()
            {
                self.abrir_origen(codigo);
            }
            return;
        };
        let ahora = ui.input(|i| i.time);
        let existe = o.documento.as_deref().is_some_and(|d| self.docs.existe(d, ahora));
        ui.horizontal(|ui| {
            match boton_documento(ui, o.documento.as_deref(), existe) {
                Some(OpDoc::Abrir) => {
                    if let Some(d) = &o.documento {
                        abrir_en_sistema(Path::new(d));
                    }
                }
                Some(OpDoc::Vincular) => {
                    if let Some(d) = elegir_documento(o.documento.as_deref()) {
                        let mut n = o.clone();
                        n.documento = Some(d);
                        self.docs.olvidar();
                        acciones.push(Accion::Origen(codigo.to_owned(), Some(Box::new(n)), false));
                    }
                }
                Some(OpDoc::Quitar) => {
                    let mut n = o.clone();
                    n.documento = None;
                    acciones.push(Accion::Origen(codigo.to_owned(), Some(Box::new(n)), false));
                }
                None => {}
            }
            let vencida = o.vencida(&hoy());
            let mut t = RichText::new(o.etiqueta());
            if vencida {
                t = t.color(NARANJA);
            }
            let mut ayuda = detalle(&o);
            if vencida {
                ayuda.push_str("\n\nOFERTA VENCIDA: pida una actualización.");
            }
            ayuda.push_str("\n\nClic: editar el origen del precio.");
            if ui.link(t).on_hover_text(ayuda).clicked() {
                self.abrir_origen(codigo);
            }
            if o.precio_distinto(precio) {
                ui.label(RichText::new("≠").color(NARANJA).strong())
                    .on_hover_text(format!(
                        "El precio ({} €) no es el ofertado ({} €).",
                        eur(precio),
                        eur(o.neto.unwrap_or_default())
                    ));
            }
        });
    }

    /// Resumen para la cabecera de Recursos: precios con oferta y vencidas.
    pub(super) fn aviso_ofertas(&self, ui: &mut Ui) {
        let hoy = hoy();
        let con: Vec<&OrigenPrecio> = self.p.conceptos.values().filter_map(|c| c.origen.as_ref()).collect();
        if con.is_empty() {
            return;
        }
        let vencidas = con.iter().filter(|o| o.vencida(&hoy)).count();
        let mut t = format!("{} precios justificados con oferta", con.len());
        if vencidas > 0 {
            t.push_str(&format!(" · {vencidas} con la oferta vencida"));
            ui.label(RichText::new(t).color(NARANJA));
        } else {
            ui.label(RichText::new(t).weak());
        }
    }

    pub(super) fn ventana_origen(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut f) = self.form_origen.take() else { return };
        let mut abierta = true;
        let mut cerrar = false;
        egui::Window::new(format!("Origen del precio · {}", f.codigo))
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&f.resumen).weak());
                ui.label(format!("Precio actual: {} €", eur(f.precio)));
                ui.separator();
                // Desde la biblioteca
                let ofertas: Vec<(String, String, String)> = f
                    .bib
                    .fabricantes
                    .iter()
                    .flat_map(|(k, fab)| {
                        fab.ofertas.iter().map(move |o| {
                            (
                                k.clone(),
                                o.numero.clone(),
                                format!("{} · {} ({})", fab.nombre, o.numero, o.fecha),
                            )
                        })
                    })
                    .collect();
                if !ofertas.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Oferta de la biblioteca:");
                        let sel = f
                            .elegida
                            .as_ref()
                            .and_then(|(k, n)| ofertas.iter().find(|o| &o.0 == k && &o.1 == n))
                            .map_or("elegir…".to_owned(), |o| o.2.clone());
                        let mut elegir = None;
                        egui::ComboBox::from_id_salt("origen_oferta")
                            .selected_text(sel)
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for (k, n, t) in &ofertas {
                                    if ui.selectable_label(false, t).clicked() {
                                        elegir = Some((k.clone(), n.clone()));
                                    }
                                }
                            });
                        if let Some((k, n)) = elegir {
                            f.desde_biblioteca(&k, &n, None);
                        }
                    });
                    if let Some((k, n)) = f.elegida.clone() {
                        let lineas: Vec<(String, String)> = f
                            .bib
                            .fabricante(&k)
                            .and_then(|x| x.ofertas.iter().find(|o| o.numero == n))
                            .map(|o| {
                                o.lineas
                                    .iter()
                                    .map(|l| {
                                        (
                                            l.referencia.clone(),
                                            format!("{} — {} — {} €", l.referencia, l.descripcion, eur(l.neto)),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !lineas.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label("Artículo:");
                                let sel = lineas
                                    .iter()
                                    .find(|l| l.0 == f.referencia)
                                    .map_or("elegir…".to_owned(), |l| l.1.clone());
                                let mut elegir = None;
                                egui::ComboBox::from_id_salt("origen_linea")
                                    .selected_text(sel)
                                    .width(380.0)
                                    .show_ui(ui, |ui| {
                                        for (r, t) in &lineas {
                                            if ui.selectable_label(*r == f.referencia, t).clicked() {
                                                elegir = Some(r.clone());
                                            }
                                        }
                                    });
                                if let Some(r) = elegir {
                                    f.desde_biblioteca(&k, &n, Some(&r));
                                }
                            });
                        }
                    }
                    ui.separator();
                }
                egui::Grid::new("form_origen").num_columns(2).show(ui, |ui| {
                    ui.label("Fabricante / proveedor");
                    ui.text_edit_singleline(&mut f.fabricante);
                    ui.end_row();
                    ui.label("Nº de oferta");
                    ui.text_edit_singleline(&mut f.oferta);
                    ui.end_row();
                    ui.label("Fecha");
                    ui.add(egui::TextEdit::singleline(&mut f.fecha).hint_text("AAAA-MM-DD"));
                    ui.end_row();
                    ui.label("Válida hasta");
                    ui.add(egui::TextEdit::singleline(&mut f.validez).hint_text("AAAA-MM-DD (opcional)"));
                    ui.end_row();
                    ui.label("Referencia del artículo");
                    ui.text_edit_singleline(&mut f.referencia);
                    ui.end_row();
                    ui.label("Neto ofertado €");
                    ui.add(egui::TextEdit::singleline(&mut f.neto).desired_width(100.0));
                    ui.end_row();
                    ui.label("Documento");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut f.documento)
                                .hint_text("ruta del PDF, donde esté")
                                .desired_width(330.0),
                        );
                        if ui.button("Elegir…").clicked()
                            && let Some(d) = elegir_documento(Some(f.documento.as_str()).filter(|d| !d.is_empty()))
                        {
                            f.documento = d;
                        }
                        let d = f.documento.trim().trim_matches('"');
                        if !d.is_empty() {
                            if Path::new(d).is_file() {
                                if ui.small_button("Abrir").clicked() {
                                    abrir_en_sistema(Path::new(d));
                                }
                            } else {
                                ui.label(RichText::new("no se encuentra").color(NARANJA));
                            }
                        }
                    });
                    ui.end_row();
                });
                let neto: Option<Decimal> = leer(&f.neto);
                if let Some(n) = neto.filter(|n| *n != f.precio) {
                    ui.checkbox(
                        &mut f.aplicar,
                        format!(
                            "Poner como precio el neto ofertado ({} € en vez de {} €)",
                            eur(n),
                            eur(f.precio)
                        ),
                    );
                }
                if let Some(e) = &f.error {
                    ui.label(RichText::new(e).color(Color32::from_rgb(220, 80, 60)));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Guardar").clicked() {
                        match f.origen() {
                            Ok(o) => {
                                // La oferta de la biblioteca aprende el documento si no lo tenía
                                if let (Some((k, n)), Some(d)) = (&f.elegida, &o.documento) {
                                    let falta = f
                                        .bib
                                        .fabricante(k)
                                        .and_then(|x| x.ofertas.iter().find(|of| &of.numero == n))
                                        .is_some_and(|of| of.documento.is_none());
                                    if falta && let Some(r) = ppto_biblioteca::ruta() {
                                        let _ = Biblioteca::modificar(&r, |b| {
                                            b.fijar_documento_oferta(k, n, Some(d.clone()))
                                        });
                                    }
                                }
                                let aplicar = f.aplicar && o.neto.is_some_and(|n| n != f.precio);
                                acciones.push(Accion::Origen(f.codigo.clone(), Some(Box::new(o)), aplicar));
                                self.docs.olvidar();
                                cerrar = true;
                            }
                            Err(e) => f.error = Some(e),
                        }
                    }
                    if self.p.conceptos.get(&f.codigo).is_some_and(|c| c.origen.is_some())
                        && ui.button("Quitar el vínculo").clicked()
                    {
                        acciones.push(Accion::Origen(f.codigo.clone(), None, false));
                        cerrar = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
            });
        if abierta && !cerrar {
            self.form_origen = Some(f);
        }
    }
}

/// Detalle del origen para la ayuda emergente.
pub(super) fn detalle(o: &OrigenPrecio) -> String {
    let mut t = format!("Fabricante: {}", o.fabricante);
    if !o.oferta.is_empty() {
        t.push_str(&format!("\nOferta: {}", o.oferta));
    }
    if !o.fecha.is_empty() {
        t.push_str(&format!("\nFecha: {}", o.fecha));
    }
    if let Some(v) = &o.validez {
        t.push_str(&format!("\nVálida hasta: {v}"));
    }
    if let Some(r) = &o.referencia {
        t.push_str(&format!("\nArtículo: {r}"));
    }
    if let Some(n) = o.neto {
        t.push_str(&format!("\nNeto ofertado: {} €", eur(n)));
    }
    t.push_str(&format!(
        "\nDocumento: {}",
        o.documento.as_deref().unwrap_or("(sin vincular)")
    ));
    t
}
