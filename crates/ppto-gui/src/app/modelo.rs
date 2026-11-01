// SPDX-License-Identifier: GPL-3.0-or-later
//! Archivo → Importar → **Modelo BIM (IFC o Revit)**: mediciones de las
//! instalaciones del modelo, agrupadas en capítulos y partidas sin precio.

use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText};
use ppto_core::formato::cant;
use ppto_modelo::{Modelo, Opciones};
use std::path::PathBuf;
use std::sync::mpsc;

pub(super) struct VentanaModelo {
    ruta: PathBuf,
    /// Lectura en curso (los .rvt grandes tardan unos segundos).
    leyendo: Option<mpsc::Receiver<Result<Modelo, String>>>,
    pub modelo: Option<Result<Modelo, String>>,
    pub opciones: Opciones,
}

impl Aplicacion {
    pub(super) fn dialogo_modelo(&mut self) {
        if let Some(r) = rfd::FileDialog::new()
            .add_filter("Modelo BIM", &["ifc", "IFC", "rvt", "RVT"])
            .set_title("Modelo BIM: IFC o Revit (.rvt)")
            .pick_file()
        {
            self.abrir_modelo(r);
        }
    }

    pub(super) fn abrir_modelo(&mut self, ruta: PathBuf) {
        let (tx, rx) = mpsc::channel();
        let r = ruta.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ppto_modelo::leer(&r).map_err(|e| e.to_string()));
        });
        self.ventana_modelo = Some(VentanaModelo {
            ruta,
            leyendo: Some(rx),
            modelo: None,
            opciones: Opciones {
                por_sistema: true,
                por_planta: true,
            },
        });
    }

    pub(super) fn ventana_modelo(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_modelo.take() else {
            return;
        };
        if let Some(rx) = &v.leyendo {
            match rx.try_recv() {
                Ok(r) => {
                    v.modelo = Some(r);
                    v.leyendo = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    v.modelo = Some(Err("la lectura se interrumpió".into()));
                    v.leyendo = None;
                }
            }
        }
        let mut abierta = true;
        let mut accion: Option<bool> = None; // true: obra nueva; false: añadir
        let nombre = v
            .ruta
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        egui::Window::new("Importar modelo BIM")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&nombre).strong());
                match &v.modelo {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Leyendo el modelo…");
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(Color32::from_rgb(220, 80, 60)));
                    }
                    Some(Ok(m)) => {
                        ui.label(format!(
                            "{} elementos de instalaciones{}",
                            m.elementos.len(),
                            if m.ignorados > 0 {
                                format!(" ({} de arquitectura no se miden)", m.ignorados)
                            } else {
                                String::new()
                            }
                        ));
                        egui::Grid::new("modelo_clases")
                            .striped(true)
                            .num_columns(3)
                            .show(ui, |ui| {
                                for t in ["Capítulo", "Elementos", "Metros"] {
                                    ui.label(RichText::new(t).strong());
                                }
                                ui.end_row();
                                for (c, n, l) in m.por_clase() {
                                    ui.label(c.capitulo());
                                    ui.label(n.to_string());
                                    ui.label(if c.lineal() { cant(l, 2) } else { String::new() });
                                    ui.end_row();
                                }
                            });
                        for a in &m.avisos {
                            ui.label(RichText::new(a).small().color(Color32::from_rgb(235, 150, 40)));
                        }
                        ui.separator();
                        ui.add_enabled(
                            m.tiene_sistemas(),
                            egui::Checkbox::new(&mut v.opciones.por_sistema, "Un capítulo por sistema"),
                        )
                        .on_disabled_hover_text("El modelo no trae sistemas");
                        ui.add_enabled(
                            m.tiene_plantas(),
                            egui::Checkbox::new(&mut v.opciones.por_planta, "Mediciones por planta"),
                        )
                        .on_disabled_hover_text("El modelo no trae plantas");
                        ui.horizontal(|ui| {
                            if ui
                                .button(RichText::new("Crear obra").strong())
                                .on_hover_text("Obra nueva con las mediciones, sin precios")
                                .clicked()
                            {
                                accion = Some(true);
                            }
                            if ui
                                .button("Añadir a esta obra")
                                .on_hover_text("Añade los capítulos al final de la obra abierta")
                                .clicked()
                            {
                                accion = Some(false);
                            }
                        });
                        ui.label(
                            RichText::new(
                                "Las partidas salen sin precio (en rojo en el árbol): valórelas como un ciego.",
                            )
                            .small()
                            .weak(),
                        );
                    }
                }
            });
        if let (Some(nueva), Some(Ok(m))) = (accion, &v.modelo) {
            match ppto_modelo::a_presupuesto(m, v.opciones) {
                Ok(pm) => {
                    let partidas = pm
                        .conceptos
                        .values()
                        .filter(|c| c.naturaleza == ppto_core::Naturaleza::Partida)
                        .count();
                    if nueva {
                        self.cargar_presupuesto(pm);
                        self.ciego = None;
                        self.info(format!("Obra creada desde el modelo: {partidas} partidas sin precio."));
                    } else {
                        let hijos: Vec<String> = pm
                            .concepto(&pm.raiz)
                            .map(|r| r.descomposicion.iter().map(|l| l.hijo.clone()).collect())
                            .unwrap_or_default();
                        match ppto_core::fragmento::copiar(&pm, &pm.raiz, &hijos) {
                            Ok(f) => {
                                let raiz = self.p.raiz.clone();
                                self.aplicar(vec![Accion::PegarFragmento(Box::new(f), raiz)]);
                            }
                            Err(e) => self.error(e.to_string()),
                        }
                    }
                    abierta = false;
                }
                Err(e) => self.error(format!("No se pudo crear el presupuesto: {e}")),
            }
        }
        if abierta {
            self.ventana_modelo = Some(v);
        }
    }
}
