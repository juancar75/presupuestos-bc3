// SPDX-License-Identifier: GPL-3.0-or-later
//! Legalización: abre el programa independiente `ppto-legalizacion` en una
//! ventana aparte con un encargo de la obra (capítulos, datos leídos de las
//! mediciones y márgenes) y convierte cada entrega en la partida
//! `LEG-RITE` o `LEG-PCI` del capítulo `LEG`, con todo el descompuesto.
//!
//! Si no encuentra el ejecutable (`ppto-legalizacion` junto a `ppto-gui`, o en
//! `PPTO_LEGALIZACION`), muestra la misma ventana dentro del programa.
//!
//! El baremo (honorarios y tablas por año) está en `legalizacion.json`, en la
//! carpeta de configuración, fuera del repositorio; los dos programas leen el
//! mismo fichero.

use super::Aplicacion;
use eframe::egui;
use ppto_core::Decimal;
use ppto_core::formato::eur;
use ppto_legalizacion::{Baremo, Baremos, Disciplina, aplicar, calcular, capitulos, codigo_partida, leer_presupuesto};
use ppto_legalizacion_app::{
    Capitulo, Encargo, Entrega, carpeta_intercambio, escribir_json, leer_entregas, ruta_ejecutable,
};
use std::path::PathBuf;
use std::time::Duration;

pub(super) use ppto_legalizacion_app::ventana::Legalizacion as VentanaLegalizacion;

/// Programa de legalización abierto aparte y las entregas ya aplicadas.
pub(super) struct Externa {
    hijo: std::process::Child,
    carpeta: PathBuf,
    entregas: PathBuf,
    aplicadas: usize,
}

/// Encargo de la obra para el programa de legalización.
pub(super) fn encargo(p: &ppto_core::Presupuesto, gg: Decimal, bi: Decimal) -> Encargo {
    let capitulos = capitulos(p)
        .unwrap_or_default()
        .into_iter()
        .map(|(codigo, resumen, importe)| Capitulo {
            codigo,
            resumen,
            importe,
        })
        .collect();
    let existentes = [Disciplina::Rite, Disciplina::Pci]
        .into_iter()
        .map(codigo_partida)
        .filter(|c| p.conceptos.contains_key(c))
        .collect();
    Encargo {
        obra: p.nombre.clone(),
        anio: 0,
        capitulos,
        lectura: Some(leer_presupuesto(p)),
        costes_indirectos: p.costes_indirectos,
        gg,
        bi,
        existentes,
    }
}

impl Aplicacion {
    /// Abre el programa de legalización aparte; si no está, la ventana interna.
    pub(super) fn abrir_legalizacion(&mut self) {
        if let Some(x) = &mut self.legalizacion_externa
            && matches!(x.hijo.try_wait(), Ok(None))
        {
            self.info("El programa de legalización ya está abierto en otra ventana.");
            return;
        }
        let enc = encargo(&self.p, self.gg, self.bi);
        match self.lanzar_legalizacion(&enc) {
            Ok(x) => {
                self.legalizacion_externa = Some(x);
                self.info(
                    "Legalización abierta en una ventana aparte: cada «Crear la partida» se añade a esta obra (se puede deshacer).",
                );
            }
            Err(e) => {
                self.ventana_legalizacion = Some(VentanaLegalizacion::nueva(enc));
                if let Some(e) = e {
                    self.error(format!(
                        "No se pudo abrir el programa de legalización ({e}); se abre aquí."
                    ));
                }
            }
        }
    }

    /// Lanza `ppto-legalizacion`. `Err(None)` si no está instalado.
    fn lanzar_legalizacion(&self, enc: &Encargo) -> Result<Externa, Option<String>> {
        let exe = ruta_ejecutable().ok_or(None)?;
        let carpeta = carpeta_intercambio().map_err(|e| Some(e.to_string()))?;
        let ruta_encargo = carpeta.join("encargo.json");
        let entregas = carpeta.join("entregas.json");
        escribir_json(&ruta_encargo, enc).map_err(|e| Some(e.to_string()))?;
        let hijo = std::process::Command::new(exe)
            .arg("--encargo")
            .arg(&ruta_encargo)
            .arg("--entregas")
            .arg(&entregas)
            .spawn()
            .map_err(|e| Some(e.to_string()))?;
        Ok(Externa {
            hijo,
            carpeta,
            entregas,
            aplicadas: 0,
        })
    }

    pub(super) fn ventana_legalizacion(&mut self, ctx: &egui::Context) {
        self.sondear_legalizacion(ctx);
        let Some(mut v) = self.ventana_legalizacion.take() else {
            return;
        };
        let mut abierta = true;
        let mut entrega = None;
        egui::Window::new("Legalización")
            .open(&mut abierta)
            .default_width(860.0)
            .default_height(640.0)
            .resizable(true)
            .show(ctx, |ui| entrega = v.mostrar(ui));
        if let Some(e) = entrega {
            let r = self.aplicar_entrega(&e);
            v.confirmar(e.expediente.disciplina, r);
        }
        if abierta {
            self.ventana_legalizacion = Some(v);
        }
    }

    /// Aplica las entregas nuevas del programa aparte y lo suelta al cerrarse.
    fn sondear_legalizacion(&mut self, ctx: &egui::Context) {
        let Some(mut x) = self.legalizacion_externa.take() else {
            return;
        };
        // Primero si sigue vivo y luego las entregas: lo que escribió antes de
        // cerrarse no se pierde.
        let vivo = matches!(x.hijo.try_wait(), Ok(None));
        match leer_entregas(&x.entregas) {
            Ok(v) => {
                for e in v.iter().skip(x.aplicadas) {
                    let d = e.expediente.disciplina;
                    match self.aplicar_entrega(e) {
                        Ok(total) => self.info(format!(
                            "Partida {} creada desde el programa de legalización: {} €.",
                            codigo_partida(d),
                            eur(total)
                        )),
                        Err(err) => self.error(format!("No se pudo crear {}: {err}", codigo_partida(d))),
                    }
                }
                x.aplicadas = x.aplicadas.max(v.len());
            }
            Err(e) => self.error(format!("Entregas de legalización ilegibles: {e}")),
        }
        if vivo {
            ctx.request_repaint_after(Duration::from_millis(700));
            self.legalizacion_externa = Some(x);
        } else {
            let _ = std::fs::remove_dir_all(&x.carpeta);
        }
    }

    /// Crea o sustituye la partida de la entrega con el baremo del año.
    pub(super) fn aplicar_entrega(&mut self, e: &Entrega) -> Result<Decimal, String> {
        let ruta = ppto_legalizacion::ruta().ok_or("no se encuentra la carpeta de configuración")?;
        let bs = Baremos::cargar(&ruta).map_err(|e| e.to_string())?;
        let b = bs.vigente(e.anio).ok_or("no hay baremo en legalizacion.json")?;
        self.aplicar_entrega_con(b, e)
    }

    /// Crea o sustituye la partida de la entrega con un baremo dado.
    pub(super) fn aplicar_entrega_con(&mut self, b: &Baremo, e: &Entrega) -> Result<Decimal, String> {
        let c = calcular(b, &e.expediente);
        let d = e.expediente.disciplina;
        self.historial
            .antes(&self.p, format!("legalización {}", d.sigla()), None);
        match aplicar(&mut self.p, d, &c) {
            Ok(()) => {
                self.marcar_cambio();
                Ok(c.total())
            }
            Err(err) => {
                self.historial.deshacer.pop();
                Err(err.to_string())
            }
        }
    }
}
