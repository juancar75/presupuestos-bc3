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
    /// Línea: tomar el recurso de una tarifa de subcontrata.
    pub de_tarifa: bool,
    pub tarifa_elegida: Option<ppto_tarifas::Encontrada>,
    /// Insertar encima de esta línea (F5) en vez de al final.
    pub antes: Option<String>,
}

const NATURALEZAS: [Naturaleza; 7] = [
    Naturaleza::Partida,
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
            // Subcapítulos numerados bajo su padre, como en Presto: C01.01, C01.02…
            TipoAlta::Capitulo { padre } if padre != &self.p.raiz => {
                (self.p.codigo_libre(&format!("{padre}."), 2), String::new())
            }
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
            de_tarifa: false,
            tarifa_elegida: None,
            antes: None,
        });
    }

    pub(super) fn ventana_alta(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut a) = self.alta.take() else {
            return;
        };
        let titulo = match &a.tipo {
            TipoAlta::Capitulo { padre } if padre == &self.p.raiz => "Nuevo capítulo".to_owned(),
            TipoAlta::Capitulo { padre } => format!("Nuevo subcapítulo en {padre}"),
            TipoAlta::Partida { capitulo } => format!("Nueva partida en {capitulo}"),
            TipoAlta::Linea { padre } => format!("Añadir línea a {padre}"),
        };
        let titulo = match &a.antes {
            Some(x) => format!("{titulo} (encima de {x})"),
            None => titulo,
        };
        let n_antes = acciones.len();
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
                    let txt = if padre == self.p.raiz {
                        "Crear capítulo"
                    } else {
                        "Crear subcapítulo"
                    };
                    if ui.button(txt).clicked() {
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
        // F5: lo creado se coloca encima de la línea elegida
        if cerrar
            && let Some(antes) = &a.antes
            && acciones.len() > n_antes
        {
            let nuevo = match acciones.last() {
                Some(Accion::NuevaPartida { capitulo, codigo, .. }) => Some((capitulo.clone(), codigo.clone())),
                Some(Accion::NuevoCapitulo { padre, codigo, .. }) => Some((padre.clone(), codigo.clone())),
                Some(Accion::NuevoRecurso { en, codigo, .. }) => Some((en.clone(), codigo.clone())),
                Some(Accion::AnadirLinea(padre, hijo, _)) => Some((padre.clone(), hijo.clone())),
                _ => None,
            };
            if let Some((padre, hijo)) = nuevo {
                acciones.push(Accion::Colocar(padre, hijo, antes.clone()));
            }
        }
        if abierta && !cerrar {
            self.alta = Some(a);
        }
    }

    /// Formulario de nueva línea. Devuelve `true` si hay que cerrar la ventana.
    fn form_linea(&self, ui: &mut egui::Ui, a: &mut Alta, padre: &str, acciones: &mut Vec<Accion>) -> bool {
        let en_capitulo = self.p.conceptos[padre].naturaleza == Naturaleza::Capitulo;
        if en_capitulo {
            a.nuevo = false;
        } else {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(!a.nuevo && !a.de_tarifa, "Concepto existente")
                    .clicked()
                {
                    (a.nuevo, a.de_tarifa) = (false, false);
                }
                if ui.selectable_label(a.nuevo && !a.de_tarifa, "Concepto nuevo").clicked() {
                    (a.nuevo, a.de_tarifa) = (true, false);
                }
                if ui
                    .selectable_label(a.de_tarifa, "De tarifa de subcontrata")
                    .on_hover_text("Tarifas importadas en «Tarifas…»")
                    .clicked()
                {
                    (a.nuevo, a.de_tarifa) = (false, true);
                }
            });
        }
        ui.separator();
        if a.de_tarifa && !en_capitulo {
            return self.form_linea_tarifa(ui, a, padre, acciones);
        }
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
                            let t = if n == Naturaleza::Partida {
                                "Subpartida (auxiliar descompuesto)"
                            } else {
                                nombre_naturaleza(n)
                            };
                            ui.selectable_value(&mut a.naturaleza, n, t);
                        }
                    });
            });
            if a.naturaleza == Naturaleza::Porcentaje {
                campo(ui, "Código", &mut a.codigo, 120.0);
                ui.label(
                    RichText::new("Ej.: «%MA». Se aplica sobre las líneas anteriores cuyo código empieza por lo que va antes del «%».")
                        .weak(),
                );
            } else if a.naturaleza == Naturaleza::Partida {
                campo(ui, "Código", &mut a.codigo, 120.0);
                campo(ui, "Unidad", &mut a.unidad, 60.0);
                ui.label(
                    RichText::new(
                        "Se crea vacía; después pulse su código en el descompuesto para entrar y añadir sus líneas.",
                    )
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
                .filter(|c| !en_capitulo || c.naturaleza == Naturaleza::Partida)
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
        let precio = if a.nuevo && !matches!(a.naturaleza, Naturaleza::Partida | Naturaleza::Porcentaje) {
            leer(&a.precio)
        } else {
            Some(Decimal::ZERO)
        };
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

impl Aplicacion {
    /// Línea con un recurso de una tarifa de subcontrata.
    fn form_linea_tarifa(&self, ui: &mut egui::Ui, a: &mut Alta, padre: &str, acciones: &mut Vec<Accion>) -> bool {
        if self.tarifas.tarifas.is_empty() {
            ui.label("No hay tarifas: impórtelas en «Tarifas…».");
            return false;
        }
        campo(ui, "Buscar", &mut a.buscar, 300.0);
        let encontradas = self.tarifas.buscar(&a.buscar);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for e in encontradas.into_iter().take(300) {
                let Some((t, l)) = self.tarifas.linea(e) else {
                    continue;
                };
                let txt = format!(
                    "{}  ·  {}  ·  {}  ·  {} €",
                    t.codigo(l),
                    l.unidad,
                    super::corto(&l.descripcion, 55),
                    eur(l.precio)
                );
                let elegido = a.tarifa_elegida == Some(e);
                if ui
                    .add(egui::Button::selectable(elegido, txt))
                    .on_hover_text(format!("{} · {}", t.nombre(), t.notas))
                    .clicked()
                {
                    a.tarifa_elegida = Some(e);
                }
            }
        });
        ui.separator();
        campo(ui, "Cantidad (rendimiento)", &mut a.cantidad, 90.0);
        let cantidad = leer(&a.cantidad);
        let elegida = a.tarifa_elegida.and_then(|e| self.tarifas.linea(e));
        if let Some((t, l)) = elegida
            && let Some(c) = self.p.conceptos.get(&t.codigo(l))
            && c.precio != l.precio
        {
            ui.label(
                RichText::new(format!(
                    "{} ya está en el presupuesto a {} € (la tarifa dice {} €): se usa el del presupuesto.",
                    c.codigo,
                    eur(c.precio),
                    eur(l.precio)
                ))
                .color(egui::Color32::from_rgb(220, 150, 40)),
            );
        }
        let listo = cantidad.is_some() && elegida.is_some();
        if ui.add_enabled(listo, egui::Button::new("Añadir línea")).clicked()
            && let (Some(q), Some(e)) = (cantidad, a.tarifa_elegida)
            && let Some(acc) = self.accion_tarifa(padre, e, q)
        {
            acciones.push(acc);
            return true;
        }
        false
    }
}

impl Aplicacion {
    /// Acción para añadir a `padre` la línea `e` de una tarifa: si el recurso
    /// ya está en el presupuesto se reutiliza; si no, se da de alta como
    /// subcontrata.
    pub(super) fn accion_tarifa(&self, padre: &str, e: ppto_tarifas::Encontrada, q: Decimal) -> Option<Accion> {
        let (t, l) = self.tarifas.linea(e)?;
        let codigo = t.codigo(l);
        Some(if self.p.conceptos.contains_key(&codigo) {
            Accion::AnadirLinea(padre.to_owned(), codigo, q)
        } else {
            Accion::NuevoRecurso {
                codigo,
                unidad: l.unidad.clone(),
                resumen: l.descripcion.clone(),
                naturaleza: Naturaleza::Subcontrata,
                precio: l.precio,
                en: padre.to_owned(),
                cantidad: q,
            }
        })
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
