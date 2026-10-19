// SPDX-License-Identifier: GPL-3.0-or-later
//! Propuestas de la IA (P-021).
//!
//! - El programa publica la obra abierta en `obra-activa.json` (como mucho
//!   cada 1,5 s y solo si cambió), para que la IA la lea por MCP.
//! - La IA deja propuestas en `propuestas.json`; aquí se vigila el fichero
//!   y se muestran las pendientes de esta obra para aceptarlas o rechazarlas.
//!   Aceptar se puede deshacer (Ctrl+Z) y la propuesta vuelve a pendiente.

use super::{Accion, Aplicacion};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::formato::{eur, num};
use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta, Propuesta};
use ppto_core::{Decimal, Naturaleza};
use ppto_propuestas::{Bandeja, ObraActiva, clave};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Estado de la conexión con la IA.
pub(super) struct Enlace {
    /// `None` en las pruebas: no se escribe ni se lee nada del disco.
    pub ruta_activa: Option<PathBuf>,
    pub ruta_bandeja: Option<PathBuf>,
    pub bandeja: Bandeja,
    pub sucia: bool,
    ultima_copia: Option<Instant>,
    ultima_lectura: Option<Instant>,
    mtime: Option<SystemTime>,
    pub ventana: bool,
    /// Solo las de confianza alta en «Aceptar todas».
    pub solo_alta: bool,
    /// Pulsado «Reaplicar aceptadas» (se convierte en acción en el fotograma).
    pub reaplicar: bool,
}

impl Enlace {
    pub fn sin_disco() -> Self {
        Self {
            ruta_activa: None,
            ruta_bandeja: None,
            bandeja: Bandeja::default(),
            sucia: false,
            ultima_copia: None,
            ultima_lectura: None,
            mtime: None,
            ventana: false,
            solo_alta: true,
            reaplicar: false,
        }
    }

    pub fn en_disco() -> Self {
        Self {
            ruta_activa: ppto_propuestas::ruta_obra_activa(),
            ruta_bandeja: ppto_propuestas::ruta_bandeja(),
            sucia: true,
            ..Self::sin_disco()
        }
    }

    /// Cambia el estado de una propuesta en memoria y en el fichero.
    pub fn marcar(&mut self, id: u64, estado: EstadoPropuesta) {
        self.bandeja.marcar(id, estado);
        if let Some(r) = &self.ruta_bandeja
            && let Ok(b) = Bandeja::modificar(r, |b| {
                b.marcar(id, estado);
                b.clone()
            })
        {
            self.bandeja = b;
            self.mtime = std::fs::metadata(r).and_then(|m| m.modified()).ok();
        }
    }
}

fn color_confianza(c: Confianza) -> Color32 {
    match c {
        Confianza::Alta => Color32::from_rgb(80, 175, 110),
        Confianza::Media => Color32::from_rgb(220, 150, 40),
        Confianza::Baja => Color32::from_rgb(220, 80, 60),
    }
}

impl Aplicacion {
    /// Publica la obra y lee la bandeja. Se llama en cada fotograma.
    pub(super) fn sincronizar_ia(&mut self, ctx: &egui::Context) {
        let e = &mut self.enlace;
        if e.ruta_activa.is_none() && e.ruta_bandeja.is_none() {
            return;
        }
        let ahora = Instant::now();
        if e.sucia && e.ultima_copia.is_none_or(|t| ahora - t > Duration::from_millis(1500)) {
            if let Some(r) = &e.ruta_activa {
                let _ = ObraActiva::guardar(&self.p, r);
            }
            e.sucia = false;
            e.ultima_copia = Some(ahora);
        }
        if e.ultima_lectura.is_none_or(|t| ahora - t > Duration::from_secs(2)) {
            e.ultima_lectura = Some(ahora);
            if let Some(r) = &e.ruta_bandeja {
                let m = std::fs::metadata(r).and_then(|m| m.modified()).ok();
                if m != e.mtime {
                    e.mtime = m;
                    e.bandeja = Bandeja::cargar(r);
                }
            }
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }

    pub(super) fn propuestas_pendientes(&self) -> Vec<Propuesta> {
        self.enlace
            .bandeja
            .pendientes(&clave(&self.p))
            .into_iter()
            .cloned()
            .collect()
    }

    /// Propuestas ya aceptadas (la última de cada partida) cuya partida está
    /// ahora sin precio ni descompuesto: pasa al volver a abrir el Excel ciego.
    pub(super) fn reaplicables(&self) -> Vec<Propuesta> {
        let mut ult: std::collections::BTreeMap<String, Propuesta> = std::collections::BTreeMap::new();
        for p in self.enlace.bandeja.de_obra(&clave(&self.p)) {
            if p.estado != EstadoPropuesta::Aceptada {
                continue;
            }
            let vacia = self.p.conceptos.get(&p.partida).is_some_and(|c| {
                c.naturaleza == Naturaleza::Partida && c.descomposicion.is_empty() && c.precio.is_zero()
            });
            if vacia && ult.get(&p.partida).is_none_or(|x| x.id < p.id) {
                ult.insert(p.partida.clone(), p.clone());
            }
        }
        ult.into_values().collect()
    }

    /// Vuelve a aplicar las propuestas aceptadas de partidas que están vacías.
    /// Devuelve cuántas se aplicaron y las que fallaron.
    pub(super) fn reaplicar_aceptadas(&mut self) -> (usize, Vec<String>) {
        let mut n = 0;
        let mut fallos = Vec::new();
        for prop in self.reaplicables() {
            match self.p.aplicar_propuesta(&prop) {
                Ok(_) => n += 1,
                Err(e) => fallos.push(format!("{} ({e})", prop.partida)),
            }
        }
        (n, fallos)
    }

    pub(super) fn boton_propuestas(&mut self, ui: &mut Ui) {
        let r = self.reaplicables().len();
        if r > 0
            && ui
                .button(RichText::new(format!("Reaplicar aceptadas ({r})")).strong())
                .on_hover_text(
                    "Hay propuestas que ya aceptó para esta obra cuyas partidas están ahora sin precio \
                     (por ejemplo, al abrir de nuevo el Excel ciego). Pulse para recuperarlas.",
                )
                .clicked()
        {
            self.enlace.reaplicar = true;
        }
        let n = self.propuestas_pendientes().len();
        let txt = if n > 0 {
            RichText::new(format!("✨ Propuestas IA ({n})")).strong()
        } else {
            RichText::new("✨ Propuestas IA")
        };
        if ui
            .button(txt)
            .on_hover_text(
                "Precios y descompuestos que propone una IA conectada por MCP. Nada cambia hasta que los acepte.",
            )
            .clicked()
        {
            self.enlace.ventana = true;
        }
    }

    pub(super) fn ventana_propuestas(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        if !self.enlace.ventana {
            return;
        }
        let mut abierta = true;
        let pendientes = self.propuestas_pendientes();
        egui::Window::new("Propuestas de la IA")
            .open(&mut abierta)
            .default_width(760.0)
            .default_height(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                if pendientes.is_empty() {
                    ui.label("No hay propuestas pendientes para esta obra.");
                    ui.label(
                        RichText::new(
                            "Pida a la IA (Claude Desktop u otra con MCP) que use «abrir_obra_activa» y valore las \
                             partidas sin precio; sus propuestas aparecerán aquí.",
                        )
                        .weak(),
                    );
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label(if pendientes.len() == 1 {
                        "1 pendiente.".to_owned()
                    } else {
                        format!("{} pendientes.", pendientes.len())
                    });
                    ui.checkbox(&mut self.enlace.solo_alta, "solo confianza alta");
                    let elegibles: Vec<u64> = pendientes
                        .iter()
                        .filter(|p| !self.enlace.solo_alta || p.confianza == Confianza::Alta)
                        .map(|p| p.id)
                        .collect();
                    if ui
                        .add_enabled(
                            !elegibles.is_empty(),
                            egui::Button::new(format!("Aceptar {} propuestas", elegibles.len())),
                        )
                        .clicked()
                    {
                        acciones.extend(elegibles.into_iter().map(Accion::AceptarPropuesta));
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for prop in &pendientes {
                        self.tarjeta_propuesta(ui, prop, acciones);
                    }
                });
            });
        self.enlace.ventana = abierta;
    }

    fn tarjeta_propuesta(&self, ui: &mut Ui, prop: &Propuesta, acciones: &mut Vec<Accion>) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let c = self.p.conceptos.get(&prop.partida);
            let sim = self.p.simular_propuesta(prop);
            ui.horizontal(|ui| {
                if ui
                    .link(RichText::new(format!(
                        "#{} {}  {}",
                        prop.id,
                        prop.partida,
                        c.map(|c| super::corto(&c.resumen, 50))
                            .or_else(|| prop.nueva.as_ref().map(|n| format!(
                                "NUEVA en {}: {}",
                                n.capitulo,
                                super::corto(&n.resumen, 40)
                            )))
                            .unwrap_or_default()
                    )))
                    .on_hover_text("Ir a la partida")
                    .clicked()
                    && let Some((cap, h)) = self.orden_partidas().into_iter().find(|(_, h)| *h == prop.partida)
                {
                    acciones.push(Accion::Seleccionar(cap, h));
                }
                ui.label(
                    RichText::new(format!("confianza {}", prop.confianza.nombre()))
                        .color(color_confianza(prop.confianza)),
                );
            });
            match &sim {
                Ok(s) => {
                    let q = self
                        .calc
                        .cantidades
                        .get(&prop.partida)
                        .copied()
                        .or_else(|| prop.nueva.as_ref().map(|n| n.cantidad))
                        .unwrap_or_default();
                    let unidad = c
                        .map(|c| c.unidad.as_str())
                        .or_else(|| prop.nueva.as_ref().map(|n| n.unidad.as_str()))
                        .unwrap_or("");
                    ui.label(
                        RichText::new(format!(
                            "actual {} €  ·  propuesto {} €/{}  ·  {} {}  ·  importe {} €",
                            eur(s.precio_antes),
                            eur(s.precio_despues),
                            unidad,
                            num(q, 2),
                            unidad,
                            eur(ppto_core::redondear(s.precio_despues * q, 2))
                        ))
                        .strong(),
                    );
                    for (cod, antes, despues) in &s.ligados {
                        ui.label(
                            RichText::new(format!(
                                "{cod}: pasa a precio de tarifa con descuento ({} → {} €)",
                                eur(*antes),
                                eur(*despues)
                            ))
                            .color(Color32::from_rgb(90, 150, 230)),
                        );
                    }
                    for (f, pct) in &s.descuentos_nuevos {
                        ui.label(
                            RichText::new(format!("Descuento {f} {} % (estimado por la IA)", num(*pct, 2)))
                                .color(Color32::from_rgb(220, 150, 40)),
                        );
                    }
                    for (cod, obra, propuesto) in &s.precios_distintos {
                        ui.label(
                            RichText::new(format!(
                                "{cod}: se usa el precio de la obra {} € (la IA indicó {} €)",
                                eur(*obra),
                                eur(*propuesto)
                            ))
                            .color(Color32::from_rgb(220, 150, 40)),
                        );
                    }
                }
                Err(e) => {
                    ui.label(RichText::new(format!("No se puede aplicar: {e}")).color(Color32::from_rgb(220, 80, 60)));
                }
            }
            ui.label(RichText::new(&prop.justificacion).italics());
            if let Contenido::Descompuesto(lineas) = &prop.contenido {
                egui::CollapsingHeader::new(format!("Descompuesto ({} líneas)", lineas.len()))
                    .id_salt(("prop", prop.id))
                    .show(ui, |ui| {
                        egui::Grid::new(("prop_lineas", prop.id))
                            .striped(true)
                            .num_columns(6)
                            .show(ui, |ui| {
                                for h in ["Código", "Tipo", "Ud", "Resumen", "Rend.", "Precio €"] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for l in lineas {
                                    let existe = self.p.conceptos.get(&l.codigo);
                                    ui.label(if existe.is_some() {
                                        RichText::new(&l.codigo)
                                    } else {
                                        RichText::new(format!("{} (nuevo)", l.codigo)).strong()
                                    });
                                    let n = existe.map(|x| x.naturaleza).or(l.naturaleza);
                                    ui.label(n.map(super::nombre_naturaleza).unwrap_or("?"));
                                    ui.label(existe.map_or(l.unidad.as_str(), |x| x.unidad.as_str()));
                                    ui.label(super::corto(
                                        existe.map_or(l.resumen.as_str(), |x| x.resumen.as_str()),
                                        45,
                                    ));
                                    ui.label(num(l.rendimiento, 3));
                                    let liga = existe.is_some_and(|x| x.tarifa.is_none() && x.naturaleza.es_basico());
                                    let precio = match existe {
                                        Some(_) if liga && l.pvp.is_some() && l.fabricante.is_some() => self.p.neto(
                                            l.pvp.unwrap_or_default(),
                                            dto_de(
                                                &self.p,
                                                prop,
                                                l.fabricante.as_deref().unwrap_or(""),
                                                l.grupo.as_deref(),
                                            ),
                                        ),
                                        Some(x) if x.naturaleza != Naturaleza::Porcentaje => {
                                            self.calc.precios.get(&x.codigo).copied().unwrap_or(x.precio)
                                        }
                                        Some(_) => Decimal::ZERO,
                                        None => match (l.pvp, &l.fabricante) {
                                            (Some(pvp), Some(f)) => {
                                                self.p.neto(pvp, dto_de(&self.p, prop, f, l.grupo.as_deref()))
                                            }
                                            _ => l.precio.unwrap_or_default(),
                                        },
                                    };
                                    let tarifa = match (existe.filter(|x| x.tarifa.is_some()), l.pvp, &l.fabricante) {
                                        (None, Some(pvp), Some(f)) => format!(
                                            "  (PVP {} − {} % {})",
                                            eur(pvp),
                                            num(dto_de(&self.p, prop, f, l.grupo.as_deref()), 2),
                                            ppto_core::descuentos::clave(f, l.grupo.as_deref())
                                        ),
                                        _ => String::new(),
                                    };
                                    ui.label(if n == Some(Naturaleza::Porcentaje) {
                                        "%".to_owned()
                                    } else {
                                        format!("{}{tarifa}", eur(precio))
                                    });
                                    ui.end_row();
                                }
                            });
                    });
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(sim.is_ok(), egui::Button::new(RichText::new("✔ Aceptar").strong()))
                    .clicked()
                {
                    acciones.push(Accion::AceptarPropuesta(prop.id));
                }
                if ui.button("✖ Rechazar").clicked() {
                    acciones.push(Accion::RechazarPropuesta(prop.id));
                }
            });
        });
    }

    /// Aplica una propuesta aceptada. Devuelve el texto para el mensaje.
    pub(super) fn aceptar_propuesta(&mut self, id: u64) -> Result<String, String> {
        let prop = self
            .enlace
            .bandeja
            .buscar(id)
            .cloned()
            .ok_or_else(|| format!("la propuesta #{id} ya no existe"))?;
        if prop.estado != EstadoPropuesta::Pendiente {
            return Err(format!("la propuesta #{id} ya no está pendiente"));
        }
        let a = self.p.aplicar_propuesta(&prop).map_err(|e| e.to_string())?;
        self.enlace.marcar(id, EstadoPropuesta::Aceptada);
        Ok(format!(
            "Propuesta #{id} aceptada: {} pasa de {} € a {} €{}.",
            prop.partida,
            eur(a.precio_antes),
            eur(a.precio_despues),
            if a.nuevos.is_empty() {
                String::new()
            } else {
                format!("; recursos nuevos {}", a.nuevos.join(", "))
            }
        ))
    }
}

/// Descuento que se aplicará: el de la obra si existe; si no, el estimado por la IA.
/// Descuento que tendrá la línea al aceptar: los de la obra mandan y la
/// propuesta añade los que falten; el del grupo, si existe, y si no el general.
fn dto_de(p: &ppto_core::Presupuesto, prop: &Propuesta, fabricante: &str, grupo: Option<&str>) -> Decimal {
    use ppto_core::descuentos::{clave, normalizar};
    let mut m = prop.descuentos.clone();
    m.extend(p.descuentos.iter().map(|(k, v)| (k.clone(), *v)));
    grupo
        .and_then(|g| m.get(&clave(fabricante, Some(g))))
        .or_else(|| m.get(&normalizar(fabricante)))
        .copied()
        .unwrap_or_default()
}

/// Número de la propuesta en la etiqueta de Deshacer («propuesta IA #12»).
pub(super) fn id_en_etiqueta(que: &str) -> Option<u64> {
    que.strip_prefix("propuesta IA #")?.parse().ok()
}
