// SPDX-License-Identifier: GPL-3.0-or-later
//! Vistas de la obra además de la ficha de partida:
//!
//! - **Presupuesto**: todo el presupuesto en una tabla con capítulos
//!   plegables, como la ventana principal de Presto. Cantidad y precio
//!   directo se editan en la celda.
//! - **Precios y márgenes**: lista plana de partidas con coste, venta y
//!   margen, ordenable y con filtros, para afinar la oferta.

use super::{Accion, Aplicacion, Pestana, corto};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::formato::{cant, eur, num};
use ppto_core::{Decimal, Naturaleza, venta};

/// Columna por la que se ordena «Precios y márgenes».
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(super) enum Orden {
    #[default]
    Presupuesto,
    Codigo,
    Coste,
    Venta,
    Margen,
    Importe,
}

#[derive(Default)]
pub(super) struct FiltroPrecios {
    pub texto: String,
    pub solo_sin_precio: bool,
    /// Solo partidas con margen sobre venta por debajo de este % (vacío: todas).
    pub margen_bajo: Option<Decimal>,
    pub orden: Orden,
    pub descendente: bool,
}

/// Fila de «Precios y márgenes».
#[derive(Clone, Debug)]
pub(super) struct FilaPrecio {
    pub capitulo: String,
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub cantidad: Decimal,
    pub coste: Decimal,
    pub venta: Decimal,
    pub margen: Option<Decimal>,
    pub sobrecoste: Option<Decimal>,
    pub bloqueada: bool,
    pub importe_venta: Decimal,
}

/// Anchos de las columnas de la tabla de presupuesto.
const W_COD: f32 = 120.0;
const W_UD: f32 = 36.0;
const W_RES: f32 = 340.0;
const W_NUM: f32 = 86.0;

fn celda(ui: &mut Ui, ancho: f32, texto: RichText) -> egui::Response {
    ui.add_sized([ancho, 18.0], egui::Label::new(texto).truncate())
}

fn celda_num(ui: &mut Ui, ancho: f32, texto: RichText) {
    ui.allocate_ui_with_layout(
        egui::vec2(ancho, 18.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| ui.label(texto),
    );
}

impl Aplicacion {
    // ------------------------------------------------------------ presupuesto

    pub(super) fn pestana_presupuesto(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal(|ui| {
            if ui.button("Desplegar todo").clicked() {
                self.plegados.clear();
            }
            if ui.button("Plegar todo").clicked() {
                self.plegados = self
                    .p
                    .conceptos
                    .values()
                    .filter(|c| c.naturaleza == Naturaleza::Capitulo)
                    .map(|c| c.codigo.clone())
                    .collect();
            }
            ui.label(
                RichText::new(
                    "Clic en el código de una partida: abre su ficha · cantidad y precio directo se editan aquí",
                )
                .weak()
                .small(),
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            celda(ui, W_COD, RichText::new("Código").weak());
            celda(ui, W_UD, RichText::new("Ud").weak());
            celda(ui, W_RES, RichText::new("Resumen").weak());
            celda_num(ui, W_NUM, RichText::new("Cantidad").weak());
            celda_num(ui, W_NUM, RichText::new("Precio").weak());
            celda_num(ui, W_NUM + 14.0, RichText::new("Importe").weak());
        });
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let raiz = self.p.raiz.clone();
            self.filas_presupuesto(ui, &raiz, 0, acciones);
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_space(18.0 + W_COD + W_UD + W_RES + 2.0 * W_NUM + 30.0);
                celda_num(
                    ui,
                    W_NUM + 14.0,
                    RichText::new(format!("{} €", eur(self.calc.pem))).strong(),
                );
                ui.label(RichText::new("PEM").weak());
            });
        });
    }

    fn filas_presupuesto(&mut self, ui: &mut Ui, padre: &str, nivel: usize, acciones: &mut Vec<Accion>) {
        let Some(lineas) = self.calc.lineas.get(padre).cloned() else {
            return;
        };
        let d = self.p.decimales;
        let sangria = 14.0 * nivel as f32;
        for l in lineas {
            let Some(c) = self.p.conceptos.get(&l.hijo).cloned() else {
                continue;
            };
            if c.naturaleza == Naturaleza::Capitulo {
                let plegado = self.plegados.contains(&c.codigo);
                ui.horizontal(|ui| {
                    ui.add_space(sangria);
                    if ui
                        .add_sized(
                            [16.0, 18.0],
                            egui::Button::new(if plegado { "⏵" } else { "⏷" }).frame(false),
                        )
                        .clicked()
                    {
                        if plegado {
                            self.plegados.remove(&c.codigo);
                        } else {
                            self.plegados.insert(c.codigo.clone());
                        }
                    }
                    celda(ui, W_COD - sangria, RichText::new(&c.codigo).strong());
                    celda(ui, W_UD, RichText::new(""));
                    celda(ui, W_RES + 2.0 * W_NUM + 14.0, RichText::new(&c.resumen).strong());
                    celda_num(
                        ui,
                        W_NUM + 14.0,
                        RichText::new(format!("{} €", eur(l.importe))).strong(),
                    );
                });
                if !plegado {
                    self.filas_presupuesto(ui, &c.codigo, nivel + 1, acciones);
                }
                continue;
            }
            ui.horizontal(|ui| {
                ui.add_space(sangria + 18.0);
                let r = ui.add_sized(
                    [W_COD - sangria, 18.0],
                    egui::Button::new(RichText::new(&c.codigo).monospace()).frame(false),
                );
                if r.on_hover_text("Abrir la ficha de la partida").clicked() {
                    acciones.push(Accion::Seleccionar(padre.to_owned(), c.codigo.clone()));
                }
                celda(ui, W_UD, RichText::new(&c.unidad));
                celda(ui, W_RES, RichText::new(&c.resumen)).on_hover_text(&c.resumen);
                if let Some(v) =
                    self.edicion
                        .decimal(ui, &format!("vp|r|{padre}|{}", c.codigo), l.cantidad, d.medicion, W_NUM)
                {
                    acciones.push(Accion::Rendimiento(padre.to_owned(), c.codigo.clone(), v));
                }
                // Precio: editable si es directo (sin descomponer); si no, el calculado
                if c.descomposicion.is_empty() && c.naturaleza == Naturaleza::Partida {
                    if let Some(v) = self
                        .edicion
                        .decimal(ui, &format!("vp|p|{}", c.codigo), l.precio, d.precio, W_NUM)
                    {
                        acciones.push(Accion::Precio(c.codigo.clone(), v));
                    }
                } else {
                    let t = RichText::new(eur(l.precio));
                    celda_num(
                        ui,
                        W_NUM + 6.0,
                        if l.precio.is_zero() {
                            t.color(Color32::from_rgb(220, 90, 80))
                        } else {
                            t
                        },
                    );
                }
                celda_num(ui, W_NUM + 14.0, RichText::new(format!("{} €", eur(l.importe))));
            });
        }
    }

    // ---------------------------------------------------- precios y márgenes

    /// Todas las partidas con coste, venta y margen (sin filtrar ni ordenar).
    pub(super) fn filas_precios(&self) -> Vec<FilaPrecio> {
        let d = self.p.decimales.importe;
        let mut v = Vec::new();
        for (cap, hijo) in self.orden_partidas() {
            let Some(l) = self
                .calc
                .lineas
                .get(&cap)
                .and_then(|v| v.iter().find(|l| l.hijo == hijo))
            else {
                continue;
            };
            let Some(c) = self.p.conceptos.get(&hijo) else { continue };
            let vt = venta::venta_concepto(l.precio, c, self.gg, self.bi, d);
            let margen = (!vt.venta.is_zero())
                .then(|| venta::margen_obtenido(l.precio, vt.venta).ok().map(|m| m.round_dp(2)))
                .flatten();
            v.push(FilaPrecio {
                capitulo: cap.clone(),
                codigo: hijo.clone(),
                unidad: c.unidad.clone(),
                resumen: c.resumen.clone(),
                cantidad: l.cantidad,
                coste: l.precio,
                venta: vt.venta,
                margen,
                sobrecoste: c.margen,
                bloqueada: c.venta_fija.is_some(),
                importe_venta: ppto_core::redondear(vt.venta * l.cantidad, d),
            });
        }
        v
    }

    /// Filas con los filtros y el orden de la vista.
    pub(super) fn filas_precios_filtradas(&self) -> Vec<FilaPrecio> {
        let f = &self.filtro_precios;
        let texto = ppto_biblioteca::plano(&f.texto);
        let palabras: Vec<&str> = texto.split_whitespace().collect();
        let mut v: Vec<FilaPrecio> = self
            .filas_precios()
            .into_iter()
            .filter(|x| !f.solo_sin_precio || x.coste.is_zero())
            .filter(|x| f.margen_bajo.is_none_or(|m| x.margen.is_none_or(|g| g < m)))
            .filter(|x| {
                let t = ppto_biblioteca::plano(&format!("{} {} {}", x.capitulo, x.codigo, x.resumen));
                palabras.iter().all(|p| t.contains(p))
            })
            .collect();
        let clave = |x: &FilaPrecio| match f.orden {
            Orden::Presupuesto | Orden::Codigo => Decimal::ZERO,
            Orden::Coste => x.coste,
            Orden::Venta => x.venta,
            Orden::Margen => x.margen.unwrap_or(Decimal::MIN),
            Orden::Importe => x.importe_venta,
        };
        match f.orden {
            Orden::Presupuesto => {}
            Orden::Codigo => v.sort_by(|a, b| a.codigo.cmp(&b.codigo)),
            _ => v.sort_by_key(clave),
        }
        if f.descendente {
            v.reverse();
        }
        v
    }

    pub(super) fn pestana_precios(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let d = self.p.decimales;
        ui.horizontal(|ui| {
            ui.label("Buscar:");
            ui.add(
                egui::TextEdit::singleline(&mut self.filtro_precios.texto)
                    .desired_width(200.0)
                    .hint_text("código, capítulo o texto"),
            );
            ui.checkbox(&mut self.filtro_precios.solo_sin_precio, "Solo sin precio");
            let mut bajo = self.filtro_precios.margen_bajo.is_some();
            if ui.checkbox(&mut bajo, "Margen por debajo de").changed() {
                self.filtro_precios.margen_bajo = bajo.then(|| Decimal::from(15));
            }
            if let Some(m) = self.filtro_precios.margen_bajo
                && let Some(n) = self.edicion.decimal(ui, "vp|margen_bajo", m, 1, 50.0)
            {
                self.filtro_precios.margen_bajo = Some(n);
            }
            if self.filtro_precios.margen_bajo.is_some() {
                ui.label("%");
            }
        });
        let filas = self.filas_precios_filtradas();
        let total_v: Decimal = filas.iter().map(|x| x.importe_venta).sum();
        let total_c: Decimal = filas
            .iter()
            .map(|x| ppto_core::redondear(x.coste * x.cantidad, d.importe))
            .sum();
        ui.horizontal(|ui| {
            ui.label(format!("{} partidas", filas.len()));
            ui.separator();
            ui.label(format!("Coste {} €", eur(total_c)));
            ui.label(format!("Venta {} €", eur(total_v)));
            if let Ok(m) = venta::margen_obtenido(total_c, total_v)
                && !total_v.is_zero()
            {
                ui.label(RichText::new(format!("Margen {} %", num(m, 2))).strong());
            }
        });
        ui.separator();
        let mut cambiar_orden = None;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("vista_precios")
                .striped(true)
                .num_columns(10)
                .show(ui, |ui| {
                    let f = &self.filtro_precios;
                    for (t, o) in [
                        ("Capítulo", Some(Orden::Presupuesto)),
                        ("Código", Some(Orden::Codigo)),
                        ("Resumen", None),
                        ("Cantidad", None),
                        ("Ud", None),
                        ("Coste €/ud", Some(Orden::Coste)),
                        ("Sobrecoste %", None),
                        ("Venta €/ud", Some(Orden::Venta)),
                        ("Margen s/venta %", Some(Orden::Margen)),
                        ("Importe venta", Some(Orden::Importe)),
                    ] {
                        match o {
                            Some(o) => {
                                let flecha = if f.orden == o {
                                    if f.descendente { " ⏷" } else { " ⏶" }
                                } else {
                                    ""
                                };
                                if ui
                                    .add(egui::Button::new(RichText::new(format!("{t}{flecha}")).strong()).frame(false))
                                    .on_hover_text("Ordenar por esta columna (otro clic invierte el orden)")
                                    .clicked()
                                {
                                    cambiar_orden = Some(o);
                                }
                            }
                            None => {
                                ui.label(RichText::new(t).strong());
                            }
                        }
                    }
                    ui.end_row();
                    for x in &filas {
                        ui.label(&x.capitulo);
                        if ui
                            .link(&x.codigo)
                            .on_hover_text("Abrir la ficha de la partida")
                            .clicked()
                        {
                            acciones.push(Accion::Seleccionar(x.capitulo.clone(), x.codigo.clone()));
                        }
                        ui.label(corto(&x.resumen, 46)).on_hover_text(&x.resumen);
                        ui.label(cant(x.cantidad, d.medicion));
                        ui.label(&x.unidad);
                        let coste = RichText::new(eur(x.coste));
                        ui.label(if x.coste.is_zero() {
                            coste.color(Color32::from_rgb(220, 90, 80))
                        } else {
                            coste
                        });
                        if x.bloqueada {
                            ui.label(RichText::new("bloqueada").weak())
                                .on_hover_text("Precio de venta bloqueado en la ficha de la partida");
                        } else if let Some(n) =
                            self.edicion
                                .opcional(ui, &format!("vp|m|{}", x.codigo), x.sobrecoste, 2, 60.0)
                        {
                            acciones.push(Accion::Margen(x.codigo.clone(), n));
                        }
                        ui.label(eur(x.venta));
                        let m = x.margen.map_or("—".to_owned(), |m| num(m, 2));
                        let bajo = self
                            .filtro_precios
                            .margen_bajo
                            .zip(x.margen)
                            .is_some_and(|(lim, g)| g < lim);
                        ui.label(if bajo {
                            RichText::new(m).color(Color32::from_rgb(230, 150, 60))
                        } else {
                            RichText::new(m)
                        });
                        ui.label(format!("{} €", eur(x.importe_venta)));
                        ui.end_row();
                    }
                });
        });
        if let Some(o) = cambiar_orden {
            let f = &mut self.filtro_precios;
            if f.orden == o {
                f.descendente = !f.descendente;
            } else {
                f.orden = o;
                f.descendente = matches!(o, Orden::Importe | Orden::Coste | Orden::Venta);
            }
        }
    }
}

/// Atajos Ctrl+1…Ctrl+6 → vista.
pub(super) const VISTAS: [(Pestana, &str, egui::Key); 6] = [
    (Pestana::Partida, "Ficha de partida", egui::Key::Num1),
    (Pestana::Presupuesto, "Presupuesto", egui::Key::Num2),
    (Pestana::Precios, "Precios y márgenes", egui::Key::Num3),
    (Pestana::Recursos, "Recursos", egui::Key::Num4),
    (Pestana::Subcontratar, "Subcontratar", egui::Key::Num5),
    (Pestana::Resumen, "Resumen y venta", egui::Key::Num6),
];
