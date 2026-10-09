// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana principal: árbol de capítulos, partida (descompuesto y mediciones),
//! recursos, subcontratación y resumen.

use crate::celdas::Edicion;
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
}

/// Resultados del motor para el estado actual del presupuesto.
#[derive(Default)]
struct Calculo {
    error: Option<String>,
    pem: Decimal,
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
            for codigo in p.conceptos.keys() {
                c.precios.insert(codigo.clone(), p.precio(codigo)?);
                if !p.conceptos[codigo].descomposicion.is_empty() {
                    c.lineas.insert(codigo.clone(), p.lineas_valoradas(codigo)?);
                }
            }
            c.pem = p.pem()?;
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
}

impl Aplicacion {
    pub fn new(cc: &eframe::CreationContext<'_>, inicial: Option<PathBuf>) -> Self {
        let mut estilo = (*cc.egui_ctx.global_style()).clone();
        estilo.spacing.item_spacing = egui::vec2(8.0, 5.0);
        cc.egui_ctx.set_global_style(estilo);
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
        };
        app.recalcular();
        // Pestaña inicial (útil para capturas y pruebas): PPTO_GUI_PESTANA=recursos|subcontratar|resumen
        app.pestana = match std::env::var("PPTO_GUI_PESTANA").as_deref() {
            Ok("recursos") => Pestana::Recursos,
            Ok("subcontratar") => Pestana::Subcontratar,
            Ok("resumen") => Pestana::Resumen,
            _ => Pestana::Partida,
        };
        if let Some(ruta) = inicial {
            app.abrir(ruta);
        }
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
            if ui.button("📂 Abrir…").clicked() {
                self.dialogo_abrir();
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
                ui.label(RichText::new("● cambios sin guardar").color(Color32::from_rgb(220, 150, 40)));
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
                egui::CollapsingHeader::new(RichText::new(format!("{}  {}", c.codigo, c.resumen)).strong())
                    .id_salt(("cap", capitulo, &l.hijo))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("{} €", eur(l.importe))).weak());
                        self.rama(ui, &l.hijo, acciones);
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
            ui.label(RichText::new(&c.unidad).weak());
            let mut resumen = c.resumen.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut resumen).desired_width(480.0))
                .changed()
            {
                acciones.push(Accion::Resumen(c.codigo.clone(), resumen));
            }
        });
        if let Some(l) = &linea_cap {
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
            ui.label(RichText::new("Precio descompuesto").strong());
            if let Some(lineas) = self.calc.lineas.get(&hijo).cloned() {
                egui::Grid::new("descompuesto")
                    .striped(true)
                    .num_columns(7)
                    .show(ui, |ui| {
                        for t in ["Código", "Ud", "Resumen", "Tipo", "Cantidad", "Precio", "Importe"] {
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
            ui.label(format!("Total explosión: {} €     PEM: {} €", eur(e.total), eur(e.pem)));
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
                            ui.label(eur(r.pem_escenario));
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

    fn pestana_resumen(&mut self, ui: &mut Ui) {
        let pem = self.calc.pem;
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
        ui.label(RichText::new("Precio de venta a partir del coste (PEM)").strong());
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
                Pestana::Resumen => self.pestana_resumen(ui),
            }
        });
        if !acciones.is_empty() {
            self.aplicar(acciones);
        }
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
