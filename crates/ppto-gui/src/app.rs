// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana principal: árbol de capítulos, partida (descompuesto y mediciones),
//! recursos, subcontratación y resumen.

mod alta;

use crate::celdas::Edicion;
use alta::{Alta, TipoAlta};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::explosion::Explosion;
use ppto_core::formato::{eur, num};
use ppto_core::medicion::{LineaMedicion, Medicion, TipoLinea};
use ppto_core::presupuesto::LineaValorada;
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, ResultadoEscenario, simular};
use ppto_core::{Decimal, Presupuesto, venta};
use ppto_db::{Almacen, InfoRevision};
use rust_decimal_macros::dec;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

const ANCHO_NUM: f32 = 78.0;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Pestana {
    Partida,
    Recursos,
    Subcontratar,
    Resumen,
}

/// Cambios pedidos desde la interfaz; se aplican al final de cada fotograma.
enum Accion {
    Seleccionar(String, String),
    Precio(String, Decimal),
    Rendimiento(String, String, Decimal),
    Medicion(String, String, Medicion),
    Resumen(String, String),
    QuitarOferta(usize),
    Ir(String),
    Ci(Decimal),
    Texto(String, String),
    OpcionesCi(ppto_core::presupuesto::OpcionesCi),
    Unidad(String, String),
    NuevoCapitulo {
        padre: String,
        codigo: String,
        resumen: String,
    },
    NuevaPartida {
        capitulo: String,
        codigo: String,
        unidad: String,
        resumen: String,
    },
    /// Crea un recurso y lo añade como línea de `en`; si algo falla, no queda nada.
    NuevoRecurso {
        codigo: String,
        unidad: String,
        resumen: String,
        naturaleza: Naturaleza,
        precio: Decimal,
        en: String,
        cantidad: Decimal,
    },
    AnadirLinea(String, String, Decimal),
    /// Quitar línea (padre, hijo); `true` = borrar también lo que quede sin usar.
    QuitarLinea(String, String, bool),
    Mover(String, String, bool),
    Alta(TipoAlta),
}

/// Resultados del motor para el estado actual del presupuesto.
#[derive(Default)]
struct Calculo {
    error: Option<String>,
    pem: Decimal,
    coste_directo: Decimal,
    lineas: HashMap<String, Vec<LineaValorada>>,
    precios: HashMap<String, Decimal>,
    cantidades: BTreeMap<String, Decimal>,
    explosion: Option<Explosion>,
}

impl Calculo {
    fn de(p: &Presupuesto) -> Self {
        let mut c = Calculo::default();
        if let Err(e) = p.validar() {
            c.error = Some(e.to_string());
            return c;
        }
        let r: Result<(), ppto_core::ErrorMotor> = (|| {
            let v = p.valorar()?;
            c.precios = v.precios;
            c.lineas = v.lineas;
            c.pem = v.pem;
            c.coste_directo = v.coste_directo;
            c.cantidades = p.cantidades_partidas()?;
            c.explosion = Some(p.explosion_recursos()?);
            Ok(())
        })();
        if let Err(e) = r {
            c.error = Some(e.to_string());
        }
        c
    }
}

/// Formulario de nueva oferta de subcontrata.
struct FormOferta {
    codigo: String,
    descripcion: String,
    partida: String,
    recursos: BTreeMap<String, bool>,
    alzado: bool,
    unidad: String,
    precio: Decimal,
    factor: Decimal,
    importe: Decimal,
}

impl Default for FormOferta {
    fn default() -> Self {
        Self {
            codigo: "SUB-1".into(),
            descripcion: String::new(),
            partida: String::new(),
            recursos: BTreeMap::new(),
            alzado: false,
            unidad: "m2".into(),
            precio: Decimal::ZERO,
            factor: Decimal::ONE,
            importe: Decimal::ZERO,
        }
    }
}

pub struct Aplicacion {
    p: Presupuesto,
    calc: Calculo,
    fichero: Option<PathBuf>,
    presupuesto_id: Option<i64>,
    revisiones: Vec<InfoRevision>,
    revision_actual: Option<i64>,
    cambios: bool,
    titulo: String,
    sel: Option<(String, String)>,
    pestana: Pestana,
    edicion: Edicion,
    avisos: Vec<String>,
    mensaje: Option<(String, bool)>,
    gg: Decimal,
    bi: Decimal,
    iva: Decimal,
    margen: Decimal,
    ofertas: Vec<PaqueteTrabajo>,
    form: FormOferta,
    informe: Option<ppto_bc3::Importacion>,
    ver_informe: bool,
    alta: Option<Alta>,
}

impl Aplicacion {
    pub fn new(cc: &eframe::CreationContext<'_>, inicial: Option<PathBuf>) -> Self {
        let mut estilo = (*cc.egui_ctx.global_style()).clone();
        estilo.spacing.item_spacing = egui::vec2(8.0, 5.0);
        cc.egui_ctx.set_global_style(estilo);
        let mut app = Self::sin_ventana();
        // Pestaña inicial (útil para capturas y pruebas): PPTO_GUI_PESTANA=recursos|subcontratar|resumen
        app.pestana = match std::env::var("PPTO_GUI_PESTANA").as_deref() {
            Ok("recursos") => Pestana::Recursos,
            Ok("subcontratar") => Pestana::Subcontratar,
            Ok("resumen") => Pestana::Resumen,
            _ => Pestana::Partida,
        };
        if let Some(ruta) = inicial {
            let es_bc3 = ruta.extension().is_some_and(|e| e.eq_ignore_ascii_case("bc3"));
            if es_bc3 {
                app.importar_bc3(ruta)
            } else {
                app.abrir(ruta)
            }
        }
        app
    }

    /// Estado inicial (ejemplo cargado) sin depender de la ventana: lo usan las pruebas.
    fn sin_ventana() -> Self {
        let mut app = Self {
            p: suelo_radiante(),
            calc: Calculo::default(),
            fichero: None,
            presupuesto_id: None,
            revisiones: Vec::new(),
            revision_actual: None,
            cambios: false,
            titulo: String::new(),
            sel: Some(("C01".into(), "SR.M2".into())),
            pestana: Pestana::Partida,
            edicion: Edicion::default(),
            avisos: Vec::new(),
            mensaje: Some(("Ejemplo sintético de suelo radiante cargado. Las celdas con recuadro se pueden editar (Intro para confirmar).".into(), false)),
            gg: venta::GG_HABITUAL,
            bi: venta::BI_HABITUAL,
            iva: venta::IVA_GENERAL,
            margen: dec!(30),
            ofertas: ofertas_montaje_suelo_radiante().to_vec(),
            form: FormOferta::default(),
            informe: None,
            ver_informe: false,
            alta: None,
        };
        app.recalcular();
        app
    }

    fn recalcular(&mut self) {
        self.calc = Calculo::de(&self.p);
    }

    fn marcar_cambio(&mut self) {
        self.cambios = true;
        self.recalcular();
    }

    fn cargar_ejemplo(&mut self) {
        self.p = suelo_radiante();
        self.fichero = None;
        self.presupuesto_id = None;
        self.revisiones.clear();
        self.revision_actual = None;
        self.cambios = false;
        self.sel = Some(("C01".into(), "SR.M2".into()));
        self.ofertas = ofertas_montaje_suelo_radiante().to_vec();
        self.avisos.clear();
        self.recalcular();
        self.info("Ejemplo sintético cargado.");
    }

    fn nuevo_presupuesto(&mut self) {
        self.p = Presupuesto::new("Presupuesto nuevo", "OBRA", "Presupuesto nuevo");
        self.fichero = None;
        self.presupuesto_id = None;
        self.revisiones.clear();
        self.revision_actual = None;
        self.cambios = false;
        self.sel = None;
        self.ofertas.clear();
        self.avisos.clear();
        self.informe = None;
        self.recalcular();
        self.info("Presupuesto vacío: cree un capítulo con «+ Capítulo» en el árbol.");
    }

    fn info(&mut self, t: impl Into<String>) {
        self.mensaje = Some((t.into(), false));
    }

    fn error(&mut self, t: impl Into<String>) {
        self.mensaje = Some((t.into(), true));
    }

    // ----------------------------------------------------------------- ficheros

    fn dialogo_abrir(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Presupuesto (SQLite)", &["sqlite", "db"])
            .pick_file()
        {
            self.abrir(ruta);
        }
    }

    fn abrir(&mut self, ruta: PathBuf) {
        let r = (|| -> ppto_db::Resultado<_> {
            let a = Almacen::abrir(&ruta)?;
            let revs = a.listar_revisiones()?;
            let ultima = revs.last().cloned();
            let p = match &ultima {
                Some(r) => Some(a.cargar_revision(r.id)?),
                None => None,
            };
            Ok((revs, ultima, p))
        })();
        match r {
            Ok((revs, Some(ultima), Some(p))) => {
                self.p = p;
                self.presupuesto_id = Some(ultima.presupuesto_id);
                self.revision_actual = Some(ultima.id);
                self.revisiones = revs;
                self.fichero = Some(ruta);
                self.cambios = false;
                self.sel = None;
                self.ofertas.clear();
                self.recalcular();
                self.info(format!(
                    "Abierta la revisión {} ({}).",
                    ultima.etiqueta, ultima.creado_en
                ));
            }
            Ok(_) => self.error("El fichero no contiene ningún presupuesto."),
            Err(e) => self.error(format!("No se pudo abrir: {e}")),
        }
    }

    /// Primera partida del árbol (en profundidad), para no dejar la vista vacía.
    fn primera_partida(&self, capitulo: &str) -> Option<(String, String)> {
        for l in &self.p.conceptos.get(capitulo)?.descomposicion {
            match self.p.conceptos.get(&l.hijo).map(|c| c.naturaleza) {
                Some(Naturaleza::Capitulo) => {
                    if let Some(r) = self.primera_partida(&l.hijo) {
                        return Some(r);
                    }
                }
                Some(_) => return Some((capitulo.to_owned(), l.hijo.clone())),
                None => {}
            }
        }
        None
    }

    fn exportar_bc3(&mut self) {
        let nombre = format!(
            "{}.bc3",
            self.p
                .nombre
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        let o = ppto_bc3::OpcionesExportacion {
            gastos_generales: Some(self.gg),
            beneficio_industrial: Some(self.bi),
            iva: Some(self.iva),
            fecha: None,
        };
        match ppto_bc3::exportar_fichero(&self.p, &o, &ruta) {
            Ok(avisos) if avisos.is_empty() => self.info(format!("BC3 exportado en {}", ruta.display())),
            Ok(avisos) => self.info(format!("BC3 exportado en {} — {}", ruta.display(), avisos.join("; "))),
            Err(e) => self.error(format!("No se pudo exportar: {e}")),
        }
    }

    fn exportar_excel(&mut self) {
        let nombre = format!(
            "{}.xlsx",
            self.p
                .nombre
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        let o = ppto_informes::OpcionesInforme {
            gastos_generales: self.gg,
            beneficio_industrial: self.bi,
            iva: self.iva,
        };
        match ppto_informes::guardar_excel(&self.p, &o, &ruta) {
            Ok(()) => self.info(format!("Informe Excel guardado en {}", ruta.display())),
            Err(e) => self.error(format!("No se pudo generar el Excel: {e}")),
        }
    }

    fn dialogo_importar(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3", "BC3"])
            .pick_file()
        {
            self.importar_bc3(ruta);
        }
    }

    fn importar_bc3(&mut self, ruta: PathBuf) {
        match ppto_bc3::importar_fichero(&ruta) {
            Ok(imp) => {
                self.p = imp.presupuesto.clone();
                let k = &imp.porcentajes;
                self.gg = k.gastos_generales.unwrap_or(self.gg);
                self.bi = k.beneficio_industrial.unwrap_or(self.bi);
                self.iva = k.iva.unwrap_or(self.iva);
                self.fichero = None;
                self.presupuesto_id = None;
                self.revisiones.clear();
                self.revision_actual = None;
                self.cambios = true;
                self.sel = None;
                self.ofertas.clear();
                self.avisos.clear();
                self.recalcular();
                self.sel = self.primera_partida(&self.p.raiz.clone());
                let nombre = ruta
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.info(format!(
                    "Importado {nombre}: {} conceptos, {} errores, {} avisos, {} precios que no cuadran. Guárdalo para conservarlo.",
                    self.p.conceptos.len(),
                    imp.errores(),
                    imp.avisos(),
                    imp.discrepancias.len()
                ));
                self.ver_informe = imp.errores() + imp.avisos() + imp.discrepancias.len() > 0;
                self.informe = Some(imp);
            }
            Err(e) => self.error(format!("No se pudo importar: {e}")),
        }
    }

    fn ventana_informe(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(imp) = &self.informe else { return };
        let mut abierta = self.ver_informe;
        egui::Window::new("Informe de importación BC3")
            .open(&mut abierta)
            .default_size([760.0, 520.0])
            .show(ctx, |ui| {
                let c = &imp.cabecera;
                ui.label(format!(
                    "{} · {} · {}",
                    c.programa, c.version_formato, c.juego_caracteres
                ));
                let regs: Vec<String> = imp.registros.iter().map(|(k, v)| format!("~{k} {v}")).collect();
                ui.label(RichText::new(regs.join("   ")).weak());
                match imp.pem_declarado {
                    Some(d) if d == self.calc.pem => {
                        ui.label(
                            RichText::new(format!("PEM {} € — coincide con el BC3", eur(d)))
                                .color(Color32::from_rgb(70, 170, 90)),
                        );
                    }
                    Some(d) => {
                        ui.label(
                            RichText::new(format!(
                                "PEM calculado {} € · declarado {} € · diferencia {} €",
                                eur(self.calc.pem),
                                eur(d),
                                eur(self.calc.pem - d)
                            ))
                            .color(Color32::from_rgb(220, 150, 40)),
                        );
                    }
                    None => {}
                }
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Precios que no cuadran con el BC3: {}",
                        imp.discrepancias.len()
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("disc")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        egui::Grid::new("discrepancias")
                            .striped(true)
                            .num_columns(5)
                            .show(ui, |ui| {
                                for t in ["Código", "Resumen", "Declarado", "Calculado", "Diferencia"] {
                                    ui.label(RichText::new(t).weak());
                                }
                                ui.end_row();
                                for d in imp.discrepancias.iter().take(500) {
                                    let r = ui.add(egui::Button::new(&d.codigo).frame(false));
                                    if r.clicked() {
                                        acciones.push(Accion::Ir(d.codigo.clone()));
                                    }
                                    ui.label(corto(&d.resumen, 40));
                                    ui.label(eur(d.declarado));
                                    ui.label(eur(d.calculado));
                                    ui.label(eur(d.diferencia()));
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Incidencias: {} errores, {} avisos, {} en total",
                        imp.errores(),
                        imp.avisos(),
                        imp.incidencias.len()
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical().id_salt("inc").show(ui, |ui| {
                    for i in &imp.incidencias {
                        let color = match i.gravedad {
                            ppto_bc3::Gravedad::Error => Color32::from_rgb(220, 80, 60),
                            ppto_bc3::Gravedad::Aviso => Color32::from_rgb(220, 150, 40),
                            ppto_bc3::Gravedad::Info => Color32::GRAY,
                        };
                        ui.label(RichText::new(i.to_string().replace('→', "->")).color(color));
                    }
                });
            });
        self.ver_informe = abierta;
    }

    fn cambiar_revision(&mut self, id: i64) {
        let Some(ruta) = self.fichero.clone() else { return };
        match Almacen::abrir(&ruta).and_then(|a| a.cargar_revision(id)) {
            Ok(p) => {
                self.p = p;
                self.revision_actual = Some(id);
                self.cambios = false;
                self.recalcular();
                let et = self.etiqueta_actual();
                self.info(format!("Revisión {et} cargada."));
            }
            Err(e) => self.error(format!("No se pudo cargar la revisión: {e}")),
        }
    }

    fn guardar(&mut self, pedir_ruta: bool) {
        let ruta = match (&self.fichero, pedir_ruta) {
            (Some(r), false) => r.clone(),
            _ => match rfd::FileDialog::new()
                .add_filter("Presupuesto (SQLite)", &["sqlite"])
                .set_file_name("presupuesto.sqlite")
                .save_file()
            {
                Some(r) => {
                    self.presupuesto_id = None;
                    r
                }
                None => return,
            },
        };
        let autor = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "usuario".into());
        let r = (|| -> ppto_db::Resultado<_> {
            let mut a = Almacen::abrir(&ruta)?;
            let pid = match self.presupuesto_id {
                Some(id) => id,
                None => a.crear_presupuesto(&self.p.nombre)?,
            };
            let etiqueta = a.siguiente_etiqueta(pid)?;
            let rev = a.guardar_revision(pid, &etiqueta, &autor, &self.p)?;
            Ok((pid, rev, etiqueta, a.listar_revisiones()?))
        })();
        match r {
            Ok((pid, rev, etiqueta, revs)) => {
                self.fichero = Some(ruta);
                self.presupuesto_id = Some(pid);
                self.revision_actual = Some(rev);
                self.revisiones = revs;
                self.cambios = false;
                self.info(format!(
                    "Guardado como revisión {etiqueta}. Las revisiones anteriores no cambian."
                ));
            }
            Err(e) => self.error(format!("No se pudo guardar: {e}")),
        }
    }

    fn etiqueta_actual(&self) -> String {
        self.revisiones
            .iter()
            .find(|r| Some(r.id) == self.revision_actual)
            .map(|r| r.etiqueta.clone())
            .unwrap_or_else(|| "sin guardar".into())
    }

    // ---------------------------------------------------------------- acciones

    fn aplicar(&mut self, acciones: Vec<Accion>) {
        for a in acciones {
            let r = match a {
                Accion::Seleccionar(padre, hijo) => {
                    self.sel = Some((padre, hijo));
                    self.pestana = Pestana::Partida;
                    self.avisos.clear();
                    continue;
                }
                Accion::Texto(codigo, texto) => {
                    if let Some(c) = self.p.conceptos.get_mut(&codigo) {
                        c.texto = (!texto.trim().is_empty()).then_some(texto);
                    }
                    self.cambios = true;
                    continue;
                }
                Accion::Ci(v) => {
                    if v < Decimal::ZERO {
                        self.error("Los costes indirectos no pueden ser negativos.");
                        continue;
                    }
                    self.p.costes_indirectos = v;
                    Ok(())
                }
                Accion::OpcionesCi(o) => {
                    self.p.opciones_ci = o;
                    Ok(())
                }
                Accion::Ir(codigo) => {
                    let padre = self
                        .p
                        .conceptos
                        .values()
                        .find(|c| c.descomposicion.iter().any(|l| l.hijo == codigo))
                        .map(|c| c.codigo.clone());
                    if let Some(padre) = padre {
                        self.sel = Some((padre, codigo));
                        self.pestana = Pestana::Partida;
                    }
                    continue;
                }
                Accion::QuitarOferta(i) => {
                    if i < self.ofertas.len() {
                        self.ofertas.remove(i);
                    }
                    continue;
                }
                Accion::Resumen(codigo, texto) => {
                    if let Some(c) = self.p.conceptos.get_mut(&codigo) {
                        c.resumen = texto;
                    }
                    self.cambios = true;
                    continue;
                }
                Accion::Alta(t) => {
                    self.abrir_alta(t);
                    continue;
                }
                Accion::Unidad(c, u) => self.p.fijar_unidad(&c, &u),
                Accion::NuevoCapitulo { padre, codigo, resumen } => self
                    .p
                    .nuevo_capitulo(&padre, &codigo, &resumen)
                    .map(|()| self.info(format!("Capítulo {codigo} creado."))),
                Accion::NuevaPartida {
                    capitulo,
                    codigo,
                    unidad,
                    resumen,
                } => self.p.nueva_partida(&capitulo, &codigo, &unidad, &resumen).map(|()| {
                    self.sel = Some((capitulo, codigo.clone()));
                    self.pestana = Pestana::Partida;
                    self.info(format!(
                        "Partida {codigo} creada: añada sus líneas con «Añadir línea…»."
                    ));
                }),
                Accion::NuevoRecurso {
                    codigo,
                    unidad,
                    resumen,
                    naturaleza,
                    precio,
                    en,
                    cantidad,
                } => self
                    .p
                    .nuevo_recurso(&codigo, &unidad, &resumen, naturaleza, precio)
                    .and_then(|()| {
                        self.p.anadir_linea(&en, &codigo, cantidad).inspect_err(|_| {
                            let _ = self.p.borrar_concepto(&codigo);
                        })
                    }),
                Accion::AnadirLinea(padre, hijo, q) => self.p.anadir_linea(&padre, &hijo, q),
                Accion::QuitarLinea(padre, hijo, purgar) => {
                    let r = if purgar {
                        self.p.quitar_y_purgar(&padre, &hijo).map(|b| {
                            if !b.is_empty() {
                                self.info(format!("Borrados por quedar sin uso: {}", b.join(", ")));
                            }
                        })
                    } else {
                        self.p.quitar_linea(&padre, &hijo)
                    };
                    if r.is_ok()
                        && self
                            .sel
                            .as_ref()
                            .is_some_and(|(sp, sh)| (sp == &padre && sh == &hijo) || !self.p.conceptos.contains_key(sh))
                    {
                        self.sel = None;
                    }
                    r
                }
                Accion::Mover(padre, hijo, arriba) => self.p.mover_linea(&padre, &hijo, arriba),
                Accion::Precio(c, v) => self.p.fijar_precio(&c, v),
                Accion::Rendimiento(padre, hijo, v) => self.p.fijar_rendimiento(&padre, &hijo, v),
                Accion::Medicion(padre, hijo, m) => self.p.actualizar_medicion(&padre, &hijo, m).map(|av| {
                    self.avisos = av;
                }),
            };
            match r {
                Ok(()) => self.marcar_cambio(),
                Err(e) => self.error(e.to_string()),
            }
        }
    }

    // --------------------------------------------------------------------- UI

    fn barra_superior(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui
                .button("📄 Ejemplo")
                .on_hover_text("Cargar el ejemplo sintético de suelo radiante")
                .clicked()
            {
                self.cargar_ejemplo();
            }
            if ui.button("Nuevo").on_hover_text("Presupuesto vacío").clicked() {
                self.nuevo_presupuesto();
            }
            if ui.button("📂 Abrir…").clicked() {
                self.dialogo_abrir();
            }
            if ui
                .button("📥 Importar BC3…")
                .on_hover_text("Leer un presupuesto FIEBDC-3 (p. ej. exportado de Presto)")
                .clicked()
            {
                self.dialogo_importar();
            }
            if ui
                .button("📤 Exportar BC3…")
                .on_hover_text("FIEBDC-3/2002 en ANSI, como lo exporta Presto 8.8")
                .clicked()
            {
                self.exportar_bc3();
            }
            if ui
                .button("📊 Excel…")
                .on_hover_text("Resumen, presupuesto, descompuestos, mediciones, recursos y horas por oficio")
                .clicked()
            {
                self.exportar_excel();
            }
            if self.informe.is_some() && ui.button("Informe de importación").clicked() {
                self.ver_informe = true;
            }
            let txt_guardar = if self.fichero.is_some() {
                "💾 Guardar revisión"
            } else {
                "💾 Guardar…"
            };
            if ui
                .add_enabled(self.cambios || self.fichero.is_none(), egui::Button::new(txt_guardar))
                .on_hover_text("Guarda una revisión nueva (R1, R2…); las anteriores no se modifican")
                .clicked()
            {
                self.guardar(false);
            }
            if ui.button("Guardar como…").clicked() {
                self.guardar(true);
            }
            ui.separator();
            if !self.revisiones.is_empty() {
                let mut elegida = self.revision_actual;
                egui::ComboBox::from_id_salt("revision")
                    .selected_text(format!("Revisión {}", self.etiqueta_actual()))
                    .show_ui(ui, |ui| {
                        for r in &self.revisiones {
                            let candado = if r.bloqueada { " 🔒" } else { "" };
                            ui.selectable_value(
                                &mut elegida,
                                Some(r.id),
                                format!(
                                    "{}{} · {} · {}",
                                    r.etiqueta,
                                    candado,
                                    r.autor,
                                    &r.creado_en[..16.min(r.creado_en.len())]
                                ),
                            );
                        }
                    });
                if elegida != self.revision_actual
                    && let Some(id) = elegida
                {
                    self.cambiar_revision(id);
                }
            }
            if self.cambios {
                ui.label(RichText::new("* cambios sin guardar").color(Color32::from_rgb(220, 150, 40)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} €", eur(self.calc.pem))).size(20.0).strong());
                ui.label(RichText::new("PEM").weak());
            });
        });
    }

    fn arbol(&self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        ui.heading(&self.p.conceptos[&self.p.raiz].resumen);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("+ Capítulo").clicked() {
                acciones.push(Accion::Alta(TipoAlta::Capitulo {
                    padre: self.p.raiz.clone(),
                }));
            }
            ui.label(RichText::new("Clic derecho: más opciones").weak().small());
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.rama(ui, &self.p.raiz, acciones);
        });
    }

    fn rama(&self, ui: &mut Ui, capitulo: &str, acciones: &mut Vec<Accion>) {
        let Some(lineas) = self.calc.lineas.get(capitulo) else {
            return;
        };
        for l in lineas {
            let c = &self.p.conceptos[&l.hijo];
            if c.naturaleza == Naturaleza::Capitulo {
                let r = egui::CollapsingHeader::new(RichText::new(format!("{}  {}", c.codigo, c.resumen)).strong())
                    .id_salt(("cap", capitulo, &l.hijo))
                    .default_open(self.calc.lineas.get(&l.hijo).is_none_or(|v| v.len() <= 40))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("{} €", eur(l.importe))).weak());
                        self.rama(ui, &l.hijo, acciones);
                        if self.calc.lineas.get(&l.hijo).is_none_or(Vec::is_empty) {
                            ui.label(RichText::new("(vacío: clic derecho en el capítulo)").weak().small());
                        }
                    });
                r.header_response.context_menu(|ui| {
                    if ui.button("Nueva partida aquí").clicked() {
                        acciones.push(Accion::Alta(TipoAlta::Partida {
                            capitulo: l.hijo.clone(),
                        }));
                    }
                    if ui.button("Nuevo subcapítulo").clicked() {
                        acciones.push(Accion::Alta(TipoAlta::Capitulo { padre: l.hijo.clone() }));
                    }
                    if ui.button("Añadir partida existente…").clicked() {
                        acciones.push(Accion::Alta(TipoAlta::Linea { padre: l.hijo.clone() }));
                    }
                    ui.separator();
                    menu_linea(ui, capitulo, &l.hijo, acciones);
                });
            } else {
                let elegido = self.sel.as_ref() == Some(&(capitulo.to_owned(), l.hijo.clone()));
                let r = ui.add(
                    egui::Button::selectable(elegido, format!("{}  {}", c.codigo, corto(&c.resumen, 34)))
                        .wrap_mode(egui::TextWrapMode::Truncate),
                );
                ui.label(
                    RichText::new(format!(
                        "    {} {} × {} = {} €",
                        num(l.cantidad, 2),
                        c.unidad,
                        eur(l.precio),
                        eur(l.importe)
                    ))
                    .weak()
                    .small(),
                );
                if r.clicked() {
                    acciones.push(Accion::Seleccionar(capitulo.to_owned(), l.hijo.clone()));
                }
                r.context_menu(|ui| menu_linea(ui, capitulo, &l.hijo, acciones));
            }
        }
    }

    fn pestana_partida(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let Some((padre, hijo)) = self.sel.clone() else {
            ui.label("Selecciona una partida en el árbol de la izquierda.");
            return;
        };
        let Some(c) = self.p.conceptos.get(&hijo).cloned() else {
            return;
        };
        let dec = self.p.decimales;
        let linea_cap = self
            .calc
            .lineas
            .get(&padre)
            .and_then(|v| v.iter().find(|l| l.hijo == hijo))
            .cloned();

        ui.horizontal(|ui| {
            ui.label(RichText::new(&c.codigo).strong().size(16.0));
            let mut unidad = c.unidad.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut unidad).desired_width(40.0))
                .on_hover_text("Unidad")
                .changed()
            {
                acciones.push(Accion::Unidad(c.codigo.clone(), unidad));
            }
            let mut resumen = c.resumen.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut resumen).desired_width(480.0))
                .changed()
            {
                acciones.push(Accion::Resumen(c.codigo.clone(), resumen));
            }
        });
        if let Some(l) = &linea_cap {
            let coste = self.calc.precios.get(&hijo).copied().unwrap_or_default();
            if coste != l.precio {
                ui.label(
                    RichText::new(format!(
                        "Coste directo {} €/{}  +  {} % costes indirectos",
                        eur(coste),
                        c.unidad,
                        num(self.p.costes_indirectos, 2)
                    ))
                    .weak(),
                );
            }
            ui.label(
                RichText::new(format!(
                    "Cantidad {} {}   ·   Precio {} €/{}   ·   Importe {} €",
                    num(l.cantidad, dec.medicion),
                    c.unidad,
                    eur(l.precio),
                    c.unidad,
                    eur(l.importe)
                ))
                .size(15.0),
            );
        }
        ui.add_space(6.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("Texto descriptivo").strong())
                .id_salt(("texto", &hijo))
                .default_open(true)
                .show(ui, |ui| {
                    let mut texto = c.texto.clone().unwrap_or_default();
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut texto)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3)
                            .hint_text("Descripción completa de la partida (texto largo)"),
                    );
                    if r.changed() {
                        acciones.push(Accion::Texto(c.codigo.clone(), texto));
                    }
                });
            ui.add_space(6.0);
            ui.label(RichText::new("Precio descompuesto").strong());
            if let Some(lineas) = self.calc.lineas.get(&hijo).cloned() {
                egui::Grid::new("descompuesto")
                    .striped(true)
                    .num_columns(8)
                    .show(ui, |ui| {
                        for t in ["Código", "Ud", "Resumen", "Tipo", "Cantidad", "Precio", "Importe", ""] {
                            ui.label(RichText::new(t).weak());
                        }
                        ui.end_row();
                        for l in &lineas {
                            let h = &self.p.conceptos[&l.hijo];
                            ui.label(&h.codigo);
                            ui.label(&h.unidad);
                            ui.label(corto(&h.resumen, 46)).on_hover_text(&h.resumen);
                            ui.label(
                                RichText::new(nombre_naturaleza(h.naturaleza)).color(color_naturaleza(h.naturaleza)),
                            );
                            if let Some(v) = self.edicion.decimal(
                                ui,
                                &format!("r|{hijo}|{}", l.hijo),
                                l.cantidad,
                                dec.rendimiento,
                                ANCHO_NUM,
                            ) {
                                acciones.push(Accion::Rendimiento(hijo.clone(), l.hijo.clone(), v));
                            }
                            if h.naturaleza.es_basico() {
                                if let Some(v) =
                                    self.edicion
                                        .decimal(ui, &format!("p|{}", l.hijo), l.precio, dec.precio, ANCHO_NUM)
                                {
                                    acciones.push(Accion::Precio(l.hijo.clone(), v));
                                }
                            } else {
                                let nota = if h.naturaleza == Naturaleza::Porcentaje {
                                    " (base)"
                                } else {
                                    ""
                                };
                                ui.label(format!("{}{nota}", eur(l.precio)));
                            }
                            ui.label(eur(l.importe));
                            ui.horizontal(|ui| {
                                if ui.small_button("^").on_hover_text("Subir").clicked() {
                                    acciones.push(Accion::Mover(hijo.clone(), l.hijo.clone(), true));
                                }
                                if ui.small_button("v").on_hover_text("Bajar").clicked() {
                                    acciones.push(Accion::Mover(hijo.clone(), l.hijo.clone(), false));
                                }
                                if ui
                                    .small_button("x")
                                    .on_hover_text("Quitar la línea (el recurso sigue en la obra)")
                                    .clicked()
                                {
                                    acciones.push(Accion::QuitarLinea(hijo.clone(), l.hijo.clone(), false));
                                }
                            });
                            ui.end_row();
                        }
                        for _ in 0..5 {
                            ui.label("");
                        }
                        ui.label(RichText::new("Precio").strong());
                        ui.label(
                            RichText::new(eur(self.calc.precios.get(&hijo).copied().unwrap_or_default())).strong(),
                        );
                        ui.end_row();
                    });
                if ui.button("+ Añadir línea…").clicked() {
                    acciones.push(Accion::Alta(TipoAlta::Linea { padre: hijo.clone() }));
                }
            } else if c.naturaleza == Naturaleza::Partida {
                ui.label(RichText::new("Partida sin descomposición: precio 0,00.").weak());
                if ui.button("+ Añadir línea…").clicked() {
                    acciones.push(Accion::Alta(TipoAlta::Linea { padre: hijo.clone() }));
                }
            } else {
                ui.label(RichText::new("Recurso básico sin descomposición.").weak());
            }

            ui.add_space(12.0);
            ui.label(RichText::new(format!("Medición en {}", padre)).strong());
            let clave = (padre.clone(), hijo.clone());
            match self.p.mediciones.get(&clave).cloned() {
                Some(m) => self.hoja_medicion(ui, &padre, &hijo, &c.unidad, m, acciones),
                None => {
                    ui.horizontal(|ui| {
                        ui.label("Cantidad sin hoja de medición:");
                        if let Some(l) = &linea_cap
                            && let Some(v) = self.edicion.decimal(
                                ui,
                                &format!("q|{padre}|{hijo}"),
                                l.cantidad,
                                dec.medicion,
                                ANCHO_NUM,
                            )
                        {
                            acciones.push(Accion::Rendimiento(padre.clone(), hijo.clone(), v));
                        }
                        if ui.button("Crear hoja de medición").clicked() {
                            let q = linea_cap.as_ref().map(|l| l.cantidad).unwrap_or(Decimal::ONE);
                            let m = Medicion::new(vec![LineaMedicion::normal("", Some(q), None, None, None)]);
                            acciones.push(Accion::Medicion(padre.clone(), hijo.clone(), m));
                        }
                    });
                }
            }
            for a in &self.avisos {
                ui.label(RichText::new(format!("⚠ {a}")).color(Color32::from_rgb(220, 150, 40)));
            }
        });
    }

    fn hoja_medicion(
        &mut self,
        ui: &mut Ui,
        padre: &str,
        hijo: &str,
        unidad: &str,
        m: Medicion,
        acciones: &mut Vec<Accion>,
    ) {
        let dec = self.p.decimales;
        let res = m.calcular(&dec, unidad);
        let parciales = res.as_ref().map(|r| r.parciales.clone()).unwrap_or_default();
        let mut nueva = m.clone();
        let mut cambio = false;
        let mut quitar = None;
        egui::Grid::new("medicion").striped(true).num_columns(8).show(ui, |ui| {
            for t in [
                "Comentario",
                "Uds",
                "Longitud",
                "Anchura",
                "Altura",
                "Fórmula",
                "Parcial",
                "",
            ] {
                ui.label(RichText::new(t).weak());
            }
            ui.end_row();
            for (i, l) in nueva.lineas.iter_mut().enumerate() {
                let mut com = l.comentario.clone();
                if ui
                    .add_sized([200.0, 20.0], egui::TextEdit::singleline(&mut com))
                    .lost_focus()
                    && com != l.comentario
                {
                    l.comentario = com;
                    cambio = true;
                }
                let subtotal = matches!(l.tipo, TipoLinea::SubtotalParcial | TipoLinea::SubtotalAcumulado);
                if subtotal {
                    for _ in 0..5 {
                        ui.label("");
                    }
                } else {
                    let campos: [(&str, &mut Option<Decimal>, u32); 4] = [
                        ("u", &mut l.unidades, dec.dimensiones),
                        ("l", &mut l.longitud, dec.dimensiones),
                        ("a", &mut l.anchura, dec.dimensiones),
                        ("h", &mut l.altura, dec.dimensiones),
                    ];
                    for (k, v, d) in campos {
                        if let Some(n) = self
                            .edicion
                            .opcional(ui, &format!("m|{padre}|{hijo}|{i}|{k}"), *v, d, 70.0)
                        {
                            *v = n;
                            cambio = true;
                        }
                    }
                    let mut f = l.formula.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut f)
                                .desired_width(110.0)
                                .hint_text("p.ej. a*b*c"),
                        )
                        .on_hover_text("Variables: a=uds, b=longitud, c=anchura, d=altura, p=π")
                        .lost_focus()
                    {
                        let f = f.trim().to_owned();
                        let nueva_f = (!f.is_empty()).then_some(f);
                        if nueva_f != l.formula {
                            l.tipo = if nueva_f.is_some() {
                                TipoLinea::Formula
                            } else {
                                TipoLinea::Normal
                            };
                            l.formula = nueva_f;
                            cambio = true;
                        }
                    }
                }
                let parcial = parciales.get(i).copied().unwrap_or_default();
                let txt = RichText::new(num(parcial, dec.medicion));
                ui.label(if subtotal { txt.italics() } else { txt });
                if ui.small_button("✖").on_hover_text("Quitar línea").clicked() {
                    quitar = Some(i);
                }
                ui.end_row();
            }
            for _ in 0..5 {
                ui.label("");
            }
            ui.label(RichText::new("Total").strong());
            let total = res.as_ref().map(|r| r.total).unwrap_or_default();
            ui.label(RichText::new(format!("{} {unidad}", num(total, dec.medicion))).strong());
            ui.end_row();
        });
        if let Err(e) = &res {
            ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(220, 80, 60)));
        }
        ui.horizontal(|ui| {
            if ui.button("+ Añadir línea").clicked() {
                nueva
                    .lineas
                    .push(LineaMedicion::normal("", Some(Decimal::ONE), None, None, None));
                cambio = true;
            }
            if ui.button("+ Subtotal").clicked() {
                nueva.lineas.push(LineaMedicion::subtotal("Subtotal"));
                cambio = true;
            }
        });
        if let Some(i) = quitar {
            nueva.lineas.remove(i);
            cambio = true;
        }
        if cambio {
            acciones.push(Accion::Medicion(padre.to_owned(), hijo.to_owned(), nueva));
        }
    }

    fn pestana_recursos(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let Some(e) = &self.calc.explosion else { return };
        let e = e.clone();
        let dec = self.p.decimales;
        ui.label("Cantidades totales necesarias para ejecutar el presupuesto. Los precios se pueden editar aquí.");
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for nat in [
                Naturaleza::ManoObra,
                Naturaleza::Material,
                Naturaleza::Maquinaria,
                Naturaleza::Subcontrata,
                Naturaleza::Otros,
                Naturaleza::Porcentaje,
            ] {
                let grupo: Vec<_> = e.por_naturaleza(nat).cloned().collect();
                if grupo.is_empty() {
                    continue;
                }
                let subtotal: Decimal = grupo.iter().map(|r| r.importe).sum();
                ui.label(
                    RichText::new(format!("{}  —  {} €", nombre_naturaleza(nat), eur(subtotal)))
                        .strong()
                        .color(color_naturaleza(nat)),
                );
                egui::Grid::new(("recursos", nombre_naturaleza(nat)))
                    .striped(true)
                    .num_columns(6)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        for t in ["Código", "Ud", "Resumen", "Cantidad", "Precio", "Importe"] {
                            ui.label(RichText::new(t).weak());
                        }
                        ui.end_row();
                        for r in &grupo {
                            ui.label(&r.codigo);
                            ui.label(&r.unidad);
                            ui.label(corto(&r.resumen, 50));
                            if nat == Naturaleza::Porcentaje {
                                ui.label("");
                                ui.label("");
                            } else {
                                ui.label(num(r.cantidad, 3));
                                if let Some(v) = self.edicion.decimal(
                                    ui,
                                    &format!("rp|{}", r.codigo),
                                    r.precio,
                                    dec.precio,
                                    ANCHO_NUM,
                                ) {
                                    acciones.push(Accion::Precio(r.codigo.clone(), v));
                                }
                            }
                            ui.label(eur(r.importe));
                            ui.end_row();
                        }
                    });
                if nat == Naturaleza::ManoObra {
                    let horas: Decimal = grupo.iter().map(|r| r.cantidad).sum();
                    ui.label(RichText::new(format!("Horas de mano de obra: {}", num(horas, 2))).italics());
                }
                ui.add_space(8.0);
            }
            ui.separator();
            ui.label(format!(
                "Total explosión: {} €     PEM: {} €",
                eur(e.total),
                eur(e.coste_directo)
            ));
            let txt = format!("Descuadre de redondeo: {} €", eur(e.descuadre_redondeo));
            ui.label(RichText::new(txt).weak()).on_hover_text(
                "El PEM suma importes de línea redondeados; la explosión multiplica cantidades totales. \
                 La diferencia se muestra, no se oculta.",
            );
        });
    }

    fn pestana_subcontratar(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let dec = self.p.decimales;
        ui.label("Simula contratar fuera parte de una partida (normalmente la mano de obra). El presupuesto original no cambia.");
        ui.add_space(6.0);

        // ---- formulario
        let partidas: Vec<String> = self.calc.cantidades.keys().cloned().collect();
        if self.form.partida.is_empty() || !partidas.contains(&self.form.partida) {
            let preferida = self
                .sel
                .as_ref()
                .map(|(_, h)| h.clone())
                .filter(|h| partidas.contains(h));
            if let Some(p) = preferida.as_ref().or(partidas.first()) {
                self.form.partida = p.clone();
                self.form.recursos.clear();
            }
        }
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Nueva oferta").strong());
            ui.horizontal(|ui| {
                ui.label("Código");
                ui.add(egui::TextEdit::singleline(&mut self.form.codigo).desired_width(70.0));
                ui.label("Descripción");
                ui.add(egui::TextEdit::singleline(&mut self.form.descripcion).desired_width(300.0));
            });
            ui.horizontal(|ui| {
                ui.label("Partida");
                let antes = self.form.partida.clone();
                let etiquetas: Vec<(String, String)> =
                    partidas.iter().map(|p| (p.clone(), self.etiqueta_partida(p))).collect();
                let actual = self.etiqueta_partida(&self.form.partida);
                egui::ComboBox::from_id_salt("partida_sub")
                    .width(360.0)
                    .selected_text(actual)
                    .show_ui(ui, |ui| {
                        for (p, t) in etiquetas {
                            ui.selectable_value(&mut self.form.partida, p, t);
                        }
                    });
                if antes != self.form.partida {
                    self.form.recursos.clear();
                }
            });
            if let Some(lineas) = self.calc.lineas.get(&self.form.partida).cloned() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sustituye:");
                    for l in lineas.iter().filter(|l| l.naturaleza != Naturaleza::Porcentaje) {
                        let marcado = self
                            .form
                            .recursos
                            .entry(l.hijo.clone())
                            .or_insert(l.naturaleza == Naturaleza::ManoObra);
                        ui.checkbox(marcado, format!("{} ({} €)", l.hijo, eur(l.importe)));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.form.alzado, false, "Por unidad");
                ui.radio_value(&mut self.form.alzado, true, "Alzado");
                ui.separator();
                if self.form.alzado {
                    ui.label("Importe €");
                    if let Some(v) = self.edicion.decimal(ui, "f|importe", self.form.importe, 2, 90.0) {
                        self.form.importe = v;
                    }
                } else {
                    ui.label("Unidad");
                    ui.add(egui::TextEdit::singleline(&mut self.form.unidad).desired_width(40.0));
                    ui.label("Precio €");
                    if let Some(v) = self
                        .edicion
                        .decimal(ui, "f|precio", self.form.precio, dec.precio, ANCHO_NUM)
                    {
                        self.form.precio = v;
                    }
                    let ud_p = self
                        .p
                        .conceptos
                        .get(&self.form.partida)
                        .map(|c| c.unidad.clone())
                        .unwrap_or_default();
                    ui.label(format!("{} por {ud_p} de partida", self.form.unidad));
                    if let Some(v) = self.edicion.decimal(ui, "f|factor", self.form.factor, 3, 60.0) {
                        self.form.factor = v;
                    }
                }
                ui.separator();
                let recursos: Vec<String> = self
                    .form
                    .recursos
                    .iter()
                    .filter(|(_, v)| **v)
                    .map(|(k, _)| k.clone())
                    .collect();
                if ui
                    .add_enabled(!recursos.is_empty(), egui::Button::new("+ Añadir oferta"))
                    .clicked()
                {
                    let contratacion = if self.form.alzado {
                        Contratacion::Alzado {
                            importe: self.form.importe,
                        }
                    } else {
                        Contratacion::PorUnidad {
                            unidad: self.form.unidad.clone(),
                            precio: self.form.precio,
                        }
                    };
                    self.ofertas.push(PaqueteTrabajo {
                        codigo: self.form.codigo.clone(),
                        descripcion: self.form.descripcion.clone(),
                        contratacion,
                        asignaciones: vec![Asignacion {
                            partida: self.form.partida.clone(),
                            recursos,
                            factor_conversion: if self.form.alzado {
                                Decimal::ZERO
                            } else {
                                self.form.factor
                            },
                        }],
                        incluye: vec![],
                        excluye: vec![],
                    });
                    self.form.codigo = format!("SUB-{}", self.ofertas.len() + 1);
                }
            });
        });

        // ---- comparación
        ui.add_space(8.0);
        ui.label(
            RichText::new("Comparación de ofertas (cada una por separado frente a ejecutarlo con medios propios)")
                .strong(),
        );
        let resultados: Vec<(usize, &PaqueteTrabajo, Result<ResultadoEscenario, String>)> = self
            .ofertas
            .iter()
            .enumerate()
            .map(|(i, o)| {
                (
                    i,
                    o,
                    simular(&self.p, std::slice::from_ref(o)).map_err(|e| e.to_string()),
                )
            })
            .collect();
        let mut mejor: Option<(usize, Decimal)> = None;
        for (i, _, r) in &resultados {
            if let Ok(r) = r
                && mejor.is_none_or(|(_, a)| r.ahorro > a)
            {
                mejor = Some((*i, r.ahorro));
            }
        }
        let mut notas: Vec<String> = Vec::new();
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("ofertas").striped(true).num_columns(9).show(ui, |ui| {
                for t in [
                    "Oferta",
                    "Contratación",
                    "Partida",
                    "Coste propio",
                    "Contratado",
                    "Equivale a",
                    "PEM escenario",
                    "Ahorro",
                    "Horas lib.",
                ] {
                    ui.label(RichText::new(t).weak());
                }
                ui.end_row();
                for (i, o, r) in &resultados {
                    let partida = o.asignaciones.first().map(|a| a.partida.clone()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").on_hover_text("Quitar oferta").clicked() {
                            acciones.push(Accion::QuitarOferta(*i));
                        }
                        let t = format!("{} {}", o.codigo, corto(&o.descripcion, 18));
                        if mejor.map(|m| m.0) == Some(*i) {
                            ui.label(RichText::new(format!("★ {t}")).strong());
                        } else {
                            ui.label(t);
                        }
                    });
                    ui.label(match &o.contratacion {
                        Contratacion::PorUnidad { unidad, precio } => format!("{} €/{unidad}", eur(*precio)),
                        Contratacion::Alzado { importe } => format!("alzado {} €", eur(*importe)),
                    });
                    ui.label(&partida);
                    match r {
                        Ok(r) => {
                            let ud = self
                                .p
                                .conceptos
                                .get(&partida)
                                .map(|c| c.unidad.clone())
                                .unwrap_or_default();
                            let eq = r.precio_equivalente(&o.codigo, &partida, 2).unwrap_or_default();
                            let horas: Decimal = r.horas_liberadas.values().copied().sum();
                            ui.label(eur(r.coste_retirado));
                            ui.label(eur(r.coste_contratado));
                            ui.label(format!("{} €/{ud}", eur(eq)));
                            ui.label(eur(r.coste_escenario));
                            let color = if r.ahorro >= Decimal::ZERO {
                                Color32::from_rgb(70, 170, 90)
                            } else {
                                Color32::from_rgb(220, 80, 60)
                            };
                            ui.label(RichText::new(eur(r.ahorro)).color(color));
                            ui.label(num(horas, 1));
                            for n in &r.notas {
                                // «SUB-A: texto» → «texto», para no repetir la misma nota por oferta
                                let t = n.split_once(": ").map_or(n.as_str(), |(_, t)| t).to_owned();
                                if !notas.contains(&t) {
                                    notas.push(t);
                                }
                            }
                        }
                        Err(e) => {
                            ui.label(RichText::new(e).color(Color32::from_rgb(220, 80, 60)));
                        }
                    }
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            for n in &notas {
                ui.label(RichText::new(format!("Nota: {n}")).weak());
            }
        });
    }

    fn pestana_resumen(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let pem = self.calc.pem;
        let cd = self.calc.coste_directo;
        ui.label(RichText::new("Costes indirectos").strong());
        ui.horizontal(|ui| {
            ui.label("Costes indirectos %");
            if let Some(n) = self.edicion.decimal(ui, "pct|ci", self.p.costes_indirectos, 2, 60.0) {
                acciones.push(Accion::Ci(n));
            }
            let mut o = self.p.opciones_ci;
            ui.checkbox(&mut o.redondear_coste_antes, "Redondear coste antes de aplicarlos");
            ui.checkbox(&mut o.aplicar_a_sin_descomponer, "Aplicar a partidas sin descomponer");
            ui.checkbox(
                &mut o.redondear_auxiliares,
                "Redondear partidas que actúan como auxiliares",
            );
            if o != self.p.opciones_ci {
                acciones.push(Accion::OpcionesCi(o));
            }
        });
        egui::Grid::new("ci")
            .striped(true)
            .num_columns(2)
            .min_col_width(220.0)
            .show(ui, |ui| {
                ui.label("Coste directo");
                ui.label(format!("{} €", eur(cd)));
                ui.end_row();
                ui.label(format!(
                    "Costes indirectos ({} % por partida)",
                    num(self.p.costes_indirectos, 2)
                ));
                ui.label(format!("{} €", eur(pem - cd)));
                ui.end_row();
                ui.label(RichText::new("PEM").strong());
                ui.label(RichText::new(format!("{} €", eur(pem))).strong());
                ui.end_row();
            });
        if !self.p.costes_indirectos.is_zero() && !cd.is_zero() {
            let margen = (pem - cd) / pem * Decimal::ONE_HUNDRED;
            ui.label(
                RichText::new(format!(
                    "Sobre el PEM, los indirectos son un {} % (margen sobre venta). Aplicados al total darían {} €; \
                     la diferencia es el redondeo por partida.",
                    num(margen, 2),
                    eur(venta::con_costes_indirectos(cd, self.p.costes_indirectos, 2))
                ))
                .weak(),
            );
        }
        ui.add_space(12.0);
        ui.label(RichText::new("Resumen del presupuesto").strong());
        egui::Grid::new("resumen_pct").num_columns(3).show(ui, |ui| {
            for (nombre, clave) in [
                ("Gastos generales %", "gg"),
                ("Beneficio industrial %", "bi"),
                ("IVA %", "iva"),
            ] {
                ui.label(nombre);
                let v = match clave {
                    "gg" => &mut self.gg,
                    "bi" => &mut self.bi,
                    _ => &mut self.iva,
                };
                if let Some(n) = self.edicion.decimal(ui, &format!("pct|{clave}"), *v, 2, 60.0) {
                    *v = n;
                }
                ui.end_row();
            }
        });
        let r = venta::resumen(pem, self.gg, self.bi, self.iva, 2);
        ui.add_space(6.0);
        egui::Grid::new("resumen")
            .striped(true)
            .num_columns(2)
            .min_col_width(220.0)
            .show(ui, |ui| {
                let filas = [
                    ("Presupuesto de ejecución material".to_string(), r.pem, false),
                    (
                        format!("{} % Gastos generales", num(self.gg, 2)),
                        r.gastos_generales,
                        false,
                    ),
                    (
                        format!("{} % Beneficio industrial", num(self.bi, 2)),
                        r.beneficio_industrial,
                        false,
                    ),
                    ("Presupuesto base".into(), r.base, true),
                    (format!("{} % IVA", num(self.iva, 2)), r.iva, false),
                    ("TOTAL".into(), r.total, true),
                ];
                for (t, v, fuerte) in filas {
                    let (a, b) = (RichText::new(t), RichText::new(format!("{} €", eur(v))));
                    if fuerte {
                        ui.label(a.strong());
                        ui.label(b.strong());
                    } else {
                        ui.label(a);
                        ui.label(b);
                    }
                    ui.end_row();
                }
            });

        ui.add_space(14.0);
        ui.label(RichText::new("Precio de venta a partir del PEM").strong());
        ui.horizontal(|ui| {
            ui.label("Margen sobre venta %");
            if let Some(n) = self.edicion.decimal(ui, "pct|margen", self.margen, 2, 60.0) {
                self.margen = n;
            }
        });
        match venta::venta_por_margen(pem, self.margen, 2) {
            Ok(pv) => {
                let rec = venta::recargo_equivalente(self.margen).unwrap_or_default();
                ui.label(format!("Venta: {} €   (beneficio {} €)", eur(pv), eur(pv - pem)));
                ui.label(
                    RichText::new(format!(
                        "Equivale a un recargo del {} % sobre coste. Un recargo del {} % daría {} €.",
                        num(rec, 2),
                        num(self.margen, 2),
                        eur(venta::venta_por_recargo(pem, self.margen, 2))
                    ))
                    .weak(),
                );
            }
            Err(e) => {
                ui.label(RichText::new(e.to_string()).color(Color32::from_rgb(220, 80, 60)));
            }
        }
    }

    fn etiqueta_partida(&self, codigo: &str) -> String {
        match self.p.conceptos.get(codigo) {
            Some(c) => format!("{} — {}", c.codigo, corto(&c.resumen, 40)),
            None => codigo.to_owned(),
        }
    }
}

impl eframe::App for Aplicacion {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let titulo = format!(
            "{}{} — {} — presupuestos-bc3",
            if self.cambios { "● " } else { "" },
            self.p.nombre,
            self.fichero
                .as_ref()
                .and_then(|f| f.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sin guardar".into())
        );
        if titulo != self.titulo {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.titulo = titulo;
        }

        let mut acciones = Vec::new();
        egui::Panel::top("barra").show(ui, |ui| {
            ui.add_space(4.0);
            self.barra_superior(ui);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("estado").show(ui, |ui| {
            ui.horizontal(|ui| match (&self.calc.error, &self.mensaje) {
                (Some(e), _) => {
                    let e = e.replace('→', "->");
                    ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(220, 80, 60)));
                }
                (None, Some((m, true))) => {
                    ui.label(RichText::new(format!("⚠ {m}")).color(Color32::from_rgb(220, 80, 60)));
                }
                (None, Some((m, false))) => {
                    ui.label(m);
                }
                _ => {}
            });
        });
        egui::Panel::left("arbol")
            .resizable(true)
            .default_size(330.0)
            .show(ui, |ui| {
                self.arbol(ui, &mut acciones);
            });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (p, t) in [
                    (Pestana::Partida, "Partida"),
                    (Pestana::Recursos, "Recursos"),
                    (Pestana::Subcontratar, "Subcontratar"),
                    (Pestana::Resumen, "Resumen y venta"),
                ] {
                    ui.selectable_value(&mut self.pestana, p, RichText::new(t).size(15.0));
                }
            });
            ui.separator();
            match self.pestana {
                Pestana::Partida => self.pestana_partida(ui, &mut acciones),
                Pestana::Recursos => self.pestana_recursos(ui, &mut acciones),
                Pestana::Subcontratar => self.pestana_subcontratar(ui, &mut acciones),
                Pestana::Resumen => self.pestana_resumen(ui, &mut acciones),
            }
        });
        let ctx = ui.ctx().clone();
        self.ventana_informe(&ctx, &mut acciones);
        self.ventana_alta(&ctx, &mut acciones);
        if !acciones.is_empty() {
            self.aplicar(acciones);
        }
    }
}

/// Opciones comunes a cualquier línea del árbol.
fn menu_linea(ui: &mut Ui, padre: &str, hijo: &str, acciones: &mut Vec<Accion>) {
    if ui.button("Subir").clicked() {
        acciones.push(Accion::Mover(padre.to_owned(), hijo.to_owned(), true));
    }
    if ui.button("Bajar").clicked() {
        acciones.push(Accion::Mover(padre.to_owned(), hijo.to_owned(), false));
    }
    ui.separator();
    if ui
        .button(format!("Quitar {hijo} de {padre}"))
        .on_hover_text("Lo que quede sin usar en ningún otro sitio se borra")
        .clicked()
    {
        acciones.push(Accion::QuitarLinea(padre.to_owned(), hijo.to_owned(), true));
    }
}

fn corto(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

fn nombre_naturaleza(n: Naturaleza) -> &'static str {
    match n {
        Naturaleza::Capitulo => "Capítulo",
        Naturaleza::Partida => "Partida",
        Naturaleza::ManoObra => "Mano de obra",
        Naturaleza::Maquinaria => "Maquinaria",
        Naturaleza::Material => "Material",
        Naturaleza::Subcontrata => "Subcontrata",
        Naturaleza::Porcentaje => "Porcentaje",
        Naturaleza::Otros => "Otros",
    }
}

fn color_naturaleza(n: Naturaleza) -> Color32 {
    match n {
        Naturaleza::ManoObra => Color32::from_rgb(90, 150, 230),
        Naturaleza::Material => Color32::from_rgb(80, 175, 110),
        Naturaleza::Maquinaria => Color32::from_rgb(200, 140, 60),
        Naturaleza::Subcontrata => Color32::from_rgb(170, 110, 210),
        Naturaleza::Porcentaje => Color32::GRAY,
        _ => Color32::LIGHT_GRAY,
    }
}

#[cfg(test)]
mod pruebas {
    //! Edición desde la interfaz (P-014): acciones y pintado sin ventana.
    use super::*;
    use eframe::egui::{Context, RawInput};

    /// Pinta árbol, pestaña Partida y ventana de alta; devuelve las acciones pedidas.
    fn pintar(app: &mut Aplicacion) -> Vec<Accion> {
        let ctx = Context::default();
        let mut acciones = Vec::new();
        for _ in 0..2 {
            let mut salida = ctx.run_ui(RawInput::default(), |ui| {
                egui::Panel::left("a").show(ui, |ui| app.arbol(ui, &mut acciones));
                egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_partida(ui, &mut acciones));
                let c = ui.ctx().clone();
                app.ventana_alta(&c, &mut acciones);
            });
            salida.textures_delta.clear();
        }
        acciones
    }

    #[test]
    fn presupuesto_nuevo_completo_desde_la_interfaz() {
        // Mismo caso que el motor: P1 = 0,5 × 20 + 2 × 3,10 + 3 % = 16,69; 12 m → 200,28 €
        let mut app = Aplicacion::sin_ventana();
        app.nuevo_presupuesto();
        assert_eq!(app.calc.pem, Decimal::ZERO);
        pintar(&mut app);
        app.aplicar(vec![Accion::NuevoCapitulo {
            padre: "OBRA".into(),
            codigo: "C01".into(),
            resumen: "Fontanería".into(),
        }]);
        app.aplicar(vec![Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P1".into(),
            unidad: "m".into(),
            resumen: "Tubería".into(),
        }]);
        assert_eq!(app.sel, Some(("C01".into(), "P1".into())));
        pintar(&mut app);
        let recurso = |codigo: &str, n, precio, q| Accion::NuevoRecurso {
            codigo: codigo.into(),
            unidad: "u".into(),
            resumen: codigo.into(),
            naturaleza: n,
            precio,
            en: "P1".into(),
            cantidad: q,
        };
        app.aplicar(vec![
            recurso("MO1", Naturaleza::ManoObra, dec!(20), dec!(0.5)),
            recurso("MT1", Naturaleza::Material, dec!(3.10), dec!(2)),
            recurso("%MA", Naturaleza::Porcentaje, dec!(0), dec!(3)),
            Accion::Rendimiento("C01".into(), "P1".into(), dec!(12)),
        ]);
        assert_eq!(app.calc.error, None);
        assert_eq!(app.calc.pem, dec!(200.28));
        assert!(app.cambios);
        pintar(&mut app);

        // Un recurso con código ya usado no se cuelga por error en lugar del nuevo
        app.aplicar(vec![recurso("MO1", Naturaleza::Material, dec!(99), dec!(1))]);
        assert!(app.mensaje.as_ref().is_some_and(|(m, e)| *e && m.contains("duplicado")));
        assert_eq!(app.calc.pem, dec!(200.28));
        // Un recurso nuevo que no puede colgarse no se queda huérfano
        app.aplicar(vec![Accion::NuevoRecurso {
            codigo: "MO9".into(),
            unidad: "h".into(),
            resumen: "x".into(),
            naturaleza: Naturaleza::ManoObra,
            precio: dec!(1),
            en: "C01".into(),
            cantidad: dec!(1),
        }]);
        assert!(!app.p.conceptos.contains_key("MO9"));

        // Quitar la partida del árbol: se borra con todo lo que solo usaba ella
        app.aplicar(vec![Accion::QuitarLinea("C01".into(), "P1".into(), true)]);
        assert_eq!(app.calc.pem, Decimal::ZERO);
        assert_eq!(app.sel, None);
        assert!(!app.p.conceptos.contains_key("MO1"));
        pintar(&mut app);
    }

    #[test]
    fn ventana_de_alta_y_lineas_del_descompuesto() {
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::Alta(TipoAlta::Linea { padre: "SR.M2".into() })]);
        assert!(app.alta.as_ref().is_some_and(|a| a.codigo == "R0001"));
        pintar(&mut app);
        assert!(app.alta.is_some(), "la ventana sigue abierta hasta confirmar");
        // Quitar y volver a poner el colector cambia el PEM y lo recupera
        app.aplicar(vec![Accion::QuitarLinea("C01".into(), "SR.COL8".into(), false)]);
        assert_eq!(app.calc.pem, dec!(6276.90));
        app.aplicar(vec![Accion::AnadirLinea("C01".into(), "SR.COL8".into(), dec!(3))]);
        assert_eq!(app.calc.pem, dec!(7005.15));
        // Mover y cambiar unidad
        let antes: Vec<_> = app.p.conceptos["SR.M2"]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect();
        app.aplicar(vec![Accion::Mover("SR.M2".into(), antes[1].clone(), true)]);
        assert_eq!(app.p.conceptos["SR.M2"].descomposicion[0].hijo, antes[1]);
        app.aplicar(vec![Accion::Unidad("SR.M2".into(), " m² ".into())]);
        assert_eq!(app.p.conceptos["SR.M2"].unidad, "m²");
        pintar(&mut app);
    }
}
