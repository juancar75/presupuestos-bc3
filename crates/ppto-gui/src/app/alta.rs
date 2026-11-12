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
    /// Línea: tomar el recurso de una tarifa de subcontrata.
    pub de_tarifa: bool,
    pub tarifa_elegida: Option<ppto_tarifas::Encontrada>,
    /// Insertar encima de esta línea (F5) en vez de al final.
    pub antes: Option<String>,
    /// Biblioteca de precios para el buscador (se lee al abrir la ventana).
    pub bib: Option<std::sync::Arc<ppto_biblioteca::Biblioteca>>,
    /// Lo elegido en el buscador.
    pub hallazgo: Option<super::valorar::Hallazgo>,
    /// Poner el cursor en el buscador al abrir.
    pub foco: bool,
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
            de_tarifa: false,
            tarifa_elegida: None,
            antes: None,
            bib: None,
            hallazgo: None,
            foco: true,
        });
        if let Some(a) = self.alta.as_mut()
            && matches!(a.tipo, TipoAlta::Linea { .. })
        {
            a.bib = self
                .ruta_biblioteca
                .as_deref()
                .map(|r| std::sync::Arc::new(ppto_biblioteca::Biblioteca::cargar(r)));
        }
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
                    .selectable_label(!a.nuevo && !a.de_tarifa, "Buscar")
                    .on_hover_text("En la obra, en las tarifas de la biblioteca de precios y en las de subcontrata")
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
            // Buscador único: obra, tarifas de la biblioteca y de subcontrata.
            // ↑/↓ para elegir, Intro para añadir.
            let mut intro = false;
            ui.horizontal(|ui| {
                ui.add_sized([130.0, 20.0], egui::Label::new("Buscar"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut a.buscar)
                        .desired_width(340.0)
                        .hint_text("p. ej. «vfc 250», «cobre 3/8», «oficial»"),
                );
                if std::mem::take(&mut a.foco) {
                    r.request_focus();
                }
                if r.changed() {
                    a.hallazgo = None;
                }
                intro = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            let res = self.buscar_todo(&a.buscar, padre, a.bib.as_deref(), en_capitulo);
            let (abajo, arriba) =
                ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp)));
            let pos = a
                .hallazgo
                .as_ref()
                .and_then(|h| res.iter().position(|(x, _, _)| x == h));
            if (abajo || arriba) && !res.is_empty() {
                let i = match (pos, abajo) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1).min(res.len() - 1),
                    (Some(i), false) => i.saturating_sub(1),
                };
                a.hallazgo = Some(res[i].0.clone());
            }
            if a.hallazgo.is_none() && intro && !res.is_empty() {
                a.hallazgo = Some(res[0].0.clone());
            }
            if a.bib.is_none() && !a.buscar.trim().is_empty() {
                ui.label(
                    RichText::new(
                        "Sin biblioteca de precios: solo se busca en la obra y en las tarifas de subcontrata.",
                    )
                    .weak()
                    .small(),
                );
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (h, txt, precio) in res.iter().take(200) {
                    let elegido = a.hallazgo.as_ref() == Some(h);
                    let color = match h {
                        super::valorar::Hallazgo::Obra(_) => None,
                        super::valorar::Hallazgo::Tarifa(_) => Some(egui::Color32::from_rgb(70, 140, 210)),
                        super::valorar::Hallazgo::Subcontrata(_) => Some(egui::Color32::from_rgb(150, 110, 200)),
                    };
                    let r = ui.horizontal(|ui| {
                        let r = ui.add(egui::Button::selectable(elegido, txt.as_str()));
                        let mut p = RichText::new(precio.as_str()).small();
                        if let Some(c) = color {
                            p = p.color(c);
                        }
                        ui.label(p);
                        r
                    });
                    let r = r.inner.on_hover_text(match h {
                        super::valorar::Hallazgo::Obra(_) => "Ya está en la obra: se añade tal cual",
                        super::valorar::Hallazgo::Tarifa(_) => {
                            "Tarifa de fabricante de la biblioteca: se crea el material con su PVP y el descuento \
                             (el de la obra si ya lo tiene). Doble clic: añadir"
                        }
                        super::valorar::Hallazgo::Subcontrata(_) => "Tarifa de subcontrata. Doble clic: añadir",
                    });
                    if elegido && (abajo || arriba) {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        a.hallazgo = Some(h.clone());
                    }
                    if r.double_clicked() {
                        a.hallazgo = Some(h.clone());
                        intro = true;
                    }
                }
            });
            if intro
                && let Some(h) = &a.hallazgo
                && let Some(q) = leer(&a.cantidad)
                && let Some(acc) = self.accion_hallazgo(h, padre, q)
            {
                acciones.push(acc);
                return true;
            }
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
        let listo = cantidad.is_some() && precio.is_some() && (a.nuevo || a.hallazgo.is_some());
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
            } else if let Some(acc) = a
                .hallazgo
                .as_ref()
                .and_then(|h| self.accion_hallazgo(h, padre, cantidad))
            {
                acciones.push(acc);
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
