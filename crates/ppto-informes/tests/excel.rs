// SPDX-License-Identifier: GPL-3.0-or-later
//! Genera el Excel del ejemplo y lo vuelve a leer para comprobar las cifras.

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};
use ppto_core::ejemplos::suelo_radiante;
use ppto_informes::{OpcionesInforme, excel};
use rust_decimal_macros::dec;
use std::io::Cursor;

type Libro = Xlsx<Cursor<Vec<u8>>>;

fn abrir(bytes: Vec<u8>) -> Libro {
    open_workbook_from_rs(Cursor::new(bytes)).unwrap()
}

/// Valor numérico de la columna `col` en la fila cuya columna `clave_col` es `clave`.
fn numero(libro: &mut Libro, hoja: &str, clave_col: usize, clave: &str, col: usize) -> f64 {
    let r = libro.worksheet_range(hoja).unwrap();
    for fila in r.rows() {
        if matches!(fila.get(clave_col), Some(Data::String(s)) if s == clave) {
            if let Some(Data::Float(v)) = fila.get(col) {
                return *v;
            }
        }
    }
    panic!("no se encontró «{clave}» en la hoja {hoja}");
}

/// Excel guarda dobles: se compara redondeando a 6 decimales, en aritmética decimal.
fn cerca(a: f64, esperado: &str) -> bool {
    use rust_decimal::prelude::FromPrimitive;
    let a = rust_decimal::Decimal::from_f64(a).map(|d| d.round_dp(6));
    a == esperado.parse::<rust_decimal::Decimal>().ok()
}

#[test]
fn excel_del_suelo_radiante() {
    let p = suelo_radiante();
    let mut l = abrir(excel(&p, &OpcionesInforme::default()).unwrap());
    assert_eq!(
        l.sheet_names(),
        vec![
            "Resumen",
            "Presupuesto",
            "Descompuestos",
            "Mediciones",
            "Recursos",
            "Horas por oficio"
        ]
    );
    // Resumen
    assert!(cerca(numero(&mut l, "Resumen", 0, "C01", 2), "7005.15"));
    assert!(cerca(
        numero(&mut l, "Resumen", 1, "Presupuesto de ejecución material (PEM)", 2),
        "7005.15"
    ));
    assert!(cerca(numero(&mut l, "Resumen", 1, "TOTAL", 2), "10086.72"));
    // Presupuesto: cantidad, precio e importe de la partida
    assert!(cerca(numero(&mut l, "Presupuesto", 0, "SR.M2", 3), "210.0"));
    assert!(cerca(numero(&mut l, "Presupuesto", 0, "SR.M2", 4), "29.89"));
    assert!(cerca(numero(&mut l, "Presupuesto", 0, "SR.M2", 5), "6276.90"));
    // Recursos y horas por oficio
    assert!(cerca(numero(&mut l, "Recursos", 0, "MT.TUBO16", 3), "1470.0"));
    assert!(cerca(numero(&mut l, "Horas por oficio", 0, "MO.OF1F", 3), "57.0"));
    assert!(cerca(
        numero(&mut l, "Horas por oficio", 1, "Total mano de obra", 3),
        "112.5"
    ));
    assert!(cerca(
        numero(&mut l, "Horas por oficio", 1, "Total mano de obra", 5),
        "2562.0"
    ));
    // Mediciones: total de la hoja de SR.M2
    let r = l.worksheet_range("Mediciones").unwrap();
    let totales: Vec<f64> = r
        .rows()
        .filter(|f| matches!(f.first(), Some(Data::String(s)) if s == "Total"))
        .filter_map(|f| {
            if let Some(Data::Float(v)) = f.get(6) {
                Some(*v)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(totales.len(), 2);
    assert!(cerca(totales[0], "3") && cerca(totales[1], "210"));
}

#[test]
fn excel_con_costes_indirectos() {
    let mut p = suelo_radiante();
    p.costes_indirectos = dec!(45);
    let mut l = abrir(excel(&p, &OpcionesInforme::default()).unwrap());
    assert!(cerca(numero(&mut l, "Resumen", 1, "Coste directo", 2), "7005.15"));
    assert!(cerca(
        numero(&mut l, "Resumen", 1, "Costes indirectos (45,00 % por partida)", 2),
        "3152.22"
    ));
    assert!(cerca(
        numero(&mut l, "Resumen", 1, "Presupuesto de ejecución material (PEM)", 2),
        "10157.37"
    ));
    // La partida lleva el precio con indirectos; el descompuesto, el coste
    assert!(cerca(numero(&mut l, "Presupuesto", 0, "SR.M2", 4), "43.34"));
    assert!(cerca(numero(&mut l, "Descompuestos", 0, "SR.M2", 6), "29.89"));
}
