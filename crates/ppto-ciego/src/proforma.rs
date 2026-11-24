// SPDX-License-Identifier: GPL-3.0-or-later
//! Proformas de certificación «a origen» del cliente: una hoja con el
//! pedido (Nº línea, tarea, descripción, cantidad, precio) y una columna
//! «Cantidad a facturar a origen» que es lo único que se rellena.
//!
//! - [`leer`]: pedido y cantidades a origen de la proforma.
//! - [`rellenar`]: el mismo fichero del cliente con las cantidades a origen
//!   escritas (solo esas celdas; Excel recalcula al abrir).
//! - [`detalle_mes`]: Excel con el detalle de la factura del mes para
//!   administración (incluidas las líneas en negativo) y los avisos.

use super::{ErrorCiego, decimal, escribir_celdas, recalcular_al_abrir, ruta_de_hoja, texto};
use calamine::{Data, Reader};
use ppto_core::Decimal;
use ppto_core::pedido_cliente::{LineaPedido, Mes, PedidoCliente};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

/// Una línea de la proforma con la celda de su cantidad a origen.
#[derive(Debug, Clone, PartialEq)]
pub struct LineaProforma {
    pub pedido: LineaPedido,
    pub origen: Decimal,
    /// Referencia de la celda a origen («G10»).
    pub celda: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Proforma {
    pub hoja: String,
    pub pedido: String,
    pub fecha: String,
    pub proyecto: String,
    /// Texto del visto bueno, si lo trae («VºBº 2026-10-29»).
    pub visto_bueno: String,
    pub lineas: Vec<LineaProforma>,
}

impl Proforma {
    pub fn origen(&self) -> BTreeMap<String, Decimal> {
        self.lineas.iter().map(|l| (l.pedido.linea.clone(), l.origen)).collect()
    }
    pub fn lineas_pedido(&self) -> Vec<LineaPedido> {
        self.lineas.iter().map(|l| l.pedido.clone()).collect()
    }
}

fn plano(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'º' | '°' | 'ª' | '.' | ':' => ' ',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn columna(c: usize) -> String {
    let mut n = c + 1;
    let mut s = String::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, char::from(b'A' + u8::try_from(r).unwrap_or(0)));
        n = (n - 1) / 26;
    }
    s
}

#[derive(Default)]
struct Columnas {
    linea: usize,
    tarea: Option<usize>,
    desc: usize,
    cantidad: usize,
    precio: usize,
    origen: usize,
}

fn columnas(fila: &[Data]) -> Option<Columnas> {
    let t: Vec<String> = fila.iter().map(|d| plano(&texto(d))).collect();
    let busca = |f: &dyn Fn(&str) -> bool| t.iter().position(|x| f(x));
    let origen = busca(&|x| x.contains("origen") && x.contains("cantidad"))?;
    Some(Columnas {
        linea: busca(&|x| x.contains("linea"))?,
        tarea: busca(&|x| x.contains("tarea")),
        desc: busca(&|x| x.contains("descrip") || x == "concepto" || x == "resumen")?,
        cantidad: busca(&|x| x.starts_with("cantidad") && !x.contains("origen") && !x.contains("factur"))?,
        precio: busca(&|x| x.contains("precio") || x.contains("coste") || x.contains("unitario"))?,
        origen,
    })
}

/// Lee la primera hoja que tenga la cabecera de una proforma a origen.
pub fn leer_bytes(bytes: &[u8]) -> Result<Proforma, ErrorCiego> {
    let mut libro =
        calamine::open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|e| ErrorCiego::Lectura(e.to_string()))?;
    for hoja in libro.sheet_names() {
        let Ok(rango) = libro.worksheet_range(&hoja) else {
            continue;
        };
        let (f0, c0) = rango.start().unwrap_or((0, 0));
        let filas: Vec<&[Data]> = rango.rows().collect();
        let Some((i, cols)) = filas.iter().enumerate().find_map(|(i, f)| columnas(f).map(|c| (i, c))) else {
            continue;
        };
        let mut p = Proforma {
            hoja: hoja.clone(),
            ..Default::default()
        };
        // Datos de cabecera: etiqueta y el valor a su derecha
        for f in &filas {
            for (k, d) in f.iter().enumerate() {
                let e = plano(&texto(d));
                let valor = || {
                    f.iter()
                        .skip(k + 1)
                        .map(texto)
                        .map(|s| s.trim().to_owned())
                        .find(|s| !s.is_empty())
                        .unwrap_or_default()
                };
                if (e == "n pedido" || e == "no pedido" || e == "pedido") && p.pedido.is_empty() {
                    p.pedido = valor();
                } else if e == "fecha pedido" && p.fecha.is_empty() {
                    p.fecha = valor();
                } else if (e == "n proyecto" || e == "no proyecto" || e == "proyecto") && p.proyecto.is_empty() {
                    p.proyecto = valor();
                } else if e.starts_with("vºbº") || e.starts_with("vb") || e.starts_with("v b") {
                    p.visto_bueno = texto(d).trim().to_owned();
                }
            }
        }
        for (j, f) in filas.iter().enumerate().skip(i + 1) {
            let g = |c: usize| f.get(c).unwrap_or(&Data::Empty);
            let linea = texto(g(cols.linea)).trim().to_owned();
            let desc = texto(g(cols.desc)).trim().to_owned();
            if linea.is_empty() && desc.is_empty() {
                // fin de la tabla (a partir de aquí, notas y firmas)
                if j > i + 1 {
                    break;
                }
                continue;
            }
            let (Some(cantidad), Some(precio)) = (decimal(g(cols.cantidad)), decimal(g(cols.precio))) else {
                continue;
            };
            if linea.is_empty() {
                continue;
            }
            let fila = f0 as usize + j + 1;
            p.lineas.push(LineaProforma {
                pedido: LineaPedido {
                    linea,
                    tarea: cols.tarea.map(|c| texto(g(c)).trim().to_owned()).unwrap_or_default(),
                    descripcion: desc,
                    cantidad,
                    precio,
                },
                origen: decimal(g(cols.origen)).unwrap_or_default(),
                celda: format!("{}{fila}", columna(c0 as usize + cols.origen)),
            });
        }
        if !p.lineas.is_empty() {
            return Ok(p);
        }
    }
    Err(ErrorCiego::Lectura(
        "no se encuentra la tabla de la proforma (columnas Nº línea, Descripción, Cantidad, precio y «Cantidad a \
         facturar a origen»)"
            .into(),
    ))
}

/// Devuelve el fichero del cliente con las cantidades a origen escritas
/// (las líneas que no estén en `origen` se dejan a 0).
pub fn rellenar(
    original: &[u8],
    proforma: &Proforma,
    origen: &BTreeMap<String, Decimal>,
) -> Result<Vec<u8>, ErrorCiego> {
    let celdas: BTreeMap<String, super::Contenido> = proforma
        .lineas
        .iter()
        .map(|l| {
            (
                l.celda.clone(),
                super::Contenido::Numero(origen.get(&l.pedido.linea).copied().unwrap_or_default()),
            )
        })
        .collect();
    let mut zip = zip::ZipArchive::new(Cursor::new(original)).map_err(|_| ErrorCiego::NoXlsx)?;
    let ruta_hoja = ruta_de_hoja(&mut zip, &proforma.hoja)?;
    let mut salida = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        let nombre = f.name().map_err(|e| ErrorCiego::Escritura(e.to_string()))?.into_owned();
        let opciones = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut contenido = Vec::new();
        f.read_to_end(&mut contenido)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        let contenido = if nombre == ruta_hoja {
            escribir_celdas(&contenido, &celdas)?.0
        } else if nombre == "xl/workbook.xml" {
            recalcular_al_abrir(&contenido)?
        } else {
            contenido
        };
        salida
            .start_file(nombre, opciones)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        salida
            .write_all(&contenido)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
    }
    Ok(salida
        .finish()
        .map_err(|e| ErrorCiego::Escritura(e.to_string()))?
        .into_inner())
}

fn f64_de(d: Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64().unwrap_or_default()
}

/// Excel con el detalle de la factura del mes y los avisos.
pub fn detalle_mes(pedido: &PedidoCliente, mes: &Mes) -> Result<Vec<u8>, ErrorCiego> {
    use rust_xlsxwriter::{Format, Workbook};
    let err = |e: rust_xlsxwriter::XlsxError| ErrorCiego::Escritura(e.to_string());
    let mut libro = Workbook::new();
    let negrita = Format::new().set_bold();
    let eur = Format::new().set_num_format("#,##0.00 €");
    let eur_n = Format::new().set_num_format("#,##0.00 €").set_bold();
    let cant = Format::new().set_num_format("#,##0.00");
    let h = libro.add_worksheet();
    h.set_name("Factura del mes").map_err(err)?;
    h.write_string_with_format(
        0,
        0,
        format!("Certificación {} ({}) — pedido {}", mes.numero, mes.mes, pedido.numero),
        &negrita,
    )
    .map_err(err)?;
    let mut f = 1u32;
    if !pedido.cliente.is_empty() || !pedido.proyecto.is_empty() {
        h.write_string(f, 0, format!("{} {}", pedido.cliente, pedido.proyecto).trim())
            .map_err(err)?;
        f += 1;
    }
    f += 1;
    for (i, t) in [
        "Nº línea",
        "Descripción",
        "Cantidad del mes",
        "Precio",
        "Importe",
        "A origen anterior",
        "A origen",
        "Pedido",
    ]
    .iter()
    .enumerate()
    {
        h.write_string_with_format(f, u16::try_from(i).unwrap_or(0), *t, &negrita)
            .map_err(err)?;
    }
    f += 1;
    for l in &mes.lineas {
        h.write_string(f, 0, &l.linea).map_err(err)?;
        let mut d = l.descripcion.clone();
        if let Some(pa) = l.precio_anterior {
            d.push_str(&format!(" (regulariza precio {pa} → {})", l.precio));
        }
        h.write_string(f, 1, d).map_err(err)?;
        h.write_number_with_format(f, 2, f64_de(l.cantidad), &cant)
            .map_err(err)?;
        h.write_number_with_format(f, 3, f64_de(ppto_core::redondear(l.precio, 2)), &eur)
            .map_err(err)?;
        h.write_number_with_format(f, 4, f64_de(l.importe), &eur).map_err(err)?;
        h.write_number_with_format(f, 5, f64_de(l.origen_anterior), &cant)
            .map_err(err)?;
        h.write_number_with_format(f, 6, f64_de(l.origen), &cant).map_err(err)?;
        h.write_number_with_format(f, 7, f64_de(l.cantidad_pedido), &cant)
            .map_err(err)?;
        f += 1;
    }
    if !mes.ajuste_redondeo.is_zero() {
        h.write_string(f, 1, "Ajuste de redondeo (cuadre con el total a origen)")
            .map_err(err)?;
        h.write_number_with_format(f, 4, f64_de(mes.ajuste_redondeo), &eur)
            .map_err(err)?;
        f += 1;
    }
    f += 1;
    for (t, v) in [
        ("TOTAL Pedido", mes.total_pedido),
        ("TOTAL a origen", mes.origen),
        ("A origen anterior", mes.origen_anterior),
        ("TOTAL a facturar este mes", mes.total),
        ("Facturado antes (facturas − abonos)", mes.facturado_antes),
    ] {
        h.write_string_with_format(f, 1, t, &negrita).map_err(err)?;
        h.write_number_with_format(f, 4, f64_de(v), &eur_n).map_err(err)?;
        f += 1;
    }
    h.set_column_width(1, 55).map_err(err)?;
    for c in [2u16, 3, 4, 5, 6, 7] {
        h.set_column_width(c, 15).map_err(err)?;
    }
    let a = libro.add_worksheet();
    a.set_name("Avisos").map_err(err)?;
    if mes.avisos.is_empty() {
        a.write_string(
            0,
            0,
            "Sin avisos: cuadra con la certificación anterior y con el pedido.",
        )
        .map_err(err)?;
    }
    for (i, t) in mes.avisos.iter().enumerate() {
        a.write_string(u32::try_from(i).unwrap_or(0), 0, t).map_err(err)?;
    }
    a.set_column_width(0, 140).map_err(err)?;
    libro.save_to_buffer().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Proforma inventada con la misma estructura que la de un cliente.
    fn proforma(origen: [f64; 3]) -> Vec<u8> {
        let mut l = rust_xlsxwriter::Workbook::new();
        let h = l.add_worksheet();
        h.set_name("Nº PedidoPED-1").unwrap();
        h.write_string(0, 3, "PROFORMA CERTIFICACIÓN A ORIGEN").unwrap();
        h.write_string(1, 1, "Nº Pedido").unwrap();
        h.write_string(1, 2, "PED-1").unwrap();
        h.write_string(2, 1, "Fecha pedido").unwrap();
        h.write_string(2, 2, "01/08/26").unwrap();
        h.write_string(5, 1, "Nº proyecto").unwrap();
        h.write_string(5, 2, "PR-1").unwrap();
        for (i, t) in [
            "Nº Línea",
            "Nº proyecto",
            "Nº tarea proyecto",
            "Descripción",
            "Cantidad",
            "Coste unitario (DL)",
            "Cantidad a facturar a origen",
            "Importe pedido",
            "Importe fra origen",
        ]
        .iter()
        .enumerate()
        {
            h.write_string(7, u16::try_from(i).unwrap(), *t).unwrap();
        }
        for (i, (d, q, p)) in [
            ("Unidad A", 3.0, 1119.38667),
            ("Tubería", 50.0, 23.52018),
            ("Rejilla", 10.0, 93.82),
        ]
        .iter()
        .enumerate()
        {
            let f = 8 + u32::try_from(i).unwrap();
            h.write_string(f, 0, format!("{}0000", i + 1)).unwrap();
            h.write_string(f, 1, "PR-1").unwrap();
            h.write_string(f, 2, format!("0{i}")).unwrap();
            h.write_string(f, 3, *d).unwrap();
            h.write_number(f, 4, *q).unwrap();
            h.write_number(f, 5, *p).unwrap();
            h.write_number(f, 6, origen[i]).unwrap();
            h.write_formula(f, 7, format!("=+E{0}*F{0}", f + 1).as_str()).unwrap();
            h.write_formula(f, 8, format!("=+F{0}*G{0}", f + 1).as_str()).unwrap();
        }
        h.write_string(13, 0, "Usted no debe modificar el formato del presente archivo.")
            .unwrap();
        h.write_string(14, 6, "VºBº 2026-10-29").unwrap();
        l.save_to_buffer().unwrap()
    }

    #[test]
    fn leer_y_rellenar_una_proforma() {
        let b = proforma([3.0, 35.0, 0.0]);
        let p = leer_bytes(&b).unwrap();
        assert_eq!(p.pedido, "PED-1");
        assert_eq!(p.proyecto, "PR-1");
        assert_eq!(p.visto_bueno, "VºBº 2026-10-29");
        assert_eq!(p.lineas.len(), 3);
        assert_eq!(p.lineas[0].pedido.precio, dec!(1119.38667));
        assert_eq!(p.lineas[1].origen, dec!(35));
        assert_eq!(p.lineas[1].celda, "G10");
        // Rellenar: solo cambian las cantidades a origen
        let mut o = p.origen();
        o.insert("20000".into(), dec!(20));
        o.insert("30000".into(), dec!(4));
        let nuevo = rellenar(&b, &p, &o).unwrap();
        let q = leer_bytes(&nuevo).unwrap();
        assert_eq!(q.lineas[1].origen, dec!(20));
        assert_eq!(q.lineas[2].origen, dec!(4));
        assert_eq!(q.lineas[0].origen, dec!(3));
        assert_eq!(q.lineas_pedido(), p.lineas_pedido());
        // Las fórmulas siguen
        let mut z = zip::ZipArchive::new(Cursor::new(&nuevo)).unwrap();
        let mut x = String::new();
        z.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut x)
            .unwrap();
        assert!(x.contains("+F10*G10"));
    }

    #[test]
    fn detalle_del_mes_en_excel() {
        let p1 = leer_bytes(&proforma([0.0, 35.0, 0.0])).unwrap();
        let p2 = leer_bytes(&proforma([3.0, 20.0, 0.0])).unwrap();
        let mut ped = PedidoCliente {
            numero: p1.pedido.clone(),
            lineas: p1.lineas_pedido(),
            ..Default::default()
        };
        ped.fijar_origen("2026-09", p1.origen());
        ped.actualizar_lineas(p2.lineas_pedido());
        ped.fijar_origen("2026-10", p2.origen());
        let m = ped.mes(2).unwrap();
        // 3 × 1119,38667 − 15 × 23,52018
        assert_eq!(m.total, dec!(3005.35));
        assert!(m.avisos.iter().any(|a| a.contains("baja de 35 a 20")));
        let x = detalle_mes(&ped, &m).unwrap();
        let mut l = calamine::open_workbook_auto_from_rs(Cursor::new(x)).unwrap();
        let r = l.worksheet_range("Factura del mes").unwrap();
        let t: Vec<String> = r.rows().flat_map(|f| f.iter().map(texto)).collect();
        assert!(t.iter().any(|s| s == "-15"));
        assert!(t.iter().any(|s| s.contains("TOTAL a facturar")));
    }
}
