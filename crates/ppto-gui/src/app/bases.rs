// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Bases de precios»: bases de precios (propias, de fabricante, de un
//! distribuidor…) para buscar en ellas y copiar conceptos (mano de obra,
//! materiales, partidas con su descompuesto) a la obra. Se admiten en BC3
//! (solo netos) y en la base de datos del programa (`.sqlite`), que conserva
//! PVP, familia, descuento y referencia de cada material: lo copiado de ella
//! se puede pasar después a la tarifa nueva del fabricante.
//!
//! Las bases se copian a la carpeta de datos del usuario (`bases/`), fuera del
//! repositorio. Lo copiado queda en la obra sin colocar en el árbol, listo
//! para «Añadir línea…», o se añade directamente a la partida elegida.

use super::{Accion, Aplicacion, nombre_naturaleza};
use eframe::egui::{self, RichText};
use ppto_core::especialidad::Especialidad;
use ppto_core::formato::eur;
use ppto_core::{Decimal, Naturaleza, Presupuesto};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Concepto de la base arrastrado al descompuesto de la partida.
pub(super) struct ArrastreBase {
    pub origen: Arc<Presupuesto>,
    pub nombre: String,
    pub codigo: String,
}

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
    /// Especialidad de cada base (nombre del fichero → clave); las que no
    /// están son generales. Se guarda en `especialidades.json` de la carpeta.
    pub especialidades: BTreeMap<String, String>,
    /// Especialidad de la obra abierta.
    pub obra: Vec<Especialidad>,
    /// Mostrar solo las bases de la especialidad de la obra (y las generales).
    pub solo_especialidad: bool,
}

/// Minúsculas y sin tildes, para buscar sin distinguir.
fn plano(s: &str) -> String {
    ppto_biblioteca::plano(s)
}

/// Extensiones de base que se admiten: BC3 (solo netos) y la base de datos
/// del programa (`.sqlite`), que conserva PVP, familia, descuento y
/// referencia de cada material.
const EXTENSIONES: [&str; 2] = ["bc3", "sqlite"];

fn es_sqlite(r: &Path) -> bool {
    r.extension().is_some_and(|x| x.eq_ignore_ascii_case("sqlite"))
}

/// Última revisión guardada de una base en `.sqlite`.
pub(super) fn leer_sqlite(ruta: &Path) -> Result<Presupuesto, String> {
    let a = ppto_db::Almacen::abrir(ruta).map_err(|e| e.to_string())?;
    let rev = a
        .listar_revisiones()
        .map_err(|e| e.to_string())?
        .last()
        .map(|r| r.id)
        .ok_or_else(|| "no tiene ninguna revisión guardada".to_owned())?;
    a.cargar_revision(rev).map_err(|e| e.to_string())
}

impl Base {
    pub fn cargar(ruta: &Path) -> Result<Self, String> {
        let (p, errores) = if es_sqlite(ruta) {
            (leer_sqlite(ruta)?, 0)
        } else {
            let imp = ppto_bc3::importar_fichero(ruta).map_err(|e| e.to_string())?;
            let errores = imp.errores();
            (imp.presupuesto, errores)
        };
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
    pub fn abrir(obra: &[Especialidad]) -> Self {
        Self::en_para(super::pdf::carpeta_config().map(|c| c.join("bases")), obra)
    }

    #[cfg(test)]
    pub fn en(carpeta: Option<PathBuf>) -> Self {
        Self::en_para(carpeta, &[])
    }

    /// Abre la carpeta de bases para una obra de una especialidad: elige la
    /// primera base de esa especialidad (o, si no hay, la primera general).
    pub fn en_para(carpeta: Option<PathBuf>, obra: &[Especialidad]) -> Self {
        let especialidades = carpeta
            .as_ref()
            .and_then(|c| std::fs::read_to_string(c.join("especialidades.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let mut v = Self {
            carpeta,
            ficheros: Vec::new(),
            elegido: None,
            base: None,
            buscar: String::new(),
            filtro: None,
            marcados: BTreeSet::new(),
            mensaje: None,
            especialidades,
            obra: obra.to_vec(),
            solo_especialidad: !obra.is_empty(),
        };
        v.releer_carpeta();
        let primera = v
            .ficheros_visibles()
            .into_iter()
            .min_by_key(|f| v.especialidad_de(f).is_none())
            .cloned();
        if let Some(f) = primera {
            v.elegir(f);
        }
        v
    }

    /// Especialidad de una base (`None` = general).
    pub fn especialidad_de(&self, f: &Path) -> Option<Especialidad> {
        self.especialidades
            .get(&nombre_de(f))
            .and_then(|k| Especialidad::leer(k))
    }

    /// Bases que se ofrecen: todas, o solo las de la especialidad de la
    /// obra y las generales.
    pub fn ficheros_visibles(&self) -> Vec<&PathBuf> {
        self.ficheros
            .iter()
            .filter(|f| {
                !self.solo_especialidad
                    || self.obra.is_empty()
                    || self.especialidad_de(f).is_none_or(|x| self.obra.contains(&x))
            })
            .collect()
    }

    /// Asigna una especialidad a una base (`None` = general) y lo guarda.
    pub fn fijar_especialidad(&mut self, f: &Path, e: Option<Especialidad>) {
        let n = nombre_de(f);
        match e {
            Some(e) => self.especialidades.insert(n, e.clave().into()),
            None => self.especialidades.remove(&n),
        };
        self.guardar_especialidades();
    }

    fn guardar_especialidades(&mut self) {
        let Some(c) = &self.carpeta else { return };
        let r = std::fs::create_dir_all(c).and_then(|()| {
            std::fs::write(
                c.join("especialidades.json"),
                serde_json::to_string_pretty(&self.especialidades).unwrap_or_default(),
            )
        });
        if let Err(e) = r {
            self.mensaje = Some((format!("No se pudo guardar la especialidad de la base: {e}"), true));
        }
    }

    fn releer_carpeta(&mut self) {
        self.ficheros = self
            .carpeta
            .as_ref()
            .and_then(|c| std::fs::read_dir(c).ok())
            .map(|d| {
                let mut v: Vec<PathBuf> = d
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|x| EXTENSIONES.iter().any(|e| x.eq_ignore_ascii_case(e)))
                    })
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

    /// Copia una base (BC3 o .sqlite) a la carpeta de bases y la abre.
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
            if self.especialidades.remove(&nombre_de(&r)).is_some() {
                self.guardar_especialidades();
            }
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

/// Texto de búsqueda para una partida: su primera palabra significativa y
/// sus medidas («REJILLA RETORNO 400 x 300 mm» → «rejilla 400x300»).
pub(super) fn texto_busqueda(resumen: &str) -> String {
    let r = ppto_core::emparejar::rasgos(resumen);
    let primera = plano(resumen)
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| w.chars().count() >= 4 && w.chars().all(char::is_alphabetic))
        .map(str::to_owned);
    let medidas = r.tecnicos.into_iter().filter(|t| {
        t.split_once('x')
            .is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok())
    });
    primera.into_iter().chain(medidas).collect::<Vec<_>>().join(" ")
}

impl Aplicacion {
    /// Partida seleccionada en el árbol (si es una partida).
    fn partida_seleccionada(&self) -> Option<String> {
        self.sel.as_ref().map(|(_, h)| h.clone()).filter(|h| {
            self.p
                .conceptos
                .get(h)
                .is_some_and(|c| c.naturaleza == Naturaleza::Partida)
        })
    }

    /// Ctrl+B: abre las bases de precios buscando la partida seleccionada.
    pub(super) fn bases_con_partida(&mut self) {
        let mut v = self
            .ventana_bases
            .take()
            .unwrap_or_else(|| VentanaBases::abrir(&self.especialidades));
        if let Some(c) = self.partida_seleccionada().and_then(|c| self.p.conceptos.get(&c)) {
            v.buscar = texto_busqueda(&c.resumen);
            v.filtro = Some(Naturaleza::Partida);
        }
        self.ventana_bases = Some(v);
    }

    /// «Buscar en la base» de una partida sin precio: la ventana de emparejar
    /// solo con ella, con la base abierta en «Bases de precios» o, si no hay,
    /// la de la especialidad de la obra.
    pub(super) fn buscar_en_base(&mut self, codigo: &str) {
        let base = self
            .ventana_bases
            .as_ref()
            .and_then(|v| v.base.as_ref())
            .map(|b| (Arc::clone(&b.p), b.nombre()))
            .or_else(|| {
                VentanaBases::abrir(&self.especialidades)
                    .base
                    .map(|b| (Arc::clone(&b.p), b.nombre()))
            });
        let Some((base, nombre)) = base else {
            self.error("No hay ninguna base de precios: añada una en Precios → Bases de precios.");
            return;
        };
        let mut w = super::emparejar::VentanaEmparejar::nueva(&self.p, base, nombre);
        w.filas.retain(|f| f.codigo == codigo);
        if w.filas.is_empty() {
            self.info(format!("«{codigo}» ya tiene precio."));
        } else {
            self.ventana_emparejar = Some(w);
        }
    }
}

impl Aplicacion {
    pub(super) fn ventana_bases(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.ventana_bases.take() else {
            return;
        };
        if v.obra != self.especialidades {
            v.obra = self.especialidades.clone();
            v.solo_especialidad = !v.obra.is_empty();
        }
        // Partida elegida en el árbol (para «añadir a la partida»)
        let partida = self.partida_seleccionada();
        let mut abierta = true;
        let mut emparejar: Option<(Arc<Presupuesto>, String)> = None;
        egui::Window::new("Bases de precios")
            .open(&mut abierta)
            .default_width(860.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| {
                if !v.obra.is_empty() {
                    ui.horizontal(|ui| {
                        let n: Vec<&str> = v.obra.iter().map(|e| e.nombre()).collect();
                        ui.label(RichText::new(format!("Obra de {}", n.join(" + "))).strong());
                        ui.checkbox(&mut v.solo_especialidad, "solo sus bases y las generales");
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Base:");
                    let actual = v.elegido.as_deref().map(nombre_de).unwrap_or_else(|| "(ninguna)".into());
                    let mut cambiar = None;
                    let visibles: Vec<PathBuf> = v.ficheros_visibles().into_iter().cloned().collect();
                    egui::ComboBox::from_id_salt("base_elegida")
                        .selected_text(actual)
                        .show_ui(ui, |ui| {
                            for f in &visibles {
                                let etiqueta = match v.especialidad_de(f) {
                                    Some(e) => format!("{} · {}", nombre_de(f), e.nombre()),
                                    None => nombre_de(f),
                                };
                                if ui.selectable_label(v.elegido.as_ref() == Some(f), etiqueta).clicked() {
                                    cambiar = Some(f.clone());
                                }
                            }
                        });
                    if let Some(f) = cambiar {
                        v.elegir(f);
                    }
                    if ui
                        .button("Añadir base…")
                        .on_hover_text(
                            "BC3 o base del programa (.sqlite). Mejor la .sqlite: sus materiales llevan PVP, familia, \
                             descuento y referencia, y la obra se podrá pasar a la tarifa nueva. Se guarda una copia en \
                             tu carpeta de datos; el original no se toca",
                        )
                        .clicked()
                        && let Some(r) = rfd::FileDialog::new()
                            .add_filter("Base de precios", &["sqlite", "bc3", "BC3"])
                            .pick_file()
                    {
                        v.anadir(&r);
                    }
                    if v.elegido.is_some() && ui.button("Quitar esta base").clicked() {
                        v.quitar();
                    }
                });
                if let Some(f) = v.elegido.clone() {
                    ui.horizontal(|ui| {
                        ui.label("Especialidad de esta base:");
                        let actual = v.especialidad_de(&f);
                        let mut nueva = None;
                        egui::ComboBox::from_id_salt("especialidad_base")
                            .selected_text(actual.map_or("General (todas)", |e| e.nombre()))
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(actual.is_none(), "General (todas)").clicked() {
                                    nueva = Some(None);
                                }
                                for e in Especialidad::TODAS {
                                    if ui.selectable_label(actual == Some(e), e.nombre()).clicked() {
                                        nueva = Some(Some(e));
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Con qué obras se ofrece esta base; las generales salen en todas");
                        if let Some(n) = nueva {
                            v.fijar_especialidad(&f, n);
                        }
                    });
                }
                if let Some((m, error)) = &v.mensaje {
                    let t = RichText::new(m).small();
                    ui.label(if *error { t.color(egui::Color32::from_rgb(220, 90, 80)) } else { t.weak() });
                }
                if v.base.is_none() {
                    ui.add_space(8.0);
                    ui.label(
                        "Añade tu base de precios (.sqlite del programa o BC3) para buscar en ella y copiar conceptos a la obra.",
                    );
                    return;
                }
                ui.separator();
                if let Some(b) = v.base.as_ref()
                    && ui
                        .button("Emparejar la obra con esta base…")
                        .on_hover_text(
                            "Propone, para cada partida sin precio de la obra, la partida de esta base que mejor encaja \
                             (por modelo, medidas, diámetro, potencia, palabras y unidad). Usted confirma.",
                        )
                        .clicked()
                {
                    emparejar = Some((Arc::clone(&b.p), b.nombre()));
                }
                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.buscar)
                            .desired_width(260.0)
                            .hint_text("código o texto, p. ej. rejilla colocación (Ctrl+B: la partida seleccionada)"),
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
                                con_partidas: false,
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
                                    con_partidas: false,
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
                let base_abierta = v.base.as_ref().map(|b| (Arc::clone(&b.p), b.nombre()));
                if partida.is_some() {
                    ui.label(
                        RichText::new("Arrastre un concepto (código o resumen) al precio descompuesto de la partida.")
                            .small()
                            .weak(),
                    );
                }
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
                            let rc = ui.add(
                                egui::Label::new(RichText::new(codigo).monospace()).sense(egui::Sense::drag()),
                            );
                            ui.label(unidad);
                            let rr = ui
                                .add(egui::Label::new(super::corto(resumen, 70)).sense(egui::Sense::drag()))
                                .on_hover_text(resumen);
                            if let Some((origen, nombre)) = &base_abierta {
                                for r in [&rc, &rr] {
                                    if r.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                                    }
                                    if r.drag_started() {
                                        r.dnd_set_drag_payload(ArrastreBase {
                                            origen: Arc::clone(origen),
                                            nombre: nombre.clone(),
                                            codigo: codigo.clone(),
                                        });
                                    }
                                    if r.dragged() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                        if let Some(p) = ui.ctx().pointer_interact_pos() {
                                            egui::Area::new(egui::Id::new("arrastre_base"))
                                                .order(egui::Order::Tooltip)
                                                .fixed_pos(p + egui::vec2(14.0, 6.0))
                                                .interactable(false)
                                                .show(ui.ctx(), |ui| {
                                                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                                                        ui.label(format!("{codigo} · {}", super::corto(resumen, 40)));
                                                    });
                                                });
                                        }
                                    }
                                }
                            }
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
        if let Some((base, nombre)) = emparejar {
            let w = super::emparejar::VentanaEmparejar::nueva(&self.p, base, nombre);
            if w.filas.is_empty() {
                self.info("No hay partidas sin precio en la obra.");
            } else {
                self.ventana_emparejar = Some(w);
            }
        }
        if abierta {
            self.ventana_bases = Some(v);
        }
    }
}
