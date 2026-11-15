// SPDX-License-Identifier: GPL-3.0-or-later
//! Aplicación independiente de costes de legalización.
//!
//! Dos modos:
//!
//! - **Lanzada por el presupuestador**: `legalizacion --solicitud <json>
//!   --respuesta <json>`. Lee la obra de la solicitud y, al pulsar «Crear la
//!   partida», escribe la respuesta y se cierra; el presupuestador la aplica.
//! - **Sola**: sin argumentos (o con un `.bc3`). Se abre un BC3, se calcula y
//!   se guarda otro BC3 con el capítulo `LEG`.
//!
//! El cálculo es de `ppto-legalizacion`; esta capa solo muestra y edita.
#![allow(clippy::float_arithmetic)]

mod ventana;

pub use ventana::Ventana;

use eframe::egui;
use ppto_legalizacion::{Respuesta, Solicitud, aplicar, codigo_partida};
use rust_decimal::Decimal;
use std::path::PathBuf;

/// Cómo se ha abierto la aplicación.
#[derive(Debug, Clone, PartialEq)]
pub enum Modo {
    /// Desde el presupuestador: la respuesta va a este fichero.
    Lanzada { respuesta: PathBuf },
    /// Sola, con la obra (opcional) de un BC3.
    Sola { bc3: Option<PathBuf> },
}

/// Argumentos: `--solicitud S --respuesta R`, o un `.bc3` suelto, o nada.
pub fn leer_argumentos(args: &[String]) -> Result<(Modo, Option<PathBuf>), String> {
    let mut solicitud = None;
    let mut respuesta = None;
    let mut bc3 = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--solicitud" => solicitud = it.next().map(PathBuf::from),
            "--respuesta" => respuesta = it.next().map(PathBuf::from),
            otro if otro.starts_with("--") => return Err(format!("argumento desconocido: {otro}")),
            fichero => bc3 = Some(PathBuf::from(fichero)),
        }
    }
    match (solicitud, respuesta) {
        (Some(s), Some(r)) => Ok((Modo::Lanzada { respuesta: r }, Some(s))),
        (None, None) => Ok((Modo::Sola { bc3 }, None)),
        _ => Err("--solicitud y --respuesta van juntos".into()),
    }
}

pub struct Aplicacion {
    modo: Modo,
    ventana: Ventana,
    /// Porcentajes del BC3 abierto (modo solo), para guardarlo igual.
    porcentajes: ppto_bc3::OpcionesExportacion,
    cerrar: bool,
}

fn obra_vacia() -> ppto_core::Presupuesto {
    ppto_core::Presupuesto::new("Legalización", "OBRA", "Legalización")
}

fn opciones(gg: Option<Decimal>, bi: Option<Decimal>, iva: Option<Decimal>) -> ppto_bc3::OpcionesExportacion {
    ppto_bc3::OpcionesExportacion {
        gastos_generales: gg,
        beneficio_industrial: bi,
        iva,
        fecha: None,
        descuentos_visibles: true,
        longitud_codigo: None,
    }
}

impl Aplicacion {
    /// Crea la aplicación; si la solicitud no se puede leer, abre vacía y lo dice.
    pub fn new(modo: Modo, solicitud: Option<PathBuf>) -> Self {
        let mut aviso = None;
        let (p, sobrecoste) = match &solicitud {
            Some(r) => match Solicitud::cargar(r) {
                Ok(s) => (s.presupuesto, s.sobrecoste),
                Err(e) => {
                    aviso = Some(format!("No se pudo leer la obra del presupuestador: {e}"));
                    (obra_vacia(), Decimal::ZERO)
                }
            },
            None => (obra_vacia(), Decimal::ZERO),
        };
        let mut app = Self {
            modo: modo.clone(),
            ventana: Ventana::abrir(p, sobrecoste),
            porcentajes: opciones(None, None, None),
            cerrar: false,
        };
        if let Modo::Sola { bc3: Some(r) } = modo {
            app.abrir_bc3(r);
        }
        if let Some(a) = aviso {
            app.ventana.avisar(a, true);
        }
        app
    }

    fn abrir_bc3(&mut self, ruta: PathBuf) {
        match ppto_bc3::importar_fichero(&ruta) {
            Ok(imp) => {
                let k = &imp.porcentajes;
                self.porcentajes = opciones(k.gastos_generales, k.beneficio_industrial, k.iva);
                let sobrecoste = k.gastos_generales.unwrap_or_default() + k.beneficio_industrial.unwrap_or_default();
                self.ventana = Ventana::abrir(imp.presupuesto, sobrecoste);
                self.ventana.avisar(format!("Obra abierta: {}", ruta.display()), false);
                self.modo = Modo::Sola { bc3: Some(ruta) };
            }
            Err(e) => self
                .ventana
                .avisar(format!("No se pudo abrir {}: {e}", ruta.display()), true),
        }
    }

    fn guardar_bc3(&mut self) {
        let nombre = match &self.modo {
            Modo::Sola { bc3: Some(r) } => r
                .file_stem()
                .map(|s| format!("{}-legalizacion.bc3", s.to_string_lossy()))
                .unwrap_or_else(|| "legalizacion.bc3".into()),
            _ => "legalizacion.bc3".into(),
        };
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        match ppto_bc3::exportar_fichero(self.ventana.presupuesto(), &self.porcentajes, &ruta) {
            Ok(avisos) if avisos.is_empty() => self
                .ventana
                .avisar(format!("BC3 guardado en {}", ruta.display()), false),
            Ok(avisos) => self.ventana.avisar(
                format!("BC3 guardado en {} — {}", ruta.display(), avisos.join("; ")),
                false,
            ),
            Err(e) => self.ventana.avisar(format!("No se pudo guardar: {e}"), true),
        }
    }

    /// Qué hacer con la partida calculada según el modo.
    fn entregar(&mut self, disciplina: ppto_legalizacion::Disciplina, c: ppto_legalizacion::Calculo) {
        match &self.modo {
            Modo::Lanzada { respuesta } => match Respuesta::nueva(disciplina, c).guardar(respuesta) {
                Ok(()) => self.cerrar = true,
                Err(e) => self
                    .ventana
                    .avisar(format!("No se pudo devolver la partida: {e}"), true),
            },
            Modo::Sola { .. } => {
                let mut p = self.ventana.presupuesto().clone();
                match aplicar(&mut p, disciplina, &c) {
                    Ok(()) => {
                        self.ventana.cambiar_presupuesto(p);
                        self.ventana.avisar(
                            format!(
                                "Partida {} creada. Use «Guardar BC3…» para llevarla a Presto o al presupuestador.",
                                codigo_partida(disciplina)
                            ),
                            false,
                        );
                    }
                    Err(e) => self.ventana.avisar(format!("No se pudo crear: {e}"), true),
                }
            }
        }
    }

    /// Pinta la aplicación (separado de eframe para poder probarlo).
    pub fn pintar(&mut self, ui: &mut egui::Ui) {
        if matches!(self.modo, Modo::Sola { .. }) {
            ui.horizontal(|ui| {
                if ui.button("Abrir BC3…").clicked()
                    && let Some(r) = rfd::FileDialog::new()
                        .add_filter("FIEBDC-3 (BC3)", &["bc3", "BC3"])
                        .pick_file()
                {
                    self.abrir_bc3(r);
                }
                if ui.button("Guardar BC3…").clicked() {
                    self.guardar_bc3();
                }
                ui.label(egui::RichText::new(&self.ventana.presupuesto().nombre).weak());
            });
            ui.separator();
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "Obra: {} — al crear la partida se devuelve al presupuestador y esta ventana se cierra.",
                    self.ventana.presupuesto().nombre
                ))
                .weak(),
            );
            ui.separator();
        }
        if let Some((d, c)) = self.ventana.ui(ui) {
            self.entregar(d, c);
        }
    }

    /// `true` cuando la partida ya se ha devuelto y hay que cerrar.
    pub fn cerrar(&self) -> bool {
        self.cerrar
    }
}

impl eframe::App for Aplicacion {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default_margins().show(ui, |ui| self.pintar(ui));
        if self.cerrar {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argumentos() {
        assert_eq!(leer_argumentos(&[]).unwrap(), (Modo::Sola { bc3: None }, None));
        assert_eq!(
            leer_argumentos(&args(&["obra.bc3"])).unwrap(),
            (
                Modo::Sola {
                    bc3: Some("obra.bc3".into())
                },
                None
            )
        );
        assert_eq!(
            leer_argumentos(&args(&["--solicitud", "s.json", "--respuesta", "r.json"])).unwrap(),
            (
                Modo::Lanzada {
                    respuesta: "r.json".into()
                },
                Some("s.json".into())
            )
        );
        assert!(leer_argumentos(&args(&["--solicitud", "s.json"])).is_err());
        assert!(leer_argumentos(&args(&["--otro"])).is_err());
    }

    #[test]
    fn se_pinta_sola_y_lanzada() {
        for modo in [
            Modo::Sola { bc3: None },
            Modo::Lanzada {
                respuesta: "r.json".into(),
            },
        ] {
            let mut app = Aplicacion::new(modo, None);
            let ctx = egui::Context::default();
            for _ in 0..3 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.pintar(ui));
                out.textures_delta.clear();
            }
            assert!(!app.cerrar());
        }
    }
}
