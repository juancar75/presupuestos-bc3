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
use ppto_core::compras::{ControlRecurso, EstadoPedido};
use ppto_core::coste_real::{CosteReal, Gasto, TipoGasto};
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
    RegistrarPedido {
        pedido: Box<ppto_core::pedidos::Pedido>,
        fecha: String,
        referencia: String,
    },
    EstadoPedido(u32, EstadoPedido),
    CantidadPedido(u32, String, Decimal),
    Recibido(u32, String, Decimal),
    RecibirTodo(u32),
    BorrarPedido(u32),
    /// Fecha, referencia, entrega y nota.
    DatosPedido(u32, [String; 4]),
    /// Descuento comercial (None = quitarlo).
    Descuento(Option<ppto_core::venta::DescuentoComercial>),
    /// Contratar con GG y BI de la obra.
    Contratar(String, Decimal, Decimal),
    AnularContrato,
    AnotarGasto(Box<Gasto>),
    CambiarGasto(Box<Gasto>),
    BorrarGasto(u32),
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
            Self::RegistrarPedido { pedido, .. } => (format!("registrar pedido a {}", pedido.nombre()), None),
            Self::EstadoPedido(n, e) => (format!("pedido {n}: {}", e.nombre().to_lowercase()), None),
            Self::CantidadPedido(n, c, _) => (format!("cantidad de {c} en el pedido {n}"), None),
            Self::Recibido(n, c, _) => (format!("recibido {c} del pedido {n}"), None),
            Self::RecibirTodo(n) => (format!("recibir todo el pedido {n}"), None),
            Self::BorrarPedido(n) => (format!("borrar el pedido {n}"), None),
            Self::DatosPedido(n, _) => (format!("datos del pedido {n}"), Some(format!("dped|{n}"))),
            Self::Descuento(_) => ("descuento comercial".into(), Some("descuento".into())),
            Self::Contratar(..) => ("contratar la obra".into(), None),
            Self::AnularContrato => ("anular el contrato".into(), None),
            Self::AnotarGasto(g) => (format!("gasto {} €", eur(g.importe)), None),
            Self::CambiarGasto(g) => (format!("gasto nº {}", g.numero), None),
            Self::BorrarGasto(n) => (format!("borrar el gasto nº {n}"), None),
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
            Self::RegistrarPedido {
                pedido,
                fecha,
                referencia,
            } => Some(match p.registrar_pedido(&pedido, &fecha, &referencia) {
                Some(n) => format!(
                    "Pedido nº {n} a {} registrado en la obra (borrador): Obra → Pedidos.",
                    pedido.nombre()
                ),
                None => format!("Todo lo de {} ya está pedido.", pedido.nombre()),
            }),
            Self::EstadoPedido(n, e) => {
                p.fijar_estado_pedido(n, e)?;
                None
            }
            Self::CantidadPedido(n, c, v) => {
                p.fijar_cantidad_pedido(n, &c, v)?;
                None
            }
            Self::Recibido(n, c, v) => {
                p.recibir(n, &c, v)?;
                None
            }
            Self::RecibirTodo(n) => {
                p.recibir_todo(n)?;
                None
            }
            Self::BorrarPedido(n) => {
                p.borrar_pedido(n)?;
                None
            }
            Self::DatosPedido(n, [f, r, e, nota]) => {
                p.fijar_datos_pedido(n, &f, &r, &e, &nota)?;
                None
            }
            Self::Descuento(d) => {
                p.seguimiento.descuento = d.filter(|d| !d.es_nulo());
                None
            }
            Self::Contratar(fecha, gg, bi) => {
                let k = p.contratar(&fecha, gg, bi)?;
                Some(format!(
                    "Obra contratada por {} € sin IVA: la venta de {} partidas queda bloqueada.",
                    eur(k.importe),
                    k.bloqueadas.len()
                ))
            }
            Self::AnularContrato => {
                p.anular_contrato();
                Some("Contrato anulado: las ventas vuelven a calcularse con los márgenes.".into())
            }
            Self::AnotarGasto(g) => {
                let n = p.anotar_gasto(*g)?;
                Some(format!("Gasto nº {n} anotado."))
            }
            Self::CambiarGasto(g) => {
                p.cambiar_gasto(*g)?;
                None
            }
            Self::BorrarGasto(n) => {
                p.borrar_gasto(n);
                None
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PestanaObra {
    Contradictorios,
    Certificaciones,
    Pedidos,
    Compras,
    CosteReal,
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
    /// Pedido registrado a la vista.
    pub pedido: Option<u32>,
    /// Control de compras: solo lo pendiente o con exceso.
    solo_pendiente: bool,
    compras: Option<Result<Vec<ControlRecurso>, String>>,
    coste: Option<Result<CosteReal, String>>,
    /// Gasto que se está escribiendo (número 0 = nuevo).
    gasto: Gasto,
    /// Textos del formulario: horas, €/h, importe.
    gasto_horas: String,
    gasto_tarifa: String,
    gasto_importe: String,
    edicion: Edicion,
    /// Hay que recalcular la relación.
    pub sucia: bool,
    relacion: Option<Result<Relacion, String>>,
    contrato: Option<ResumenContrato>,
    mensaje: Option<(String, bool)>,
    ultimo: Option<PathBuf>,
}

fn gasto_vacio() -> Gasto {
    Gasto {
        numero: 0,
        fecha: fecha_hoy(),
        tipo: TipoGasto::Material,
        proveedor: String::new(),
        documento: String::new(),
        concepto: String::new(),
        capitulo: None,
        horas: None,
        importe: Decimal::ZERO,
        pedido: None,
    }
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
            pedido: None,
            solo_pendiente: true,
            compras: None,
            coste: None,
            gasto: gasto_vacio(),
            gasto_horas: String::new(),
            gasto_tarifa: String::new(),
            gasto_importe: String::new(),
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
        let t = if self.p.seguimiento.contrato.is_some() {
            "Contrato 🔒…"
        } else {
            "Contratar…"
        };
        if ui
            .button(t)
            .on_hover_text("Bloquea los precios de venta de la obra vendida y guarda la revisión del contrato")
            .clicked()
        {
            ui.close();
            self.ver_contratar = true;
        }
        ui.separator();
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
            v.compras = Some(self.p.control_compras().map_err(|e| e.to_string()));
            v.coste = Some(self.p.coste_real(self.gg, self.bi).map_err(|e| e.to_string()));
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
                    ui.selectable_value(&mut v.pestana, PestanaObra::Pedidos, "Pedidos");
                    ui.selectable_value(&mut v.pestana, PestanaObra::Compras, "Control de compras");
                    ui.selectable_value(&mut v.pestana, PestanaObra::CosteReal, "Coste real");
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
                    PestanaObra::Pedidos => self.vista_pedidos(ui, &mut v, &mut ops),
                    PestanaObra::Compras => self.vista_compras(ui, &mut v),
                    PestanaObra::CosteReal => self.vista_coste_real(ui, &mut v, &mut ops),
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

impl Aplicacion {
    // ------------------------------------------------------------ pedidos

    fn vista_pedidos(&self, ui: &mut Ui, v: &mut VentanaObra, ops: &mut Vec<OpObra>) {
        let pedidos = &self.p.seguimiento.pedidos;
        if pedidos.is_empty() {
            ui.label("No hay pedidos registrados.");
            ui.label(
                RichText::new(
                    "Informes → Pedido a fabricante (o a subcontrata) → «Registrar en la obra»: queda aquí con su estado y lo recibido. \
                     Cada pedido nuevo al mismo proveedor propone solo lo que falta por pedir.",
                )
                .weak(),
            );
            return;
        }
        if v.pedido.is_none_or(|n| !pedidos.iter().any(|p| p.numero == n)) {
            v.pedido = pedidos.last().map(|p| p.numero);
        }
        egui::Panel::left("obra_pedidos")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for p in pedidos.iter().rev() {
                        let color = match p.situacion() {
                            "Recibido" => VERDE,
                            "Borrador" => NARANJA,
                            "Anulado" => ui.visuals().weak_text_color(),
                            _ => ui.visuals().text_color(),
                        };
                        let t = RichText::new(format!(
                            "nº {} · {}\n{} · {} € · {}",
                            p.numero,
                            p.nombre(),
                            p.fecha,
                            eur(p.total()),
                            p.situacion()
                        ))
                        .color(color);
                        if ui.selectable_label(v.pedido == Some(p.numero), t).clicked() {
                            v.pedido = Some(p.numero);
                        }
                    }
                });
            });
        let Some(p) = v.pedido.and_then(|n| self.p.pedido_registrado(n)).cloned() else {
            return;
        };
        let n = p.numero;
        ui.horizontal(|ui| {
            ui.heading(format!("Pedido nº {n} · {}", p.nombre()));
            ui.label(RichText::new(format!("{} € · {}", eur(p.total()), p.situacion())).strong());
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Estado");
            let mut e = p.estado;
            egui::ComboBox::from_id_salt(("estado_pedido", n))
                .selected_text(e.nombre())
                .width(100.0)
                .show_ui(ui, |ui| {
                    for x in EstadoPedido::TODOS {
                        ui.selectable_value(&mut e, x, x.nombre());
                    }
                });
            if e != p.estado {
                ops.push(OpObra::EstadoPedido(n, e));
            }
            let mut datos = [p.fecha.clone(), p.referencia.clone(), p.entrega.clone(), p.nota.clone()];
            let mut cambio = false;
            for (i, (t, ancho)) in [("Fecha", 80.0), ("Ref.", 110.0), ("Entrega", 80.0), ("Nota", 200.0)]
                .into_iter()
                .enumerate()
            {
                ui.label(t);
                if let Some(x) = v.edicion.texto(ui, &format!("ped_{n}_{i}"), &datos[i], ancho) {
                    datos[i] = x;
                    cambio = true;
                }
            }
            if cambio {
                ops.push(OpObra::DatosPedido(n, datos));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("PDF…")
                .on_hover_text("Pedido para enviar al proveedor")
                .clicked()
            {
                let perfil = ruta_perfiles()
                    .map(|r| ppto_pdf::Perfiles::cargar(&r))
                    .unwrap_or_default()
                    .activo()
                    .clone();
                let o = ppto_pdf::DatosPedido {
                    empresa: perfil.empresa,
                    empresa_datos: perfil.empresa_datos,
                    logo: perfil.logo.filter(|l| l.is_file()),
                    fecha: p.fecha.clone(),
                    referencia: if p.referencia.is_empty() {
                        format!("Pedido nº {n}")
                    } else {
                        format!("{} · pedido nº {n}", p.referencia)
                    },
                    proveedor: Vec::new(),
                    condiciones: [p.entrega.trim(), p.nota.trim()]
                        .iter()
                        .filter(|x| !x.is_empty())
                        .enumerate()
                        .map(|(i, x)| {
                            if i == 0 && !p.entrega.trim().is_empty() {
                                format!("Entrega: {x}")
                            } else {
                                (*x).to_owned()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(". "),
                };
                let b = ppto_pdf::pdf_pedido(&self.p, &p.como_pedido(), &o).map_err(|e| e.to_string());
                v.guardar(&format!("Pedido {n} {} - {}", p.nombre(), self.p.nombre), "pdf", b);
            }
            if ui.button("Excel…").clicked() {
                let cab = ppto_informes::CabeceraPedido {
                    fecha: p.fecha.clone(),
                    proveedor: Vec::new(),
                    condiciones: p.nota.clone(),
                };
                let b = ppto_informes::excel_pedido(&self.p, &p.como_pedido(), &cab).map_err(|e| e.to_string());
                v.guardar(&format!("Pedido {n} {} - {}", p.nombre(), self.p.nombre), "xlsx", b);
            }
            if p.estado == EstadoPedido::Enviado && ui.button("Recibido todo").clicked() {
                ops.push(OpObra::RecibirTodo(n));
            }
            if p.estado == EstadoPedido::Borrador
                && ui
                    .button("Borrar")
                    .on_hover_text("Solo en borrador; si ya se envió, anúlelo")
                    .clicked()
            {
                ops.push(OpObra::BorrarPedido(n));
                v.pedido = None;
            }
        });
        ui.label(
            RichText::new(match p.estado {
                EstadoPedido::Borrador => "Borrador: ajuste las cantidades (0 quita la línea) y pase a «Enviado».",
                EstadoPedido::Enviado => "Enviado: anote lo recibido de cada línea.",
                EstadoPedido::Anulado => "Anulado: no cuenta en el control de compras.",
            })
            .small()
            .weak(),
        );
        ui.separator();
        let dm = self.p.decimales.medicion;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new(("lineas_pedido", n))
                .striped(true)
                .num_columns(7)
                .show(ui, |ui| {
                    for t in [
                        "Código",
                        "Descripción",
                        "Ud",
                        "Cantidad",
                        "Precio €",
                        "Importe €",
                        "Recibido",
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for l in &p.lineas {
                        ui.label(&l.codigo);
                        ui.label(corto(&l.resumen, 50)).on_hover_text(&l.resumen);
                        ui.label(&l.unidad);
                        if p.estado == EstadoPedido::Borrador {
                            if let Some(x) =
                                v.edicion
                                    .decimal(ui, &format!("pedc_{n}_{}", l.codigo), l.cantidad, dm, 70.0)
                            {
                                ops.push(OpObra::CantidadPedido(n, l.codigo.clone(), x));
                            }
                        } else {
                            ui.label(cant(l.cantidad, dm));
                        }
                        ui.label(eur(l.precio));
                        ui.label(eur(l.importe()));
                        if p.estado == EstadoPedido::Enviado {
                            if let Some(x) =
                                v.edicion
                                    .decimal(ui, &format!("pedr_{n}_{}", l.codigo), l.recibido, dm, 70.0)
                            {
                                ops.push(OpObra::Recibido(n, l.codigo.clone(), x));
                            }
                        } else {
                            ui.label(cant(l.recibido, dm));
                        }
                        ui.end_row();
                    }
                });
        });
    }

    // ------------------------------------------------------------ control de compras

    fn vista_compras(&self, ui: &mut Ui, v: &mut VentanaObra) {
        let lista = match &v.compras {
            Some(Ok(l)) => l.clone(),
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ROJO));
                return;
            }
            None => return,
        };
        let excesos = lista.iter().filter(|r| r.exceso()).count();
        let por_pedir = lista.iter().filter(|r| !r.por_pedir().is_zero()).count();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} recursos · {por_pedir} con algo por pedir", lista.len()));
            if excesos > 0 {
                ui.label(RichText::new(format!("· {excesos} pedidos por encima del presupuesto")).color(ROJO));
            }
            ui.separator();
            ui.checkbox(&mut v.solo_pendiente, "Solo lo pendiente o con exceso");
            ui.add(
                egui::TextEdit::singleline(&mut v.filtro)
                    .hint_text("Buscar…")
                    .desired_width(140.0),
            );
        });
        ui.label(
            RichText::new(
                "Certificado: lo que hace falta para lo certificado a origen en la última certificación. Material, maquinaria, otros y subcontratas.",
            )
            .small()
            .weak(),
        );
        ui.separator();
        let filtro = v.filtro.to_lowercase();
        let dm = self.p.decimales.medicion;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("control_compras")
                .striped(true)
                .num_columns(10)
                .show(ui, |ui| {
                    for t in [
                        "Código",
                        "Descripción",
                        "Proveedor",
                        "Ud",
                        "Presupuesto",
                        "Certificado",
                        "Pedido",
                        "Recibido",
                        "Por pedir",
                        "Por recibir",
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for r in &lista {
                        if v.solo_pendiente && r.por_pedir().is_zero() && r.por_recibir().is_zero() && !r.exceso() {
                            continue;
                        }
                        if !filtro.is_empty()
                            && !r.codigo.to_lowercase().contains(&filtro)
                            && !r.resumen.to_lowercase().contains(&filtro)
                            && !r.proveedor.to_lowercase().contains(&filtro)
                        {
                            continue;
                        }
                        ui.label(&r.codigo);
                        ui.label(corto(&r.resumen, 44)).on_hover_text(&r.resumen);
                        ui.label(corto(&r.proveedor, 20));
                        ui.label(&r.unidad);
                        ui.label(cant(r.presupuestado, dm));
                        ui.label(cant(r.certificado, dm));
                        let t = RichText::new(cant(r.pedido, dm));
                        ui.label(if r.exceso() { t.color(ROJO).strong() } else { t })
                            .on_hover_text(if r.exceso() {
                                "Pedido por encima del presupuesto"
                            } else {
                                ""
                            });
                        ui.label(cant(r.recibido, dm));
                        let pp = r.por_pedir();
                        ui.label(if pp.is_zero() {
                            RichText::new("")
                        } else {
                            RichText::new(cant(pp, dm)).color(NARANJA)
                        });
                        ui.label(cant(r.por_recibir(), dm));
                        ui.end_row();
                    }
                });
        });
    }
}

impl Aplicacion {
    // ------------------------------------------------------------ coste real

    fn vista_coste_real(&self, ui: &mut Ui, v: &mut VentanaObra, ops: &mut Vec<OpObra>) {
        let c = match &v.coste {
            Some(Ok(c)) => c.clone(),
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ROJO));
                return;
            }
            None => return,
        };
        let t = &c.total;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Gastado {} € de {} € previstos",
                    eur(t.real.total()),
                    eur(t.previsto.total())
                ))
                .strong(),
            );
            if let Some(g) = t.gastado_pct() {
                ui.label(format!("({} %)", num(g, 1)));
            }
            ui.separator();
            ui.label(format!(
                "Certificado {} € de {} € ({} %)",
                eur(t.certificado),
                eur(t.venta),
                t.certificado_pct().map(|x| num(x, 1)).unwrap_or_default()
            ));
            ui.separator();
            let m = t.margen_real();
            ui.label(
                RichText::new(format!("Margen real a fecha {} €", eur(m)))
                    .strong()
                    .color(if m.is_sign_negative() { ROJO } else { VERDE }),
            )
            .on_hover_text(format!(
                "Certificado − gastado. Margen previsto de la obra: {} €",
                eur(t.margen_previsto())
            ));
            if !c.horas_reales.is_zero() {
                ui.separator();
                ui.label(format!("{} h de mano de obra apuntadas", num(c.horas_reales, 1)));
            }
            if ui.button("Excel…").clicked() {
                let b =
                    ppto_informes::excel_coste_real(&self.p, &c, &self.p.seguimiento.gastos).map_err(|e| e.to_string());
                v.guardar(&format!("Coste real - {}", self.p.nombre), "xlsx", b);
            }
        });
        if !c.indirectos_previstos.is_zero() {
            ui.label(
                RichText::new(format!(
                    "Previsto = coste directo. Los costes indirectos previstos ({} €) no se reparten por tipo.",
                    eur(c.indirectos_previstos)
                ))
                .small()
                .weak(),
            );
        }
        ui.separator();
        egui::CollapsingHeader::new(
            RichText::new(if v.gasto.numero == 0 {
                "Anotar gasto".to_owned()
            } else {
                format!("Cambiar el gasto nº {}", v.gasto.numero)
            })
            .strong(),
        )
        .id_salt("form_gasto")
        .default_open(true)
        .show(ui, |ui| self.form_gasto(ui, v, ops));
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("coste_real")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("tabla_coste_real")
                    .striped(true)
                    .num_columns(14)
                    .show(ui, |ui| {
                        for h in [
                            "Capítulo",
                            "Resumen",
                            "Material",
                            "",
                            "Mano de obra",
                            "",
                            "Subcontrata",
                            "",
                            "Otros",
                            "",
                            "Total",
                            "",
                            "% gastado",
                            "% certificado",
                        ] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for h in [
                            "", "", "previsto", "real", "previsto", "real", "previsto", "real", "previsto", "real",
                            "previsto", "real", "", "",
                        ] {
                            ui.label(RichText::new(h).small().weak());
                        }
                        ui.end_row();
                        let fila = |ui: &mut Ui, f: &ppto_core::coste_real::FilaCosteReal, fuerte: bool| {
                            let txt = |s: String| {
                                if fuerte {
                                    RichText::new(s).strong()
                                } else {
                                    RichText::new(s)
                                }
                            };
                            ui.label(txt(f.capitulo.clone()));
                            ui.label(txt(corto(&f.resumen, 36)));
                            for t in TipoGasto::TODOS {
                                let (pv, re) = (f.previsto.de(t), f.real.de(t));
                                ui.label(RichText::new(eur(pv)).weak());
                                let r = txt(eur(re));
                                ui.label(if re > pv && !pv.is_zero() { r.color(ROJO) } else { r });
                            }
                            ui.label(RichText::new(eur(f.previsto.total())).weak());
                            ui.label(txt(eur(f.real.total())));
                            let g =
                                RichText::new(f.gastado_pct().map(|x| format!("{} %", num(x, 1))).unwrap_or_default());
                            ui.label(if f.alerta() { g.color(ROJO).strong() } else { g })
                                .on_hover_text(if f.alerta() {
                                    "Se ha gastado más de lo que se ha certificado"
                                } else {
                                    ""
                                });
                            ui.label(
                                f.certificado_pct()
                                    .map(|x| format!("{} %", num(x, 1)))
                                    .unwrap_or_default(),
                            );
                            ui.end_row();
                        };
                        for f in &c.capitulos {
                            fila(ui, f, false);
                        }
                        fila(ui, &c.total, true);
                    });
                ui.add_space(8.0);
                ui.label(RichText::new("Gastos apuntados").strong());
                let mut gastos = self.p.seguimiento.gastos.clone();
                gastos.reverse();
                egui::Grid::new("gastos").striped(true).num_columns(9).show(ui, |ui| {
                    for h in [
                        "Nº",
                        "Fecha",
                        "Tipo",
                        "Proveedor",
                        "Documento",
                        "Concepto",
                        "Capítulo",
                        "Importe €",
                        "",
                    ] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for g in &gastos {
                        ui.label(g.numero.to_string());
                        ui.label(&g.fecha);
                        ui.label(g.tipo.nombre());
                        ui.label(corto(&g.proveedor, 24));
                        ui.label(&g.documento);
                        ui.label(corto(&g.concepto, 30)).on_hover_text(&g.concepto);
                        ui.label(g.capitulo.clone().unwrap_or_else(|| "obra".into()));
                        ui.label(eur(g.importe));
                        ui.horizontal(|ui| {
                            if ui.small_button("Cambiar").clicked() {
                                v.gasto = g.clone();
                                v.gasto_horas = g.horas.map(|h| num(h, 2)).unwrap_or_default();
                                v.gasto_importe = num(g.importe, 2);
                            }
                            if ui.small_button("Borrar").clicked() {
                                ops.push(OpObra::BorrarGasto(g.numero));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
    }

    fn form_gasto(&self, ui: &mut Ui, v: &mut VentanaObra, ops: &mut Vec<OpObra>) {
        let leer = |t: &str| ppto_core::formato::leer(t);
        let capitulos: Vec<(String, String)> = self
            .p
            .concepto(&self.p.raiz)
            .map(|r| {
                r.descomposicion
                    .iter()
                    .filter_map(|l| self.p.conceptos.get(&l.hijo))
                    .filter(|c| c.naturaleza == ppto_core::Naturaleza::Capitulo)
                    .map(|c| (c.codigo.clone(), c.resumen.clone()))
                    .collect()
            })
            .unwrap_or_default();
        egui::Grid::new("gasto_form").num_columns(4).show(ui, |ui| {
            ui.label("Fecha");
            ui.add(egui::TextEdit::singleline(&mut v.gasto.fecha).desired_width(90.0));
            ui.label("Tipo");
            egui::ComboBox::from_id_salt("gasto_tipo")
                .selected_text(v.gasto.tipo.nombre())
                .show_ui(ui, |ui| {
                    for t in TipoGasto::TODOS {
                        ui.selectable_value(&mut v.gasto.tipo, t, t.nombre());
                    }
                });
            ui.end_row();
            ui.label("Proveedor");
            ui.add(
                egui::TextEdit::singleline(&mut v.gasto.proveedor)
                    .hint_text("o subcontrata, operario…")
                    .desired_width(220.0),
            );
            ui.label("Documento");
            ui.add(
                egui::TextEdit::singleline(&mut v.gasto.documento)
                    .hint_text("nº de factura, parte…")
                    .desired_width(160.0),
            );
            ui.end_row();
            ui.label("Concepto");
            ui.add(egui::TextEdit::singleline(&mut v.gasto.concepto).desired_width(220.0));
            ui.label("Capítulo");
            let sel = v
                .gasto
                .capitulo
                .as_ref()
                .and_then(|c| capitulos.iter().find(|x| x.0 == *c))
                .map_or("Toda la obra".to_owned(), |x| corto(&format!("{} {}", x.0, x.1), 40));
            egui::ComboBox::from_id_salt("gasto_capitulo")
                .selected_text(sel)
                .width(220.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut v.gasto.capitulo, None, "Toda la obra");
                    for (c, r) in &capitulos {
                        ui.selectable_value(&mut v.gasto.capitulo, Some(c.clone()), corto(&format!("{c} {r}"), 50));
                    }
                });
            ui.end_row();
            if v.gasto.tipo == TipoGasto::ManoObra {
                ui.label("Horas");
                ui.add(egui::TextEdit::singleline(&mut v.gasto_horas).desired_width(70.0));
                ui.label("€/h");
                let r = ui.add(egui::TextEdit::singleline(&mut v.gasto_tarifa).desired_width(70.0));
                if (r.changed() || ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)))
                    && let (Some(h), Some(t)) = (leer(&v.gasto_horas), leer(&v.gasto_tarifa))
                {
                    v.gasto_importe = num(redondear2(h * t), 2);
                }
                ui.end_row();
            }
            if !self.p.seguimiento.pedidos.is_empty() {
                ui.label("Pedido");
                let t = v.gasto.pedido.map_or("ninguno".to_owned(), |n| format!("nº {n}"));
                let antes = v.gasto.pedido;
                egui::ComboBox::from_id_salt("gasto_pedido")
                    .selected_text(t)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut v.gasto.pedido, None, "ninguno");
                        for p in &self.p.seguimiento.pedidos {
                            ui.selectable_value(
                                &mut v.gasto.pedido,
                                Some(p.numero),
                                format!("nº {} · {}", p.numero, p.nombre()),
                            );
                        }
                    });
                if v.gasto.pedido != antes
                    && let Some(p) = v.gasto.pedido.and_then(|n| self.p.pedido_registrado(n))
                {
                    v.gasto.proveedor = p.proveedor.clone();
                    v.gasto.tipo = if p.material {
                        TipoGasto::Material
                    } else {
                        TipoGasto::Subcontrata
                    };
                    if v.gasto_importe.trim().is_empty() {
                        v.gasto_importe = num(p.total(), 2);
                    }
                }
                ui.end_row();
            }
            ui.label(RichText::new("Importe € (sin IVA)").strong());
            ui.add(egui::TextEdit::singleline(&mut v.gasto_importe).desired_width(100.0));
            ui.end_row();
        });
        let importe = leer(&v.gasto_importe);
        ui.horizontal(|ui| {
            let ok = importe.is_some_and(|i| !i.is_zero());
            let t = if v.gasto.numero == 0 {
                "Anotar"
            } else {
                "Guardar cambios"
            };
            if ui
                .add_enabled(ok, egui::Button::new(RichText::new(t).strong()))
                .clicked()
                && let Some(i) = importe
            {
                let mut g = v.gasto.clone();
                g.importe = i;
                g.horas = (g.tipo == TipoGasto::ManoObra).then(|| leer(&v.gasto_horas)).flatten();
                ops.push(if g.numero == 0 {
                    OpObra::AnotarGasto(Box::new(g))
                } else {
                    OpObra::CambiarGasto(Box::new(g))
                });
                let fecha = v.gasto.fecha.clone();
                v.gasto = gasto_vacio();
                v.gasto.fecha = fecha;
                v.gasto_horas.clear();
                v.gasto_importe.clear();
            }
            if v.gasto.numero != 0 && ui.button("Cancelar").clicked() {
                v.gasto = gasto_vacio();
                v.gasto_horas.clear();
                v.gasto_importe.clear();
            }
        });
    }
}

fn redondear2(x: Decimal) -> Decimal {
    ppto_core::redondear(x, 2)
}

impl Aplicacion {
    // ------------------------------------------------------------ contrato

    /// Obra → Contratar…: bloquea las ventas, guarda y bloquea la revisión.
    pub(super) fn ventana_contratar(&mut self, ctx: &egui::Context) {
        if !self.ver_contratar {
            return;
        }
        let mut abierta = true;
        let mut contratar = false;
        let mut anular = false;
        let (venta, desc) = self.p.venta_y_descuento(self.gg, self.bi).unwrap_or_default();
        egui::Window::new("Contratar la obra")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| match self.p.seguimiento.contrato.clone() {
                Some(k) => {
                    ui.label(RichText::new(format!("Contratada el {} por {} € sin IVA.", k.fecha, eur(k.importe))).strong());
                    ui.label(format!("{} partidas tienen la venta bloqueada al precio del contrato.", k.bloqueadas.len()));
                    ui.add_space(6.0);
                    if ui
                        .button("Anular el contrato")
                        .on_hover_text("Quita la marca y desbloquea las ventas que se bloquearon al contratar")
                        .clicked()
                    {
                        anular = true;
                    }
                }
                None => {
                    egui::Grid::new("contratar").num_columns(2).show(ui, |ui| {
                        ui.label("Venta (sin IVA)");
                        ui.label(format!("{} €", eur(venta)));
                        ui.end_row();
                        if !desc.is_zero() {
                            let d = self.p.seguimiento.descuento.clone().unwrap_or_default();
                            ui.label(d.etiqueta());
                            ui.label(format!("−{} €", eur(desc)));
                            ui.end_row();
                        }
                        ui.label(RichText::new("Importe del contrato").strong());
                        ui.label(RichText::new(format!("{} €", eur(venta - desc))).strong());
                        ui.end_row();
                        ui.label("Fecha");
                        ui.add(egui::TextEdit::singleline(&mut self.fecha_contrato).desired_width(90.0));
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Al contratar: la venta de cada partida queda bloqueada al precio de hoy (si cambian costes o tarifas, \
                             cambia el margen, no lo que se cobra), se guarda una revisión y queda bloqueada como referencia del contrato. \
                             El descuento comercial se aplica en el mismo % a cada certificación.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(4.0);
                    if desc.is_zero() {
                        ui.label(RichText::new("Sin descuento comercial (se pone en la pestaña Resumen).").small().weak());
                    }
                    if ui.button(RichText::new("Contratar y guardar").strong()).clicked() {
                        contratar = true;
                    }
                }
            });
        if contratar {
            self.aplicar(vec![Accion::Obra(OpObra::Contratar(
                self.fecha_contrato.trim().to_owned(),
                self.gg,
                self.bi,
            ))]);
            if self.p.seguimiento.contrato.is_some() {
                self.guardar(false);
                self.bloquear_revision_actual();
                abierta = false;
            }
        }
        if anular {
            self.aplicar(vec![Accion::Obra(OpObra::AnularContrato)]);
        }
        self.ver_contratar = abierta;
    }

    /// Bloquea la revisión en pantalla (referencia del contrato).
    pub(super) fn bloquear_revision_actual(&mut self) {
        let (Some(f), Some(rev), false) = (self.fichero.clone(), self.revision_actual, self.cambios) else {
            return;
        };
        let usuario = ppto_db::bloqueo::Ocupante::yo().usuario;
        let r = ppto_db::Almacen::abrir(&f).and_then(|mut a| {
            a.bloquear_revision(rev, &usuario)?;
            a.listar_revisiones()
        });
        match r {
            Ok(revs) => {
                self.revisiones = revs;
                let etiqueta = self.etiqueta_actual();
                self.info(format!(
                    "Contrato guardado como revisión {etiqueta} 🔒 (no se puede cambiar)."
                ));
            }
            Err(e) => self.error(format!("No se pudo bloquear la revisión: {e}")),
        }
    }
}
