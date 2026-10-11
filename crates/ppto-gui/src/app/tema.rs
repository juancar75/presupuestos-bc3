// SPDX-License-Identifier: GPL-3.0-or-later
//! Modo diurno (claro) / nocturno (oscuro). La elección se guarda en
//! `tema.txt` de la carpeta de configuración; sin elección, sigue al sistema.

use super::Aplicacion;
use eframe::egui::{Context, ThemePreference};
use std::path::PathBuf;

fn ruta() -> Option<PathBuf> {
    super::pdf::carpeta_config().map(|c| c.join("tema.txt"))
}

/// Aplica el tema guardado (si lo hay) al arrancar.
pub(super) fn aplicar_guardado(ctx: &Context) {
    let guardado = ruta().and_then(|r| std::fs::read_to_string(r).ok());
    match guardado.as_deref().map(str::trim) {
        Some("claro") => ctx.set_theme(ThemePreference::Light),
        Some("oscuro") => ctx.set_theme(ThemePreference::Dark),
        _ => {}
    }
}

impl Aplicacion {
    pub(super) fn cambiar_tema(&mut self, ctx: &Context, oscuro: bool) {
        ctx.set_theme(if oscuro {
            ThemePreference::Dark
        } else {
            ThemePreference::Light
        });
        if let Some(r) = ruta() {
            let _ = r.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(r, if oscuro { "oscuro" } else { "claro" });
        }
    }
}
