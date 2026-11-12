// SPDX-License-Identifier: GPL-3.0-or-later
//! Selección de varias líneas del descompuesto (como en Presto): clic,
//! Mayús+clic (un bloque) y Ctrl+clic (sueltas); Ctrl+C / Ctrl+V entre
//! partidas (y entre ventanas), Supr para quitarlas y «Copiar a varias
//! partidas…» de una vez.

use super::*;
use ppto_core::fragmento::{Fragmento, SiYaEsta};
use std::collections::BTreeSet;

/// Líneas elegidas en el descompuesto de una partida.
#[derive(Debug, Clone, Default)]
pub(super) struct LineasSel {
    pub padre: String,
    pub elegidas: Vec<String>,
    /// Última pulsada sin Mayús (origen del bloque con Mayús+clic).
    pub ancla: Option<String>,
}

/// Ventana «Copiar a varias partidas».
pub(super) struct CopiarA {
    pub f: Fragmento,
    pub origen: String,
    pub filtro: String,
    pub marcadas: BTreeSet<String>,
    pub sumar: bool,
}

/// Ventana «Guardar como kit».
pub(super) struct GuardarKit {
    pub f: Fragmento,
    pub nombre: String,
    pub unidad: String,
    /// Kits que ya hay en la biblioteca (leída al abrir).
    pub existentes: BTreeSet<String>,
}

/// Pegado pendiente de contestar (alguna línea ya estaba).
pub(super) struct PegadoPendiente {
    pub f: Fragmento,
    pub destinos: Vec<String>,
    pub repetidas: Vec<String>,
}

impl Aplicacion {
    /// Líneas elegidas de `padre` (vacío si la selección es de otra partida).
    pub(super) fn lineas_elegidas(&self, padre: &str) -> Vec<String> {
        if self.lineas_sel.padre != padre {
            return Vec::new();
        }
        let orden: Vec<&String> = self
            .p
            .conceptos
            .get(padre)
            .map_or(Vec::new(), |c| c.descomposicion.iter().map(|l| &l.hijo).collect());
        // En el orden del descompuesto, solo las que siguen ahí
        orden
            .into_iter()
            .filter(|h| self.lineas_sel.elegidas.contains(h))
            .cloned()
            .collect()
    }

    /// Clic en el código de una línea, con Mayús o Ctrl.
    pub(super) fn clic_linea(&mut self, padre: &str, hijo: &str, m: egui::Modifiers) {
        let s = &mut self.lineas_sel;
        if s.padre != padre {
            *s = LineasSel {
                padre: padre.to_owned(),
                ..Default::default()
            };
        }
        if m.shift {
            let orden: Vec<String> = self.p.conceptos.get(padre).map_or(Vec::new(), |c| {
                c.descomposicion.iter().map(|l| l.hijo.clone()).collect()
            });
            let a = s
                .ancla
                .as_ref()
                .and_then(|x| orden.iter().position(|y| y == x))
                .unwrap_or(0);
            let b = orden.iter().position(|y| y == hijo).unwrap_or(a);
            let (i, j) = (a.min(b), a.max(b));
            s.elegidas = orden[i..=j].to_vec();
        } else if m.command || m.ctrl {
            if let Some(i) = s.elegidas.iter().position(|x| x == hijo) {
                s.elegidas.remove(i);
            } else {
                s.elegidas.push(hijo.to_owned());
            }
            s.ancla = Some(hijo.to_owned());
        } else {
            s.elegidas = vec![hijo.to_owned()];
            s.ancla = Some(hijo.to_owned());
        }
    }

    /// Copia las líneas elegidas (también al portapapeles del sistema).
    pub(super) fn copiar_lineas(&mut self, padre: &str) -> bool {
        let el = self.lineas_elegidas(padre);
        if el.is_empty() {
            return false;
        }
        match ppto_core::fragmento::copiar(&self.p, padre, &el) {
            Ok(f) => {
                self.copiar_al_sistema = Some(f.a_texto());
                self.info(format!(
                    "{} líneas copiadas de {padre}. En otra partida: Ctrl+V (también en otra ventana del programa).",
                    el.len()
                ));
                true
            }
            Err(e) => {
                self.error(e.to_string());
                true
            }
        }
    }

    /// Abre «Copiar a varias partidas» con las líneas elegidas.
    pub(super) fn abrir_copiar_a(&mut self, padre: &str) {
        let el = self.lineas_elegidas(padre);
        match ppto_core::fragmento::copiar(&self.p, padre, &el) {
            Ok(f) if !el.is_empty() => {
                self.copiar_a = Some(CopiarA {
                    f,
                    origen: padre.to_owned(),
                    filtro: String::new(),
                    marcadas: BTreeSet::new(),
                    sumar: false,
                });
            }
            Ok(_) => {}
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Abre «Guardar como kit» con las líneas elegidas.
    pub(super) fn abrir_guardar_kit(&mut self, padre: &str) {
        let el = self.lineas_elegidas(padre);
        match ppto_core::fragmento::copiar(&self.p, padre, &el) {
            Ok(f) if !el.is_empty() => {
                self.guardar_kit = Some(GuardarKit {
                    f,
                    nombre: String::new(),
                    unidad: "ud".into(),
                    existentes: self
                        .ruta_biblioteca
                        .as_deref()
                        .map(ppto_biblioteca::Biblioteca::cargar)
                        .map(|b| b.kits.into_keys().collect())
                        .unwrap_or_default(),
                });
            }
            Ok(_) => {}
            Err(e) => self.error(e.to_string()),
        }
    }

    pub(super) fn ventana_guardar_kit(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.guardar_kit.take() else { return };
        let mut abierta = true;
        let mut hecho = false;
        let existe = v.existentes.contains(v.nombre.trim());
        egui::Window::new(format!("Guardar {} líneas como kit", v.f.lineas.len()))
            .open(&mut abierta)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        v.f.lineas
                            .iter()
                            .map(|l| format!("{} × {}", l.linea.rendimiento.normalize(), l.linea.hijo))
                            .collect::<Vec<_>>()
                            .join(" · "),
                    )
                    .weak()
                    .small(),
                );
                egui::Grid::new("kit_nuevo").num_columns(2).show(ui, |ui| {
                    ui.label("Nombre:");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.nombre)
                            .hint_text("p. ej. Toma de fancoil")
                            .desired_width(300.0),
                    );
                    ui.end_row();
                    ui.label("Unidad:");
                    ui.add(egui::TextEdit::singleline(&mut v.unidad).desired_width(50.0))
                        .on_hover_text("Unidad de la subpartida cuando el kit se inserta como subpartida");
                    ui.end_row();
                });
                if existe {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 140, 30),
                        "Ya hay un kit con ese nombre: se sustituye.",
                    );
                }
                ui.label(
                    RichText::new(
                        "Se guarda en la biblioteca compartida, con sus recursos y precios de ahora. \
                         Para usarlo: «+ Añadir línea…» y busque «kit».",
                    )
                    .weak()
                    .small(),
                );
                if ui
                    .add_enabled(!v.nombre.trim().is_empty(), egui::Button::new("Guardar kit"))
                    .clicked()
                {
                    hecho = true;
                }
            });
        if hecho {
            let k = ppto_biblioteca::Kit {
                nombre: v.nombre.trim().to_owned(),
                unidad: v.unidad.trim().to_owned(),
                lineas: v.f,
            };
            self.guardar_kit(k);
        } else if abierta {
            self.guardar_kit = Some(v);
        }
    }

    /// Pegar líneas en `destinos`: si alguna ya está, se pregunta.
    pub(super) fn pegar_lineas(&mut self, f: Fragmento, destinos: Vec<String>, acciones: &mut Vec<Accion>) {
        let repetidas: Vec<String> = destinos
            .iter()
            .flat_map(|d| {
                let ya: Vec<&String> = self
                    .p
                    .conceptos
                    .get(d)
                    .map_or(Vec::new(), |c| c.descomposicion.iter().map(|l| &l.hijo).collect());
                f.lineas
                    .iter()
                    .filter(move |l| ya.contains(&&l.linea.hijo))
                    .map(move |l| format!("{} en {d}", l.linea.hijo))
            })
            .collect();
        if repetidas.is_empty() {
            acciones.push(Accion::PegarLineas(Box::new(f), destinos, SiYaEsta::Dejar));
        } else {
            self.pegado_pendiente = Some(PegadoPendiente { f, destinos, repetidas });
        }
    }

    pub(super) fn ventana_pegado_pendiente(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(pp) = self.pegado_pendiente.take() else { return };
        let mut abierta = true;
        let mut r = None;
        egui::Window::new("Líneas que ya están")
            .open(&mut abierta)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} de las líneas pegadas ya están en la partida: {}.",
                    pp.repetidas.len(),
                    pp.repetidas.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
                ));
                ui.horizontal(|ui| {
                    if ui.button("Sumar la cantidad").clicked() {
                        r = Some(SiYaEsta::Sumar);
                    }
                    if ui.button("Dejarlas como están").clicked() {
                        r = Some(SiYaEsta::Dejar);
                    }
                });
                ui.label(RichText::new("Las que no estaban se añaden igual.").weak().small());
            });
        match r {
            Some(s) => acciones.push(Accion::PegarLineas(Box::new(pp.f), pp.destinos, s)),
            None if abierta => self.pegado_pendiente = Some(pp),
            None => {}
        }
    }

    /// Ventana «Copiar a varias partidas».
    pub(super) fn ventana_copiar_a(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.copiar_a.take() else { return };
        let mut abierta = true;
        let mut hecho = false;
        let orden = self.orden_partidas();
        egui::Window::new(format!("Copiar {} líneas de {} a…", v.f.lineas.len(), v.origen))
            .open(&mut abierta)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        v.f.lineas
                            .iter()
                            .map(|l| l.linea.hijo.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                    .weak()
                    .small(),
                );
                ui.horizontal(|ui| {
                    ui.label("Filtrar:");
                    ui.add(egui::TextEdit::singleline(&mut v.filtro).hint_text("capítulo, código o texto"));
                });
                let f = super::valorar::plano(&v.filtro);
                let visibles: Vec<&(String, String)> = orden
                    .iter()
                    .filter(|(_, h)| *h != v.origen)
                    .filter(|(cap, h)| {
                        f.is_empty()
                            || self.p.conceptos.get(h).is_some_and(|c| {
                                super::valorar::plano(&format!("{cap} {} {}", c.codigo, c.resumen)).contains(&f)
                            })
                    })
                    .collect();
                ui.horizontal(|ui| {
                    if ui.small_button("Marcar las visibles").clicked() {
                        v.marcadas.extend(visibles.iter().map(|(_, h)| h.clone()));
                    }
                    if ui.small_button("Desmarcar todas").clicked() {
                        v.marcadas.clear();
                    }
                });
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (cap, h) in &visibles {
                        let c = &self.p.conceptos[h];
                        let mut m = v.marcadas.contains(h);
                        let precio = self.calc.precios.get(h).copied().unwrap_or_default();
                        let t = format!("{cap} · {}  {}  ·  {} €", c.codigo, corto(&c.resumen, 50), eur(precio));
                        if ui.checkbox(&mut m, self.color_estado(RichText::new(t), h)).changed() {
                            if m {
                                v.marcadas.insert(h.clone());
                            } else {
                                v.marcadas.remove(h);
                            }
                        }
                    }
                });
                ui.checkbox(
                    &mut v.sumar,
                    "Si una línea ya está en la partida, sumar la cantidad (si no, se deja)",
                );
                if ui
                    .add_enabled(
                        !v.marcadas.is_empty(),
                        egui::Button::new(format!("Copiar a {} partidas", v.marcadas.len())),
                    )
                    .clicked()
                {
                    hecho = true;
                }
            });
        if hecho {
            let destinos: Vec<String> = orden
                .iter()
                .filter(|(_, h)| v.marcadas.contains(h))
                .map(|(_, h)| h.clone())
                .collect();
            let s = if v.sumar { SiYaEsta::Sumar } else { SiYaEsta::Dejar };
            acciones.push(Accion::PegarLineas(Box::new(v.f), destinos, s));
        } else if abierta {
            self.copiar_a = Some(v);
        }
    }

    /// Aplica [`Accion::PegarLineas`].
    pub(super) fn aplicar_pegar_lineas(
        &mut self,
        f: &Fragmento,
        destinos: &[String],
        s: SiYaEsta,
    ) -> Result<(), ppto_core::ErrorMotor> {
        let copia = self.p.clone();
        let mut n = 0;
        for d in destinos {
            match ppto_core::fragmento::pegar_en_partida(&mut self.p, f, d, s) {
                Ok(inf) => n += inf.pegados.len(),
                Err(e) => {
                    self.p = copia;
                    return Err(e);
                }
            }
        }
        self.info(format!("{n} líneas pegadas en {} partidas.", destinos.len()));
        Ok(())
    }
}
