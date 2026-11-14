// SPDX-License-Identifier: GPL-3.0-or-later
//! `legalizacion` — costes de legalización en su propia ventana.
//!
//! `legalizacion` (sola), `legalizacion obra.bc3`, o desde el presupuestador
//! `legalizacion --solicitud <json> --respuesta <json>`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::float_arithmetic)]

use ppto_legalizacion_app::{Aplicacion, leer_argumentos};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (modo, solicitud) = match leer_argumentos(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("legalizacion: {e}");
            eprintln!("uso: legalizacion [obra.bc3] | legalizacion --solicitud <json> --respuesta <json>");
            std::process::exit(2);
        }
    };
    let opciones = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Legalización — presupuestos-bc3")
            .with_inner_size([960.0, 760.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "legalizacion",
        opciones,
        Box::new(move |_cc| Ok(Box::new(Aplicacion::new(modo, solicitud)))),
    )
}
