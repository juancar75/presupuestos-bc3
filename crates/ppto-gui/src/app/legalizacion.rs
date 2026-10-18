// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Legalización»: costes de tramitación, visitas de la OCA,
//! justificaciones RITE/RSIF, desplazamiento y repercutidos (tasas, EICI,
//! visado) como partidas descompuestas en el capítulo `LEG`.
//!
//! El baremo (honorarios y tablas por año) está en `legalizacion.json`, en la
//! carpeta de configuración, fuera del repositorio.

use super::Aplicacion;
use eframe::egui::{self, Color32, RichText};
use ppto_core::Decimal;
use ppto_core::formato::{eur, leer, num};
use ppto_legalizacion::{
    Baremo, Baremos, Calculo, Disciplina, Documento, Expediente, RegistroIf, Tramo, Uso, aplicar, calcular, capitulos,
};
use std::path::PathBuf;

/// Honorarios propios editables en la ventana (las tablas oficiales, en el JSON).
const CAMPOS: [&str; 14] = [
    "Mínimo de tramitación (€)",
    "Mínimo por vivienda (€)",
    "Recargo proyecto ajeno (%)",
    "Visita de la OCA (€)",
    "Segunda visita (€)",
    "Justificación RITE (% PEM)",
    "Justificación RITE mínimo (€)",
    "RSIF por circuito (€)",
    "RSIF mínimo (€)",
    "Kilometraje (€/km)",
    "Umbral desplazamiento (km)",
    "Hora de viaje (€/h)",
    "Hora de referencia (€/h)",
    "EICI 2ª visita (€)",
];

fn campo(b: &mut Baremo, i: usize) -> &mut Decimal {
    match i {
        0 => &mut b.tramitacion_minimo,
        1 => &mut b.minimo_vivienda,
        2 => &mut b.recargo_ajeno,
        3 => &mut b.visita_oca,
        4 => &mut b.segunda_visita,
        5 => &mut b.justif_rite_pct,
        6 => &mut b.justif_rite_minimo,
        7 => &mut b.rsif_circuito,
        8 => &mut b.rsif_minimo,
        9 => &mut b.km,
        10 => &mut b.km_umbral,
        11 => &mut b.hora_viaje,
        12 => &mut b.hora_referencia,
        _ => &mut b.eici_segunda_visita,
    }
}

fn texto(v: Decimal) -> String {
    num(v.normalize(), v.normalize().scale())
}

pub(super) struct VentanaLegalizacion {
    ruta: Option<PathBuf>,
    baremos: Baremos,
    anio: i32,
    error_baremo: Option<String>,
    e: Expediente,
    // Base del PEM: capítulos de primer nivel con su importe y si cuentan.
    capitulos: Vec<(String, String, Decimal, bool)>,
    pem_manual: String,
    km: String,
    horas: String,
    potencia: String,
    superficie: String,
    // Edición del baremo
    editar: bool,
    campos: Vec<String>,
    tramos: Vec<(String, String)>,
    mensaje: Option<(String, bool)>,
}

impl VentanaLegalizacion {
    pub fn abrir(p: &ppto_core::Presupuesto) -> Self {
        let ruta = ppto_legalizacion::ruta();
        let anio = ppto_biblioteca::anio(&ppto_biblioteca::hoy());
        let mut v = Self {
            ruta,
            baremos: Baremos::default(),
            anio,
            error_baremo: None,
            e: Expediente::nuevo(Disciplina::Rite, Documento::Memoria, Decimal::ZERO),
            capitulos: Vec::new(),
            pem_manual: String::new(),
            km: String::new(),
            horas: String::new(),
            potencia: String::new(),
            superficie: String::new(),
            editar: false,
            campos: Vec::new(),
            tramos: Vec::new(),
            mensaje: None,
        };
        v.recargar();
        v.leer_capitulos(p);
        v
    }

    fn recargar(&mut self) {
        self.error_baremo = None;
        self.baremos = match self.ruta.as_deref().map(Baremos::cargar) {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                self.error_baremo = Some(e.to_string());
                Baremos::default()
            }
            None => Baremos::default(),
        };
        self.cargar_edicion();
    }

    fn cargar_edicion(&mut self) {
        let mut b = self.baremo().clone();
        self.campos = (0..CAMPOS.len()).map(|i| texto(*campo(&mut b, i))).collect();
        self.tramos = b
            .tramitacion
            .iter()
            .map(|t| (t.hasta.map(texto).unwrap_or_default(), texto(t.valor)))
            .collect();
    }

    fn leer_capitulos(&mut self, p: &ppto_core::Presupuesto) {
        match capitulos(p) {
            Ok(c) => self.capitulos = c.into_iter().map(|(a, b, i)| (a, b, i, true)).collect(),
            Err(e) => self.mensaje = Some((format!("No se pudo valorar la obra: {e}"), true)),
        }
    }

    fn baremo(&self) -> Baremo {
        self.baremos.vigente(self.anio).cloned().unwrap_or_default()
    }

    fn pem(&self) -> Decimal {
        leer(&self.pem_manual).unwrap_or_else(|| self.capitulos.iter().filter(|c| c.3).map(|c| c.2).sum())
    }

    fn expediente(&self) -> Expediente {
        let mut e = self.e.clone();
        e.pem = self.pem();
        e.km = leer(&self.km).unwrap_or_default();
        e.horas_viaje = leer(&self.horas).unwrap_or_default();
        e.potencia_kw = leer(&self.potencia).unwrap_or_default();
        e.superficie = leer(&self.superficie).unwrap_or_default();
        if e.disciplina == Disciplina::Pci {
            e.justificar_rite = false;
        }
        e
    }

    fn guardar_baremo(&mut self) {
        let Some(r) = self.ruta.clone() else {
            self.mensaje = Some(("No se encuentra la carpeta de configuración.".into(), true));
            return;
        };
        let mut valores = Vec::new();
        for (i, t) in self.campos.iter().enumerate() {
            match leer(t) {
                Some(v) => valores.push(v),
                None => {
                    self.mensaje = Some((format!("Revise «{}».", CAMPOS[i]), true));
                    return;
                }
            }
        }
        let mut tramos = Vec::new();
        for (h, p) in &self.tramos {
            let Some(valor) = leer(p) else {
                self.mensaje = Some(("Revise los % de los tramos de tramitación.".into(), true));
                return;
            };
            tramos.push(Tramo { hasta: leer(h), valor });
        }
        // Se relee el fichero para no pisar cambios hechos desde otro equipo.
        let mut bs = match Baremos::cargar(&r) {
            Ok(b) => b,
            Err(e) => {
                self.mensaje = Some((e.to_string(), true));
                return;
            }
        };
        let b = bs.del_anio(self.anio);
        for (i, v) in valores.into_iter().enumerate() {
            *campo(b, i) = v;
        }
        b.tramitacion = tramos;
        match bs.guardar(&r) {
            Ok(()) => {
                self.baremos = bs;
                self.mensaje = Some((format!("Baremo {} guardado.", self.anio), false));
            }
            Err(e) => self.mensaje = Some((format!("No se pudo guardar el baremo: {e}"), true)),
        }
    }

    fn crear_plantilla(&mut self) {
        let Some(r) = self.ruta.clone() else {
            return;
        };
        let bs = Baremos {
            versiones: vec![Baremo::ejemplo(self.anio)],
        };
        match bs.guardar(&r) {
            Ok(()) => {
                self.recargar();
                self.mensaje = Some((
                    "Plantilla creada con valores de EJEMPLO. Sustitúyalos por los suyos (aquí o en el fichero)."
                        .into(),
                    false,
                ));
            }
            Err(e) => self.mensaje = Some((e.to_string(), true)),
        }
    }
}

fn caja(ui: &mut egui::Ui, t: &mut String, ancho: f32) {
    ui.add(egui::TextEdit::singleline(t).desired_width(ancho));
}

fn numero(ui: &mut egui::Ui, n: &mut u32) {
    ui.add(egui::DragValue::new(n).range(0..=9999));
}

impl Aplicacion {
    pub(super) fn ventana_legalizacion(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_legalizacion.take() else {
            return;
        };
        let mut abierta = true;
        let mut crear: Option<(Disciplina, Calculo)> = None;
        egui::Window::new("Legalización")
            .open(&mut abierta)
            .default_width(860.0)
            .default_height(640.0)
            .resizable(true)
            .show(ctx, |ui| {
                let baremo = v.baremo();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Baremo
                    ui.horizontal(|ui| {
                        ui.label("Baremo del año");
                        ui.add(egui::DragValue::new(&mut v.anio).range(2000..=2100));
                        if baremo.anio == 0 {
                            ui.label(RichText::new("sin baremo").color(Color32::from_rgb(220, 80, 60)));
                        } else if baremo.anio != v.anio {
                            ui.label(
                                RichText::new(format!("se usa el de {}", baremo.anio))
                                    .color(Color32::from_rgb(220, 150, 40)),
                            );
                        }
                        if ui.button("Recargar").clicked() {
                            v.recargar();
                        }
                        ui.toggle_value(&mut v.editar, "Editar honorarios");
                        if let Some(r) = &v.ruta
                            && ui
                                .button("Abrir carpeta")
                                .on_hover_text(
                                    "Las tablas oficiales (tasas, EICI, visado) se editan en legalizacion.json",
                                )
                                .clicked()
                        {
                            super::asistente::abrir_en_sistema(r.parent().unwrap_or(r));
                        }
                    });
                    if !baremo.nota.is_empty() {
                        ui.label(RichText::new(&baremo.nota).small().weak());
                    }
                    if let Some(e) = &v.error_baremo {
                        ui.label(RichText::new(e).color(Color32::from_rgb(220, 80, 60)));
                    } else if v.baremos.versiones.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("No hay baremo en legalizacion.json.")
                                    .color(Color32::from_rgb(220, 150, 40)),
                            );
                            if ui.button("Crear plantilla de ejemplo").clicked() {
                                v.crear_plantilla();
                            }
                        });
                    }
                    if v.editar {
                        ui.group(|ui| {
                            ui.label(RichText::new(format!("Honorarios {}", v.anio)).strong());
                            egui::Grid::new("leg_campos").num_columns(4).show(ui, |ui| {
                                for (i, (rotulo, t)) in CAMPOS.iter().zip(v.campos.iter_mut()).enumerate() {
                                    ui.label(*rotulo);
                                    caja(ui, t, 70.0);
                                    if i % 2 == 1 {
                                        ui.end_row();
                                    }
                                }
                            });
                            ui.label("Tramitación por tramos acumulativos (hasta € · %; el último sin límite):");
                            let mut quitar = None;
                            for (i, (h, p)) in v.tramos.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label("hasta");
                                    ui.add(
                                        egui::TextEdit::singleline(h)
                                            .hint_text("sin límite")
                                            .desired_width(90.0),
                                    );
                                    ui.label("€ ·");
                                    caja(ui, p, 50.0);
                                    ui.label("%");
                                    if ui.small_button("✕").clicked() {
                                        quitar = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = quitar {
                                v.tramos.remove(i);
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Añadir tramo").clicked() {
                                    v.tramos.push((String::new(), String::new()));
                                }
                                if ui.button(format!("Guardar baremo {}", v.anio)).clicked() {
                                    v.guardar_baremo();
                                }
                            });
                        });
                    }
                    ui.separator();

                    // Expediente
                    ui.horizontal(|ui| {
                        ui.label("Disciplina:");
                        ui.radio_value(&mut v.e.disciplina, Disciplina::Rite, "RITE");
                        ui.radio_value(&mut v.e.disciplina, Disciplina::Pci, "PCI");
                        ui.separator();
                        ui.radio_value(&mut v.e.documento, Documento::Memoria, "Memoria técnica");
                        ui.radio_value(&mut v.e.documento, Documento::Proyecto, "Proyecto");
                        ui.separator();
                        ui.checkbox(&mut v.e.ajeno, "Proyecto de otro técnico");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Uso:");
                        ui.radio_value(&mut v.e.uso, Uso::Terciario, "Terciario");
                        ui.radio_value(&mut v.e.uso, Uso::Viviendas, "Viviendas");
                        if v.e.uso == Uso::Viviendas {
                            ui.label("nº de viviendas");
                            numero(ui, &mut v.e.viviendas);
                        }
                    });
                    ui.collapsing(format!("PEM de la disciplina: {} €", eur(v.pem())), |ui| {
                        ui.label("Marque los capítulos que forman la base (o escriba el PEM a mano):");
                        for c in &mut v.capitulos {
                            ui.checkbox(&mut c.3, format!("{} {} — {} €", c.0, c.1, eur(c.2)));
                        }
                        ui.horizontal(|ui| {
                            ui.label("PEM a mano");
                            caja(ui, &mut v.pem_manual, 100.0);
                        });
                    });
                    egui::Grid::new("leg_datos").num_columns(4).show(ui, |ui| {
                        match v.e.disciplina {
                            Disciplina::Rite => {
                                ui.label("Potencia (kW)");
                                caja(ui, &mut v.potencia, 70.0);
                                ui.end_row();
                                ui.checkbox(&mut v.e.primario_frio, "Primario frigorífico");
                                ui.checkbox(&mut v.e.justificar_rite, "Justificación RITE");
                            }
                            Disciplina::Pci => {
                                ui.label("Nº instalaciones PCI");
                                numero(ui, &mut v.e.instalaciones_pci);
                                ui.end_row();
                                ui.label("Superficie (m²)");
                                caja(ui, &mut v.superficie, 70.0);
                                ui.checkbox(&mut v.e.presion_diferencial, "Presión diferencial");
                            }
                        }
                        ui.end_row();
                        ui.label("Circuitos RSIF (0 = sin RSIF)");
                        numero(ui, &mut v.e.circuitos_rsif);
                        ui.label("Registro frigorífico");
                        egui::ComboBox::from_id_salt("leg_if")
                            .selected_text(match v.e.registro_if {
                                None => "no",
                                Some(RegistroIf::MemoriaA2l) => "memoria A2L",
                                Some(RegistroIf::MemoriaL1) => "memoria L1",
                                Some(RegistroIf::Proyecto) => "proyecto sin sala",
                                Some(RegistroIf::ProyectoConSala) => "proyecto con sala",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut v.e.registro_if, None, "no");
                                ui.selectable_value(&mut v.e.registro_if, Some(RegistroIf::MemoriaA2l), "memoria A2L");
                                ui.selectable_value(&mut v.e.registro_if, Some(RegistroIf::MemoriaL1), "memoria L1");
                                ui.selectable_value(
                                    &mut v.e.registro_if,
                                    Some(RegistroIf::Proyecto),
                                    "proyecto sin sala",
                                );
                                ui.selectable_value(
                                    &mut v.e.registro_if,
                                    Some(RegistroIf::ProyectoConSala),
                                    "proyecto con sala",
                                );
                            });
                        ui.end_row();
                        ui.label("Segundas visitas");
                        numero(ui, &mut v.e.segundas_visitas);
                        ui.checkbox(&mut v.e.repercutir, "Repercutir tasas, EICI y visado");
                        ui.end_row();
                        ui.label("Km ida y vuelta por viaje");
                        caja(ui, &mut v.km, 70.0);
                        ui.label("Horas de viaje por viaje");
                        caja(ui, &mut v.horas, 70.0);
                        ui.end_row();
                        ui.label("Viajes");
                        numero(ui, &mut v.e.viajes);
                        ui.end_row();
                    });
                    ui.separator();

                    // Vista previa
                    let e = v.expediente();
                    let c = calcular(&baremo, &e);
                    for a in &c.avisos {
                        ui.label(RichText::new(a).color(Color32::from_rgb(220, 150, 40)));
                    }
                    egui::Grid::new("leg_previa")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            for t in ["Partida / línea", "Cantidad", "Precio €", "Importe €"] {
                                ui.label(RichText::new(t).strong());
                            }
                            ui.end_row();
                            for p in &c.partidas {
                                ui.label(RichText::new(&p.resumen).strong());
                                ui.label("1 ud");
                                ui.label("");
                                ui.label(RichText::new(eur(p.importe())).strong());
                                ui.end_row();
                                for l in &p.lineas {
                                    let t = if l.propia {
                                        l.resumen.clone()
                                    } else {
                                        format!("{} ·", l.resumen)
                                    };
                                    ui.label(RichText::new(format!("   {t}")).weak());
                                    ui.label(format!(
                                        "{} {}",
                                        num(l.cantidad.normalize(), l.cantidad.normalize().scale()),
                                        l.unidad
                                    ));
                                    ui.label(eur(l.precio));
                                    ui.label(eur(l.importe()));
                                    ui.end_row();
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Honorarios: {} €", eur(c.honorarios))).strong());
                        if let Some(h) = c.horas {
                            ui.label(format!("≈ {} h a {} €/h", num(h, 1), eur(baremo.hora_referencia)));
                        }
                        ui.separator();
                        ui.label(format!("Repercutidos: {} €", eur(c.repercutidos)));
                        ui.separator();
                        ui.label(RichText::new(format!("Total: {} €", eur(c.total()))).strong());
                    });
                    if let Some((m, error)) = &v.mensaje {
                        let col = if *error {
                            Color32::from_rgb(220, 80, 60)
                        } else {
                            Color32::from_rgb(80, 175, 110)
                        };
                        ui.label(RichText::new(m).color(col));
                    }
                    let sub = ppto_legalizacion::subcapitulo(e.disciplina);
                    let existe = self.p.conceptos.contains_key(&sub);
                    let rotulo = if existe {
                        format!("Sustituir el capítulo {sub}")
                    } else {
                        format!("Crear el capítulo {sub}")
                    };
                    if ui
                        .add_enabled(baremo.anio != 0 && !c.partidas.is_empty(), egui::Button::new(rotulo))
                        .on_hover_text(
                            "Partidas descompuestas dentro del capítulo LEG (LEGALIZACIÓN). Se puede deshacer.",
                        )
                        .clicked()
                    {
                        crear = Some((e.disciplina, c));
                    }
                });
            });
        if let Some((disciplina, c)) = crear {
            self.historial
                .antes(&self.p, format!("legalización {}", disciplina.sigla()), None);
            match aplicar(&mut self.p, disciplina, &c) {
                Ok(()) => {
                    self.marcar_cambio();
                    v.mensaje = Some((
                        format!(
                            "Capítulo {} creado: {} €.",
                            ppto_legalizacion::subcapitulo(disciplina),
                            eur(c.total())
                        ),
                        false,
                    ));
                }
                Err(e) => {
                    self.historial.deshacer.pop();
                    v.mensaje = Some((format!("No se pudo crear: {e}"), true));
                }
            }
        }
        if abierta {
            self.ventana_legalizacion = Some(v);
        }
    }
}
