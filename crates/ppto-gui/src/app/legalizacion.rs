// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Legalización»: una partida por disciplina en el capítulo `LEG`
//! con todos los costes en el descompuesto (horas por perfil, visitas de la
//! OCA, desplazamientos, costes indirectos y beneficio, y tasas, EICI y
//! visados repercutidos).
//!
//! El baremo (honorarios y tablas por año) está en `legalizacion.json`, en la
//! carpeta de configuración, fuera del repositorio.

use super::Aplicacion;
use eframe::egui::{self, Color32, RichText};
use ppto_core::Decimal;
use ppto_core::formato::{eur, leer, num};
use ppto_legalizacion::{
    Baremo, Baremos, Calculo, Disciplina, Documento, Expediente, RegistroIf, Tipo, Tramo, Uso, aplicar, calcular,
    capitulos,
};
use std::path::PathBuf;

/// Honorarios y costes editables en la ventana (las tablas oficiales y los
/// repartos de horas por perfil, en el JSON).
const CAMPOS: [&str; 24] = [
    "Mínimo tramitación proyecto (€)",
    "Mínimo tramitación memoria (€)",
    "Mínimo por vivienda (€)",
    "Recargo proyecto ajeno (%)",
    "Mínimo redacción proyecto propio (€)",
    "Justificación RITE (% PEM)",
    "Justificación RITE mínimo (€)",
    "RSIF circuito de cada modelo (€)",
    "RSIF circuito repetido (€)",
    "RSIF mínimo (€)",
    "Coste ingeniero senior (€/h)",
    "Coste ingeniero junior (€/h)",
    "Coste técnico (€/h)",
    "Coste documentación (€/h)",
    "Costes indirectos y beneficio (%)",
    "Horas por visita OCA",
    "Viaje habitual: km ida y vuelta",
    "Viaje habitual: horas",
    "Kilometraje (€/km)",
    "Aparcamiento (€/viaje)",
    "Dieta (€/viaje)",
    "Dietas a partir de (km)",
    "EICI 2ª visita (€)",
    "Tasa registro frigorífica (€)",
];

fn campo(b: &mut Baremo, i: usize) -> &mut Decimal {
    match i {
        0 => &mut b.tramitacion_minimo,
        1 => &mut b.tramitacion_minimo_memoria,
        2 => &mut b.minimo_vivienda,
        3 => &mut b.recargo_ajeno,
        4 => &mut b.redaccion_minimo,
        5 => &mut b.justif_rite_pct,
        6 => &mut b.justif_rite_minimo,
        7 => &mut b.rsif_circuito,
        8 => &mut b.rsif_repetido,
        9 => &mut b.rsif_minimo,
        10 => &mut b.coste_senior,
        11 => &mut b.coste_junior,
        12 => &mut b.coste_tecnico,
        13 => &mut b.coste_documentacion,
        14 => &mut b.margen,
        15 => &mut b.oca_horas,
        16 => &mut b.viaje_km,
        17 => &mut b.viaje_horas,
        18 => &mut b.km,
        19 => &mut b.aparcamiento,
        20 => &mut b.dieta,
        21 => &mut b.km_umbral,
        22 => &mut b.eici_segunda_visita,
        _ => &mut b.tasa_if,
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
    potencia_calor: String,
    potencia_frio: String,
    superficie: String,
    // Edición del baremo
    editar: bool,
    campos: Vec<String>,
    tramos: Vec<(String, String)>,
    tramos_memoria: Vec<(String, String)>,
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
            potencia_calor: String::new(),
            potencia_frio: String::new(),
            superficie: String::new(),
            editar: false,
            campos: Vec::new(),
            tramos: Vec::new(),
            tramos_memoria: Vec::new(),
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
        let a_texto = |v: &[Tramo]| {
            v.iter()
                .map(|t| (t.hasta.map(texto).unwrap_or_default(), texto(t.valor)))
                .collect()
        };
        self.tramos = a_texto(&b.tramitacion);
        self.tramos_memoria = a_texto(&b.tramitacion_memoria);
        self.e.visitas_obra = match self.e.documento {
            Documento::Memoria => b.visitas_memoria,
            Documento::Proyecto => b.visitas_proyecto,
        };
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
        e.potencia_calor_kw = leer(&self.potencia_calor).unwrap_or_default();
        e.potencia_frio_kw = leer(&self.potencia_frio).unwrap_or_default();
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
        let leer_tramos = |v: &[(String, String)]| -> Option<Vec<Tramo>> {
            v.iter()
                .map(|(h, p)| {
                    Some(Tramo {
                        hasta: leer(h),
                        valor: leer(p)?,
                    })
                })
                .collect()
        };
        let (Some(tramos), Some(tramos_memoria)) = (leer_tramos(&self.tramos), leer_tramos(&self.tramos_memoria))
        else {
            self.mensaje = Some(("Revise los % de los tramos de tramitación.".into(), true));
            return;
        };
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
        b.tramitacion_memoria = tramos_memoria;
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

/// Lista editable de tramos «hasta € · %» (el último, sin límite).
fn editar_tramos(ui: &mut egui::Ui, tramos: &mut Vec<(String, String)>) {
    let mut quitar = None;
    for (i, (h, p)) in tramos.iter_mut().enumerate() {
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
        tramos.remove(i);
    }
    if ui.small_button("Añadir tramo").clicked() {
        tramos.push((String::new(), String::new()));
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
                            ui.columns(2, |cols| {
                                cols[0].label(RichText::new("Proyectos: % del PEM por tramos").strong());
                                editar_tramos(&mut cols[0], &mut v.tramos);
                                cols[1].label(RichText::new("Memorias: % del PEM por tramos").strong());
                                editar_tramos(&mut cols[1], &mut v.tramos_memoria);
                            });
                            ui.horizontal(|ui| {
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
                        let disciplina = v.e.disciplina;
                        ui.radio_value(&mut v.e.disciplina, Disciplina::Rite, "RITE");
                        ui.radio_value(&mut v.e.disciplina, Disciplina::Pci, "PCI");
                        if v.e.disciplina != disciplina {
                            v.e.justificar_rite = v.e.justificar_rite_por_defecto();
                            v.e.circuitos_rsif = u32::from(v.e.disciplina == Disciplina::Rite);
                            v.e.modelos_rsif = 0;
                        }
                        ui.separator();
                        let antes = v.e.documento;
                        ui.radio_value(&mut v.e.documento, Documento::Memoria, "Memoria técnica");
                        ui.radio_value(&mut v.e.documento, Documento::Proyecto, "Proyecto");
                        if v.e.documento != antes {
                            v.e.visitas_obra = match v.e.documento {
                                Documento::Memoria => baremo.visitas_memoria,
                                Documento::Proyecto => baremo.visitas_proyecto,
                            };
                            v.e.justificar_rite = v.e.justificar_rite_por_defecto();
                        }
                        ui.separator();
                        if ui.checkbox(&mut v.e.ajeno, "Proyecto de otro técnico").changed() {
                            v.e.justificar_rite = v.e.justificar_rite_por_defecto();
                        }
                    });
                    // RITE art. 15: con más de 70 kW hace falta proyecto.
                    let mut exp = v.e.clone();
                    exp.potencia_calor_kw = leer(&v.potencia_calor).unwrap_or_default();
                    exp.potencia_frio_kw = leer(&v.potencia_frio).unwrap_or_default();
                    if exp.documento_exigido() == Some(Some(Documento::Proyecto)) && v.e.documento == Documento::Memoria {
                        v.e.documento = Documento::Proyecto;
                        v.e.visitas_obra = baremo.visitas_proyecto;
                        v.e.justificar_rite = v.e.justificar_rite_por_defecto();
                        v.mensaje = Some((
                            format!(
                                "{} kW: el RITE (art. 15) exige proyecto; se ha cambiado de memoria a proyecto.",
                                texto(exp.potencia())
                            ),
                            false,
                        ));
                    }
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
                                ui.label("Potencia calor (kW)").on_hover_text(
                                    "Suma de las potencias nominales de todos los generadores de calor del edificio (RITE art. 15.2)",
                                );
                                caja(ui, &mut v.potencia_calor, 70.0);
                                ui.label("Potencia frío (kW)").on_hover_text(
                                    "Suma de las potencias nominales de todos los generadores de frío del edificio (RITE art. 15.2)",
                                );
                                caja(ui, &mut v.potencia_frio, 70.0);
                                ui.end_row();
                                ui.checkbox(&mut v.e.sala_maquinas, "Sala de máquinas")
                                    .on_hover_text("En edificios de viviendas, la EICI va por potencia en vez de por nº de viviendas");
                                ui.label("");
                                ui.end_row();
                                ui.checkbox(&mut v.e.primario_frio, "Primario frigorífico");
                                ui.checkbox(&mut v.e.justificar_rite, "Justificación RITE")
                                    .on_hover_text(
                                        "Por defecto en memorias y en proyectos de otro técnico; en los proyectos propios va en la redacción.",
                                    );
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
                        ui.label("de ellos, modelos distintos");
                        numero(ui, &mut v.e.modelos_rsif);
                        ui.end_row();
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
                        ui.label("Visitas de dirección de obra");
                        numero(ui, &mut v.e.visitas_obra);
                        ui.label("Segundas visitas OCA");
                        numero(ui, &mut v.e.segundas_visitas);
                        ui.end_row();
                        ui.label("Km ida y vuelta por viaje");
                        ui.add(
                            egui::TextEdit::singleline(&mut v.km)
                                .hint_text(texto(baremo.viaje_km))
                                .desired_width(70.0),
                        );
                        ui.label("Horas por viaje");
                        ui.add(
                            egui::TextEdit::singleline(&mut v.horas)
                                .hint_text(texto(baremo.viaje_horas))
                                .desired_width(70.0),
                        );
                        ui.end_row();
                        ui.checkbox(&mut v.e.repercutir, "Repercutir tasas, EICI y visado");
                        ui.end_row();
                    });
                    ui.separator();

                    // Vista previa
                    let e = v.expediente();
                    let c = calcular(&baremo, &e);
                    for a in &c.avisos {
                        ui.label(RichText::new(a).color(Color32::from_rgb(220, 150, 40)));
                    }
                    let codigo = ppto_legalizacion::codigo_partida(e.disciplina);
                    ui.label(RichText::new(format!("{codigo}  {}", c.resumen)).strong());
                    egui::Grid::new("leg_previa")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            for t in ["Línea del descompuesto", "Cantidad", "Precio €", "Importe €"] {
                                ui.label(RichText::new(t).strong());
                            }
                            ui.end_row();
                            for bl in &c.bloques {
                                ui.label(RichText::new(&bl.titulo).strong())
                                    .on_hover_text(&bl.regla);
                                ui.label(RichText::new(&bl.regla).small().weak());
                                ui.label("");
                                ui.label(RichText::new(eur(bl.importe())).strong());
                                ui.end_row();
                                for l in &bl.lineas {
                                    ui.label(RichText::new(format!("   {}", l.resumen)).weak());
                                    ui.label(format!("{} {}", texto(l.cantidad), l.unidad));
                                    ui.label(if l.tipo == Tipo::Margen {
                                        format!("{} (base/100)", eur(l.precio))
                                    } else {
                                        eur(l.precio)
                                    });
                                    ui.label(eur(l.importe()));
                                    ui.end_row();
                                }
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Coste: {} € ({} h)", eur(c.coste), num(c.horas, 1)));
                        ui.separator();
                        ui.label(format!("Indirectos y beneficio: {} €", eur(c.margen)));
                        ui.separator();
                        ui.label(RichText::new(format!("Honorarios: {} €", eur(c.honorarios()))).strong());
                        ui.separator();
                        ui.label(format!("Repercutidos: {} €", eur(c.repercutidos)));
                        ui.separator();
                        ui.label(RichText::new(format!("Total partida: {} €", eur(c.total()))).strong());
                    });
                    if let Some((m, error)) = &v.mensaje {
                        let col = if *error {
                            Color32::from_rgb(220, 80, 60)
                        } else {
                            Color32::from_rgb(80, 175, 110)
                        };
                        ui.label(RichText::new(m).color(col));
                    }
                    let existe = self.p.conceptos.contains_key(&codigo);
                    let rotulo = if existe {
                        format!("Sustituir la partida {codigo}")
                    } else {
                        format!("Crear la partida {codigo}")
                    };
                    if ui
                        .add_enabled(baremo.anio != 0, egui::Button::new(rotulo))
                        .on_hover_text("Una partida con todo el descompuesto en el capítulo LEG (LEGALIZACIÓN). Se puede deshacer.")
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
                            "Partida {} creada: {} €.",
                            ppto_legalizacion::codigo_partida(disciplina),
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
