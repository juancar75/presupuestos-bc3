// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Nuevo presupuesto»: pregunta qué se va a presupuestar
//! (especialidad) y de dónde se parte (ciego del cliente, BC3 o plantilla),
//! y deja la obra orientada a esa especialidad: plantilla y bases de precios
//! de la especialidad. «Presupuesto en blanco» es el camino de siempre.

use super::Aplicacion;
use eframe::egui::{self, RichText};
use ppto_core::especialidad::Especialidad;

/// Estado de la ventana. Primer paso: qué se presupuesta (una especialidad
/// o varias marcadas, p. ej. clima y PCI). Segundo: de dónde se parte y, si
/// es desde cero, qué capítulos de las plantillas entran.
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct AsistenteNuevo {
    /// Marcadas en el primer paso.
    pub marcadas: Vec<Especialidad>,
    /// Elegidas (vacío = primer paso).
    pub elegidas: Vec<Especialidad>,
    /// Capítulos de plantilla que entran (especialidad, número).
    pub capitulos: Vec<(Especialidad, &'static str)>,
}

impl AsistenteNuevo {
    /// Pasa al segundo paso con estas especialidades y todos sus capítulos.
    pub fn elegir(&mut self, es: Vec<Especialidad>) {
        self.capitulos = todos_los_capitulos(&es);
        self.elegidas = es;
    }
}

/// Todos los capítulos de las plantillas de estas especialidades.
pub(super) fn todos_los_capitulos(es: &[Especialidad]) -> Vec<(Especialidad, &'static str)> {
    es.iter()
        .filter_map(|e| e.plantilla().map(|t| (*e, t)))
        .flat_map(|(e, t)| t.capitulos().into_iter().map(move |(n, _)| (e, n)))
        .collect()
}

/// Lo que se elige en la ventana.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Inicio {
    EnBlanco,
    /// Desde cero: especialidades y capítulos de plantilla que entran.
    Plantilla(Vec<Especialidad>, Vec<(Especialidad, &'static str)>),
    Ciego(Vec<Especialidad>),
    Bc3(Vec<Especialidad>),
}

impl Inicio {
    /// Desde cero con una especialidad y toda su plantilla.
    #[cfg(test)]
    pub fn plantilla(e: Especialidad) -> Self {
        Self::Plantilla(vec![e], todos_los_capitulos(&[e]))
    }

    fn especialidades(&self) -> Vec<Especialidad> {
        match self {
            Self::Plantilla(e, _) | Self::Ciego(e) | Self::Bc3(e) => e.clone(),
            Self::EnBlanco => Vec::new(),
        }
    }
}

impl Aplicacion {
    /// Abre la ventana «Nuevo presupuesto» (Archivo → Nuevo, Ctrl+N).
    pub(super) fn pedir_nuevo(&mut self) {
        self.asistente_nuevo = Some(AsistenteNuevo::default());
    }

    /// Crea la obra según lo elegido y la deja con sus especialidades; con
    /// especialidad, abre la ficha de datos del edificio (paso 3).
    /// Si no se pudo (fichero cancelado o que no se lee), vuelve a la ventana
    /// con lo elegido y la obra abierta no cambia.
    pub(super) fn empezar(&mut self, inicio: Inicio) {
        let es = inicio.especialidades();
        let caps = match &inicio {
            Inicio::Plantilla(_, c) => c.clone(),
            _ => todos_los_capitulos(&es),
        };
        if self.empezar_sin_ficha(inicio) {
            self.ver_ficha = !self.especialidades.is_empty();
        } else {
            self.ver_ficha = false;
            self.asistente_nuevo = Some(AsistenteNuevo {
                marcadas: es.clone(),
                elegidas: es,
                capitulos: caps,
            });
        }
    }

    fn empezar_sin_ficha(&mut self, inicio: Inicio) -> bool {
        match inicio {
            Inicio::EnBlanco => {
                self.nuevo_presupuesto();
                true
            }
            Inicio::Plantilla(es, caps) => {
                let elegidos: Vec<(ppto_core::plantillas::Plantilla, Vec<&str>)> = es
                    .iter()
                    .filter_map(|e| e.plantilla().map(|t| (*e, t)))
                    .map(|(e, t)| {
                        (
                            t,
                            caps.iter()
                                .filter(|(x, _)| *x == e)
                                .map(|(_, n)| *n)
                                .collect::<Vec<_>>(),
                        )
                    })
                    .filter(|(_, c)| !c.is_empty())
                    .collect();
                let nombres: Vec<&str> = es.iter().map(|e| e.nombre()).collect();
                let nombres = nombres.join(" + ");
                self.nuevo_presupuesto();
                if !elegidos.is_empty() {
                    match ppto_core::plantillas::crear_con(&elegidos, &format!("Nueva obra — {nombres}")) {
                        Ok(p) => {
                            self.p = p;
                            self.recalcular();
                        }
                        Err(e) => {
                            self.error(format!("No se pudo crear la plantilla: {e}"));
                            return false;
                        }
                    }
                }
                self.especialidades = es;
                self.info(format!(
                    "Obra de {nombres}: {}la Ventana → Bases de precios muestra las bases de {}.",
                    if elegidos.is_empty() {
                        "presupuesto vacío (sin plantilla); "
                    } else {
                        "plantilla cargada (partidas sin precio); "
                    },
                    if self.especialidades.len() > 1 {
                        "estas especialidades"
                    } else {
                        "esta especialidad"
                    }
                ));
                true
            }
            Inicio::Ciego(es) => {
                let ok = rfd::FileDialog::new()
                    .add_filter("Excel", &["xlsx", "xls"])
                    .set_title("Presupuesto ciego del cliente (Excel)")
                    .pick_file()
                    .is_some_and(|r| self.importar_ciego(r));
                if ok {
                    self.especialidades = es;
                }
                ok
            }
            Inicio::Bc3(es) => {
                let ok = rfd::FileDialog::new()
                    .add_filter("BC3", &["bc3", "BC3"])
                    .set_title("Presupuesto del cliente (BC3)")
                    .pick_file()
                    .is_some_and(|r| self.importar_bc3(r));
                if ok {
                    self.especialidades = es;
                }
                ok
            }
        }
    }

    pub(super) fn ventana_nuevo(&mut self, ctx: &egui::Context) {
        let Some(mut estado) = self.asistente_nuevo.take() else {
            return;
        };
        let mut abierta = true;
        let mut elegido: Option<Inicio> = None;
        egui::Window::new("Nuevo presupuesto")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if estado.elegidas.is_empty() {
                    paso_especialidades(ui, &mut estado, &mut elegido);
                } else {
                    paso_origen(ui, &mut estado, &mut elegido);
                }
            });
        self.asistente_nuevo = (abierta && elegido.is_none()).then_some(estado);
        if let Some(i) = elegido {
            self.empezar(i);
        }
    }
}

/// Primer paso: pulsar una especialidad sigue con ella sola; marcando varias
/// (clima y PCI…) se sigue con todas.
fn paso_especialidades(ui: &mut egui::Ui, estado: &mut AsistenteNuevo, elegido: &mut Option<Inicio>) {
    ui.label(RichText::new("¿Qué vas a presupuestar?").heading());
    ui.label(
        RichText::new("Pulse una, o marque varias si la oferta junta especialidades (p. ej. clima y PCI).")
            .small()
            .weak(),
    );
    ui.add_space(6.0);
    let mut siguiente = None;
    for e in Especialidad::TODAS {
        ui.horizontal(|ui| {
            let mut m = estado.marcadas.contains(&e);
            if ui
                .checkbox(&mut m, "")
                .on_hover_text("Marcar para juntarla con otras")
                .changed()
            {
                if m {
                    estado.marcadas.push(e);
                } else {
                    estado.marcadas.retain(|x| *x != e);
                }
            }
            let b = egui::Button::new(RichText::new(format!("{}\n", e.nombre())).strong().size(15.0))
                .min_size(egui::vec2(430.0, 0.0));
            if ui.add(b).on_hover_text(e.descripcion()).clicked() {
                siguiente = Some(vec![e]);
            }
        });
        ui.label(RichText::new(e.descripcion()).small().weak());
        ui.add_space(4.0);
    }
    if estado.marcadas.len() > 1 {
        // En el orden de la lista
        let es: Vec<Especialidad> = Especialidad::TODAS
            .into_iter()
            .filter(|e| estado.marcadas.contains(e))
            .collect();
        let n: Vec<&str> = es.iter().map(|e| e.nombre()).collect();
        if ui
            .add(egui::Button::new(
                RichText::new(format!("Seguir con {} →", n.join(" + "))).strong(),
            ))
            .clicked()
        {
            siguiente = Some(es);
        }
    }
    ui.separator();
    if ui
        .button("Presupuesto en blanco")
        .on_hover_text("Sin especialidad ni plantilla, como siempre")
        .clicked()
    {
        *elegido = Some(Inicio::EnBlanco);
    }
    if let Some(es) = siguiente {
        estado.elegir(es);
    }
}

/// Segundo paso: de dónde se parte y, desde cero, qué capítulos entran.
fn paso_origen(ui: &mut egui::Ui, estado: &mut AsistenteNuevo, elegido: &mut Option<Inicio>) {
    let es = estado.elegidas.clone();
    let n: Vec<&str> = es.iter().map(|e| e.nombre()).collect();
    ui.label(RichText::new(n.join(" + ")).heading());
    ui.label(RichText::new("¿De dónde partes?").strong());
    ui.add_space(6.0);
    let ancho = egui::vec2(460.0, 0.0);
    if ui
        .add(egui::Button::new("Tengo el presupuesto ciego del cliente (Excel)").min_size(ancho))
        .clicked()
    {
        *elegido = Some(Inicio::Ciego(es.clone()));
    }
    if ui
        .add(egui::Button::new("Tengo el presupuesto del cliente en BC3").min_size(ancho))
        .clicked()
    {
        *elegido = Some(Inicio::Bc3(es.clone()));
    }
    let con_plantilla: Vec<(Especialidad, ppto_core::plantillas::Plantilla)> =
        es.iter().filter_map(|e| e.plantilla().map(|t| (*e, t))).collect();
    let desde = if con_plantilla.is_empty() {
        "Empiezo desde cero (presupuesto vacío)".to_owned()
    } else {
        let n = estado.capitulos.len();
        format!(
            "Empiezo desde cero (plantilla: {n} capítulo{})",
            if n == 1 { "" } else { "s" }
        )
    };
    if ui.add(egui::Button::new(desde).min_size(ancho)).clicked() {
        *elegido = Some(Inicio::Plantilla(es.clone(), estado.capitulos.clone()));
    }
    if !con_plantilla.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new("Capítulos de la plantilla")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    for (e, t) in &con_plantilla {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(e.nombre()).strong());
                            if ui.small_button("todos").clicked() {
                                estado.capitulos.retain(|(x, _)| x != e);
                                estado.capitulos.extend(t.capitulos().into_iter().map(|(n, _)| (*e, n)));
                            }
                            if ui.small_button("ninguno").clicked() {
                                estado.capitulos.retain(|(x, _)| x != e);
                            }
                        });
                        for (num, resumen) in t.capitulos() {
                            let mut m = estado.capitulos.contains(&(*e, num));
                            if ui.checkbox(&mut m, format!("{num}  {resumen}")).changed() {
                                if m {
                                    estado.capitulos.push((*e, num));
                                } else {
                                    estado.capitulos.retain(|c| *c != (*e, num));
                                }
                            }
                        }
                        ui.add_space(4.0);
                    }
                });
                ui.label(
                    RichText::new("Siempre entran las observaciones (C00) y los contradictorios (C99).")
                        .small()
                        .weak(),
                );
            });
    }
    ui.add_space(6.0);
    if ui.small_button("← Otra especialidad").clicked() {
        estado.elegidas.clear();
    }
}
