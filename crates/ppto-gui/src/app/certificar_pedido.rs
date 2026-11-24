// SPDX-License-Identifier: GPL-3.0-or-later
//! «Certificaciones del cliente»: el pedido del cliente con sus proformas a
//! origen, lo que se factura cada mes (con las líneas en negativo) y el
//! registro de facturas y abonos, con avisos cuando algo no cuadra.

use super::{Accion, Aplicacion, corto};
use eframe::egui::{self, Color32, RichText};
use ppto_ciego::proforma::{self, Proforma};
use ppto_core::Decimal;
use ppto_core::formato::eur;
use ppto_core::pedido_cliente::{FacturaPedido, PedidoCliente, TipoFactura};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct VentanaPedidos {
    /// Pedido elegido (índice).
    pub pedido: usize,
    /// Certificación elegida.
    pub cert: Option<u32>,
    /// Última proforma traída de cada pedido (para rellenarla).
    pub plantillas: std::collections::BTreeMap<String, (PathBuf, Proforma)>,
    pub mes_nuevo: String,
    pub factura: FormFactura,
    pub aviso: Option<(String, bool)>,
}

#[derive(Default)]
pub(super) struct FormFactura {
    pub numero: String,
    pub fecha: String,
    pub importe: String,
    pub abono: bool,
    pub rectifica: String,
}

/// «2026-10» a partir del nombre del fichero o de un texto con fecha.
fn mes_de(t: &str) -> Option<String> {
    let b = t.as_bytes();
    (0..b.len().saturating_sub(6)).find_map(|i| {
        let s = t.get(i..i + 7)?;
        let (a, m) = s.split_once('-')?;
        (a.len() == 4
            && a.starts_with("20")
            && a.bytes().all(|c| c.is_ascii_digit())
            && m.len() == 2
            && m.parse::<u8>().is_ok_and(|m| (1..=12).contains(&m)))
        .then(|| s.to_owned())
    })
}

fn mes_actual() -> String {
    ppto_biblioteca::hoy().get(..7).unwrap_or_default().to_owned()
}

fn leer_num(t: &str) -> Option<Decimal> {
    let t = t.trim();
    if t.is_empty() {
        return Some(Decimal::ZERO);
    }
    t.parse::<Decimal>().ok().or_else(|| ppto_core::formato::leer(t))
}

impl Aplicacion {
    pub(super) fn abrir_pedidos(&mut self) {
        let mut v = VentanaPedidos {
            mes_nuevo: mes_actual(),
            ..Default::default()
        };
        v.factura.fecha = ppto_biblioteca::hoy();
        v.cert = self
            .p
            .seguimiento
            .pedidos_cliente
            .first()
            .and_then(|p| p.certificaciones.last())
            .map(|c| c.numero);
        self.ventana_pedidos = Some(v);
    }

    fn importar_proforma(&mut self, v: &mut VentanaPedidos, acciones: &mut Vec<Accion>) {
        let Some(f) = rfd::FileDialog::new()
            .set_title("Proforma de certificación del cliente")
            .add_filter("Excel", &["xlsx"])
            .pick_file()
        else {
            return;
        };
        let leida = std::fs::read(&f)
            .map_err(|e| e.to_string())
            .and_then(|b| proforma::leer_bytes(&b).map_err(|e| e.to_string()));
        let pf = match leida {
            Ok(p) => p,
            Err(e) => {
                v.aviso = Some((format!("No se pudo leer la proforma: {e}"), true));
                return;
            }
        };
        let nombre = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mes = mes_de(&nombre)
            .or_else(|| mes_de(&pf.visto_bueno))
            .unwrap_or_else(mes_actual);
        let mut todos = self.p.seguimiento.pedidos_cliente.clone();
        let i = match todos.iter().position(|p| p.numero == pf.pedido) {
            Some(i) => i,
            None => {
                todos.push(PedidoCliente {
                    numero: pf.pedido.clone(),
                    fecha: pf.fecha.clone(),
                    proyecto: pf.proyecto.clone(),
                    ..Default::default()
                });
                todos.len() - 1
            }
        };
        let p = &mut todos[i];
        let nuevo = p.lineas.is_empty();
        let cambios = p.actualizar_lineas(pf.lineas_pedido());
        let origen = pf.origen();
        let con_origen = origen.values().any(|q| !q.is_zero());
        let mut t = if nuevo {
            format!(
                "Pedido {} con {} líneas ({}).",
                pf.pedido,
                pf.lineas.len(),
                eur(p.total())
            )
        } else if cambios.esta_vacio() {
            format!("Pedido {}: sin cambios en las líneas.", pf.pedido)
        } else {
            format!(
                "Pedido {} actualizado: {} líneas nuevas, {} quitadas, {} con otro precio, {} con otra cantidad.",
                pf.pedido,
                cambios.nuevas.len(),
                cambios.quitadas.len(),
                cambios.precios.len(),
                cambios.cantidades.len()
            )
        };
        if con_origen {
            let n = p.fijar_origen(&mes, origen);
            v.cert = Some(n);
            t.push_str(&format!(
                " Certificación {n} ({mes}) a origen: {}.",
                eur(p
                    .certificaciones
                    .iter()
                    .find(|c| c.numero == n)
                    .map(PedidoCliente::total_origen)
                    .unwrap_or_default())
            ));
        } else {
            t.push_str(" La proforma viene sin cantidades a origen: se guarda para rellenarla.");
        }
        v.plantillas.insert(pf.pedido.clone(), (f, pf));
        v.pedido = i;
        v.aviso = Some((t, false));
        acciones.push(Accion::PedidosCliente(todos));
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn ventana_pedidos(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.ventana_pedidos.take() else {
            return;
        };
        let mut abierta = true;
        egui::Window::new("Certificaciones del cliente (pedido a origen)")
            .open(&mut abierta)
            .default_width(1000.0)
            .default_height(700.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("📥 Importar proforma del cliente…")
                        .on_hover_text(
                            "El Excel del cliente («Proforma certificación a origen»): crea o actualiza el pedido y, \
                             si trae cantidades a origen, la certificación de ese mes (el mes sale del nombre del fichero).",
                        )
                        .clicked()
                    {
                        self.importar_proforma(&mut v, acciones);
                    }
                    let pedidos = &self.p.seguimiento.pedidos_cliente;
                    if !pedidos.is_empty() {
                        v.pedido = v.pedido.min(pedidos.len() - 1);
                        egui::ComboBox::from_id_salt("pedido_cliente")
                            .selected_text(format!("Pedido {}", pedidos[v.pedido].numero))
                            .show_ui(ui, |ui| {
                                for (i, p) in pedidos.iter().enumerate() {
                                    if ui
                                        .selectable_label(v.pedido == i, format!("{}  {}  {}", p.numero, p.proyecto, eur(p.total())))
                                        .clicked()
                                    {
                                        v.pedido = i;
                                        v.cert = p.certificaciones.last().map(|c| c.numero);
                                    }
                                }
                            });
                    }
                });
                if let Some((t, error)) = &v.aviso {
                    ui.colored_label(if *error { Color32::from_rgb(200, 60, 60) } else { ui.visuals().text_color() }, t);
                }
                let Some(pedido) = self.p.seguimiento.pedidos_cliente.get(v.pedido).cloned() else {
                    ui.label(
                        "Importe la proforma del cliente (su Excel con el pedido y la columna «Cantidad a facturar a \
                         origen»). Puede traer también las de meses anteriores para tener el histórico.",
                    );
                    return;
                };
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Pedido {}", pedido.numero)).strong());
                    ui.label(format!(
                        "{} · total {} · certificado {} · por certificar {} · facturado {}",
                        pedido.proyecto,
                        eur(pedido.total()),
                        eur(pedido.certificaciones.last().map(PedidoCliente::total_origen).unwrap_or_default()),
                        eur(pedido.pendiente()),
                        eur(pedido.facturado())
                    ));
                });
                // Certificaciones
                ui.horizontal_wrapped(|ui| {
                    ui.label("Certificación:");
                    for c in &pedido.certificaciones {
                        if ui.selectable_label(v.cert == Some(c.numero), format!("{} · {}", c.numero, c.mes)).clicked() {
                            v.cert = Some(c.numero);
                        }
                    }
                    ui.separator();
                    ui.add(egui::TextEdit::singleline(&mut v.mes_nuevo).desired_width(70.0).hint_text("AAAA-MM"));
                    if ui
                        .button("+ Nueva")
                        .on_hover_text("Empieza con el origen de la anterior; cambie las cantidades a origen de este mes.")
                        .clicked()
                    {
                        if mes_de(&v.mes_nuevo).is_none() {
                            v.aviso = Some(("El mes va como AAAA-MM (por ejemplo 2026-11).".into(), true));
                        } else if pedido.certificaciones.iter().any(|c| c.mes == v.mes_nuevo) {
                            v.aviso = Some((format!("Ya hay una certificación de {}.", v.mes_nuevo), true));
                        } else {
                            let mut todos = self.p.seguimiento.pedidos_cliente.clone();
                            v.cert = Some(todos[v.pedido].nueva_certificacion(&v.mes_nuevo));
                            acciones.push(Accion::PedidosCliente(todos));
                        }
                    }
                    if let Some(n) = v.cert
                        && pedido.certificaciones.last().is_some_and(|c| c.numero == n)
                        && ui
                            .button("🗑 Quitar")
                            .on_hover_text("Quita esta certificación (la última). Se puede deshacer.")
                            .clicked()
                    {
                        let mut todos = self.p.seguimiento.pedidos_cliente.clone();
                        todos[v.pedido].certificaciones.retain(|c| c.numero != n);
                        v.cert = todos[v.pedido].certificaciones.last().map(|c| c.numero);
                        acciones.push(Accion::PedidosCliente(todos));
                    }
                });
                let Some(n) = v.cert.filter(|n| pedido.certificaciones.iter().any(|c| c.numero == *n)) else {
                    ui.label("Elija o cree una certificación.");
                    return;
                };
                let Some(mes) = pedido.mes(n) else { return };
                let cert = pedido.certificaciones.iter().find(|c| c.numero == n).cloned().unwrap_or_default();
                // Resumen y avisos
                egui::Grid::new("resumen_mes").num_columns(2).show(ui, |ui| {
                    for (t, x, fuerte) in [
                        ("Total a origen", mes.origen, false),
                        ("Origen anterior", mes.origen_anterior, false),
                        ("A facturar este mes", mes.total, true),
                        ("Facturado antes (facturas − abonos)", mes.facturado_antes, false),
                    ] {
                        ui.label(t);
                        let r = RichText::new(eur(x));
                        ui.label(if fuerte { r.strong() } else { r });
                        ui.end_row();
                    }
                });
                if mes.avisos.is_empty() {
                    ui.colored_label(Color32::from_rgb(40, 150, 60), "✔ Cuadra con la certificación anterior y con el pedido.");
                } else {
                    for a in &mes.avisos {
                        ui.colored_label(Color32::from_rgb(200, 120, 20), format!("⚠ {a}"));
                    }
                }
                ui.horizontal(|ui| {
                    let plantilla = v.plantillas.get(&pedido.numero).cloned();
                    if ui
                        .button("📝 Rellenar proforma del cliente…")
                        .on_hover_text(
                            "Escribe las cantidades a origen de esta certificación en el Excel del cliente (solo esa \
                             columna; el resto del fichero queda igual). Pide primero su proforma si no la ha importado.",
                        )
                        .clicked()
                    {
                        let base = plantilla.or_else(|| {
                            let f = rfd::FileDialog::new()
                                .set_title("Proforma del cliente (este mes o la última)")
                                .add_filter("Excel", &["xlsx"])
                                .pick_file()?;
                            let b = std::fs::read(&f).ok()?;
                            Some((f, proforma::leer_bytes(&b).ok()?))
                        });
                        match base {
                            None => v.aviso = Some(("No se pudo leer la proforma del cliente.".into(), true)),
                            Some((f, pf)) if pf.pedido != pedido.numero && !pf.pedido.is_empty() => {
                                v.aviso = Some((
                                    format!("{} es del pedido {}, no del {}.", f.display(), pf.pedido, pedido.numero),
                                    true,
                                ));
                            }
                            Some((f, pf)) => {
                                let nombre = format!("{}_PROFORMA_CERT{:02}.xlsx", cert.mes, n);
                                let destino = rfd::FileDialog::new()
                                    .set_file_name(&nombre)
                                    .set_directory(f.parent().unwrap_or(std::path::Path::new(".")))
                                    .add_filter("Excel", &["xlsx"])
                                    .save_file();
                                if let Some(d) = destino {
                                    let r = std::fs::read(&f)
                                        .map_err(|e| e.to_string())
                                        .and_then(|b| proforma::rellenar(&b, &pf, &cert.origen).map_err(|e| e.to_string()))
                                        .and_then(|x| std::fs::write(&d, x).map_err(|e| e.to_string()));
                                    let faltan: Vec<&String> =
                                        cert.origen.keys().filter(|k| !pf.lineas.iter().any(|l| &l.pedido.linea == *k)).collect();
                                    v.aviso = Some(match r {
                                        Ok(()) if faltan.is_empty() => (format!("Proforma rellena: {}", d.display()), false),
                                        Ok(()) => (
                                            format!(
                                                "Proforma rellena: {}. Ojo: las líneas {:?} tienen origen y no están en esa proforma.",
                                                d.display(),
                                                faltan
                                            ),
                                            true,
                                        ),
                                        Err(e) => (format!("No se pudo rellenar: {e}"), true),
                                    });
                                }
                            }
                        }
                    }
                    if ui
                        .button("📊 Detalle de la factura (Excel)…")
                        .on_hover_text("Para administración: líneas del mes (también las negativas), precios del pedido, totales y avisos.")
                        .clicked()
                        && let Some(d) = rfd::FileDialog::new()
                            .set_file_name(format!("{}_FACTURA_CERT{:02}_{}.xlsx", cert.mes, n, pedido.numero))
                            .add_filter("Excel", &["xlsx"])
                            .save_file()
                    {
                        v.aviso = Some(
                            match proforma::detalle_mes(&pedido, &mes).map_err(|e| e.to_string()).and_then(|x| std::fs::write(&d, x).map_err(|e| e.to_string())) {
                                Ok(()) => (format!("Detalle guardado: {}", d.display()), false),
                                Err(e) => (format!("No se pudo guardar: {e}"), true),
                            },
                        );
                    }
                });
                ui.separator();
                // Líneas: a origen editable
                let alto = (ui.available_height() - 190.0).max(160.0);
                egui::ScrollArea::vertical().id_salt("lineas_pedido").max_height(alto).show(ui, |ui| {
                    egui::Grid::new("lineas_cert_pedido").striped(true).num_columns(8).show(ui, |ui| {
                        for t in ["Línea", "Descripción", "Pedido", "Precio", "Origen ant.", "A origen", "Mes", "Importe mes"] {
                            ui.label(RichText::new(t).weak());
                        }
                        ui.end_row();
                        for l in &pedido.lineas {
                            let lm = mes.lineas.iter().find(|x| x.linea == l.linea);
                            let q = cert.origen.get(&l.linea).copied().unwrap_or_default();
                            ui.label(&l.linea);
                            ui.label(corto(&l.descripcion, 48)).on_hover_text(&l.descripcion);
                            ui.label(ppto_core::formato::cant(l.cantidad, 3));
                            ui.label(l.precio.normalize().to_string());
                            ui.label(lm.map(|x| ppto_core::formato::cant(x.origen_anterior, 3)).unwrap_or_else(|| {
                                ppto_core::formato::cant(q, 3)
                            }));
                            let id = ui.id().with(("origen", &pedido.numero, n, &l.linea));
                            let mut t = ui
                                .data_mut(|d| d.get_temp::<String>(id))
                                .unwrap_or_else(|| ppto_core::formato::cant(q, 3));
                            let exceso = q > l.cantidad;
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut t)
                                    .desired_width(70.0)
                                    .text_color_opt(exceso.then_some(Color32::from_rgb(200, 60, 60))),
                            );
                            if r.changed() {
                                ui.data_mut(|d| d.insert_temp(id, t.clone()));
                            }
                            if r.lost_focus() {
                                ui.data_mut(|d| d.remove::<String>(id));
                                match leer_num(&t) {
                                    Some(x) if x != q => {
                                        let mut todos = self.p.seguimiento.pedidos_cliente.clone();
                                        if let Some(c) = todos[v.pedido].certificaciones.iter_mut().find(|c| c.numero == n) {
                                            if x.is_zero() {
                                                c.origen.remove(&l.linea);
                                            } else {
                                                c.origen.insert(l.linea.clone(), x);
                                            }
                                            acciones.push(Accion::PedidosCliente(todos));
                                        }
                                    }
                                    Some(_) => {}
                                    None => v.aviso = Some((format!("«{t}» no es un número."), true)),
                                }
                            }
                            match lm {
                                Some(x) => {
                                    let c = if x.cantidad.is_sign_negative() {
                                        RichText::new(ppto_core::formato::cant(x.cantidad, 3)).color(Color32::from_rgb(200, 60, 60))
                                    } else {
                                        RichText::new(ppto_core::formato::cant(x.cantidad, 3))
                                    };
                                    ui.label(c);
                                    ui.label(eur(x.importe));
                                }
                                None => {
                                    ui.label("");
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                // Facturas
                ui.label(RichText::new("Facturas y abonos de este pedido").strong());
                let mut quitar = None;
                egui::Grid::new("facturas_pedido").striped(true).num_columns(5).show(ui, |ui| {
                    for (i, f) in pedido.facturas.iter().enumerate() {
                        ui.label(&f.numero);
                        ui.label(&f.fecha);
                        ui.label(eur(f.importe));
                        ui.label(match (&f.tipo, &f.rectifica) {
                            (TipoFactura::Abono, Some(r)) => format!("abono de {r}"),
                            (TipoFactura::Abono, None) => "abono".into(),
                            _ => f.certificacion.map(|c| format!("certificación {c}")).unwrap_or_default(),
                        });
                        if ui.small_button("🗑").clicked() {
                            quitar = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = quitar {
                    let mut todos = self.p.seguimiento.pedidos_cliente.clone();
                    todos[v.pedido].facturas.remove(i);
                    acciones.push(Accion::PedidosCliente(todos));
                }
                ui.horizontal(|ui| {
                    let f = &mut v.factura;
                    ui.add(egui::TextEdit::singleline(&mut f.numero).hint_text("Nº factura").desired_width(110.0));
                    ui.add(egui::TextEdit::singleline(&mut f.fecha).hint_text("Fecha").desired_width(85.0));
                    ui.add(egui::TextEdit::singleline(&mut f.importe).hint_text("Base imponible").desired_width(100.0));
                    ui.checkbox(&mut f.abono, "Abono");
                    if f.abono {
                        ui.add(egui::TextEdit::singleline(&mut f.rectifica).hint_text("Rectifica a").desired_width(110.0));
                    }
                    if ui
                        .button(format!("Añadir a la certificación {n}"))
                        .on_hover_text("Los abonos van en negativo aunque escriba el importe en positivo.")
                        .clicked()
                    {
                        match leer_num(&f.importe) {
                            Some(x) if !f.numero.trim().is_empty() && !x.is_zero() => {
                                let mut todos = self.p.seguimiento.pedidos_cliente.clone();
                                todos[v.pedido].facturas.push(FacturaPedido {
                                    numero: f.numero.trim().to_owned(),
                                    fecha: f.fecha.trim().to_owned(),
                                    importe: if f.abono { -x.abs() } else { x },
                                    tipo: if f.abono { TipoFactura::Abono } else { TipoFactura::Factura },
                                    certificacion: Some(n),
                                    rectifica: (f.abono && !f.rectifica.trim().is_empty()).then(|| f.rectifica.trim().to_owned()),
                                });
                                acciones.push(Accion::PedidosCliente(todos));
                                f.numero.clear();
                                f.importe.clear();
                                f.rectifica.clear();
                                f.abono = false;
                            }
                            _ => v.aviso = Some(("Ponga el número de factura y su base imponible.".into(), true)),
                        }
                    }
                });
            });
        if abierta {
            self.ventana_pedidos = Some(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mes_del_nombre_del_fichero() {
        assert_eq!(mes_de("2026-10_PROFORMA_CERT02.xlsx").as_deref(), Some("2026-10"));
        assert_eq!(mes_de("VºBº 2026-10-29").as_deref(), Some("2026-10"));
        assert_eq!(mes_de("Vº.Bº. 2026-30-09"), None);
        assert_eq!(mes_de("proforma.xlsx"), None);
    }
}
