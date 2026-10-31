// SPDX-License-Identifier: GPL-3.0-or-later
//! Menú **Obra**: seguimiento de la obra contratada.
//!
//! - **Contradictorios**: unidades nuevas o precios no previstos. Cada uno es
//!   un capítulo `CTR-01`… que se valora como el resto; sale en PDF con
//!   recuadro «Conforme» para que lo firme el cliente.
//! - **Certificaciones**: lo ejecutado cada mes, partida a partida (del mes o
//!   a origen); relación valorada en PDF y Excel. Al cerrarla, sus precios
//!   quedan congelados.

use super::asistente::abrir_en_sistema;
use super::pdf::ruta_perfiles;
use super::{Accion, Aplicacion, corto};
use crate::celdas::Edicion;
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::Decimal;
use ppto_core::formato::{cant, eur, num};
use ppto_core::seguimiento::{
    Contradictorio, EstadoContradictorio, FilaRelacion, Precios, Relacion, ResumenCertificacion, ResumenContrato,
};
use std::path::PathBuf;

const ROJO: Color32 = Color32::from_rgb(220, 80, 60);
const VERDE: Color32 = Color32::from_rgb(80, 175, 110);
const NARANJA: Color32 = Color32::from_rgb(235, 150, 40);

/// Cambios de seguimiento (van al historial como el resto).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum OpObra {
    NuevoContradictorio {
        resumen: String,
        fecha: String,
        nota: String,
    },
    /// Marcar (o desmarcar con `None`) un capítulo como contradictorio.
    Marcar(String, Option<Contradictorio>),
    Estado(String, EstadoContradictorio),
    NuevaCertificacion(String),
    FechaCertificacion(u32, String),
    Origen(u32, String, String, Decimal),
    Mes(u32, String, String, Decimal),
    Completa(u32, String, String),
    /// Cerrar con GG y BI de la obra.
    Cerrar(u32, Decimal, Decimal),
    Reabrir(u32),
    Borrar(u32),
}

impl OpObra {
    pub fn describir(&self) -> (String, Option<String>) {
        match self {
            Self::NuevoContradictorio { .. } => ("nuevo contradictorio".into(), None),
            Self::Marcar(c, Some(_)) => (format!("contradictorio {c}"), None),
            Self::Marcar(c, None) => (format!("{c} deja de ser contradictorio"), None),
            Self::Estado(c, e) => (format!("{c}: {}", e.nombre().to_lowercase()), None),
            Self::NuevaCertificacion(_) => ("nueva certificación".into(), None),
            Self::FechaCertificacion(n, _) => (format!("fecha de la certificación {n}"), Some(format!("fcert|{n}"))),
            Self::Origen(n, _, h, _) | Self::Mes(n, _, h, _) | Self::Completa(n, _, h) => {
                (format!("certificar {h} (nº {n})"), None)
            }
            Self::Cerrar(n, ..) => (format!("cerrar la certificación {n}"), None),
            Self::Reabrir(n) => (format!("reabrir la certificación {n}"), None),
            Self::Borrar(n) => (format!("borrar la certificación {n}"), None),
        }
    }

    /// Aplica el cambio; devuelve un mensaje para la barra de estado.
    pub fn aplicar(self, p: &mut ppto_core::Presupuesto) -> Result<Option<String>, ppto_core::ErrorMotor> {
        Ok(match self {
            Self::NuevoContradictorio { resumen, fecha, nota } => {
                let c = p.nuevo_contradictorio(&resumen, &fecha, &nota)?;
                Some(format!(
                    "Creado el contradictorio {c}: añada sus partidas en el árbol (capítulo {c})."
                ))
            }
            Self::Marcar(c, d) => {
                p.marcar_contradictorio(&c, d)?;
                None
            }
            Self::Estado(c, e) => {
                p.fijar_estado_contradictorio(&c, e)?;
                None
            }
            Self::NuevaCertificacion(f) => {
                let n = p.nueva_certificacion(&f)?;
                Some(format!("Abierta la certificación nº {n}."))
            }
            Self::FechaCertificacion(n, f) => {
                if let Some(c) = p.seguimiento.certificaciones.iter_mut().find(|c| c.numero == n) {
                    c.fecha = f;
                }
                None
            }
            Self::Origen(n, c, h, v) => {
                p.certificar_origen(n, &c, &h, v)?;
                None
            }
            Self::Mes(n, c, h, v) => {
                p.certificar_mes(n, &c, &h, v)?;
                None
            }
            Self::Completa(n, c, h) => {
                p.certificar_completa(n, &c, &h)?;
                None
            }
            Self::Cerrar(n, gg, bi) => {
                p.cerrar_certificacion(n, gg, bi)?;
                Some(format!("Certificación nº {n} cerrada: sus precios ya no cambian."))
            }
            Self::Reabrir(n) => {
                p.reabrir_certificacion(n)?;
                Some(format!("Certificación nº {n} reabierta: vuelve a precios actuales."))
            }
            Self::Borrar(n) => {
                p.borrar_certificacion(n)?;
                Some(format!("Certificación nº {n} borrada."))
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PestanaObra {
    Contradictorios,
    Certificaciones,
}

pub(super) struct VentanaObra {
    pub pestana: PestanaObra,
    nuevo_resumen: String,
    nuevo_fecha: String,
    nuevo_nota: String,
    /// Certificación a la vista.
    pub numero: Option<u32>,
    /// Se teclea lo del mes (si no, a origen).
    pub del_mes: bool,
    solo_movimiento: bool,
    filtro: String,
    /// A precios de venta (instaladora).
    pub venta: bool,
    nueva_fecha: String,
    edicion: Edicion,
    /// Hay que recalcular la relación.
    pub sucia: bool,
    relacion: Option<Result<Relacion, String>>,
    contrato: Option<ResumenContrato>,
    mensaje: Option<(String, bool)>,
    ultimo: Option<PathBuf>,
}

fn fecha_hoy() -> String {
    let h = ppto_biblioteca::hoy();
    let p: Vec<&str> = h.split('-').collect();
    if p.len() == 3 {
        format!("{}/{}/{}", p[2], p[1], p[0])
    } else {
        h
    }
}

impl VentanaObra {
    pub fn nueva(pestana: PestanaObra, venta: bool) -> Self {
        Self {
            pestana,
            nuevo_resumen: String::new(),
            nuevo_fecha: fecha_hoy(),
            nuevo_nota: String::new(),
            numero: None,
            del_mes: true,
            solo_movimiento: false,
            filtro: String::new(),
            venta,
            nueva_fecha: fecha_hoy(),
            edicion: Edicion::default(),
            sucia: true,
            relacion: None,
            contrato: None,
            mensaje: None,
            ultimo: None,
        }
    }

    fn guardar(&mut self, nombre: &str, ext: &str, bytes: Result<Vec<u8>, String>) {
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                self.mensaje = Some((e, true));
                return;
            }
        };
        let filtro = if ext == "pdf" { "PDF" } else { "Excel" };
        let nombre: String = nombre
            .chars()
            .map(|c| if "<>:\"/\\|?*".contains(c) { '-' } else { c })
            .collect();
        let Some(r) = rfd::FileDialog::new()
            .add_filter(filtro, &[ext])
            .set_file_name(format!("{}.{ext}", nombre.trim()))
            .save_file()
        else {
            return;
        };
        match std::fs::write(&r, bytes) {
            Ok(()) => {
                self.mensaje = Some((format!("Guardado en {}", r.display()), false));
                self.ultimo = Some(r);
            }
            Err(e) => self.mensaje = Some((format!("No se pudo guardar: {e}"), true)),
        }
    }
}

impl Aplicacion {
    fn precios_cert(&self, venta: bool) -> Precios {
        if venta {
            Precios::Venta {
                gg: self.gg,
                bi: self.bi,
            }
        } else {
            Precios::Ejecucion {
                gg: self.gg,
                bi: self.bi,
            }
        }
    }

    pub(super) fn menu_obra(&mut self, ui: &mut Ui) {
        if ui
            .button("Contradictorios…")
            .on_hover_text("Unidades o precios no previstos, para que los apruebe el cliente")
            .clicked()
        {
            ui.close();
            self.abrir_obra(PestanaObra::Contradictorios);
        }
        if ui
            .button("Certificaciones…")
            .on_hover_text("Lo ejecutado cada mes y la relación valorada")
            .clicked()
        {
            ui.close();
            self.abrir_obra(PestanaObra::Certificaciones);
        }
    }

    pub(super) fn abrir_obra(&mut self, t: PestanaObra) {
        let venta = ruta_perfiles()
            .map(|r| ppto_pdf::Perfiles::cargar(&r))
            .unwrap_or_default()
            .activo()
            .precios_venta;
        let mut v = self.ventana_obra.take().unwrap_or_else(|| VentanaObra::nueva(t, venta));
        v.pestana = t;
        v.sucia = true;
        self.ventana_obra = Some(v);
    }

    pub(super) fn ventana_obra(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_obra.take() else {
            return;
        };
        // La última certificación si no hay otra elegida (o ya no existe)
        let nums: Vec<u32> = self.p.seguimiento.certificaciones.iter().map(|c| c.numero).collect();
        if v.numero.is_none_or(|n| !nums.contains(&n)) {
            v.numero = nums.last().copied();
            v.sucia = true;
        }
        if v.sucia {
            v.contrato = self.p.resumen_contrato().ok();
            v.relacion = v.numero.map(|n| {
                self.p
                    .relacion_valorada(n, self.precios_cert(v.venta))
                    .map_err(|e| e.to_string())
            });
            v.sucia = false;
        }
        let mut abierta = true;
        let mut ops: Vec<OpObra> = Vec::new();
        let mut otras: Vec<Accion> = Vec::new();
        egui::Window::new("Seguimiento de obra")
            .open(&mut abierta)
            .default_width(1060.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut v.pestana, PestanaObra::Contradictorios, "Contradictorios");
                    ui.selectable_value(&mut v.pestana, PestanaObra::Certificaciones, "Certificaciones");
                });
                if let Some((m, error)) = v.mensaje.clone() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(m).color(if error { ROJO } else { VERDE }));
                        if !error
                            && let Some(r) = v.ultimo.clone()
                            && ui.small_button("Abrir").clicked()
                        {
                            abrir_en_sistema(&r);
                        }
                    });
                }
                ui.separator();
                match v.pestana {
                    PestanaObra::Contradictorios => self.vista_contradictorios(ui, &mut v, &mut ops, &mut otras),
                    PestanaObra::Certificaciones => self.vista_certificaciones(ui, &mut v, &mut ops),
                }
            });
        if !ops.is_empty() {
            v.sucia = true;
            self.aplicar(ops.into_iter().map(Accion::Obra).collect());
        }
        if !otras.is_empty() {
            self.aplicar(otras);
        }
        if abierta {
            self.ventana_obra = Some(v);
        }
    }

    // ------------------------------------------------------------ contradictorios

    fn vista_contradictorios(&self, ui: &mut Ui, v: &mut VentanaObra, ops: &mut Vec<OpObra>, otras: &mut Vec<Accion>) {
        if let Some(r) = v.contrato {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Contratado {} €", eur(r.contratado))).strong());
                ui.label(RichText::new(format!("+ aprobados {} €", eur(r.aprobados))).color(VERDE));
                ui.label(RichText::new(format!("= vigente {} €", eur(r.vigente))).strong());
                ui.separator();
                ui.label(RichText::new(format!("Pendientes {} €", eur(r.pendientes))).color(NARANJA));
                ui.label(RichText::new(format!("Rechazados {} €", eur(r.rechazados))).weak());
            });
            ui.label(
                RichText::new("Importes de ejecución material (sin GG, BI ni IVA).")
                    .small()
                    .weak(),
            );
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Nuevo contradictorio").strong())
            .default_open(self.p.seguimiento.contradictorios.is_empty())
            .show(ui, |ui| {
                egui::Grid::new("nuevo_ctr").num_columns(2).show(ui, |ui| {
                    ui.label("Descripción");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.nuevo_resumen)
                            .hint_text("p. ej. Rejillas adicionales en planta 2")
                            .desired_width(420.0),
                    );
                    ui.end_row();
                    ui.label("Fecha");
                    ui.add(egui::TextEdit::singleline(&mut v.nuevo_fecha).desired_width(90.0));
                    ui.end_row();
                    ui.label("Motivo");
                    ui.add(
                        egui::TextEdit::singleline(&mut v.nuevo_nota)
                            .hint_text("quién lo pide, orden, acta…")
                            .desired_width(420.0),
                    );
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    let ok = !v.nuevo_resumen.trim().is_empty();
                    if ui
                        .add_enabled(ok, egui::Button::new("Crear"))
                        .on_hover_text("Crea el capítulo CTR-nn al final de la obra")
                        .clicked()
                    {
                        ops.push(OpObra::NuevoContradictorio {
                            resumen: v.nuevo_resumen.trim().to_owned(),
                            fecha: v.nuevo_fecha.trim().to_owned(),
                            nota: v.nuevo_nota.trim().to_owned(),
                        });
                        v.nuevo_resumen.clear();
                        v.nuevo_nota.clear();
                    }
                    // Un capítulo ya hecho a mano
                    let cap = self
                        .capitulo_activo
                        .clone()
                        .filter(|c| *c != self.p.raiz && !self.p.seguimiento.contradictorios.contains_key(c));
                    if let Some(c) = cap
                        && ui
                            .button(format!("Marcar {c} como contradictorio"))
                            .on_hover_text("El capítulo elegido en el árbol")
                            .clicked()
                    {
                        let numero = self
                            .p
                            .seguimiento
                            .contradictorios
                            .values()
                            .map(|c| c.numero)
                            .max()
                            .unwrap_or(0)
                            + 1;
                        ops.push(OpObra::Marcar(
                            c,
                            Some(Contradictorio {
                                numero,
                                fecha: v.nuevo_fecha.clone(),
                                estado: EstadoContradictorio::Propuesto,
                                nota: v.nuevo_nota.clone(),
                            }),
                        ));
                    }
                });
            });
        ui.separator();
        if self.p.seguimiento.contradictorios.is_empty() {
            ui.label("Todavía no hay contradictorios.");
            return;
        }
        let precios = self.calc.precios.clone();
        let mut lista: Vec<(&String, &Contradictorio)> = self.p.seguimiento.contradictorios.iter().collect();
        lista.sort_by_key(|(_, c)| c.numero);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("contradictorios")
                .striped(true)
                .num_columns(8)
                .show(ui, |ui| {
                    for t in [
                        "Nº",
                        "Código",
                        "Descripción",
                        "Fecha",
                        "Importe €",
                        "Estado",
                        "Motivo",
                        "",
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for (codigo, c) in lista {
                        ui.label(c.numero.to_string());
                        ui.label(codigo);
                        let resumen = self
                            .p
                            .conceptos
                            .get(codigo)
                            .map(|x| x.resumen.clone())
                            .unwrap_or_default();
                        ui.label(corto(&resumen, 50));
                        if let Some(f) = v.edicion.texto(ui, &format!("ctr_fecha_{codigo}"), &c.fecha, 80.0) {
                            ops.push(OpObra::Marcar(
                                codigo.clone(),
                                Some(Contradictorio { fecha: f, ..c.clone() }),
                            ));
                        }
                        ui.label(precios.get(codigo).map(|x| eur(*x)).unwrap_or_default());
                        let mut e = c.estado;
                        let color = match e {
                            EstadoContradictorio::Aprobado => VERDE,
                            EstadoContradictorio::Propuesto => NARANJA,
                            EstadoContradictorio::Rechazado => ui.visuals().weak_text_color(),
                        };
                        egui::ComboBox::from_id_salt(("ctr_estado", codigo.as_str()))
                            .selected_text(RichText::new(e.nombre()).color(color))
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for x in EstadoContradictorio::TODOS {
                                    ui.selectable_value(&mut e, x, x.nombre());
                                }
                            });
                        if e != c.estado {
                            ops.push(OpObra::Estado(codigo.clone(), e));
                        }
                        if let Some(n) = v.edicion.texto(ui, &format!("ctr_nota_{codigo}"), &c.nota, 220.0) {
                            ops.push(OpObra::Marcar(
                                codigo.clone(),
                                Some(Contradictorio { nota: n, ..c.clone() }),
                            ));
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("Ver")
                                .on_hover_text("Ir al capítulo en el árbol")
                                .clicked()
                            {
                                otras.push(Accion::Ir(codigo.clone()));
                            }
                            if ui
                                .small_button("PDF…")
                                .on_hover_text("Para que lo firme el cliente")
                                .clicked()
                            {
                                let b = self.pdf_contradictorio(codigo, c);
                                v.guardar(&format!("Contradictorio {} - {}", c.numero, self.p.nombre), "pdf", b);
                            }
                            if ui
                                .small_button("Quitar marca")
                                .on_hover_text("Deja de ser contradictorio (el capítulo no se borra)")
                                .clicked()
                            {
                                ops.push(OpObra::Marcar(codigo.clone(), None));
                            }
                        });
                        ui.end_row();
                    }
                });
        });
    }

    /// PDF del contradictorio con el perfil activo y los datos de la obra.
    pub(super) fn pdf_contradictorio(&self, codigo: &str, c: &Contradictorio) -> Result<Vec<u8>, String> {
        let solo = self.p.solo_contradictorio(codigo).map_err(|e| e.to_string())?;
        let mut o = self.opciones_pdf_obra();
        o.tipo_documento = format!("Contradictorio nº {}", c.numero);
        o.titulo = self.p.conceptos[&self.p.raiz].resumen.clone();
        o.fecha = c.fecha.clone();
        o.revision = String::new();
        o.conformidad = true;
        o.observaciones = [
            (!c.nota.is_empty()).then(|| format!("Motivo: {}.", c.nota)),
            Some("Unidades o precios no incluidos en el presupuesto contratado. Su ejecución requiere la aprobación previa del cliente.".into()),
            Some("Mediciones estimadas: se certificará la cantidad realmente ejecutada.".into()),
        ]
        .into_iter()
        .flatten()
        .collect();
        ppto_pdf::pdf(&solo, &o).map_err(|e| format!("No se pudo generar el PDF: {e}"))
    }

    // ------------------------------------------------------------ certificaciones

    fn vista_certificaciones(&self, ui: &mut Ui, v: &mut VentanaObra, ops: &mut Vec<OpObra>) {
        let certs = &self.p.seguimiento.certificaciones;
        let abierta = certs.last().filter(|c| !c.cerrada).map(|c| c.numero);
        ui.horizontal_wrapped(|ui| {
            if !certs.is_empty() {
                ui.label("Certificación");
                let actual = v.numero;
                egui::ComboBox::from_id_salt("cert_numero")
                    .selected_text(actual.map(|n| format!("nº {n}")).unwrap_or_default())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for c in certs {
                            let t = format!(
                                "nº {} · {}{}",
                                c.numero,
                                c.fecha,
                                if c.cerrada { "" } else { " (abierta)" }
                            );
                            ui.selectable_value(&mut v.numero, Some(c.numero), t);
                        }
                    });
                if v.numero != actual {
                    v.sucia = true;
                }
            }
            if abierta.is_none() {
                ui.separator();
                ui.label("Fecha");
                ui.add(egui::TextEdit::singleline(&mut v.nueva_fecha).desired_width(80.0));
                if ui
                    .button(if certs.is_empty() {
                        "Empezar la 1ª certificación"
                    } else {
                        "Nueva certificación"
                    })
                    .on_hover_text("Parte de lo certificado a origen en la anterior")
                    .clicked()
                {
                    ops.push(OpObra::NuevaCertificacion(v.nueva_fecha.trim().to_owned()));
                    v.numero = Some(certs.last().map_or(1, |c| c.numero + 1));
                }
            }
        });
        let Some(n) = v.numero else {
            ui.add_space(8.0);
            ui.label(
                "Cada mes: nueva certificación → teclee lo ejecutado de cada partida (del mes o a origen) → PDF o Excel → Cerrar.",
            );
            return;
        };
        let Some(cert) = self.p.certificacion(n).cloned() else {
            return;
        };
        let editable = !cert.cerrada;
        ui.horizontal_wrapped(|ui| {
            if editable {
                ui.label(RichText::new("Abierta").color(NARANJA).strong());
                ui.label("Fecha");
                if let Some(f) = v.edicion.texto(ui, &format!("cert_fecha_{n}"), &cert.fecha, 80.0) {
                    ops.push(OpObra::FechaCertificacion(n, f));
                }
                ui.separator();
                ui.label("Teclear:");
                ui.selectable_value(&mut v.del_mes, true, "lo del mes");
                ui.selectable_value(&mut v.del_mes, false, "a origen");
                ui.separator();
                if ui
                    .button("Cerrar")
                    .on_hover_text("Congela los precios. Solo se puede reabrir la última.")
                    .clicked()
                {
                    ops.push(OpObra::Cerrar(n, self.gg, self.bi));
                }
                if ui.button("Borrar").on_hover_text("Borra esta certificación").clicked() {
                    ops.push(OpObra::Borrar(n));
                    v.numero = None;
                }
            } else {
                ui.label(RichText::new(format!("Cerrada · {}", cert.fecha)).color(VERDE).strong());
                if certs.last().map(|c| c.numero) == Some(n) && ui.button("Reabrir").clicked() {
                    ops.push(OpObra::Reabrir(n));
                }
            }
            ui.separator();
            if ui
                .checkbox(&mut v.venta, "A precios de venta")
                .on_hover_text(
                    "Precio de cada partida con GG y BI (o su margen), como en la oferta de la instaladora. Sin marcar: ejecución material + GG + BI en el resumen.",
                )
                .changed()
            {
                v.sucia = true;
            }
            ui.checkbox(&mut v.solo_movimiento, "Solo con algo certificado");
            ui.add(
                egui::TextEdit::singleline(&mut v.filtro)
                    .hint_text("Buscar…")
                    .desired_width(140.0),
            );
        });
        let r = match &v.relacion {
            Some(Ok(r)) => r.clone(),
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ROJO));
                return;
            }
            None => return,
        };
        let pr = self.precios_cert(v.venta);
        let resumen = self.p.resumen_certificacion(&r, pr, self.iva, self.retencion);
        self.pie_certificacion(ui, v, &r, &resumen);
        ui.separator();
        self.tabla_certificacion(ui, v, &r, editable, ops);
    }

    fn pie_certificacion(&self, ui: &mut Ui, v: &mut VentanaObra, r: &Relacion, s: &ResumenCertificacion) {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Presupuesto {} € · anterior {} € · ",
                eur(r.total.presupuesto),
                eur(r.total.anterior)
            ));
            ui.label(RichText::new(format!("este mes {} €", eur(r.total.actual))).strong());
            ui.label(format!(
                " · a origen {} € ({} %)",
                eur(r.total.origen),
                num(r.total.porcentaje(), 1)
            ));
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "A cobrar {} € (IVA {} %{})",
                    eur(s.a_cobrar),
                    num(self.iva, 0),
                    if self.retencion.is_zero() {
                        String::new()
                    } else {
                        format!(", retención {} %", num(self.retencion, 2))
                    }
                ))
                .strong()
                .color(VERDE),
            )
            .on_hover_text(format!(
                "Base del mes {} € + IVA {} € − retención {} €",
                eur(s.actual.base),
                eur(s.actual.iva),
                eur(s.retencion)
            ));
            let excesos = r.excesos().len();
            if excesos > 0 {
                ui.label(RichText::new(format!("{excesos} partida(s) por encima del presupuesto")).color(ROJO));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("PDF…")
                .on_hover_text("Relación valorada con resumen y firmas")
                .clicked()
            {
                let perfil = ruta_perfiles()
                    .map(|x| ppto_pdf::Perfiles::cargar(&x))
                    .unwrap_or_default()
                    .activo()
                    .clone();
                let obra = self.opciones_pdf_obra();
                let o = ppto_pdf::DatosCertificacion {
                    empresa: perfil.empresa,
                    empresa_datos: perfil.empresa_datos,
                    logo: perfil.logo.filter(|l| l.is_file()),
                    obra: obra.titulo,
                    cliente: obra.cliente,
                    referencia: obra.referencia,
                    fecha: r.fecha.clone(),
                    precios_venta: v.venta,
                    gastos_generales: self.gg,
                    beneficio_industrial: self.bi,
                    iva: self.iva,
                    retencion: self.retencion,
                    solo_con_movimiento: v.solo_movimiento,
                };
                let b = ppto_pdf::pdf_certificacion(&self.p, r, s, &o).map_err(|e| e.to_string());
                v.guardar(&format!("Certificación {} - {}", r.numero, self.p.nombre), "pdf", b);
            }
            if ui.button("Excel…").clicked() {
                let b = ppto_informes::excel_certificacion(&self.p, r, s, v.venta).map_err(|e| e.to_string());
                v.guardar(&format!("Certificación {} - {}", r.numero, self.p.nombre), "xlsx", b);
            }
            ui.label(
                RichText::new("GG, BI, IVA y retención de garantía: en la pestaña Resumen.")
                    .small()
                    .weak(),
            );
        });
    }

    fn tabla_certificacion(
        &self,
        ui: &mut Ui,
        v: &mut VentanaObra,
        r: &Relacion,
        editable: bool,
        ops: &mut Vec<OpObra>,
    ) {
        let dm = self.p.decimales.medicion;
        let filtro = v.filtro.to_lowercase();
        let n = r.numero;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("certificacion")
                .striped(true)
                .num_columns(12)
                .show(ui, |ui| {
                    for t in [
                        "Código",
                        "Descripción",
                        "Ud",
                        "Presup.",
                        "Precio €",
                        "Anterior",
                        "Mes",
                        "A origen",
                        "",
                        "Importe mes €",
                        "Importe origen €",
                        "%",
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for f in &r.filas {
                        match f {
                            FilaRelacion::Capitulo {
                                codigo,
                                resumen,
                                importes,
                                ..
                            } => {
                                if v.solo_movimiento && importes.origen.is_zero() {
                                    continue;
                                }
                                if !filtro.is_empty() {
                                    continue;
                                }
                                ui.label(RichText::new(codigo).strong());
                                ui.label(RichText::new(corto(resumen, 50)).strong());
                                for _ in 0..7 {
                                    ui.label("");
                                }
                                ui.label(RichText::new(eur(importes.actual)).strong());
                                ui.label(RichText::new(eur(importes.origen)).strong());
                                ui.label(RichText::new(format!("{} %", num(importes.porcentaje(), 1))).weak());
                                ui.end_row();
                            }
                            FilaRelacion::Partida(x) => {
                                if v.solo_movimiento && x.cant_origen.is_zero() {
                                    continue;
                                }
                                if !filtro.is_empty()
                                    && !x.codigo.to_lowercase().contains(&filtro)
                                    && !x.resumen.to_lowercase().contains(&filtro)
                                {
                                    continue;
                                }
                                ui.label(&x.codigo);
                                ui.label(corto(&x.resumen, 50)).on_hover_text(&x.resumen);
                                ui.label(&x.unidad);
                                ui.label(cant(x.medicion, dm));
                                ui.label(eur(x.precio));
                                ui.label(RichText::new(cant(x.cant_anterior, dm)).weak());
                                let clave = format!("cert_{n}_{}_{}", x.capitulo, x.codigo);
                                let color_origen = if x.exceso { Some(ROJO) } else { None };
                                if editable && v.del_mes {
                                    if let Some(m) = v.edicion.decimal(ui, &clave, x.cant_actual, dm, 70.0) {
                                        ops.push(OpObra::Mes(n, x.capitulo.clone(), x.codigo.clone(), m));
                                    }
                                    let t = RichText::new(cant(x.cant_origen, dm));
                                    ui.label(color_origen.map_or(t.clone(), |c| t.color(c)));
                                } else {
                                    ui.label(cant(x.cant_actual, dm));
                                    if editable {
                                        if let Some(o) = v.edicion.decimal(ui, &clave, x.cant_origen, dm, 70.0) {
                                            ops.push(OpObra::Origen(n, x.capitulo.clone(), x.codigo.clone(), o));
                                        }
                                    } else {
                                        let t = RichText::new(cant(x.cant_origen, dm));
                                        ui.label(color_origen.map_or(t.clone(), |c| t.color(c)));
                                    }
                                }
                                if editable
                                    && x.cant_origen != x.medicion
                                    && ui
                                        .small_button("100 %")
                                        .on_hover_text("Certificar a origen toda la cantidad del presupuesto")
                                        .clicked()
                                {
                                    ops.push(OpObra::Completa(n, x.capitulo.clone(), x.codigo.clone()));
                                } else if !editable || x.cant_origen == x.medicion {
                                    ui.label("");
                                }
                                ui.label(eur(x.importes.actual));
                                ui.label(eur(x.importes.origen));
                                let t = RichText::new(format!("{} %", num(x.importes.porcentaje(), 1)));
                                ui.label(if x.exceso { t.color(ROJO) } else { t.weak() });
                                ui.end_row();
                            }
                        }
                    }
                });
        });
    }
}
