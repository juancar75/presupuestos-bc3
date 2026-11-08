// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana «Emparejar con la base»: para cada partida sin precio de la obra
//! (p. ej. un presupuesto ciego) propone las partidas de la base de precios
//! que mejor encajan, en verde (segura), naranja (dudosa) o rojo (sin
//! candidato). El técnico confirma: la partida de la obra se queda con su
//! código, texto y medición y recibe el descompuesto de la de la base. Si la
//! partida junta varias cosas («ventilador + variador + compuerta») se
//! compone con varias partidas de la base, cada una con su cantidad. Las
//! parejas confirmadas se recuerdan en la biblioteca y la próxima vez salen
//! ya en verde (★), con su composición.

use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText};
use ppto_core::emparejar::{Candidato, Indice, Nivel, clave, sin_precio};
use ppto_core::formato::{eur, leer, num};
use ppto_core::{Decimal, Naturaleza, Presupuesto};
use std::path::PathBuf;
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
    /// Otras partidas de la base que completan esta (código, cantidad).
    pub extra: Vec<(String, Decimal)>,
    /// Buscador de «+ otra partida» abierto y su texto.
    pub buscando: Option<String>,
}

impl Fila {
    pub fn nivel(&self) -> Nivel {
        if !self.extra.is_empty() {
            // Compuesta a mano por el técnico (o recordada)
            return Nivel::Seguro;
        }
        self.elegido
            .and_then(|i| self.candidatos.get(i))
            .map_or(Nivel::Ninguno, Candidato::nivel)
    }

    /// Partidas de la base con que se compone (la elegida y las añadidas).
    pub fn partes(&self) -> Vec<(String, Decimal)> {
        self.elegido
            .and_then(|i| self.candidatos.get(i))
            .map(|c| (c.codigo.clone(), Decimal::ONE))
            .into_iter()
            .chain(self.extra.iter().cloned())
            .collect()
    }
}

pub(super) struct VentanaEmparejar {
    pub base: Arc<Presupuesto>,
    pub nombre: String,
    pub filas: Vec<Fila>,
    pub filtro: Option<Nivel>,
    /// Fichero de la biblioteca donde se recuerdan las parejas (`None`: no se recuerdan).
    bib: Option<PathBuf>,
}

/// Pareja confirmada: (resumen y unidad de la partida de la obra, partes de la base).
type Confirmada = (String, String, Vec<(String, Decimal)>);

fn color(n: Nivel) -> Color32 {
    match n {
        Nivel::Seguro => VERDE,
        Nivel::Dudoso => NARANJA,
        Nivel::Ninguno => ROJO,
    }
}

/// Partidas de la base cuyo código o resumen contienen todas las palabras.
fn buscar(base: &Presupuesto, texto: &str, n: usize) -> Vec<(String, String)> {
    let palabras: Vec<String> = ppto_biblioteca::plano(texto)
        .replace(',', ".")
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if palabras.is_empty() {
        return Vec::new();
    }
    base.conceptos
        .values()
        .filter(|c| c.naturaleza == Naturaleza::Partida)
        .filter(|c| {
            let t = ppto_biblioteca::plano(&format!("{} {}", c.codigo, c.resumen)).replace(',', ".");
            palabras.iter().all(|p| t.contains(p.as_str()))
        })
        .take(n)
        .map(|c| (c.codigo.clone(), c.resumen.chars().take(60).collect()))
        .collect()
}

impl VentanaEmparejar {
    /// Prepara las filas: partidas sin precio de `obra` con sus candidatos en
    /// `base` y, delante, la pareja recordada si la hay.
    pub fn nueva(obra: &Presupuesto, base: Arc<Presupuesto>, nombre: String) -> Self {
        Self::en(obra, base, nombre, ppto_biblioteca::ruta())
    }

    /// Como [`Self::nueva`] con la biblioteca de `ruta`.
    pub fn en(obra: &Presupuesto, base: Arc<Presupuesto>, nombre: String, ruta: Option<PathBuf>) -> Self {
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
                let mut extra = Vec::new();
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
                    extra = e
                        .partes
                        .iter()
                        .filter(|(c, _)| base.conceptos.contains_key(c))
                        .cloned()
                        .collect();
                }
                let elegido = (!candidatos.is_empty()).then_some(0);
                Fila {
                    codigo: c.codigo.clone(),
                    unidad: c.unidad.clone(),
                    resumen: c.resumen.clone(),
                    texto,
                    candidatos,
                    elegido,
                    extra,
                    buscando: None,
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

    /// (partida de la obra, partidas de la base con su cantidad) de las filas
    /// con nivel ≥ `minimo` (o solo la `solo`), y las quita de la lista.
    pub fn sacar(&mut self, minimo: Nivel, solo: Option<&str>) -> Vec<(String, Vec<(String, Decimal)>)> {
        let mut v = Vec::new();
        self.filas.retain(|f| {
            let toca = solo.map_or(f.nivel() >= minimo, |c| c == f.codigo);
            let partes = f.partes();
            if toca && !partes.is_empty() {
                v.push((f.codigo.clone(), partes));
                false
            } else {
                true
            }
        });
        v
    }

    /// Recuerda en la biblioteca las parejas confirmadas: (resumen, unidad, partes).
    fn recordar(&self, filas: &[Confirmada]) {
        let Some(r) = &self.bib else { return };
        let _ = ppto_biblioteca::Biblioteca::modificar(r, |b| {
            for (resumen, ud, partes) in filas {
                let Some(((codigo, _), resto)) = partes.split_first() else {
                    continue;
                };
                let res_base = self
                    .base
                    .conceptos
                    .get(codigo)
                    .map(|c| c.resumen.clone())
                    .unwrap_or_default();
                b.recordar_equivalencia(&clave(resumen, ud), &self.nombre, codigo, &res_base, resto);
            }
        });
    }

    fn asignar(&mut self, minimo: Nivel, solo: Option<&str>, acciones: &mut Vec<Accion>) {
        let datos: Vec<Confirmada> = self
            .filas
            .iter()
            .filter(|f| solo.map_or(f.nivel() >= minimo, |c| c == f.codigo))
            .map(|f| (f.resumen.clone(), f.unidad.clone(), f.partes()))
            .filter(|d| !d.2.is_empty())
            .collect();
        let parejas = self.sacar(minimo, solo);
        if parejas.is_empty() {
            return;
        }
        self.recordar(&datos);
        acciones.push(Accion::AsignarDeBase {
            origen: Arc::clone(&self.base),
            nombre: self.nombre.clone(),
            parejas,
        });
    }
}

/// Composición de una fila: partidas añadidas con su cantidad y buscador.
fn composicion(ui: &mut egui::Ui, f: &mut Fila, base: &Presupuesto) {
    let mut quitar = None;
    for (i, (c, cant)) in f.extra.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label("+");
            let mut t = num(*cant, 2);
            if ui
                .add(egui::TextEdit::singleline(&mut t).desired_width(46.0))
                .on_hover_text("Cantidad de esta partida por unidad de la de la obra")
                .changed()
                && let Some(x) = leer(&t).filter(|x| *x >= Decimal::ZERO)
            {
                *cant = x;
            }
            ui.label("×");
            let r = base.conceptos.get(c.as_str()).map_or("", |x| x.resumen.as_str());
            ui.label(format!(
                "{c} · {} · {} €",
                r.chars().take(60).collect::<String>(),
                eur(base.precio(c).unwrap_or_default())
            ));
            if ui.small_button("✕").on_hover_text("Quitar").clicked() {
                quitar = Some(i);
            }
        });
    }
    if let Some(i) = quitar {
        f.extra.remove(i);
    }
    let mut anadir = None;
    if let Some(t) = f.buscando.as_mut() {
        ui.add(
            egui::TextEdit::singleline(t)
                .hint_text("buscar en la base: variador 1,1 kw, compuerta…")
                .desired_width(320.0),
        );
        for (c, r) in buscar(base, t, 6) {
            if ui.small_button(format!("+ {c} · {r}")).clicked() {
                anadir = Some(c);
            }
        }
    }
    if let Some(c) = anadir {
        f.extra.push((c, Decimal::ONE));
        f.buscando = None;
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
                        "Revise la partida propuesta y cámbiela si no es la buena. Si la partida de la obra junta \
                         varias cosas, «+ otra partida» añade las que la completan. Al asignar, la partida de la obra \
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
                let base = Arc::clone(&v.base);
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
                                ui.vertical(|ui| {
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
                                            .button("+ otra partida")
                                            .on_hover_text(
                                                "Si la partida de la obra junta varias cosas (ventilador + variador + \
                                             compuerta), añada las otras partidas de la base que la completan",
                                            )
                                            .clicked()
                                        {
                                            f.buscando = if f.buscando.is_some() {
                                                None
                                            } else {
                                                Some(String::new())
                                            };
                                        }
                                        if ui
                                            .add_enabled(!f.partes().is_empty(), egui::Button::new("Asignar"))
                                            .clicked()
                                        {
                                            una = Some(f.codigo.clone());
                                        }
                                    });
                                    if !f.extra.is_empty() || f.buscando.is_some() {
                                        composicion(ui, f, &base);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn buscar_en_la_base_y_partes_de_una_fila() {
        let mut b = Presupuesto::new("t", "B", "B");
        b.nuevo_capitulo("B", "C", "C").unwrap();
        b.nueva_partida("C", "VAR.11", "ud", "Variador de frecuencia 1,1 kW")
            .unwrap();
        b.nueva_partida("C", "VAR.22", "ud", "Variador de frecuencia 2,2 kW")
            .unwrap();
        let r = buscar(&b, "variador 1,1 kw", 6);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "VAR.11");
        assert!(buscar(&b, "  ", 6).is_empty());
        let mut f = Fila {
            codigo: "01".into(),
            unidad: "ud".into(),
            resumen: "x".into(),
            texto: String::new(),
            candidatos: Vec::new(),
            elegido: None,
            extra: Vec::new(),
            buscando: None,
        };
        assert_eq!(f.nivel(), Nivel::Ninguno);
        f.extra.push(("VAR.11".into(), dec!(2)));
        assert_eq!(
            (f.nivel(), f.partes()),
            (Nivel::Seguro, vec![("VAR.11".to_owned(), dec!(2))])
        );
    }
}
