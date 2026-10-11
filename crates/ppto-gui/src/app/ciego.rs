// SPDX-License-Identifier: GPL-3.0-or-later
//! Presupuestos ciegos en Excel: importar, traer precios de un BC3 y
//! devolver el mismo Excel del cliente con los precios.

use super::Aplicacion;
use ppto_core::formato::eur;
use ppto_core::{Naturaleza, Presupuesto};
use std::path::PathBuf;

/// Ciego abierto: fichero original y dónde va el precio de cada partida.
pub(super) struct CiegoAbierto {
    pub ruta: PathBuf,
    pub ciego: ppto_ciego::Ciego,
}

impl Aplicacion {
    pub(super) fn dialogo_ciego(&mut self) {
        if let Some(r) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx", "xls"])
            .set_title("Presupuesto ciego (listado de Presto en Excel)")
            .pick_file()
        {
            self.importar_ciego(r);
        }
    }

    pub(super) fn importar_ciego(&mut self, ruta: PathBuf) {
        match ppto_ciego::leer(&ruta) {
            Ok(c) => {
                self.cargar_presupuesto(c.presupuesto.clone());
                let p = &self.p;
                let partidas = p
                    .conceptos
                    .values()
                    .filter(|x| x.naturaleza == Naturaleza::Partida)
                    .count();
                let avisos = if c.incidencias.is_empty() {
                    String::new()
                } else {
                    format!(" {} avisos: {}", c.incidencias.len(), c.incidencias.join(" · "))
                };
                self.info(format!(
                    "Ciego importado: {partidas} partidas. Ponga precio a cada partida (precio directo o «+ Añadir línea…»), \
                     o «Traer precios…» de un BC3, y luego «Devolver valorado…».{avisos}"
                ));
                self.ciego = Some(CiegoAbierto { ruta, ciego: c });
            }
            Err(e) => self.error(format!("No se pudo importar el ciego: {e}")),
        }
    }

    /// Pone en pantalla un presupuesto nuevo (sin fichero ni revisiones).
    pub(super) fn cargar_presupuesto(&mut self, p: Presupuesto) {
        self.p = p;
        self.fichero = None;
        self.presupuesto_id = None;
        self.revisiones.clear();
        self.revision_actual = None;
        self.cambios = true;
        self.ofertas.clear();
        self.avisos.clear();
        self.informe = None;
        self.recalcular();
        self.sel = self.primera_partida(&self.p.raiz.clone());
    }

    /// Da precio a las partidas sin descomponer con las de un BC3 (base de
    /// precios, obra anterior o el mismo ciego valorado en Presto).
    pub(super) fn traer_precios(&mut self) {
        let Some(r) = rfd::FileDialog::new()
            .add_filter("BC3", &["bc3", "BC3"])
            .set_title("BC3 con los precios (base de precios, obra anterior o Presto)")
            .pick_file()
        else {
            return;
        };
        match ppto_bc3::importar_fichero(&r) {
            Ok(imp) => {
                let t = ppto_ciego::traer_precios(&mut self.p, &imp.presupuesto);
                self.marcar_cambio();
                let faltan = if t.no_encontradas.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Sin encontrar ({}): {}",
                        t.no_encontradas.len(),
                        t.no_encontradas.iter().take(15).cloned().collect::<Vec<_>>().join(", ")
                    )
                };
                self.info(format!("{} partidas con precio.{faltan}", t.con_precio.len()));
            }
            Err(e) => self.error(format!("No se pudo leer el BC3: {e}")),
        }
    }

    /// Guarda el Excel del cliente con los precios. Si el ciego no se abrió
    /// en esta sesión (p. ej. se guardó y se volvió a abrir), se pide el
    /// fichero original.
    pub(super) fn devolver_valorado(&mut self) {
        if self.ciego.is_none() {
            let Some(r) = rfd::FileDialog::new()
                .add_filter("Excel", &["xlsx"])
                .set_title("Excel ciego original (el que envió el cliente)")
                .pick_file()
            else {
                return;
            };
            match ppto_ciego::leer(&r) {
                Ok(c) => self.ciego = Some(CiegoAbierto { ruta: r, ciego: c }),
                Err(e) => {
                    self.error(format!("No se pudo leer el ciego: {e}"));
                    return;
                }
            }
        }
        let Some(c) = &self.ciego else {
            return;
        };
        let original = match std::fs::read(&c.ruta) {
            Ok(b) => b,
            Err(e) => {
                self.error(format!("No se pudo leer {}: {e}", c.ruta.display()));
                return;
            }
        };
        let nombre = format!(
            "{} valorado.xlsx",
            c.ruta
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        let valorado = match ppto_ciego::valorar(&original, &c.ciego, &self.p) {
            Ok(v) => v,
            Err(e) => {
                self.error(format!("No se pudo valorar: {e}"));
                return;
            }
        };
        let Some(destino) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        if destino == c.ruta {
            self.error("No se sobrescribe el ciego original: elija otro nombre.");
            return;
        }
        match std::fs::write(&destino, &valorado.bytes) {
            Ok(()) => {
                let faltan = if valorado.sin_precio.is_empty() {
                    String::new()
                } else {
                    format!(
                        " ATENCIÓN: {} partidas sin precio: {}",
                        valorado.sin_precio.len(),
                        valorado
                            .sin_precio
                            .iter()
                            .take(15)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                self.info(format!(
                    "Excel valorado guardado en {}: {} precios; total que mostrará Excel {} €.{faltan}",
                    destino.display(),
                    valorado.celdas_escritas,
                    eur(valorado.total_excel)
                ));
            }
            Err(e) => self.error(format!("No se pudo guardar: {e}")),
        }
    }
}
