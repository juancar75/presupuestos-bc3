// SPDX-License-Identifier: GPL-3.0-or-later
//! Revit (`.rvt`) sin Revit, con [rvt-rs](https://github.com/DrunkOnJava/rvt-rs)
//! (Apache-2.0, lectura de ingeniería limpia del formato).
//!
//! rvt-rs da de cada elemento su clase IFC, nombre (familia:tipo), nivel y
//! la caja que ocupa. No da parámetros (sistema, diámetro, longitud), así
//! que la longitud y la medida de los tramos salen de esa caja:
//!
//! - Tramo alineado con los ejes: longitud = lado mayor; sección = los otros
//!   dos (Ø si son iguales).
//! - Tubería inclinada: longitud aproximada (diagonal de la caja menos el
//!   diámetro). Se marca como aproximada.
//!
//! Funciona mejor con modelos de Revit 2023–2025.

use crate::{Clase, Elemento, ErrorModelo, Modelo, limpiar_tipo, medida_mm};
use ppto_core::{Decimal, redondear};
use rvt::ifc::schedule_csv::{CsvOptions, LengthUnit, elements_csv};
use std::path::Path;
use std::str::FromStr;

/// Lee un `.rvt`.
pub fn leer(ruta: &Path) -> Result<Modelo, ErrorModelo> {
    let mut f = rvt::RevitFile::open(ruta).map_err(|e| ErrorModelo::Revit(e.to_string()))?;
    let version = f.basic_file_info().ok().map(|b| b.version);
    let model = rvt::ifc::RvtDocExporter
        .export_with_diagnostics_mode_and_limits(
            &mut f,
            rvt::ifc::ExportQualityMode::Scaffold,
            rvt::walker::WalkerLimits::default(),
        )
        .map_err(|e| ErrorModelo::Revit(e.to_string()))?
        .model;
    let csv = elements_csv(
        &model,
        &CsvOptions {
            unit: LengthUnit::Metres,
            excel_bom: false,
        },
    );
    let mut m = desde_csv(&csv)?;
    m.nombre = ruta
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(v) = version
        && !(2023..=2025).contains(&v)
    {
        m.avisos.push(format!(
            "Modelo de Revit {v}: la lectura directa es fiable en 2023–2025. Compare con un IFC exportado."
        ));
    }
    m.avisos.push(
        "Leído directamente del .rvt: sin sistemas ni parámetros; longitudes y diámetros sacados de la geometría."
            .into(),
    );
    Ok(m)
}

/// Campos de una línea CSV (RFC 4180).
fn campos(linea: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut actual = String::new();
    let mut comillas = false;
    let mut cs = linea.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '"' if comillas && cs.peek() == Some(&'"') => {
                actual.push('"');
                cs.next();
            }
            '"' => comillas = !comillas,
            ',' if !comillas => out.push(std::mem::take(&mut actual)),
            c => actual.push(c),
        }
    }
    out.push(actual);
    out
}

/// Elementos desde el listado CSV de rvt-rs (columnas `ifc_type`, `name`,
/// `level`, `width_m`, `depth_m`, `height_m`).
pub fn desde_csv(csv: &str) -> Result<Modelo, ErrorModelo> {
    let mut lineas = csv.trim_start_matches('\u{feff}').lines();
    let cab = campos(lineas.next().unwrap_or_default());
    let col = |n: &str| cab.iter().position(|c| c == n);
    let (Some(ct), Some(cn)) = (col("ifc_type"), col("name")) else {
        return Err(ErrorModelo::Revit(
            "listado de elementos sin columnas ifc_type/name".into(),
        ));
    };
    let cl = col("level");
    let dims = [col("width_m"), col("depth_m"), col("height_m")];
    let mut m = Modelo::default();
    let mut sin_medida = 0usize;
    let mut dudosos: Vec<(usize, Decimal, Decimal)> = Vec::new();
    for l in lineas {
        if l.trim().is_empty() {
            continue;
        }
        let f = campos(l);
        let g = |i: Option<usize>| i.and_then(|i| f.get(i)).map(|s| s.trim()).unwrap_or_default();
        let ifc_type = g(Some(ct));
        let Some(clase) = Clase::de_ifc(ifc_type) else {
            if ifc_type != "IFCSPACE" {
                m.ignorados += 1;
            }
            continue;
        };
        let mut d: Vec<Decimal> = dims
            .iter()
            .filter_map(|c| Decimal::from_str(g(*c)).ok())
            .filter(|x| *x > Decimal::ZERO)
            .collect();
        d.sort();
        let (mut longitud, mut medida, mut aproximada) = (None, String::new(), false);
        if clase.lineal() {
            if let [a, b, c] = d[..] {
                if clase == Clase::Tuberia {
                    // Sección: la pareja de lados iguales (también en
                    // trozos más cortos que su diámetro)
                    if medida_mm(a, b).starts_with('Ø') {
                        medida = medida_mm(a, b);
                        longitud = Some(c);
                    } else if medida_mm(b, c).starts_with('Ø') {
                        // Trozo corto (Ø = b) o tubería a 45° en un plano
                        // (Ø = a): se decide al final con los diámetros
                        // seguros del modelo
                        dudosos.push((m.elementos.len(), a, b));
                        medida = medida_mm(b, c);
                        longitud = Some(a);
                    } else {
                        // Tubería inclinada: la caja crece en dos ejes
                        let (da, db) = (b - a, c - a);
                        longitud = rust_decimal::MathematicalOps::sqrt(&(da * da + db * db));
                        medida = medida_mm(a, a);
                        aproximada = true;
                    }
                } else {
                    medida = medida_mm(a, b);
                    longitud = Some(c);
                }
            } else {
                sin_medida += 1;
            }
        }
        m.elementos.push(Elemento {
            clase,
            tipo: limpiar_tipo(g(Some(cn))),
            medida,
            planta: g(cl).to_owned(),
            sistema: String::new(),
            longitud: longitud.map(|x| redondear(x, 3)),
            aproximada,
        });
    }
    // Diámetros seguros: tramos alineados
    let seguros: std::collections::BTreeSet<String> = m
        .elementos
        .iter()
        .enumerate()
        .filter(|(i, e)| e.clase == Clase::Tuberia && !e.aproximada && !dudosos.iter().any(|d| d.0 == *i))
        .map(|(_, e)| e.medida.clone())
        .collect();
    for (i, a, b) in dudosos {
        let corto = medida_mm(b, b);
        let inclinado = medida_mm(a, a);
        if !seguros.contains(&corto) && seguros.contains(&inclinado) {
            let e = &mut m.elementos[i];
            e.medida = inclinado;
            let d = b - a;
            e.longitud = rust_decimal::MathematicalOps::sqrt(&(d * d * Decimal::TWO)).map(|x| redondear(x, 3));
            e.aproximada = true;
        }
    }
    agrupar_diametros(&mut m.elementos);
    if sin_medida > 0 {
        m.avisos
            .push(format!("{sin_medida} tramo(s) sin geometría: no tienen longitud."));
    }
    Ok(m)
}

/// La geometría da diámetros con ruido (Ø37, Ø39 junto a 400 tramos de
/// Ø40). Cada diámetro poco frecuente se junta con el más frecuente que
/// esté a menos de un 10 %.
fn agrupar_diametros(elementos: &mut [Elemento]) {
    let mut cuenta: std::collections::BTreeMap<Decimal, usize> = std::collections::BTreeMap::new();
    let diam = |e: &Elemento| -> Option<Decimal> {
        (e.clase == Clase::Tuberia)
            .then(|| e.medida.strip_prefix('Ø'))
            .flatten()
            .and_then(|x| Decimal::from_str(x).ok())
    };
    for e in elementos.iter() {
        if let Some(d) = diam(e) {
            *cuenta.entry(d).or_default() += 1;
        }
    }
    let mut orden: Vec<(Decimal, usize)> = cuenta.into_iter().collect();
    orden.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut representantes: Vec<Decimal> = Vec::new();
    let mut destino: std::collections::BTreeMap<Decimal, Decimal> = std::collections::BTreeMap::new();
    for (d, _) in orden {
        let r = representantes
            .iter()
            .find(|r| (**r - d).abs() * Decimal::from(10) <= **r)
            .copied();
        match r {
            Some(r) => {
                destino.insert(d, r);
            }
            None => {
                representantes.push(d);
                destino.insert(d, d);
            }
        }
    }
    for e in elementos.iter_mut() {
        if let Some(d) = diam(e)
            && let Some(r) = destino.get(&d)
            && *r != d
        {
            e.medida = format!("Ø{r}");
            e.aproximada = true;
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn listado_de_rvt() {
        let csv = "revit_element_id,ifc_type,predefined_type,name,level,level_elevation_m,material,x_m,y_m,z_m,rotation_deg,width_m,depth_m,height_m,host_revit_element_id,body_source,profile_resolved\n\
1,IFCPIPESEGMENT,,Pipe-1,Planta 1,0,,0,0,0,,0.8706,0.032,0.032,,bbox,false\n\
2,IFCPIPESEGMENT,,Pipe-2,Planta 1,0,,0,0,0,,0.04,0.3,0.4,,bbox,false\n\
3,IFCPIPEFITTING,,\"Codo, 90:UT_1:3\",Planta 1,0,,0,0,0,,0.07,0.07,0.05,,bbox,false\n\
4,IFCDUCTSEGMENT,,Duct-4,,,,0,0,0,,2.5,0.3,0.2,,bbox,false\n\
5,IFCWALL,,Muro,,,,0,0,0,,1,1,1,,bbox,false\n\
6,IFCSPACE,,Sala,,,,,,,,,,,,,\n";
        let m = desde_csv(csv).unwrap();
        assert_eq!(m.elementos.len(), 4);
        assert_eq!(m.ignorados, 1);
        let t = &m.elementos[0];
        assert_eq!(
            (t.medida.as_str(), t.longitud, t.aproximada),
            ("Ø32", Some(dec!(0.871)), false)
        );
        assert_eq!(t.tipo, "");
        let i = &m.elementos[1];
        assert_eq!(i.medida, "Ø40");
        assert!(i.aproximada);
        // √(0.26² + 0.36²) = 0.444
        assert_eq!(i.longitud, Some(dec!(0.444)));
        assert_eq!(m.elementos[2].tipo, "Codo, 90:UT_1");
        assert_eq!(m.elementos[2].clase, Clase::AccesorioTuberia);
        let c = &m.elementos[3];
        assert_eq!((c.medida.as_str(), c.longitud), ("300x200", Some(dec!(2.5))));
    }
}
