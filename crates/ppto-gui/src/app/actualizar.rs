// SPDX-License-Identifier: GPL-3.0-or-later
//! Aviso de versión nueva y actualización con un clic.
//!
//! Al arrancar se consulta en segundo plano la lista de versiones de GitHub
//! (sin enviar ningún dato). Si hay una más nueva, aparece un botón en la
//! barra de arriba. «Actualizar ahora» descarga el instalador, cierra el
//! programa, lo instala y lo vuelve a abrir. Con la variable de entorno
//! `PPTO_SIN_ACTUALIZAR` no se consulta nada.

use super::{Aplicacion, VERSION};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_actualizar::Version;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

pub(super) enum Estado {
    Nada,
    Buscando(mpsc::Receiver<Result<Option<Version>, String>>),
    Disponible(Version),
    Descargando {
        version: Version,
        rx: mpsc::Receiver<Result<PathBuf, String>>,
        bytes: Arc<AtomicU64>,
    },
    Error(String),
}

pub(super) struct Actualizacion {
    pub estado: Estado,
    pub ventana: bool,
    /// Se buscó a mano (Ayuda): avisar también si no hay nada.
    manual: bool,
    mensaje: Option<String>,
}

impl Default for Actualizacion {
    fn default() -> Self {
        Self {
            estado: Estado::Nada,
            ventana: false,
            manual: false,
            mensaje: None,
        }
    }
}

impl Actualizacion {
    /// Empieza a buscar en segundo plano.
    pub fn buscar(&mut self, manual: bool) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ppto_actualizar::buscar(VERSION));
        });
        self.estado = Estado::Buscando(rx);
        self.manual = manual;
        self.mensaje = None;
    }

    pub fn al_arrancar() -> Self {
        let mut a = Self::default();
        if std::env::var_os("PPTO_SIN_ACTUALIZAR").is_none() {
            a.buscar(false);
        }
        a
    }

    /// Recoge resultados de los hilos.
    fn revisar(&mut self, ctx: &egui::Context) {
        match &self.estado {
            Estado::Buscando(rx) => match rx.try_recv() {
                Ok(Ok(Some(v))) => self.estado = Estado::Disponible(v),
                Ok(Ok(None)) => {
                    if self.manual {
                        self.mensaje = Some(format!("Tiene la última versión ({VERSION})."));
                        self.ventana = true;
                    }
                    self.estado = Estado::Nada;
                }
                Ok(Err(e)) => {
                    // Al arrancar sin conexión no se molesta
                    self.estado = if self.manual { Estado::Error(e) } else { Estado::Nada };
                    self.ventana = self.manual;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(500)),
                Err(mpsc::TryRecvError::Disconnected) => self.estado = Estado::Nada,
            },
            Estado::Descargando { rx, .. } => match rx.try_recv() {
                Ok(Ok(ps1)) => match ppto_actualizar::instalar(&ps1) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => self.estado = Estado::Error(e),
                },
                Ok(Err(e)) => self.estado = Estado::Error(e),
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.estado = Estado::Error("la descarga se interrumpió".into());
                }
            },
            _ => {}
        }
    }

    fn descargar(&mut self, v: Version) {
        let (tx, rx) = mpsc::channel();
        let bytes = Arc::new(AtomicU64::new(0));
        let b = bytes.clone();
        let carpeta = std::env::temp_dir().join("presupuestos-bc3-actualizacion");
        let w = v.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ppto_actualizar::descargar(&w, &carpeta, b));
        });
        self.estado = Estado::Descargando { version: v, rx, bytes };
    }
}

fn mb(b: u64) -> String {
    format!("{} MB", (b + 524_288) / 1_048_576)
}

impl Aplicacion {
    /// Botón «Versión nueva» en la barra de arriba (si la hay).
    pub(super) fn boton_actualizar(&mut self, ui: &mut Ui) {
        self.actualizacion.revisar(ui.ctx());
        let txt = match &self.actualizacion.estado {
            Estado::Disponible(v) => format!("⏶ Versión {} disponible", v.numero),
            Estado::Descargando { .. } => "⏶ Descargando la actualización…".into(),
            _ => return,
        };
        if ui
            .button(RichText::new(txt).color(Color32::from_rgb(80, 175, 110)).strong())
            .on_hover_text("Ver novedades y actualizar")
            .clicked()
        {
            self.actualizacion.ventana = true;
        }
    }

    pub(super) fn ventana_actualizar(&mut self, ctx: &egui::Context) {
        if !self.actualizacion.ventana {
            return;
        }
        let mut abierta = true;
        let mut descargar: Option<Version> = None;
        let mut guardar_antes = false;
        let cambios = self.cambios;
        egui::Window::new("Actualizar Presupuestos BC3")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                if let Some(m) = &self.actualizacion.mensaje {
                    ui.label(m);
                }
                match &self.actualizacion.estado {
                    Estado::Nada => {}
                    Estado::Buscando(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Buscando versiones…");
                        });
                    }
                    Estado::Error(e) => {
                        ui.label(
                            RichText::new(format!("No se pudo actualizar: {e}")).color(Color32::from_rgb(220, 80, 60)),
                        );
                        ui.hyperlink_to(
                            "Descargar a mano",
                            "https://github.com/juancar75/presupuestos-bc3/releases",
                        );
                    }
                    Estado::Disponible(v) => {
                        ui.label(format!("Tiene la {VERSION}. Hay una nueva: {}.", v.numero));
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            ui.label(RichText::new(&v.notas).small());
                        });
                        ui.hyperlink_to("Ver en GitHub", &v.pagina);
                        ui.separator();
                        if cfg!(windows) {
                            if cambios {
                                ui.label(
                                    RichText::new("Hay cambios sin guardar: se guardan antes de actualizar.")
                                        .color(Color32::from_rgb(235, 150, 40)),
                                );
                            }
                            let t = if cambios {
                                "Guardar y actualizar"
                            } else {
                                "Actualizar ahora"
                            };
                            if ui
                                .button(RichText::new(format!("{t} ({})", mb(v.tamano))).strong())
                                .on_hover_text(
                                    "Descarga el instalador, cierra el programa, lo instala y lo vuelve a abrir",
                                )
                                .clicked()
                            {
                                guardar_antes = cambios;
                                descargar = Some(v.clone());
                            }
                        } else {
                            ui.label("Descárguela desde GitHub.");
                        }
                    }
                    Estado::Descargando { version, bytes, .. } => {
                        let b = bytes.load(Ordering::Relaxed);
                        ui.label(format!("Descargando la {}…", version.numero));
                        let total = version.tamano.max(1);
                        ui.add(egui::ProgressBar::new(fraccion(b, total)).text(format!("{} de {}", mb(b), mb(total))));
                        ui.label(
                            RichText::new("Al terminar, el programa se cierra, se instala y se vuelve a abrir.")
                                .small()
                                .weak(),
                        );
                    }
                }
            });
        if let Some(v) = descargar {
            if guardar_antes {
                self.guardar(false);
            }
            // Si no se guardó (cancelado o error), no se sigue
            if !self.cambios {
                self.actualizacion.descargar(v);
            }
        }
        if !abierta {
            self.actualizacion.ventana = false;
            if matches!(self.actualizacion.estado, Estado::Error(_)) {
                self.actualizacion.estado = Estado::Nada;
            }
        }
    }
}

/// b / total entre 0 y 1, sin aritmética de coma flotante en el cálculo.
fn fraccion(b: u64, total: u64) -> f32 {
    let milesimas = (b.min(total) * 1000 / total) as u16;
    f32::from(milesimas) / 1000.0
}
