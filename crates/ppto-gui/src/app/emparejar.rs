// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Emparejar con la base»: para cada partida sin precio de la obra
//! (p. ej. un presupuesto ciego) propone las partidas de la base de precios
//! que mejor encajan, en verde (segura), naranja (dudosa) o rojo (sin
//! candidato). El técnico confirma: la partida de la obra se queda con su
//! código, texto y medición y recibe el descompuesto de la de la base. Las
//! parejas confirmadas se recuerdan en la biblioteca y la próxima vez salen
//! ya en verde.

use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText};
use ppto_core::Presupuesto;
use ppto_core::emparejar::{Candidato, Indice, Nivel, clave, sin_precio};
use ppto_core::formato::eur;
use std::sync::Arc;

pub(super) const VERDE: Color32 = Color32::from_rgb(80, 175, 110);
pub(super) const NARANJA: Color32 = Color32::from_rgb(225, 150, 40);
pub(super) const ROJO: Color32 = Color32::from_rgb(215, 85, 70);

pub(super) struct Fila {
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub texto: String,
    pub candidatos: Vec<Candidato>,
    /// Índice del candidato elegido (`None`: ninguno).
    pub elegido: Option<usize>,
}

impl Fila {
    pub fn nivel(&self) -> Nivel {
        self.elegido
            .and_then(|i| self.candidatos.get(i))
            .map_or(Nivel::Ninguno, Candidato::nivel)
    }
}

pub(super) struct VentanaEmparejar {
    pub base: Arc<Presupuesto>,
    pub nombre: String,
    pub filas: Vec<Fila>,
    pub filtro: Option<Nivel>,
    /// Fichero de la biblioteca donde se recuerdan las parejas (`None`: no se recuerdan).
    bib: Option<std::path::PathBuf>,
}

fn color(n: Nivel) -> Color32 {
    match n {
        Nivel::Seguro => VERDE,
        Nivel::Dudoso => NARANJA,
        Nivel::Ninguno => ROJO,
    }
}

impl VentanaEmparejar {
    /// Prepara las filas: partidas sin precio de `obra` con sus candidatos en
    /// `base` y, delante, la pareja recordada si la hay.
    pub fn nueva(obra: &Presupuesto, base: Arc<Presupuesto>, nombre: String) -> Self {
        Self::en(obra, base, nombre, ppto_biblioteca::ruta())
    }

    /// Como [`Self::nueva`] con la biblioteca de `ruta`.
    pub fn en(obra: &Presupuesto, base: Arc<Presupuesto>, nombre: String, ruta: Option<std::path::PathBuf>) -> Self {
        let indice = Indice::de(&base);
        let bib = ruta
            .as_deref()
            .map(ppto_biblioteca::Biblioteca::cargar)
            .unwrap_or_default();
        let filas = sin_precio(obra)
            .into_iter()
            .filter_map(|c| obra.conceptos.get(&c))
            .map(|c| {
                let texto = c.texto.clone().unwrap_or_default();
                let mut candidatos = indice.candidatos(&base, &c.resumen, &texto, &c.unidad, 5);
                if let Some(e) = bib.equivalencia(&clave(&c.resumen, &c.unidad), &nombre)
                    && let Some(b) = base.conceptos.get(&e.codigo)
                {
                    candidatos.retain(|x| x.codigo != e.codigo);
                    candidatos.insert(
                        0,
                        Candidato {
                            codigo: b.codigo.clone(),
                            unidad: b.unidad.clone(),
                            resumen: b.resumen.clone(),
                            precio: base.precio(&b.codigo).unwrap_or_default(),
                            puntos: 100,
                            recordado: true,
                            empate: false,
                        },
                    );
                }
                let elegido = (!candidatos.is_empty()).then_some(0);
                Fila {
                    codigo: c.codigo.clone(),
                    unidad: c.unidad.clone(),
                    resumen: c.resumen.clone(),
                    texto,
                    candidatos,
                    elegido,
                }
            })
            .collect();
        Self {
            base,
            nombre,
            filas,
            filtro: None,
            bib: ruta,
        }
    }

    pub fn cuenta(&self, n: Nivel) -> usize {
        self.filas.iter().filter(|f| f.nivel() == n).count()
    }

    /// Parejas (obra, base) de las filas elegidas con nivel ≥ `minimo`, y las
    /// quita de la lista.
    pub fn sacar(&mut self, minimo: Nivel, solo: Option<&str>) -> Vec<(String, String)> {
        let mut v = Vec::new();
        self.filas.retain(|f| {
            let toca = solo.map_or(f.nivel() >= minimo, |c| c == f.codigo);
            match f.elegido.and_then(|i| f.candidatos.get(i)) {
                Some(c) if toca => {
                    v.push((f.codigo.clone(), c.codigo.clone()));
                    false
                }
                _ => true,
            }
        });
        v
    }

    /// Recuerda en la biblioteca las parejas confirmadas.
    fn recordar(&self, filas: &[(String, String, String, String)]) {
        let Some(r) = &self.bib else { return };
        let _ = ppto_biblioteca::Biblioteca::modificar(r, |b| {
            for (resumen, ud, codigo, res_base) in filas {
                b.recordar_equivalencia(&clave(resumen, ud), &self.nombre, codigo, res_base);
            }
        });
    }

    fn asignar(&mut self, minimo: Nivel, solo: Option<&str>, acciones: &mut Vec<Accion>) {
        let datos: Vec<(String, String, String, String, String)> = self
            .filas
            .iter()
            .filter(|f| solo.map_or(f.nivel() >= minimo, |c| c == f.codigo))
            .filter_map(|f| {
                let c = f.elegido.and_then(|i| f.candidatos.get(i))?;
                Some((
                    f.codigo.clone(),
                    f.resumen.clone(),
                    f.unidad.clone(),
                    c.codigo.clone(),
                    c.resumen.clone(),
                ))
            })
            .collect();
        let parejas = self.sacar(minimo, solo);
        if parejas.is_empty() {
            return;
        }
        self.recordar(
            &datos
                .into_iter()
                .map(|(_, r, u, c, rb)| (r, u, c, rb))
                .collect::<Vec<_>>(),
        );
        acciones.push(Accion::AsignarDeBase {
            origen: Arc::clone(&self.base),
            nombre: self.nombre.clone(),
            parejas,
        });
    }
}

impl Aplicacion {
    pub(super) fn ventana_emparejar(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.ventana_emparejar.take() else {
            return;
        };
        let mut abierta = true;
        egui::Window::new(format!("Emparejar la obra con {}", v.nombre))
            .open(&mut abierta)
            .default_width(980.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| {
                let (s, d, n) = (
                    v.cuenta(Nivel::Seguro),
                    v.cuenta(Nivel::Dudoso),
                    v.cuenta(Nivel::Ninguno),
                );
                ui.horizontal(|ui| {
                    ui.label(format!("{} partidas sin precio:", v.filas.len()));
                    ui.label(RichText::new(format!("● {s} seguras")).color(VERDE));
                    ui.label(RichText::new(format!("● {d} dudosas")).color(NARANJA));
                    ui.label(RichText::new(format!("● {n} sin candidato")).color(ROJO));
                });
                ui.label(
                    RichText::new(
                        "Revise la partida propuesta y cámbiela si no es la buena. Al asignar, la partida de la obra \
                         conserva su código, texto y medición y recibe el descompuesto de la base. Lo que quede sin \
                         candidato: Revisar omisiones, propuestas de la IA u oferta.",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(s > 0, egui::Button::new(format!("Asignar las {s} seguras")))
                        .clicked()
                    {
                        v.asignar(Nivel::Seguro, None, acciones);
                    }
                    if ui
                        .add_enabled(
                            s + d > 0,
                            egui::Button::new(format!("Asignar seguras y dudosas ({})", s + d)),
                        )
                        .on_hover_text("Solo después de revisar las naranjas")
                        .clicked()
                    {
                        v.asignar(Nivel::Dudoso, None, acciones);
                    }
                    ui.separator();
                    ui.label("Ver:");
                    ui.radio_value(&mut v.filtro, None, "todas");
                    ui.radio_value(&mut v.filtro, Some(Nivel::Seguro), "seguras");
                    ui.radio_value(&mut v.filtro, Some(Nivel::Dudoso), "dudosas");
                    ui.radio_value(&mut v.filtro, Some(Nivel::Ninguno), "sin candidato");
                });
                ui.separator();
                let mut una: Option<String> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("emparejar")
                        .striped(true)
                        .num_columns(5)
                        .show(ui, |ui| {
                            for t in ["", "Código", "Ud", "Partida de la obra", "Partida de la base"] {
                                ui.label(RichText::new(t).strong());
                            }
                            ui.end_row();
                            let filtro = v.filtro;
                            for f in v.filas.iter_mut().filter(|f| filtro.is_none_or(|n| f.nivel() == n)) {
                                ui.label(RichText::new("●").color(color(f.nivel())));
                                ui.label(&f.codigo);
                                ui.label(&f.unidad);
                                let r = ui.label(f.resumen.chars().take(70).collect::<String>());
                                if !f.texto.is_empty() {
                                    r.on_hover_text(&f.texto);
                                }
                                ui.horizontal(|ui| {
                                    let texto = |c: &Candidato| {
                                        format!(
                                            "{}{} % · {} · {} · {} €",
                                            if c.recordado { "★ " } else { "" },
                                            c.puntos,
                                            c.codigo,
                                            c.resumen.chars().take(55).collect::<String>(),
                                            eur(c.precio)
                                        )
                                    };
                                    let actual = f
                                        .elegido
                                        .and_then(|i| f.candidatos.get(i))
                                        .map_or("— ninguna —".to_owned(), texto);
                                    egui::ComboBox::from_id_salt(("emp", f.codigo.as_str()))
                                        .width(520.0)
                                        .selected_text(actual)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut f.elegido, None, "— ninguna —");
                                            for (i, c) in f.candidatos.iter().enumerate() {
                                                ui.selectable_value(&mut f.elegido, Some(i), texto(c));
                                            }
                                        });
                                    if ui
                                        .add_enabled(f.elegido.is_some(), egui::Button::new("Asignar"))
                                        .clicked()
                                    {
                                        una = Some(f.codigo.clone());
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
                if let Some(c) = una {
                    v.asignar(Nivel::Ninguno, Some(&c), acciones);
                }
            });
        if abierta && !v.filas.is_empty() {
            self.ventana_emparejar = Some(v);
        }
    }
}
