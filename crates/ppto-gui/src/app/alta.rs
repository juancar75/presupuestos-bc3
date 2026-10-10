// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana de alta: nuevo capítulo, nueva partida o nueva línea de
//! descomposición (con un concepto existente o con un recurso nuevo).

use super::{Accion, Aplicacion, nombre_naturaleza};
use eframe::egui::{self, RichText};
use ppto_core::formato::{eur, leer};
use ppto_core::{Decimal, Naturaleza};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum TipoAlta {
    Capitulo { padre: String },
    Partida { capitulo: String },
    Linea { padre: String },
}

pub(super) struct Alta {
    pub tipo: TipoAlta,
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub cantidad: String,
    /// Línea: `true` = crear un recurso nuevo; `false` = usar uno existente.
    pub nuevo: bool,
    pub naturaleza: Naturaleza,
    pub precio: String,
    pub buscar: String,
    pub elegido: Option<String>,
}

const NATURALEZAS: [Naturaleza; 6] = [
    Naturaleza::ManoObra,
    Naturaleza::Material,
    Naturaleza::Maquinaria,
    Naturaleza::Subcontrata,
    Naturaleza::Otros,
    Naturaleza::Porcentaje,
];

impl Aplicacion {
    /// Abre la ventana de alta con un código libre ya propuesto.
    pub(super) fn abrir_alta(&mut self, tipo: TipoAlta) {
        let (codigo, unidad) = match &tipo {
            TipoAlta::Capitulo { .. } => (self.p.codigo_libre("C", 2), String::new()),
            TipoAlta::Partida { .. } => (self.p.codigo_libre("P", 4), "ud".into()),
            TipoAlta::Linea { .. } => (self.p.codigo_libre("R", 4), "h".into()),
        };
        self.alta = Some(Alta {
            tipo,
            codigo,
            unidad,
            resumen: String::new(),
            cantidad: "1".into(),
            nuevo: false,
            naturaleza: Naturaleza::ManoObra,
            precio: "0".into(),
            buscar: String::new(),
            elegido: None,
        });
    }

    pub(super) fn ventana_alta(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut a) = self.alta.take() else {
            return;
        };
        let titulo = match &a.tipo {
            TipoAlta::Capitulo { padre } => format!("Nuevo capítulo en {padre}"),
            TipoAlta::Partida { capitulo } => format!("Nueva partida en {capitulo}"),
            TipoAlta::Linea { padre } => format!("Añadir línea a {padre}"),
        };
        let mut abierta = true;
        let mut cerrar = false;
        egui::Window::new(titulo)
            .open(&mut abierta)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| match a.tipo.clone() {
                TipoAlta::Capitulo { padre } => {
                    campo(ui, "Código", &mut a.codigo, 120.0);
                    campo(ui, "Resumen", &mut a.resumen, 380.0);
                    if ui.button("Crear capítulo").clicked() {
                        acciones.push(Accion::NuevoCapitulo {
                            padre,
                            codigo: a.codigo.clone(),
                            resumen: a.resumen.clone(),
                        });
                        cerrar = true;
                    }
                }
                TipoAlta::Partida { capitulo } => {
                    campo(ui, "Código", &mut a.codigo, 120.0);
                    campo(ui, "Unidad", &mut a.unidad, 60.0);
                    campo(ui, "Resumen", &mut a.resumen, 380.0);
                    ui.label(
                        RichText::new("Se crea con cantidad 1 y sin descomposición; después añada sus líneas.").weak(),
                    );
                    if ui.button("Crear partida").clicked() {
                        acciones.push(Accion::NuevaPartida {
                            capitulo,
                            codigo: a.codigo.clone(),
                            unidad: a.unidad.clone(),
                            resumen: a.resumen.clone(),
                        });
                        cerrar = true;
                    }
                }
                TipoAlta::Linea { padre } => {
                    cerrar = self.form_linea(ui, &mut a, &padre, acciones);
                }
            });
        if abierta && !cerrar {
            self.alta = Some(a);
        }
    }

    /// Formulario de nueva línea. Devuelve `true` si hay que cerrar la ventana.
    fn form_linea(&self, ui: &mut egui::Ui, a: &mut Alta, padre: &str, acciones: &mut Vec<Accion>) -> bool {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut a.nuevo, false, "Concepto existente");
            ui.selectable_value(&mut a.nuevo, true, "Recurso nuevo");
        });
        ui.separator();
        let usados: Vec<&str> = self.p.conceptos[padre]
            .descomposicion
            .iter()
            .map(|l| l.hijo.as_str())
            .collect();
        if a.nuevo {
            ui.horizontal(|ui| {
                ui.label("Tipo");
                egui::ComboBox::from_id_salt("alta_naturaleza")
                    .selected_text(nombre_naturaleza(a.naturaleza))
                    .show_ui(ui, |ui| {
                        for n in NATURALEZAS {
                            ui.selectable_value(&mut a.naturaleza, n, nombre_naturaleza(n));
                        }
                    });
            });
            if a.naturaleza == Naturaleza::Porcentaje {
                campo(ui, "Código", &mut a.codigo, 120.0);
                ui.label(
                    RichText::new("Ej.: «%MA». Se aplica sobre las líneas anteriores cuyo código empieza por lo que va antes del «%».")
                        .weak(),
                );
            } else {
                campo(ui, "Código", &mut a.codigo, 120.0);
                campo(ui, "Unidad", &mut a.unidad, 60.0);
                campo(ui, "Precio €", &mut a.precio, 90.0);
            }
            campo(ui, "Resumen", &mut a.resumen, 380.0);
        } else {
            campo(ui, "Buscar", &mut a.buscar, 300.0);
            let filtro = sin_tildes(&a.buscar);
            let candidatos: Vec<_> = self
                .p
                .conceptos
                .values()
                .filter(|c| c.naturaleza != Naturaleza::Capitulo && c.codigo != padre)
                .filter(|c| !usados.contains(&c.codigo.as_str()))
                .filter(|c| filtro.is_empty() || sin_tildes(&format!("{} {}", c.codigo, c.resumen)).contains(&filtro))
                .take(200)
                .collect();
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for c in candidatos {
                    let precio = self.calc.precios.get(&c.codigo).copied().unwrap_or(c.precio);
                    let txt = format!(
                        "{}  ·  {}  ·  {}  ·  {} €",
                        c.codigo,
                        c.unidad,
                        super::corto(&c.resumen, 50),
                        eur(precio)
                    );
                    let elegido = a.elegido.as_deref() == Some(c.codigo.as_str());
                    if ui.add(egui::Button::selectable(elegido, txt)).clicked() {
                        a.elegido = Some(c.codigo.clone());
                    }
                }
            });
        }
        ui.separator();
        let etiqueta = if a.naturaleza == Naturaleza::Porcentaje && a.nuevo {
            "Porcentaje (puntos)"
        } else {
            "Cantidad (rendimiento)"
        };
        campo(ui, etiqueta, &mut a.cantidad, 90.0);
        let cantidad = leer(&a.cantidad);
        let precio = if a.nuevo { leer(&a.precio) } else { Some(Decimal::ZERO) };
        let listo = cantidad.is_some() && precio.is_some() && (a.nuevo || a.elegido.is_some());
        if cantidad.is_none() || precio.is_none() {
            ui.label(
                RichText::new("Número no válido (use coma decimal: 1,25).").color(egui::Color32::from_rgb(220, 80, 60)),
            );
        }
        if ui.add_enabled(listo, egui::Button::new("Añadir línea")).clicked() {
            let cantidad = cantidad.unwrap_or_default();
            if a.nuevo {
                acciones.push(Accion::NuevoRecurso {
                    codigo: a.codigo.clone(),
                    unidad: a.unidad.clone(),
                    resumen: a.resumen.clone(),
                    naturaleza: a.naturaleza,
                    precio: precio.unwrap_or_default(),
                    en: padre.to_owned(),
                    cantidad,
                });
            } else if let Some(h) = &a.elegido {
                acciones.push(Accion::AnadirLinea(padre.to_owned(), h.clone(), cantidad));
            }
            return true;
        }
        false
    }
}

fn campo(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String, ancho: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([130.0, 20.0], egui::Label::new(etiqueta));
        ui.add(egui::TextEdit::singleline(valor).desired_width(ancho));
    });
}

fn sin_tildes(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}
