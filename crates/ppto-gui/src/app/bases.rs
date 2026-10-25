// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Bases de precios»: BC3 de bases de precios (propias, de
//! fabricante, de un distribuidor…) para buscar en ellas y copiar conceptos
//! (mano de obra, materiales, partidas con su descompuesto) a la obra.
//!
//! Las bases se copian a la carpeta de datos del usuario (`bases/`), fuera del
//! repositorio. Lo copiado queda en la obra sin colocar en el árbol, listo
//! para «Añadir línea…», o se añade directamente a la partida elegida.

use super::{Accion, Aplicacion, nombre_naturaleza};
use eframe::egui::{self, RichText};
use ppto_core::formato::eur;
use ppto_core::{Decimal, Naturaleza, Presupuesto};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Máximo de filas que se dibujan a la vez (la búsqueda acota el resto).
const MAX_FILAS: usize = 400;

pub(super) struct Fila {
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub naturaleza: Naturaleza,
    pub precio: Decimal,
    /// Código y resumen en minúsculas y sin tildes, para buscar.
    clave: String,
}

pub(super) struct Base {
    pub ruta: PathBuf,
    pub p: Arc<Presupuesto>,
    pub filas: Vec<Fila>,
    pub errores: usize,
}

pub(super) struct VentanaBases {
    carpeta: Option<PathBuf>,
    pub ficheros: Vec<PathBuf>,
    pub elegido: Option<PathBuf>,
    pub base: Option<Base>,
    pub buscar: String,
    /// `None` = todos los tipos.
    pub filtro: Option<Naturaleza>,
    pub marcados: BTreeSet<String>,
    mensaje: Option<(String, bool)>,
}

/// Minúsculas y sin tildes, para buscar sin distinguir.
fn plano(s: &str) -> String {
    ppto_biblioteca::plano(s)
}

impl Base {
    pub fn cargar(ruta: &Path) -> Result<Self, String> {
        let imp = ppto_bc3::importar_fichero(ruta).map_err(|e| e.to_string())?;
        let errores = imp.errores();
        let p = imp.presupuesto;
        let mut filas: Vec<Fila> = p
            .conceptos
            .values()
            .filter(|c| c.naturaleza != Naturaleza::Capitulo && c.codigo != p.raiz)
            .map(|c| Fila {
                codigo: c.codigo.clone(),
                unidad: c.unidad.clone(),
                resumen: c.resumen.clone(),
                naturaleza: c.naturaleza,
                precio: p.precio(&c.codigo).unwrap_or(c.precio),
                clave: plano(&format!("{} {}", c.codigo, c.resumen)),
            })
            .collect();
        filas.sort_by(|a, b| a.codigo.cmp(&b.codigo));
        Ok(Self {
            ruta: ruta.to_owned(),
            p: Arc::new(p),
            filas,
            errores,
        })
    }

    pub fn nombre(&self) -> String {
        nombre_de(&self.ruta)
    }
}

fn nombre_de(r: &Path) -> String {
    r.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl VentanaBases {
    pub fn abrir() -> Self {
        Self::en(super::pdf::carpeta_config().map(|c| c.join("bases")))
    }

    pub fn en(carpeta: Option<PathBuf>) -> Self {
        let mut v = Self {
            carpeta,
            ficheros: Vec::new(),
            elegido: None,
            base: None,
            buscar: String::new(),
            filtro: None,
            marcados: BTreeSet::new(),
            mensaje: None,
        };
        v.releer_carpeta();
        if let Some(f) = v.ficheros.first().cloned() {
            v.elegir(f);
        }
        v
    }

    fn releer_carpeta(&mut self) {
        self.ficheros = self
            .carpeta
            .as_ref()
            .and_then(|c| std::fs::read_dir(c).ok())
            .map(|d| {
                let mut v: Vec<PathBuf> = d
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bc3")))
                    .collect();
                v.sort();
                v
            })
            .unwrap_or_default();
    }

    pub fn elegir(&mut self, ruta: PathBuf) {
        self.marcados.clear();
        self.elegido = Some(ruta.clone());
        match Base::cargar(&ruta) {
            Ok(b) => {
                let aviso = if b.errores > 0 {
                    format!(" ({} registros con errores se han saltado)", b.errores)
                } else {
                    String::new()
                };
                self.mensaje = Some((format!("{}: {} conceptos{aviso}.", b.nombre(), b.filas.len()), false));
                self.base = Some(b);
            }
            Err(e) => {
                self.base = None;
                self.mensaje = Some((format!("No se pudo leer {}: {e}", nombre_de(&ruta)), true));
            }
        }
    }

    /// Copia un BC3 a la carpeta de bases y lo abre.
    pub fn anadir(&mut self, origen: &Path) {
        let Some(c) = &self.carpeta else {
            self.mensaje = Some(("No hay carpeta de datos del usuario.".into(), true));
            return;
        };
        let destino = c.join(nombre_de(origen));
        let r = std::fs::create_dir_all(c).and_then(|()| std::fs::copy(origen, &destino));
        match r {
            Ok(_) => {
                self.releer_carpeta();
                self.elegir(destino);
            }
            Err(e) => self.mensaje = Some((format!("No se pudo copiar la base: {e}"), true)),
        }
    }

    fn quitar(&mut self) {
        if let Some(r) = self.elegido.take() {
            let _ = std::fs::remove_file(&r);
            self.base = None;
            self.marcados.clear();
            self.releer_carpeta();
            self.mensaje = Some((format!("{} quitada de las bases.", nombre_de(&r)), false));
        }
    }

    /// Filas que pasan la búsqueda y el filtro de tipo.
    pub fn visibles(&self) -> Vec<&Fila> {
        let Some(b) = &self.base else { return Vec::new() };
        let palabras: Vec<String> = plano(&self.buscar).split_whitespace().map(str::to_owned).collect();
        b.filas
            .iter()
            .filter(|f| self.filtro.is_none_or(|n| f.naturaleza == n))
            .filter(|f| palabras.iter().all(|w| f.clave.contains(w.as_str())))
            .collect()
    }
}

impl Aplicacion {
    pub(super) fn ventana_bases(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.ventana_bases.take() else {
            return;
        };
        // Partida elegida en el árbol (para «añadir a la partida»)
        let partida = self.sel.as_ref().map(|(_, h)| h.clone()).filter(|h| {
            self.p
                .conceptos
                .get(h)
                .is_some_and(|c| c.naturaleza == Naturaleza::Partida)
        });
        let mut abierta = true;
        egui::Window::new("Bases de precios")
            .open(&mut abierta)
            .default_width(860.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Base:");
                    let actual = v.elegido.as_deref().map(nombre_de).unwrap_or_else(|| "(ninguna)".into());
                    let mut cambiar = None;
                    egui::ComboBox::from_id_salt("base_elegida")
                        .selected_text(actual)
                        .show_ui(ui, |ui| {
                            for f in &v.ficheros {
                                if ui
                                    .selectable_label(v.elegido.as_ref() == Some(f), nombre_de(f))
                                    .clicked()
                                {
                                    cambiar = Some(f.clone());
                                }
                            }
                        });
                    if let Some(f) = cambiar {
                        v.elegir(f);
                    }
                    if ui
                        .button("Añadir base BC3…")
                        .on_hover_text("Se guarda una copia en tu carpeta de datos; el original no se toca")
                        .clicked()
                        && let Some(r) = rfd::FileDialog::new().add_filter("BC3", &["bc3", "BC3"]).pick_file()
                    {
                        v.anadir(&r);
                    }
                    if v.elegido.is_some() && ui.button("Quitar esta base").clicked() {
                        v.quitar();
                    }
                });
                if let Some((m, error)) = &v.mensaje {
                    let t = RichText::new(m).small();
                    ui.label(if *error { t.color(egui::Color32::from_rgb(220, 90, 80)) } else { t.weak() });
                }
                if v.base.is_none() {
                    ui.add_space(8.0);
                    ui.label("Añade un BC3 con tu base de precios para buscar en él y copiar conceptos a la obra.");
                    return;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.buscar)
                            .desired_width(260.0)
                            .hint_text("código o texto, p. ej. rejilla colocación"),
                    );
                    ui.separator();
                    ui.radio_value(&mut v.filtro, None, "Todo");
                    for n in [
                        Naturaleza::ManoObra,
                        Naturaleza::Material,
                        Naturaleza::Maquinaria,
                        Naturaleza::Subcontrata,
                        Naturaleza::Partida,
                    ] {
                        ui.radio_value(&mut v.filtro, Some(n), nombre_naturaleza(n));
                    }
                });
                let visibles: Vec<(String, String, String, Naturaleza, Decimal)> = v
                    .visibles()
                    .into_iter()
                    .map(|f| (f.codigo.clone(), f.unidad.clone(), f.resumen.clone(), f.naturaleza, f.precio))
                    .collect();
                ui.horizontal(|ui| {
                    ui.label(format!("{} conceptos · {} marcados", visibles.len(), v.marcados.len()));
                    if ui.button(format!("Marcar los {} visibles", visibles.len())).clicked() {
                        v.marcados.extend(visibles.iter().map(|f| f.0.clone()));
                    }
                    if !v.marcados.is_empty() && ui.button("Desmarcar todo").clicked() {
                        v.marcados.clear();
                    }
                });
                ui.horizontal(|ui| {
                    let n = v.marcados.len();
                    let base = v.base.as_ref().map(|b| (Arc::clone(&b.p), b.nombre()));
                    if let Some((origen, nombre)) = base {
                        if ui
                            .add_enabled(n > 0, egui::Button::new(format!("Copiar {n} a la obra")))
                            .on_hover_text(
                                "Quedan en la obra (con su descompuesto) para «Añadir línea…»; lo que ya exista en la obra no se toca",
                            )
                            .clicked()
                        {
                            acciones.push(Accion::CopiarDeBase {
                                origen: Arc::clone(&origen),
                                nombre: nombre.clone(),
                                codigos: v.marcados.iter().cloned().collect(),
                                partida: None,
                            });
                            v.marcados.clear();
                        }
                        if let Some(pt) = &partida {
                            let basicos = v
                                .marcados
                                .iter()
                                .filter(|c| origen.conceptos.get(*c).is_some_and(|c| c.naturaleza != Naturaleza::Partida))
                                .count();
                            if ui
                                .add_enabled(
                                    basicos > 0,
                                    egui::Button::new(format!("Copiar y añadir {basicos} a la partida {pt}")),
                                )
                                .on_hover_text("Cada recurso marcado entra como línea de la partida con cantidad 1")
                                .clicked()
                            {
                                acciones.push(Accion::CopiarDeBase {
                                    origen,
                                    nombre,
                                    codigos: v.marcados.iter().cloned().collect(),
                                    partida: Some(pt.clone()),
                                });
                                v.marcados.clear();
                            }
                        } else {
                            ui.label(
                                RichText::new("(elige una partida en el árbol para añadirle los recursos marcados)")
                                    .small()
                                    .weak(),
                            );
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("filas_base").striped(true).num_columns(6).show(ui, |ui| {
                        ui.label("");
                        ui.strong("Código");
                        ui.strong("Ud");
                        ui.strong("Resumen");
                        ui.strong("Tipo");
                        ui.strong("Precio");
                        ui.end_row();
                        for (codigo, unidad, resumen, nat, precio) in visibles.iter().take(MAX_FILAS) {
                            let mut m = v.marcados.contains(codigo);
                            if ui.checkbox(&mut m, "").changed() {
                                if m {
                                    v.marcados.insert(codigo.clone());
                                } else {
                                    v.marcados.remove(codigo);
                                }
                            }
                            ui.monospace(codigo);
                            ui.label(unidad);
                            ui.label(super::corto(resumen, 70)).on_hover_text(resumen);
                            ui.label(nombre_naturaleza(*nat));
                            ui.label(format!("{} €", eur(*precio)));
                            ui.end_row();
                        }
                    });
                    if visibles.len() > MAX_FILAS {
                        ui.label(
                            RichText::new(format!(
                                "… y {} más: afina la búsqueda («Marcar los visibles» los incluye todos).",
                                visibles.len() - MAX_FILAS
                            ))
                            .weak(),
                        );
                    }
                });
            });
        if abierta {
            self.ventana_bases = Some(v);
        }
    }
}
