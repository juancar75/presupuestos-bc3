// SPDX-License-Identifier: GPL-3.0-or-later
//! Obra ↔ biblioteca de precios (compartida entre varias personas).
//!
//! - Al **guardar** la obra, si sus PVP o descuentos no coinciden con la
//!   biblioteca, pregunta si se pasan a la biblioteca (con fecha, hora y
//!   usuario).
//! - Al **abrir** una obra, si la biblioteca tiene PVP o descuentos
//!   distintos (p. ej. los que ha actualizado un compañero), lo avisa con
//!   la lista y quién y cuándo los cambió; la obra no cambia sola.

use super::*;
use ppto_biblioteca::{Biblioteca, Diferencia};

/// Lo que hay que preguntar.
pub(super) enum Sincro {
    /// Tras guardar: la obra tiene precios que la biblioteca no.
    Subir(Vec<Diferencia>),
    /// Tras abrir: la biblioteca tiene precios distintos de los de la obra.
    Bajar(Vec<Diferencia>),
}

impl Aplicacion {
    fn diferencias_biblioteca(&self) -> Option<(std::path::PathBuf, Vec<Diferencia>)> {
        let r = self.ruta_biblioteca.clone()?;
        let d = Biblioteca::cargar(&r).diferencias_con(&self.p);
        (!d.is_empty()).then_some((r, d))
    }

    /// Tras guardar la obra.
    pub(super) fn tras_guardar(&mut self) {
        if let Some((_, d)) = self.diferencias_biblioteca() {
            self.sincro = Some(Sincro::Subir(d));
        }
    }

    /// Tras abrir una obra.
    pub(super) fn tras_abrir(&mut self) {
        if let Some((_, d)) = self.diferencias_biblioteca() {
            // Solo lo que la biblioteca tiene distinto (no lo que le falta)
            let d: Vec<Diferencia> = d.into_iter().filter(|x| x.biblioteca.is_some()).collect();
            if !d.is_empty() {
                self.sincro = Some(Sincro::Bajar(d));
            }
        }
    }

    fn subir_a_biblioteca(&mut self) {
        let Some(r) = self.ruta_biblioteca.clone() else { return };
        let obra = ppto_propuestas::clave(&self.p);
        let anio = ppto_biblioteca::anio(&ppto_biblioteca::hoy());
        let p = &self.p;
        match Biblioteca::modificar(&r, |b| b.actualizar_con(p, &obra, anio)) {
            Ok((n_pvp, n_dto)) => self.info(format!(
                "Biblioteca actualizada: {n_pvp} PVP y {n_dto} descuentos ({}, {}).",
                ppto_biblioteca::usuario(),
                ppto_biblioteca::ahora()
            )),
            Err(e) => self.error(format!("No se pudo actualizar la biblioteca: {e}")),
        }
    }

    /// Acciones para poner en la obra los precios de la biblioteca.
    pub(super) fn bajar_de_biblioteca(d: &[Diferencia]) -> Vec<Accion> {
        let mut pvps: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
        let mut acciones = Vec::new();
        for x in d {
            let Some(b) = x.biblioteca else { continue };
            if x.es_descuento {
                acciones.push(Accion::Descuento(x.clave.clone(), x.cadena.clone()));
            } else {
                pvps.entry(x.fabricante.clone()).or_default().insert(x.clave.clone(), b);
            }
        }
        for (fab, p) in pvps {
            acciones.insert(0, Accion::TarifaNueva(fab, "biblioteca".into(), p));
        }
        acciones
    }

    pub(super) fn ventana_sincro(&mut self, ctx: &egui::Context) {
        let Some(s) = self.sincro.take() else { return };
        let (titulo, texto, d, subir) = match &s {
            Sincro::Subir(d) => (
                "Actualizar la biblioteca",
                format!(
                    "Esta obra tiene {} precios que la biblioteca de precios no tiene o tiene distintos. \
                     ¿Pasarlos a la biblioteca para que los use todo el equipo?",
                    d.len()
                ),
                d,
                true,
            ),
            Sincro::Bajar(d) => (
                "Precios más nuevos en la biblioteca",
                format!(
                    "La biblioteca tiene {} precios distintos de los de esta obra. \
                     La obra no cambia sola: actualícela si son los buenos.",
                    d.len()
                ),
                d,
                false,
            ),
        };
        let mut abierta = true;
        let mut hecho = None::<bool>;
        egui::Window::new(titulo)
            .open(&mut abierta)
            .collapsible(false)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.label(texto);
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("sincro").striped(true).num_columns(4).show(ui, |ui| {
                        for t in ["Precio", "Obra", "Biblioteca", "Cambiado en la biblioteca"] {
                            ui.label(RichText::new(t).strong());
                        }
                        ui.end_row();
                        for x in d {
                            let u = if x.es_descuento { " %" } else { " €" };
                            ui.label(if x.codigo.is_empty() {
                                x.que.clone()
                            } else {
                                format!("{} ({})", x.que, x.codigo)
                            });
                            ui.label(format!("{}{u}", num(x.obra, 2)));
                            ui.label(
                                x.biblioteca
                                    .map_or("— (no la tiene)".into(), |b| format!("{}{u}", num(b, 2))),
                            );
                            ui.label(
                                x.sello
                                    .as_ref()
                                    .map_or("—".into(), |s| format!("{} · {}", s.quien, s.cuando)),
                            );
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let si = if subir {
                        "Actualizar la biblioteca"
                    } else {
                        "Actualizar la obra"
                    };
                    if ui.button(si).clicked() {
                        hecho = Some(true);
                    }
                    if ui.button("Ahora no").clicked() {
                        hecho = Some(false);
                    }
                });
            });
        match (hecho, s) {
            (Some(true), Sincro::Subir(_)) => self.subir_a_biblioteca(),
            (Some(true), Sincro::Bajar(d)) => {
                let a = Self::bajar_de_biblioteca(&d);
                self.aplicar(a);
            }
            (Some(false), _) => {}
            (None, s) if abierta => self.sincro = Some(s),
            _ => {}
        }
    }
}
