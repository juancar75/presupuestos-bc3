// SPDX-License-Identifier: GPL-3.0-or-later
//! `ppto-legalizacion` — programa independiente de costes de legalización.
//!
//! Con `--encargo` y `--entregas` lo abre presupuestos-bc3 para una obra:
//! cada «Crear la partida» añade una entrega que presupuestos-bc3 convierte
//! en la partida `LEG-RITE` o `LEG-PCI`. Sin argumentos calcula fuera de una
//! obra (PEM a mano) y copia el descompuesto.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::float_arithmetic)]

use eframe::egui;
use ppto_legalizacion_app::ventana::Legalizacion;
use ppto_legalizacion_app::{Argumentos, Encargo, anadir_entrega, leer_encargo};
use std::path::PathBuf;

struct Programa {
    ventana: Legalizacion,
    entregas: Option<PathBuf>,
    error: Option<String>,
}

impl eframe::App for Programa {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default_margins().show(ui, |ui| {
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 60), e);
                ui.separator();
            }
            if let Some(entrega) = self.ventana.mostrar(ui) {
                let disciplina = entrega.expediente.disciplina;
                let total = {
                    let b = self.ventana.baremo();
                    ppto_legalizacion::calcular(&b, &entrega.expediente).total()
                };
                let r = match &self.entregas {
                    Some(ruta) => anadir_entrega(ruta, entrega).map(|()| total).map_err(|e| e.to_string()),
                    None => Err("no hay presupuesto al que enviarla".into()),
                };
                self.ventana.confirmar(disciplina, r);
            }
        });
    }
}

fn main() -> eframe::Result {
    let (encargo, entregas, error) = match Argumentos::leer(std::env::args().skip(1)) {
        Ok(a) => {
            let (encargo, error) = match a.encargo.as_deref().map(leer_encargo) {
                Some(Ok(e)) => (e, None),
                Some(Err(e)) => (Encargo::default(), Some(format!("No se pudo leer el encargo: {e}"))),
                None => (Encargo::default(), None),
            };
            (encargo, a.entregas, error)
        }
        Err(e) => (Encargo::default(), None, Some(e)),
    };
    let titulo = if encargo.obra.is_empty() {
        "Legalización — presupuestos-bc3".to_owned()
    } else {
        format!("Legalización — {}", encargo.obra)
    };
    let mut ventana = Legalizacion::nueva(encargo);
    ventana.enviar = entregas.is_some();
    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&titulo)
            .with_inner_size([940.0, 760.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ppto-legalizacion",
        opciones,
        Box::new(move |_cc| {
            Ok(Box::new(Programa {
                ventana,
                entregas,
                error,
            }))
        }),
    )
}
