// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Biblioteca»: la base de precios propia que crece con cada obra.
//!
//! - Guardar la obra abierta (partidas descompuestas con precio y sus
//!   descuentos por fabricante).
//! - Fabricantes: tarifa vigente por año (en naranja si es de un año
//!   anterior), descuentos y subida estimada; cargar la tarifa de un año.
//! - Ofertas de fabricantes y proveedores con su documento (🖹, en la ruta
//!   que se elija en cada caso) y datos del fabricante (CIF, contacto…).
//! - Buscar partidas guardadas.
//!
//! La IA consulta la misma biblioteca (`buscar_biblioteca`) antes de estimar.

use super::asistente::abrir_en_sistema;
use super::ofertas::{NARANJA, OpDoc, boton_documento, elegir_documento};
use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText};
use ppto_biblioteca::{Biblioteca, DatosFabricante, LineaPvp, Oferta, Origen, anio, hoy};
use ppto_core::Decimal;
use ppto_core::formato::{eur, leer, num};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) struct VentanaBiblioteca {
    ruta: Option<PathBuf>,
    b: Biblioteca,
    origen: Origen,
    buscar: String,
    // Cargar tarifa
    fabricante: String,
    etiqueta: String,
    // Descuento / subida
    dto_fab: String,
    dto_pct: String,
    subida: String,
    // Nueva oferta
    of_fab: String,
    of_num: String,
    of_fecha: String,
    of_validez: String,
    of_doc: String,
    // Datos del fabricante que se editan
    datos_de: String,
    datos: DatosFabricante,
    mensaje: Option<(String, bool)>,
    // Carpeta de tarifas (documentos de cada fabricante)
    tar_carpeta: Option<PathBuf>,
    tar: ppto_biblioteca::carpeta::Carpeta,
    tar_fab: String,
    tar_fecha: String,
}

impl VentanaBiblioteca {
    /// Kits guardados (grupos de líneas): ver y borrar.
    fn seccion_kits(&mut self, ui: &mut egui::Ui) {
        let mut borrar = None;
        egui::CollapsingHeader::new(RichText::new(format!("Kits ({})", self.b.kits.len())).strong())
            .id_salt("kits_bib")
            .show(ui, |ui| {
                if self.b.kits.is_empty() {
                    ui.label(
                        RichText::new(
                            "Aún no hay kits. En el descompuesto de una partida, elija varias líneas \
                             (Mayús/Ctrl+clic), botón derecho → «Guardar como kit…».",
                        )
                        .weak(),
                    );
                    return;
                }
                egui::Grid::new("kits_bib_grid")
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        for t in ["Nombre", "Ud", "Líneas", "Precio €", "Último cambio", ""] {
                            ui.label(RichText::new(t).strong());
                        }
                        ui.end_row();
                        for (n, k) in &self.b.kits {
                            ui.label(n).on_hover_text(
                                k.lineas
                                    .lineas
                                    .iter()
                                    .map(|l| format!("{} × {}", l.linea.rendimiento.normalize(), l.linea.hijo))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                            ui.label(&k.unidad);
                            ui.label(k.lineas.lineas.len().to_string());
                            ui.label(eur(k.precio()));
                            ui.label(self.b.sellos.get(&format!("kit:{n}")).map_or(String::new(), |s| {
                                format!("{} · {}", s.quien, &s.cuando[..16.min(s.cuando.len())])
                            }));
                            if ui.small_button("Borrar").clicked() {
                                borrar = Some(n.clone());
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(n) = borrar
            && self.modificar(|b| b.kits.remove(&n)).is_some()
        {
            self.mensaje = Some((format!("Kit «{n}» borrado."), false));
        }
    }

    pub fn abrir() -> Self {
        let ruta = ppto_biblioteca::ruta();
        let mut v = Self::en(ruta);
        v.tar_carpeta = ppto_biblioteca::carpeta::ruta_carpeta();
        v.releer_tarifas();
        v
    }

    fn releer_tarifas(&mut self) {
        self.tar = self
            .tar_carpeta
            .as_deref()
            .map(ppto_biblioteca::carpeta::explorar)
            .unwrap_or_default();
    }

    /// Ventana sobre un fichero concreto (`None`: sin disco, para pruebas).
    pub fn en(ruta: Option<PathBuf>) -> Self {
        let b = ruta.as_deref().map(Biblioteca::cargar).unwrap_or_default();
        Self {
            ruta,
            b,
            origen: Origen::Revisado,
            buscar: String::new(),
            fabricante: String::new(),
            etiqueta: anio(&hoy()).to_string(),
            dto_fab: String::new(),
            dto_pct: String::new(),
            subida: String::new(),
            of_fab: String::new(),
            of_num: String::new(),
            of_fecha: hoy(),
            of_validez: String::new(),
            of_doc: String::new(),
            datos_de: String::new(),
            datos: DatosFabricante::default(),
            mensaje: None,
            tar_carpeta: None,
            tar: Default::default(),
            tar_fab: String::new(),
            tar_fecha: hoy().get(..7).unwrap_or_default().to_owned(),
        }
    }

    fn modificar<R>(&mut self, f: impl FnOnce(&mut Biblioteca) -> R) -> Option<R> {
        let Some(r) = self.ruta.clone() else {
            self.mensaje = Some(("No se encuentra la carpeta de configuración.".into(), true));
            return None;
        };
        match Biblioteca::modificar(&r, f) {
            Ok(x) => {
                self.b = Biblioteca::cargar(&r);
                Some(x)
            }
            Err(e) => {
                self.mensaje = Some((format!("No se pudo guardar la biblioteca: {e}"), true));
                None
            }
        }
    }

    fn cargar_tarifas_de_base(&mut self) {
        let etiqueta = self.etiqueta.trim().to_owned();
        let an = anio(&etiqueta);
        if an < 2000 {
            self.mensaje = Some(("Indique el año de las tarifas (p. ej. 2026).".into(), true));
            return;
        }
        let Some(r) = rfd::FileDialog::new()
            .add_filter("Base de precios del programa", &["sqlite"])
            .set_title("Base de precios (.sqlite) con las tarifas")
            .pick_file()
        else {
            return;
        };
        match super::bases::leer_sqlite(&r) {
            Ok(p) => {
                if let Some(v) = self.modificar(|b| b.cargar_tarifas_de(&p, &etiqueta, an)) {
                    self.mensaje = Some(if v.is_empty() {
                        ("Esa base no tiene materiales con tarifa y referencia.".into(), true)
                    } else {
                        (
                            format!(
                                "Tarifas {etiqueta} cargadas: {}. Con sus descuentos por familia.",
                                v.iter()
                                    .map(|(f, n)| format!("{f} ({n} ref.)"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            false,
                        )
                    });
                }
            }
            Err(e) => self.mensaje = Some((format!("No se pudo abrir la base: {e}"), true)),
        }
    }

    fn cargar_tarifa(&mut self) {
        let fab = self.fabricante.trim().to_owned();
        let etiqueta = self.etiqueta.trim().to_owned();
        let an = anio(&etiqueta);
        if fab.is_empty() || an < 2000 {
            self.mensaje = Some((
                "Indique el fabricante y el año de la tarifa (p. ej. 2027).".into(),
                true,
            ));
            return;
        }
        let Some(r) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx", "xls", "xlsm", "ods"])
            .set_title("Tarifa del fabricante (columnas Referencia, Descripción, Precio/PVP y, si las hay, Unidad)")
            .pick_file()
        else {
            return;
        };
        match ppto_tarifas::leer_excel(&r, &fab, &etiqueta) {
            Ok(t) => {
                let lineas: BTreeMap<String, LineaPvp> = t
                    .lineas
                    .into_iter()
                    .map(|l| {
                        (
                            l.referencia,
                            LineaPvp {
                                descripcion: l.descripcion,
                                unidad: l.unidad,
                                pvp: l.precio,
                                grupo: None,
                            },
                        )
                    })
                    .collect();
                if let Some(c) = self.modificar(|b| b.cargar_tarifa(&fab, &etiqueta, an, lineas)) {
                    let mut m = format!("Tarifa {fab} {etiqueta} cargada: {} referencias.", c.lineas);
                    if let Some(media) = c.media {
                        m.push_str(&format!(
                            " Frente a la anterior: {} % de media, {} nuevas, {} desaparecidas.",
                            num(media, 2),
                            c.nuevas.len(),
                            c.desaparecidas.len()
                        ));
                    }
                    self.mensaje = Some((m, false));
                }
            }
            Err(e) => self.mensaje = Some((e.to_string(), true)),
        }
    }
}

impl Aplicacion {
    pub(super) fn ventana_biblioteca(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_biblioteca.take() else {
            return;
        };
        let mut abierta = true;
        let actual = anio(&hoy());
        egui::Window::new("Biblioteca de precios")
            .open(&mut abierta)
            .default_width(820.0)
            .default_height(600.0)
            .resizable(true)
            .show(ctx, |ui| {
                // Carpeta compartida (dos o más personas actualizando precios)
                ui.horizontal(|ui| {
                    match &v.ruta {
                        Some(r) => ui.label(RichText::new(format!("Fichero: {}", r.display())).small().weak()),
                        None => ui.label(""),
                    };
                    if ui
                        .small_button("Compartir en una carpeta…")
                        .on_hover_text(
                            "Elija una carpeta compartida (OneDrive, Teams, red). Cada persona guarda sus cambios en \
                             su propio fichero y el programa los junta: de cada precio, gana el cambio más reciente.",
                        )
                        .clicked()
                        && let Some(c) = rfd::FileDialog::new().set_title("Carpeta compartida de la biblioteca").pick_folder()
                    {
                        match ppto_biblioteca::compartir_en(Some(&c)) {
                            Ok(r) => {
                                v.b = Biblioteca::cargar(&r);
                                v.ruta = Some(r.clone());
                                self.ruta_biblioteca = Some(r);
                                v.mensaje = Some((format!("Biblioteca compartida en {}.", c.display()), false));
                            }
                            Err(e) => v.mensaje = Some((format!("No se pudo compartir: {e}"), true)),
                        }
                    }
                });
                if let Some(r) = &v.ruta {
                    let otros: Vec<String> = ppto_biblioteca::ficheros(r)
                        .iter()
                        .skip(1)
                        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .collect();
                    if !otros.is_empty() {
                        ui.label(
                            RichText::new(format!("Junta también: {} (gana el cambio más reciente).", otros.join(", ")))
                                .small()
                                .weak(),
                        );
                    }
                }
                self.carpeta_tarifas(ui, &mut v);
                // Guardar la obra
                ui.horizontal(|ui| {
                    ui.label(format!("{} partidas guardadas.", v.b.partidas.len()));
                    ui.separator();
                    ui.label("Origen de los precios:");
                    for (o, t) in [(Origen::Real, "real"), (Origen::Revisado, "revisado"), (Origen::Estimado, "estimado")] {
                        ui.radio_value(&mut v.origen, o, t);
                    }
                    if ui
                        .button("Guardar esta obra en la biblioteca")
                        .on_hover_text("Partidas descompuestas con precio y descuentos por fabricante. No duplica las que no cambian.")
                        .clicked()
                    {
                        let obra = ppto_propuestas::clave(&self.p);
                        let origen = v.origen;
                        let p = &self.p;
                        if let Some(i) = v.modificar(|b| b.guardar_obra(p, &obra, origen, None)) {
                            v.mensaje = Some((
                                format!(
                                    "Guardadas: {} nuevas, {} con versión nueva, {} sin cambios.",
                                    i.nuevas.len(),
                                    i.actualizadas.len(),
                                    i.iguales
                                ),
                                false,
                            ));
                        }
                    }
                });
                if let Some((m, error)) = &v.mensaje {
                    let c = if *error { Color32::from_rgb(220, 80, 60) } else { Color32::from_rgb(80, 175, 110) };
                    ui.label(RichText::new(m).color(c));
                }
                ui.separator();

                // Fabricantes
                ui.heading("Fabricantes");
                let antiguas = v.b.tarifas_antiguas(actual);
                if !antiguas.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Tarifas de años anteriores: {}. Cargue la nueva o ponga una subida estimada.",
                            antiguas.iter().map(|(f, e, _)| format!("{f} {e}")).collect::<Vec<_>>().join(", ")
                        ))
                        .color(Color32::from_rgb(220, 150, 40)),
                    );
                }
                let mut a_tarifa: Option<String> = None;
                egui::Grid::new("fabricantes_bib").striped(true).num_columns(8).show(ui, |ui| {
                    for t in ["Fabricante", "Tarifa vigente", "Refs", "Descuentos", "Ofertas", "Subida est.", "Último cambio", ""] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for f in v.b.fabricantes.values() {
                        ui.label(&f.nombre);
                        match f.vigente() {
                            Some(t) if t.anio < actual => {
                                ui.label(RichText::new(format!("{} (antigua)", t.etiqueta)).color(Color32::from_rgb(220, 150, 40)))
                            }
                            Some(t) => ui.label(&t.etiqueta),
                            None => ui.label("—"),
                        };
                        ui.label(f.vigente().map_or(0, |t| t.lineas.len()).to_string());
                        ui.label(
                            f.descuentos
                                .iter()
                                .map(|(g, p)| if g.is_empty() { format!("{} %", num(*p, 2)) } else { format!("{g}: {} %", num(*p, 2)) })
                                .collect::<Vec<_>>()
                                .join(", "),
                        );
                        ui.label(f.ofertas.len().to_string());
                        ui.label(f.subida_estimada.map_or("—".into(), |s| format!("{} %", num(s, 2))));
                        ui.label(f.sellos.values().max().map_or("—".into(), |s| format!("{} · {}", s.quien, s.cuando)));
                        if f.vigente().is_some() {
                            if ui
                                .small_button("Pasar la obra a esta tarifa")
                                .on_hover_text(
                                    "Cambia el PVP de los recursos de este fabricante por el de la tarifa vigente, \
                                     buscando cada uno por su referencia. Los descuentos por familia no cambian.",
                                )
                                .clicked()
                            {
                                a_tarifa = Some(f.nombre.clone());
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
                if let Some(fab) = a_tarifa {
                    match v.b.pvps_vigentes(&fab, actual) {
                        Some((etiqueta, _, pvps)) => self.aplicar(vec![Accion::TarifaNueva(fab, etiqueta, pvps)]),
                        None => v.mensaje = Some((format!("{fab} no tiene tarifa en la biblioteca."), true)),
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Cargar tarifa: fabricante");
                    ui.add(egui::TextEdit::singleline(&mut v.fabricante).desired_width(110.0));
                    ui.label("año");
                    ui.add(egui::TextEdit::singleline(&mut v.etiqueta).desired_width(60.0));
                    if ui.button("Elegir Excel…").clicked() {
                        v.cargar_tarifa();
                    }
                    ui.separator();
                    if ui
                        .button("Tarifas de una base (.sqlite)…")
                        .on_hover_text(
                            "Carga todas las tarifas que lleva una base de precios del programa: por fabricante, \
                             referencias, PVP y familias, y sus descuentos (también encadenados). Con el año de la izquierda.",
                        )
                        .clicked()
                    {
                        v.cargar_tarifas_de_base();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Descuento: fabricante (o FAB/GRUPO)");
                    ui.add(egui::TextEdit::singleline(&mut v.dto_fab).desired_width(110.0));
                    ui.label("%");
                    ui.add(egui::TextEdit::singleline(&mut v.dto_pct).desired_width(50.0));
                    ui.label("subida estimada %");
                    ui.add(egui::TextEdit::singleline(&mut v.subida).desired_width(50.0));
                    if ui.button("Guardar").clicked() {
                        let fab = v.dto_fab.trim().to_owned();
                        let pct = ppto_core::descuentos::leer_cadena(&v.dto_pct).ok();
                        let sub: Option<Decimal> = leer(&v.subida);
                        if fab.is_empty() || (pct.is_none() && sub.is_none()) {
                            v.mensaje = Some(("Indique el fabricante y el descuento o la subida.".into(), true));
                        } else if v
                            .modificar(|b| {
                                if pct.is_some() {
                                    b.fijar_descuento_encadenado(&fab, pct.as_deref());
                                }
                                if sub.is_some() {
                                    let (f, _) = ppto_core::descuentos::partes(&fab);
                                    b.fijar_subida(&f, sub);
                                }
                            })
                            .is_some()
                        {
                            v.mensaje = Some((format!("Guardado para {fab}."), false));
                        }
                    }
                });
                ui.separator();
                v.seccion_ofertas(ui);
                ui.separator();
                v.seccion_kits(ui);
                ui.separator();

                // Buscar partidas
                ui.horizontal(|ui| {
                    ui.heading("Partidas");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.buscar)
                            .hint_text("buscar: tubo ppr 32, rejilla koolair…")
                            .desired_width(280.0),
                    );
                });
                let res = v.b.buscar(&v.buscar, 100);
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("partidas_bib").striped(true).num_columns(6).show(ui, |ui| {
                        for t in ["Código", "Ud", "Resumen", "Precio €", "Fecha", "Obra · origen"] {
                            ui.label(RichText::new(t).strong());
                        }
                        ui.end_row();
                        for (c, p) in &res {
                            ui.label(*c);
                            ui.label(&p.unidad);
                            ui.label(&p.resumen);
                            ui.label(eur(p.precio));
                            ui.label(&p.fecha);
                            ui.label(format!("{} · {}", p.obra, p.origen.texto()));
                            ui.end_row();
                        }
                    });
                    if !v.buscar.trim().is_empty() && res.is_empty() {
                        ui.label(RichText::new("Sin resultados.").weak());
                    }
                });
            });
        if abierta {
            self.ventana_biblioteca = Some(v);
        }
    }
}

impl VentanaBiblioteca {
    /// Ofertas con su documento y datos de cada fabricante o proveedor.
    fn seccion_ofertas(&mut self, ui: &mut egui::Ui) {
        let n: usize = self.b.fabricantes.values().map(|f| f.ofertas.len()).sum();
        egui::CollapsingHeader::new(RichText::new(format!("Ofertas ({n}) y datos de fabricantes")).strong())
            .id_salt("ofertas_bib")
            .show(ui, |ui| {
                let hoy = hoy();
                let mut op: Option<(String, String, OpDoc, Option<String>)> = None;
                egui::ScrollArea::vertical()
                    .id_salt("ofertas_scroll")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        egui::Grid::new("ofertas_bib_grid")
                            .striped(true)
                            .num_columns(7)
                            .show(ui, |ui| {
                                for t in ["", "Fabricante", "Nº", "Fecha", "Válida hasta", "Líneas", "Obra"] {
                                    ui.label(RichText::new(t).strong());
                                }
                                ui.end_row();
                                for (k, f) in &self.b.fabricantes {
                                    for o in &f.ofertas {
                                        let existe = o
                                            .documento
                                            .as_deref()
                                            .is_some_and(|d| std::path::Path::new(d).is_file());
                                        if let Some(x) = boton_documento(ui, o.documento.as_deref(), existe) {
                                            op = Some((k.clone(), o.numero.clone(), x, o.documento.clone()));
                                        }
                                        ui.label(&f.nombre);
                                        ui.label(&o.numero);
                                        ui.label(&o.fecha);
                                        let val = o.validez.clone().unwrap_or_default();
                                        if o.vencida(&hoy) {
                                            ui.label(RichText::new(format!("{val} (vencida)")).color(NARANJA));
                                        } else {
                                            ui.label(val);
                                        }
                                        ui.label(o.lineas.len().to_string());
                                        ui.label(&o.obra);
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                if let Some((k, num, x, doc)) = op {
                    match x {
                        OpDoc::Abrir => {
                            if let Some(d) = doc {
                                abrir_en_sistema(std::path::Path::new(&d));
                            }
                        }
                        OpDoc::Vincular => {
                            if let Some(d) = elegir_documento(doc.as_deref()) {
                                self.modificar(|b| b.fijar_documento_oferta(&k, &num, Some(d)));
                            }
                        }
                        OpDoc::Quitar => {
                            self.modificar(|b| b.fijar_documento_oferta(&k, &num, None));
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Nueva oferta (las líneas las puede leer la IA del PDF)").weak());
                ui.horizontal_wrapped(|ui| {
                    ui.label("Fabricante");
                    ui.add(egui::TextEdit::singleline(&mut self.of_fab).desired_width(100.0));
                    ui.label("Nº");
                    ui.add(egui::TextEdit::singleline(&mut self.of_num).desired_width(80.0));
                    ui.label("Fecha");
                    ui.add(egui::TextEdit::singleline(&mut self.of_fecha).desired_width(80.0));
                    ui.label("Válida hasta");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.of_validez)
                            .hint_text("AAAA-MM-DD")
                            .desired_width(80.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Documento");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.of_doc)
                            .hint_text("ruta del PDF, donde esté")
                            .desired_width(380.0),
                    );
                    if ui.button("Elegir…").clicked()
                        && let Some(d) = elegir_documento(Some(self.of_doc.as_str()).filter(|d| !d.is_empty()))
                    {
                        self.of_doc = d;
                    }
                    if ui.button("Añadir oferta").clicked() {
                        let fab = self.of_fab.trim().to_owned();
                        if fab.is_empty() || self.of_num.trim().is_empty() {
                            self.mensaje = Some(("Indique el fabricante y el número de la oferta.".into(), true));
                        } else {
                            let o = Oferta {
                                numero: self.of_num.trim().to_owned(),
                                fecha: self.of_fecha.trim().to_owned(),
                                validez: Some(self.of_validez.trim().to_owned()).filter(|v| !v.is_empty()),
                                obra: String::new(),
                                lineas: Vec::new(),
                                documento: Some(self.of_doc.trim().trim_matches('"').to_owned())
                                    .filter(|d| !d.is_empty()),
                            };
                            let t = format!("Oferta {fab} {} añadida.", o.numero);
                            if self.modificar(|b| b.anadir_oferta(&fab, o)).is_some() {
                                self.mensaje = Some((t, false));
                                self.of_num.clear();
                                self.of_doc.clear();
                            }
                        }
                    }
                });
                ui.separator();
                // Datos del fabricante
                ui.horizontal(|ui| {
                    ui.label("Datos de");
                    let mut elegir = None;
                    egui::ComboBox::from_id_salt("datos_fab")
                        .selected_text(if self.datos_de.is_empty() {
                            "fabricante…"
                        } else {
                            self.datos_de.as_str()
                        })
                        .show_ui(ui, |ui| {
                            for (k, f) in &self.b.fabricantes {
                                if ui.selectable_label(*k == self.datos_de, &f.nombre).clicked() {
                                    elegir = Some(k.clone());
                                }
                            }
                        });
                    if let Some(k) = elegir {
                        self.datos = self.b.fabricante(&k).map(|f| f.datos.clone()).unwrap_or_default();
                        self.datos_de = k;
                    }
                });
                if !self.datos_de.is_empty() {
                    let d = &mut self.datos;
                    egui::Grid::new("datos_fab_grid").num_columns(4).show(ui, |ui| {
                        ui.label("Razón social");
                        ui.text_edit_singleline(&mut d.razon_social);
                        ui.label("CIF");
                        ui.text_edit_singleline(&mut d.cif);
                        ui.end_row();
                        ui.label("Contacto");
                        ui.text_edit_singleline(&mut d.contacto);
                        ui.label("Teléfono");
                        ui.text_edit_singleline(&mut d.telefono);
                        ui.end_row();
                        ui.label("Email");
                        ui.text_edit_singleline(&mut d.email);
                        ui.end_row();
                    });
                    ui.label("Condiciones (plazo, portes, forma de pago…)");
                    ui.add(
                        egui::TextEdit::multiline(&mut d.condiciones)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if ui.button("Guardar datos").clicked() {
                        let (k, d) = (self.datos_de.clone(), self.datos.clone());
                        if self.modificar(|b| b.fijar_datos(&k, d)).is_some() {
                            self.mensaje = Some((format!("Datos de {k} guardados."), false));
                        }
                    }
                }
            });
    }
}

impl Aplicacion {
    /// Carpeta de tarifas: dónde está, cómo deben estar los ficheros, qué
    /// tarifa está vigente de cada fabricante y «Añadir tarifa…».
    fn carpeta_tarifas(&mut self, ui: &mut egui::Ui, v: &mut VentanaBiblioteca) {
        use ppto_biblioteca::carpeta;
        egui::CollapsingHeader::new(RichText::new("Carpeta de tarifas (documentos de cada fabricante)").strong())
            .default_open(v.tar_carpeta.is_none())
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match &v.tar_carpeta {
                        Some(c) => {
                            ui.label(format!("Carpeta: {}", c.display()));
                            if ui.small_button("Abrir").clicked() {
                                super::asistente::abrir_en_sistema(c);
                            }
                        }
                        None => {
                            ui.label(
                                RichText::new(
                                    "Elija la carpeta compartida donde estarán las tarifas (p. ej. OneDrive\\Tarifas).",
                                )
                                .color(NARANJA),
                            );
                        }
                    }
                    if ui.small_button("Elegir carpeta…").clicked()
                        && let Some(c) = rfd::FileDialog::new().set_title("Carpeta de tarifas").pick_folder()
                    {
                        match carpeta::fijar_carpeta(&c) {
                            Ok(()) => {
                                v.tar_carpeta = Some(c);
                                v.releer_tarifas();
                            }
                            Err(e) => v.mensaje = Some((format!("No se pudo guardar la carpeta: {e}"), true)),
                        }
                    }
                    if v.tar_carpeta.is_some() && ui.small_button("⟳ Releer").clicked() {
                        v.releer_tarifas();
                    }
                });
                egui::CollapsingHeader::new("¿Cómo deben estar los ficheros?")
                    .default_open(v.tar_carpeta.is_none() || v.tar.fabricantes.is_empty())
                    .show(ui, |ui| {
                        ui.label(RichText::new(carpeta::NORMAS).small());
                        ui.label(
                            RichText::new("Ejemplo:  Tarifas\\KOOLAIR\\2026-05 Tarifa KOOLAIR.pdf")
                                .small()
                                .monospace(),
                        );
                    });
                let Some(c) = v.tar_carpeta.clone() else { return };
                // Añadir tarifa
                ui.horizontal(|ui| {
                    ui.label("Fabricante:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.tar_fab)
                            .desired_width(160.0)
                            .hint_text("KOOLAIR"),
                    );
                    ui.label("Fecha de la tarifa:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.tar_fecha)
                            .desired_width(70.0)
                            .hint_text("AAAA-MM"),
                    );
                    let listo = !v.tar_fab.trim().is_empty() && carpeta::fecha_del_nombre(v.tar_fecha.trim()).is_some();
                    if ui
                        .add_enabled(listo, egui::Button::new("Añadir tarifa…"))
                        .on_hover_text(
                            "Elija el PDF o Excel del fabricante: se copia a su carpeta con el nombre correcto",
                        )
                        .on_disabled_hover_text("Escriba el fabricante y la fecha de la tarifa (AAAA-MM)")
                        .clicked()
                        && let Some(f) = rfd::FileDialog::new()
                            .set_title(format!("Tarifa de {}", v.tar_fab.trim()))
                            .add_filter("Tarifa (PDF o Excel)", &carpeta::EXTENSIONES)
                            .pick_file()
                    {
                        match carpeta::anadir(&c, &v.tar_fab, &v.tar_fecha, &f) {
                            Ok(r) => {
                                v.mensaje = Some((format!("Tarifa añadida: {}", r.display()), false));
                                v.releer_tarifas();
                            }
                            Err(e) => v.mensaje = Some((format!("No se pudo añadir: {e}"), true)),
                        }
                    }
                });
                // Lo que hay
                if v.tar.fabricantes.is_empty() {
                    ui.label(RichText::new("Todavía no hay tarifas en la carpeta.").weak());
                } else {
                    egui::Grid::new("carpeta_tarifas")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            for t in ["Fabricante", "Vigente", "Anteriores", ""] {
                                ui.label(RichText::new(t).strong());
                            }
                            ui.end_row();
                            for (fab, docs) in &v.tar.fabricantes {
                                ui.label(fab);
                                let d = &docs[0];
                                let nombre = d
                                    .ruta
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                ui.label(nombre);
                                ui.label(if docs.len() > 1 {
                                    (docs.len() - 1).to_string()
                                } else {
                                    "—".into()
                                });
                                if ui.small_button("🖹 Abrir").clicked() {
                                    super::asistente::abrir_en_sistema(&d.ruta);
                                }
                                ui.end_row();
                            }
                        });
                }
                if !v.tar.mal.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "{} ficheros no siguen las normas (no se usan):",
                            v.tar.mal.len()
                        ))
                        .color(NARANJA),
                    );
                    for (p, motivo) in v.tar.mal.iter().take(12) {
                        let rel = p.strip_prefix(&c).unwrap_or(p);
                        ui.label(
                            RichText::new(format!("  • {} — {motivo}", rel.display()))
                                .small()
                                .color(NARANJA),
                        );
                    }
                }
            });
        ui.separator();
    }
}
