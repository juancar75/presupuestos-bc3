// SPDX-License-Identifier: GPL-3.0-or-later
//! «Comparar con obras parecidas»: los ratios de la obra (€/plaza, €/m²,
//! horas/plaza…) frente a la mediana de las obras del mismo tipo y tamaño
//! parecido guardadas en la biblioteca. Al guardar la obra se actualiza su
//! huella en la biblioteca, para que la siguiente obra aprenda de esta.

use super::*;
use ppto_biblioteca::Biblioteca;
use ppto_biblioteca::huellas::{self, Comparacion, Estado, Huella};

/// Desviación a partir de la cual se marca en rojo (%).
const LIMITE: i64 = 25;

impl Aplicacion {
    /// Estado de la oferta: adjudicada si está contratada; si no, el elegido
    /// en Datos de la obra (ofertada por defecto).
    pub(super) fn estado_oferta(&self) -> Estado {
        if self.p.seguimiento.contrato.is_some() {
            return Estado::Adjudicada;
        }
        self.obra_textos
            .get("estado")
            .and_then(|e| Estado::leer(e))
            .unwrap_or_default()
    }

    fn huella_actual(&self) -> Result<Huella, String> {
        let nombre = self
            .obra_textos
            .get("titulo")
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.p.nombre.clone());
        let es: Vec<String> = self.especialidades.iter().map(|e| e.clave().to_owned()).collect();
        huellas::huella(
            &self.p,
            &nombre,
            &es,
            &self.ficha,
            self.estado_oferta(),
            &ppto_biblioteca::hoy(),
        )
    }

    /// Tras guardar: la huella de la obra a la biblioteca (si tiene
    /// especialidad y algún dato del edificio).
    pub(super) fn guardar_huella(&mut self) {
        if self.especialidades.is_empty() || self.ficha.is_empty() {
            return;
        }
        let Some(r) = self.ruta_biblioteca.clone() else { return };
        let Ok(h) = self.huella_actual() else { return };
        let clave = ppto_propuestas::clave(&self.p);
        let _ = Biblioteca::modificar(&r, |b| {
            // Solo si cambia (cada cambio lleva sello)
            if b.huellas.get(&clave) != Some(&h) {
                b.huellas.insert(clave, h);
            }
        });
    }

    pub(super) fn abrir_parecidas(&mut self) {
        let clave = ppto_propuestas::clave(&self.p);
        let b = self
            .ruta_biblioteca
            .as_deref()
            .map(Biblioteca::cargar)
            .unwrap_or_default();
        self.parecidas = Some(match self.huella_actual() {
            Ok(h) => huellas::comparar(&clave, &h, &b.huellas),
            Err(e) => Comparacion {
                avisos: vec![format!("No se puede valorar la obra: {e}")],
                ..Default::default()
            },
        });
    }

    /// Primer capítulo de la obra de ese grupo (para «Ir»).
    fn capitulo_del_grupo(&self, grupo: &str) -> Option<(String, String)> {
        self.p.conceptos.values().find_map(|padre| {
            padre.descomposicion.iter().find_map(|l| {
                let h = self.p.conceptos.get(&l.hijo)?;
                (h.naturaleza == Naturaleza::Capitulo && huellas::grupo_de(&h.resumen) == Some(grupo))
                    .then(|| (padre.codigo.clone(), h.codigo.clone()))
            })
        })
    }

    pub(super) fn ventana_parecidas(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(c) = &self.parecidas else { return };
        let mut abierta = true;
        let (mut otra_vez, mut guardar, mut copiar, mut ir) = (false, false, None, None);
        egui::Window::new("Comparar con obras parecidas")
            .open(&mut abierta)
            .default_width(720.0)
            .show(ctx, |ui| {
                let mut cab = format!("{} obras parecidas", c.parecidas.len());
                if let Some(m) = c.medida {
                    cab += &format!(" (tamaño por {m})");
                }
                if c.solo_adjudicadas {
                    cab += ", solo adjudicadas";
                }
                if c.ampliado {
                    cab += ", ampliado a todas las del mismo tipo";
                }
                ui.label(RichText::new(cab).strong());
                for a in &c.avisos {
                    ui.colored_label(egui::Color32::from_rgb(210, 150, 30), format!("⚠ {a}"));
                }
                if !c.parecidas.is_empty() {
                    egui::CollapsingHeader::new("Obras usadas").show(ui, |ui| {
                        for (_, n, f, e, t) in &c.parecidas {
                            ui.label(format!(
                                "{n} · {f} · {}{}",
                                e.nombre(),
                                t.map_or(String::new(), |t| format!(" · {}", num(t, 0)))
                            ));
                        }
                    });
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    egui::Grid::new("parecidas").striped(true).num_columns(6).show(ui, |ui| {
                        for t in ["Ratio", "Esta obra", "Mediana", "Mín.–máx.", "Desv.", ""] {
                            ui.label(RichText::new(t).strong());
                        }
                        ui.end_row();
                        let f = |x: Option<Decimal>| x.map_or("—".to_owned(), |x| num(x, 2));
                        for r in &c.ratios {
                            let fuera = r.desviacion.is_some_and(|d| d.abs() > Decimal::from(LIMITE));
                            let nombre = RichText::new(&r.nombre);
                            ui.label(if fuera { nombre.color(egui::Color32::from_rgb(210, 60, 60)) } else { nombre });
                            ui.label(f(r.esta));
                            ui.label(f(r.mediana)).on_hover_text(format!("{} obras", r.n));
                            ui.label(format!("{}–{}", f(r.minimo), f(r.maximo)));
                            let d = r.desviacion.map_or("—".to_owned(), |d| format!("{d:+} %"));
                            if fuera {
                                ui.colored_label(egui::Color32::from_rgb(210, 60, 60), d);
                            } else {
                                ui.label(d);
                            }
                            let grupo = r.nombre.split(" €/").next().unwrap_or("");
                            match self.capitulo_del_grupo(grupo) {
                                Some(x) if ui.small_button("Ir").on_hover_text("Ir al capítulo").clicked() => {
                                    ir = Some(x);
                                }
                                _ => {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "En rojo, lo que se aparta más de un {LIMITE} % de la mediana. Al guardar la obra (con especialidad \\
                         y datos de la obra) su huella pasa a la biblioteca compartida; las contratadas cuentan como \\
                         adjudicadas. Los importes de otros años no se actualizan."
                    ))
                    .weak()
                    .small(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Volver a comparar").clicked() {
                        otra_vez = true;
                    }
                    if ui
                        .button("Guardar esta obra para comparar")
                        .on_hover_text("Pasa ya la huella de esta obra a la biblioteca (se hace sola al guardar)")
                        .clicked()
                    {
                        guardar = true;
                    }
                    if ui.button("Copiar").clicked() {
                        copiar = Some(c.texto(Decimal::from(LIMITE)));
                    }
                });
            });
        if let Some(t) = copiar {
            ctx.copy_text(t);
        }
        if let Some((padre, cap)) = ir {
            acciones.push(Accion::Seleccionar(padre, cap));
        }
        if guardar {
            if self.especialidades.is_empty() || self.ficha.is_empty() {
                self.error("Falta la especialidad o los datos de la obra (Obra → Datos de la obra).");
            } else {
                self.guardar_huella();
                self.info("Huella de la obra guardada en la biblioteca.");
            }
        }
        if !abierta {
            self.parecidas = None;
        } else if otra_vez || guardar {
            self.abrir_parecidas();
        }
    }
}
