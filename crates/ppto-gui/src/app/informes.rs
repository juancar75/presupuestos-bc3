// SPDX-License-Identifier: GPL-3.0-or-later
//! Menú **Informes**: un desplegable con los informes de compras y control
//! de la obra.
//!
//! 1. **Pedido a fabricante**: material de cada fabricante (por su tarifa o
//!    por la oferta de su precio) con PVP, descuento y neto, en Excel o PDF.
//! 2. **Pedido a subcontrata**: trabajos de cada subcontrata.
//! 3. **Mano de obra prevista**: horas y jornadas por oficio y capítulo.
//! 4. **Coste de la obra**: coste por naturaleza y capítulo frente a la venta.

use super::asistente::abrir_en_sistema;
use super::pdf::ruta_perfiles;
use super::{Aplicacion, corto};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_biblioteca::Biblioteca;
use ppto_core::formato::{cant, eur, num};
use ppto_core::pedidos::{self, CosteObra, ManoObra, Pedido, TipoPedido};
use ppto_core::{Decimal, Presupuesto};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TipoInforme {
    PedidoFabricante,
    PedidoSubcontrata,
    ManoObra,
    Coste,
}

impl TipoInforme {
    pub const TODOS: [Self; 4] = [
        Self::PedidoFabricante,
        Self::PedidoSubcontrata,
        Self::ManoObra,
        Self::Coste,
    ];

    pub fn texto(self) -> &'static str {
        match self {
            Self::PedidoFabricante => "Pedido a fabricante",
            Self::PedidoSubcontrata => "Pedido a subcontrata",
            Self::ManoObra => "Mano de obra prevista",
            Self::Coste => "Coste de la obra",
        }
    }

    fn ayuda(self) -> &'static str {
        match self {
            Self::PedidoFabricante => "Material de cada fabricante con PVP, descuento y neto, para enviar el pedido",
            Self::PedidoSubcontrata => "Trabajos de cada subcontrata con cantidad y precio",
            Self::ManoObra => "Horas y jornadas por oficio, en toda la obra y por capítulo",
            Self::Coste => "Material, mano de obra, subcontratas… por capítulo frente a la venta y el margen",
        }
    }

    fn tipo_pedido(self) -> Option<TipoPedido> {
        match self {
            Self::PedidoFabricante => Some(TipoPedido::Material),
            Self::PedidoSubcontrata => Some(TipoPedido::Subcontrata),
            _ => None,
        }
    }
}

pub(super) struct VentanaInformes {
    pub tipo: TipoInforme,
    pedidos: Vec<Pedido>,
    mano_obra: Option<ManoObra>,
    coste: Option<CosteObra>,
    /// Proveedor elegido (nombre del pedido).
    elegido: String,
    fecha: String,
    referencia: String,
    condiciones: String,
    /// Condiciones ya cargadas de la biblioteca para este proveedor.
    condiciones_de: String,
    bib: Biblioteca,
    mensaje: Option<(String, bool)>,
    ultimo: Option<PathBuf>,
}

fn fecha_hoy() -> String {
    // AAAA-MM-DD → DD/MM/AAAA
    let h = ppto_biblioteca::hoy();
    let p: Vec<&str> = h.split('-').collect();
    if p.len() == 3 {
        format!("{}/{}/{}", p[2], p[1], p[0])
    } else {
        h
    }
}

impl VentanaInformes {
    pub fn nueva(tipo: TipoInforme, bib: Biblioteca) -> Self {
        Self {
            tipo,
            pedidos: Vec::new(),
            mano_obra: None,
            coste: None,
            elegido: String::new(),
            fecha: fecha_hoy(),
            referencia: String::new(),
            condiciones: String::new(),
            condiciones_de: String::new(),
            bib,
            mensaje: None,
            ultimo: None,
        }
    }

    /// Recalcula con el presupuesto actual.
    pub fn calcular(&mut self, p: &Presupuesto, gg: Decimal, bi: Decimal) {
        let r = (|| -> Result<(), ppto_core::ErrorMotor> {
            self.pedidos = pedidos::pedidos(p)?;
            self.mano_obra = Some(pedidos::mano_obra(p)?);
            self.coste = Some(pedidos::coste_obra(p, gg, bi)?);
            Ok(())
        })();
        if let Err(e) = r {
            self.mensaje = Some((format!("No se puede calcular: {e}"), true));
        }
    }

    fn de_tipo(&self, t: TipoPedido) -> Vec<&Pedido> {
        self.pedidos.iter().filter(|x| x.tipo == t).collect()
    }

    fn pedido(&self) -> Option<&Pedido> {
        let t = self.tipo.tipo_pedido()?;
        self.pedidos
            .iter()
            .find(|x| x.tipo == t && x.nombre() == self.elegido)
            .or_else(|| self.pedidos.iter().find(|x| x.tipo == t))
    }

    /// Datos del proveedor guardados en la biblioteca.
    fn datos_proveedor(&self, nombre: &str) -> Vec<(String, String)> {
        let Some(f) = self.bib.fabricante(nombre) else {
            return Vec::new();
        };
        let d = &f.datos;
        [
            ("Razón social", &d.razon_social),
            ("CIF", &d.cif),
            ("Contacto", &d.contacto),
            ("Teléfono", &d.telefono),
            ("Email", &d.email),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect()
    }

    fn cabecera(&self, x: &Pedido) -> ppto_informes::CabeceraPedido {
        ppto_informes::CabeceraPedido {
            fecha: self.fecha.clone(),
            proveedor: self.datos_proveedor(&x.proveedor),
            condiciones: if x.nombre() == self.elegido {
                self.condiciones.clone()
            } else {
                self.bib
                    .fabricante(&x.proveedor)
                    .map(|f| f.datos.condiciones.clone())
                    .unwrap_or_default()
            },
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
        let Some(r) = rfd::FileDialog::new()
            .add_filter(filtro, &[ext])
            .set_file_name(format!("{}.{ext}", limpio(nombre)))
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

/// Nombre de fichero sin caracteres prohibidos en Windows.
fn limpio(s: &str) -> String {
    s.chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '-' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Aplicacion {
    pub(super) fn menu_informes(&mut self, ui: &mut Ui) {
        for t in TipoInforme::TODOS {
            if ui.button(format!("{}…", t.texto())).on_hover_text(t.ayuda()).clicked() {
                ui.close();
                self.abrir_informe(t);
            }
        }
    }

    pub(super) fn abrir_informe(&mut self, t: TipoInforme) {
        let bib = ppto_biblioteca::ruta()
            .as_deref()
            .map(Biblioteca::cargar)
            .unwrap_or_default();
        let mut v = self
            .ventana_informes
            .take()
            .unwrap_or_else(|| VentanaInformes::nueva(t, bib.clone()));
        v.tipo = t;
        v.bib = bib;
        v.calcular(&self.p, self.gg, self.bi);
        self.ventana_informes = Some(v);
    }

    pub(super) fn ventana_informes(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_informes.take() else {
            return;
        };
        let mut abierta = true;
        egui::Window::new("Informes")
            .open(&mut abierta)
            .default_width(980.0)
            .default_height(600.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Informe:");
                    let antes = v.tipo;
                    egui::ComboBox::from_id_salt("tipo_informe")
                        .selected_text(v.tipo.texto())
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for t in TipoInforme::TODOS {
                                ui.selectable_value(&mut v.tipo, t, t.texto()).on_hover_text(t.ayuda());
                            }
                        });
                    if v.tipo != antes {
                        v.mensaje = None;
                    }
                    if ui
                        .button("⟳ Recalcular")
                        .on_hover_text("Con los cambios hechos en la obra")
                        .clicked()
                    {
                        v.calcular(&self.p, self.gg, self.bi);
                    }
                });
                if let Some((m, error)) = v.mensaje.clone() {
                    ui.horizontal(|ui| {
                        let c = if error {
                            Color32::from_rgb(220, 80, 60)
                        } else {
                            Color32::from_rgb(80, 175, 110)
                        };
                        ui.label(RichText::new(m).color(c));
                        if !error
                            && let Some(r) = v.ultimo.clone()
                            && ui.small_button("Abrir").clicked()
                        {
                            abrir_en_sistema(&r);
                        }
                    });
                }
                ui.separator();
                match v.tipo {
                    TipoInforme::PedidoFabricante | TipoInforme::PedidoSubcontrata => self.informe_pedidos(ui, &mut v),
                    TipoInforme::ManoObra => self.informe_mano_obra(ui, &mut v),
                    TipoInforme::Coste => self.informe_coste(ui, &mut v),
                }
            });
        let pendiente = self.registrar.borrow_mut().take();
        if let Some((x, fecha, referencia)) = pendiente {
            self.aplicar(vec![super::Accion::Obra(super::obra::OpObra::RegistrarPedido {
                pedido: Box::new(x),
                fecha,
                referencia,
            })]);
            v.calcular(&self.p, self.gg, self.bi);
        }
        if abierta {
            self.ventana_informes = Some(v);
        }
    }

    fn informe_pedidos(&self, ui: &mut Ui, v: &mut VentanaInformes) {
        let Some(t) = v.tipo.tipo_pedido() else { return };
        let lista: Vec<(String, usize, Decimal, bool)> = v
            .de_tipo(t)
            .iter()
            .map(|x| (x.nombre().to_owned(), x.lineas.len(), x.total, x.proveedor.is_empty()))
            .collect();
        if lista.is_empty() {
            ui.label(match t {
                TipoPedido::Material => "No hay material en la obra.",
                TipoPedido::Subcontrata => {
                    "No hay subcontratas en la obra (recursos de naturaleza Subcontrata, p. ej. de Subcontratar → ratio)."
                }
            });
            return;
        }
        if !lista.iter().any(|l| l.0 == v.elegido) {
            v.elegido = lista[0].0.clone();
        }
        // Al cambiar de proveedor, sus condiciones de la biblioteca
        if v.condiciones_de != v.elegido {
            v.condiciones = v
                .pedido()
                .and_then(|x| v.bib.fabricante(&x.proveedor))
                .map(|f| f.datos.condiciones.clone())
                .unwrap_or_default();
            v.condiciones_de = v.elegido.clone();
        }
        egui::Panel::left("informes_proveedores")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| {
                ui.label(RichText::new(if t == TipoPedido::Material { "Fabricantes" } else { "Subcontratas" }).strong());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (n, l, total, sin) in &lista {
                        let mut txt = RichText::new(format!("{n}\n{l} líneas · {} €", eur(*total)));
                        if *sin {
                            txt = txt.italics().color(Color32::from_rgb(230, 150, 60));
                        }
                        if ui.selectable_label(v.elegido == *n, txt).clicked() {
                            v.elegido = n.clone();
                        }
                    }
                });
                if lista.iter().any(|l| l.3) {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "En naranja: sin proveedor. Asígnelo ligando el recurso a una tarifa o a una oferta (Recursos → Origen del precio).",
                        )
                        .small()
                        .weak(),
                    );
                }
            });
        let Some(x) = v.pedido().cloned() else { return };
        let material = t == TipoPedido::Material;
        ui.horizontal(|ui| {
            ui.heading(x.nombre());
            ui.label(RichText::new(format!("{} € sin IVA", eur(x.total))).strong());
        });
        let datos = v.datos_proveedor(&x.proveedor);
        if datos.is_empty() {
            if !x.proveedor.is_empty() {
                ui.label(
                    RichText::new(
                        "Sin datos del proveedor: póngalos en Precios → Biblioteca → Ofertas y datos de fabricantes.",
                    )
                    .small()
                    .weak(),
                );
            }
        } else {
            ui.label(
                RichText::new(
                    datos
                        .iter()
                        .map(|(k, d)| format!("{k}: {d}"))
                        .collect::<Vec<_>>()
                        .join(" · "),
                )
                .weak(),
            );
        }
        ui.horizontal(|ui| {
            ui.label("Fecha");
            ui.add(egui::TextEdit::singleline(&mut v.fecha).desired_width(80.0));
            ui.label("Nuestra ref.");
            ui.add(
                egui::TextEdit::singleline(&mut v.referencia)
                    .hint_text("nº de pedido u obra")
                    .desired_width(140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Condiciones");
            ui.add(
                egui::TextEdit::singleline(&mut v.condiciones)
                    .hint_text("plazo, portes, forma de pago…")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("PDF…").on_hover_text("Pedido listo para enviar").clicked() {
                let perfil = ruta_perfiles()
                    .map(|r| ppto_pdf::Perfiles::cargar(&r))
                    .unwrap_or_default()
                    .activo()
                    .clone();
                let o = ppto_pdf::DatosPedido {
                    empresa: perfil.empresa,
                    empresa_datos: perfil.empresa_datos,
                    logo: perfil.logo.filter(|l| l.is_file()),
                    fecha: v.fecha.clone(),
                    referencia: v.referencia.clone(),
                    proveedor: datos.clone(),
                    condiciones: v.condiciones.clone(),
                };
                let b = ppto_pdf::pdf_pedido(&self.p, &x, &o).map_err(|e| e.to_string());
                v.guardar(&format!("Pedido {} - {}", x.nombre(), self.p.nombre), "pdf", b);
            }
            if ui.button("Excel…").clicked() {
                let b = ppto_informes::excel_pedido(&self.p, &x, &v.cabecera(&x)).map_err(|e| e.to_string());
                v.guardar(&format!("Pedido {} - {}", x.nombre(), self.p.nombre), "xlsx", b);
            }
            ui.separator();
            if ui
                .button("Registrar en la obra")
                .on_hover_text("Lo guarda como pedido de la obra (Obra → Pedidos) para seguir su estado y lo recibido. Solo lo que falte por pedir.")
                .clicked()
            {
                self.registrar.borrow_mut().replace((x.clone(), v.fecha.clone(), v.referencia.clone()));
            }
            ui.separator();
            if ui
                .button("Todos en un Excel…")
                .on_hover_text("Un libro con una hoja por proveedor y un resumen")
                .clicked()
            {
                let todos: Vec<Pedido> = v.de_tipo(t).into_iter().cloned().collect();
                let b = ppto_informes::excel_pedidos(&self.p, &todos, |p| v.cabecera(p)).map_err(|e| e.to_string());
                let n = if material {
                    "Pedidos de material"
                } else {
                    "Pedidos de subcontratas"
                };
                v.guardar(&format!("{n} - {}", self.p.nombre), "xlsx", b);
            }
        });
        ui.separator();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let n = if material { 10 } else { 8 };
            egui::Grid::new("informe_pedido")
                .striped(true)
                .num_columns(n)
                .show(ui, |ui| {
                    let mut cols = vec!["Código", "Ref.", "Descripción", "Ud", "Cantidad"];
                    if material {
                        cols.extend(["PVP €", "Dto."]);
                    }
                    cols.extend(["Precio €", "Importe €", "Oferta"]);
                    for c in cols {
                        ui.label(RichText::new(c).strong());
                    }
                    ui.end_row();
                    for l in &x.lineas {
                        ui.label(&l.codigo);
                        ui.label(l.referencia.as_deref().unwrap_or(""));
                        ui.label(corto(&l.resumen, 52)).on_hover_text(format!(
                            "{}\n\nPartidas: {}",
                            l.resumen,
                            l.partidas.join(", ")
                        ));
                        ui.label(&l.unidad);
                        ui.label(cant(l.cantidad, self.p.decimales.medicion));
                        if material {
                            ui.label(l.pvp.map(eur).unwrap_or_default());
                            ui.label(l.descuento.map(|d| format!("{} %", num(d, 2))).unwrap_or_default());
                        }
                        ui.label(eur(l.precio));
                        ui.label(eur(l.importe));
                        ui.label(l.oferta.as_deref().unwrap_or(""));
                        ui.end_row();
                    }
                });
        });
    }

    fn informe_mano_obra(&self, ui: &mut Ui, v: &mut VentanaInformes) {
        let Some(mo) = v.mano_obra.clone() else { return };
        ui.horizontal(|ui| {
            ui.heading(format!(
                "{} h · {} jornadas · {} €",
                cant(mo.horas, 2),
                cant((mo.horas / Decimal::from(8)).round_dp(1), 1),
                eur(mo.importe)
            ));
            if ui.button("Excel…").clicked() {
                let b = ppto_informes::excel_mano_obra(&self.p, &mo).map_err(|e| e.to_string());
                v.guardar(&format!("Mano de obra - {}", self.p.nombre), "xlsx", b);
            }
        });
        if mo.oficios.is_empty() {
            ui.label("La obra no tiene mano de obra propia (puede estar subcontratada).");
            return;
        }
        let tabla = |ui: &mut Ui, id: &str, oficios: &[pedidos::Oficio]| {
            egui::Grid::new(id).striped(true).num_columns(6).show(ui, |ui| {
                for c in ["Código", "Oficio", "Ud", "Cantidad", "Jornadas (8 h)", "Importe €"] {
                    ui.label(RichText::new(c).weak());
                }
                ui.end_row();
                for o in oficios {
                    ui.label(&o.codigo);
                    ui.label(corto(&o.resumen, 50));
                    ui.label(&o.unidad);
                    ui.label(cant(o.cantidad, 2));
                    ui.label(if o.en_horas() {
                        cant((o.cantidad / Decimal::from(8)).round_dp(1), 1)
                    } else {
                        String::new()
                    });
                    ui.label(eur(o.importe));
                    ui.end_row();
                }
            });
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Toda la obra").strong());
            tabla(ui, "mo_total", &mo.oficios);
            ui.add_space(8.0);
            ui.label(RichText::new("Por capítulo").strong());
            for c in &mo.capitulos {
                let t = format!(
                    "{} {} — {} h · {} €",
                    c.capitulo,
                    c.resumen,
                    cant(c.horas, 2),
                    eur(c.importe)
                );
                egui::CollapsingHeader::new(t.trim())
                    .id_salt(("mo_cap", c.capitulo.as_str()))
                    .show(ui, |ui| tabla(ui, &format!("mo_{}", c.capitulo), &c.oficios));
            }
        });
    }

    fn informe_coste(&self, ui: &mut Ui, v: &mut VentanaInformes) {
        let Some(c) = v.coste.clone() else { return };
        ui.horizontal(|ui| {
            ui.heading(format!("Coste {} € · Venta {} €", eur(c.pem), eur(c.venta)));
            let b = c.beneficio();
            let col = if b.is_sign_negative() {
                Color32::from_rgb(220, 80, 60)
            } else {
                Color32::from_rgb(80, 175, 110)
            };
            ui.label(
                RichText::new(format!(
                    "Beneficio {} €{}",
                    eur(b),
                    c.margen_pct()
                        .map(|m| format!(" ({} % s/venta)", num(m, 2)))
                        .unwrap_or_default()
                ))
                .strong()
                .color(col),
            );
            if ui.button("Excel…").clicked() {
                let x = ppto_informes::excel_coste(&self.p, &c).map_err(|e| e.to_string());
                v.guardar(&format!("Coste de obra - {}", self.p.nombre), "xlsx", x);
            }
        });
        ui.label(
            RichText::new(format!(
                "Coste directo {} € + costes indirectos {} € = {} €. Venta: margen de cada partida o GG {} % + BI {} %.",
                eur(c.coste_directo),
                eur(c.costes_indirectos),
                eur(c.pem),
                num(self.gg, 2),
                num(self.bi, 2)
            ))
            .weak(),
        );
        ui.separator();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("informe_coste")
                .striped(true)
                .num_columns(12)
                .show(ui, |ui| {
                    for t in [
                        "Capítulo",
                        "Resumen",
                        "Material",
                        "Mano de obra",
                        "Maquinaria",
                        "Subcontrata",
                        "Otros",
                        "Sin descomponer",
                        "Coste",
                        "Venta",
                        "Beneficio",
                        "Margen",
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    let fila = |ui: &mut Ui, d: &pedidos::Desglose, coste: Decimal, venta: Decimal, fuerte: bool| {
                        for x in [
                            d.material,
                            d.mano_obra,
                            d.maquinaria,
                            d.subcontrata,
                            d.otros,
                            d.sin_descomponer,
                            coste,
                            venta,
                        ] {
                            let t = RichText::new(eur(x));
                            ui.label(if fuerte { t.strong() } else { t });
                        }
                        let b = venta - coste;
                        let col = if b.is_sign_negative() {
                            Color32::from_rgb(220, 80, 60)
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(RichText::new(eur(b)).color(col));
                        ui.label(if venta.is_zero() {
                            "—".to_owned()
                        } else {
                            format!("{} %", num((b / venta * Decimal::ONE_HUNDRED).round_dp(1), 1))
                        });
                        ui.end_row();
                    };
                    for x in &c.capitulos {
                        ui.label(&x.capitulo);
                        ui.label(corto(&x.resumen, 40));
                        fila(ui, &x.desglose, x.coste, x.venta, false);
                    }
                    ui.label("");
                    ui.label(RichText::new("Total obra").strong());
                    fila(ui, &c.desglose, c.pem, c.venta, true);
                });
        });
    }
}
