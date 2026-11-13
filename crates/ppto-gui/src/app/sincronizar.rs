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

    /// Recursos que acaban de entrar en la obra (de una base, un kit, otra
    /// obra…): sus PVP con los de la biblioteca y, si la obra no tiene
    /// descuento para su fabricante, el de la biblioteca. Los que ya
    /// estaban en la obra no se tocan. Devuelve el aviso, si cambia algo.
    pub(super) fn al_dia_nuevos(&mut self, antes: &std::collections::BTreeSet<String>) -> Option<String> {
        use ppto_core::descuentos::{clave, normalizar};
        let b = Biblioteca::cargar(self.ruta_biblioteca.as_ref()?);
        let nuevos: Vec<(String, ppto_core::PrecioTarifa, Option<String>)> = self
            .p
            .conceptos
            .values()
            .filter(|c| !antes.contains(&c.codigo))
            .filter_map(|c| Some((c.codigo.clone(), c.tarifa.clone()?, Presupuesto::referencia_tarifa(c))))
            .collect();
        if nuevos.is_empty() {
            return None;
        }
        // Descuentos que faltan en la obra
        let mut dtos = Vec::new();
        for (_, t, _) in &nuevos {
            let fab = normalizar(&t.fabricante);
            let Some(f) = b.fabricante(&fab) else { continue };
            let tiene = |p: &Presupuesto, k: &str| p.descuentos.contains_key(k);
            let g = t
                .grupo
                .as_deref()
                .map(normalizar)
                .filter(|g| f.descuentos.contains_key(g));
            let (k, gb) = match &g {
                Some(g) => (clave(&fab, Some(g)), g.clone()),
                None => (fab.clone(), String::new()),
            };
            if tiene(&self.p, &k) || (g.is_some() && tiene(&self.p, &fab) && !f.descuentos.contains_key(&gb)) {
                continue;
            }
            let Some(d) = f.descuentos.get(&gb) else { continue };
            let cadena = f.cadenas.get(&gb).cloned().unwrap_or_else(|| vec![*d]);
            if self.p.fijar_descuento_encadenado(&k, &cadena).is_ok() {
                dtos.push(format!(
                    "{k} {} %",
                    ppto_core::descuentos::texto_cadena(&cadena).replace('.', ",")
                ));
            }
        }
        // PVP de la biblioteca
        let mut n = 0;
        for (codigo, t, r) in &nuevos {
            let Some(r) = r else { continue };
            let pvp = b
                .fabricante(&t.fabricante)
                .and_then(|f| f.tarifas.iter().rev().find(|x| x.lineas.contains_key(r)))
                .map(|x| x.lineas[r].pvp);
            if let Some(pvp) = pvp.filter(|v| *v != t.pvp)
                && self
                    .p
                    .fijar_tarifa_grupo(codigo, &t.fabricante, t.grupo.as_deref(), pvp)
                    .is_ok()
            {
                n += 1;
            }
        }
        (n > 0 || !dtos.is_empty()).then(|| {
            let mut m = String::from("Al día con la biblioteca:");
            if n > 0 {
                m += &format!(" {n} PVP");
            }
            if !dtos.is_empty() {
                m += &format!("{} descuento {}", if n > 0 { ";" } else { "" }, dtos.join(", "));
            }
            m + "."
        })
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
