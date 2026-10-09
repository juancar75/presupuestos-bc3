// SPDX-License-Identifier: GPL-3.0-or-later
//! `ppto-gui` — interfaz de escritorio básica (prototipo de P-013).
//!
//! Todo cálculo económico lo hace `ppto-core`; esta capa solo muestra y edita.
//! La aritmética en coma flotante de este crate es exclusivamente de maquetación.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::float_arithmetic)]

mod app;
mod celdas;

fn main() -> eframe::Result {
    let opciones = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("presupuestos-bc3")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    let inicial = std::env::args().nth(1).map(std::path::PathBuf::from);
    eframe::run_native(
        "presupuestos-bc3",
        opciones,
        Box::new(move |cc| Ok(Box::new(app::Aplicacion::new(cc, inicial)))),
    )
}
