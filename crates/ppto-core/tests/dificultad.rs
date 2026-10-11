// SPDX-License-Identifier: GPL-3.0-or-later
//! Coeficiente de dificultad sobre la mano de obra propia (cifras a mano).

use ppto_core::{Naturaleza as N, Presupuesto};
use rust_decimal_macros::dec;

/// - P1 (C01, 10 ud) = MAT 1 × 10 + MO.OF 2 × 25 + AUX 1 × 2,50 + 3 % = 64,38
///   (base 62,50 → 0,625 × 3 = 1,88)
/// - P2 (C01 4 ud y C02 6 ud) = MO.OF 1 × 25 = 25,00
/// - P3 (C02, 5 ud) = SUB 1 × 100 + MO.OF 0,5 × 25 + AUX 1 × 2,50 = 115,00
/// - AUX (auxiliar) = MO.OF 0,1 × 25 = 2,50
///
/// Coste: 643,80 + 250,00 + 575,00 = 1.468,80. Horas MO.OF: 21 + 10 + 3 = 34.
fn obra() -> Presupuesto {
    let mut p = Presupuesto::new("D", "OBRA", "Obra");
    p.nuevo_capitulo("OBRA", "C01", "Planta sótano").unwrap();
    p.nuevo_capitulo("OBRA", "C02", "Plantas").unwrap();
    p.nuevo_recurso("MO.OF", "h", "Oficial", N::ManoObra, dec!(25)).unwrap();
    p.nuevo_recurso("MAT", "ud", "Material", N::Material, dec!(10)).unwrap();
    p.nuevo_recurso("SUB", "ud", "Subcontrata", N::Subcontrata, dec!(100))
        .unwrap();
    p.nuevo_recurso("%MA", "%", "Medios auxiliares", N::Porcentaje, dec!(0))
        .unwrap();
    p.nuevo_recurso("AUX", "ud", "Auxiliar", N::Partida, dec!(0)).unwrap();
    p.anadir_linea("AUX", "MO.OF", dec!(0.1)).unwrap();
    p.nueva_partida("C01", "P1", "ud", "P1").unwrap();
    for (h, r) in [("MAT", dec!(1)), ("MO.OF", dec!(2)), ("AUX", dec!(1)), ("%MA", dec!(3))] {
        p.anadir_linea("P1", h, r).unwrap();
    }
    p.fijar_rendimiento("C01", "P1", dec!(10)).unwrap();
    p.nueva_partida("C01", "P2", "ud", "P2").unwrap();
    p.anadir_linea("P2", "MO.OF", dec!(1)).unwrap();
    p.fijar_rendimiento("C01", "P2", dec!(4)).unwrap();
    p.anadir_linea("C02", "P2", dec!(6)).unwrap();
    p.nueva_partida("C02", "P3", "ud", "P3").unwrap();
    for (h, r) in [("SUB", dec!(1)), ("MO.OF", dec!(0.5)), ("AUX", dec!(1))] {
        p.anadir_linea("P3", h, r).unwrap();
    }
    p.fijar_rendimiento("C02", "P3", dec!(5)).unwrap();
    p
}

#[test]
fn toda_la_obra() {
    let mut p = obra();
    assert_eq!(p.precio("P1").unwrap(), dec!(64.38));
    assert_eq!(p.coste_directo().unwrap(), dec!(1468.80));
    let inf = p.aplicar_dificultad("OBRA", dec!(1.2)).unwrap();
    assert_eq!(inf.total_antes(), dec!(34));
    assert_eq!(inf.total_despues(), dec!(40.8), "34 × 1,20");
    assert!(inf.omitidas.is_empty());
    assert_eq!(inf.modificadas, ["AUX", "P1", "P2", "P3"]);
    // P1 = 10 + 2,4 × 25 + 0,12 × 25 + (73 → 0,73 × 3 = 2,19) = 75,19
    assert_eq!(p.precio("P1").unwrap(), dec!(75.19));
    // La subcontrata y el material no cambian
    let r = |c: &str, h: &str| {
        p.concepto(c)
            .unwrap()
            .descomposicion
            .iter()
            .find(|l| l.hijo == h)
            .unwrap()
            .rendimiento
    };
    assert_eq!(r("P3", "SUB"), dec!(1));
    assert_eq!(r("P1", "MAT"), dec!(1));
    assert_eq!(r("P1", "%MA"), dec!(3));
    assert_eq!(r("AUX", "MO.OF"), dec!(0.12));
    assert_eq!(
        p.concepto("MO.OF").unwrap().precio,
        dec!(25),
        "el precio de la hora no cambia"
    );
}

#[test]
fn un_capitulo_sin_tocar_lo_compartido() {
    let mut p = obra();
    let inf = p.aplicar_dificultad("C01", dec!(1.2)).unwrap();
    // P2 está también en C02 y AUX lo usa P3 (C02): no se tocan
    assert_eq!(inf.omitidas, ["AUX", "P2"]);
    assert_eq!(inf.modificadas, ["P1"]);
    // P1 = 10 + 60 + 2,50 + (72,50 → 0,725 × 3 = 2,175 → 2,18) = 74,68
    assert_eq!(p.precio("P1").unwrap(), dec!(74.68));
    assert_eq!(p.precio("P2").unwrap(), dec!(25));
    // Horas: P1 10 × 2,5 = 25; P2 10; P3 3 → 38
    assert_eq!(inf.total_despues(), dec!(38));
    assert_eq!(inf.coste_despues, dec!(1571.80));

    // Una partida suelta sí se cambia aunque esté en dos capítulos
    let mut p = obra();
    p.aplicar_dificultad("P2", dec!(1.1)).unwrap();
    assert_eq!(p.precio("P2").unwrap(), dec!(27.50));
}

#[test]
fn errores_sin_tocar_el_presupuesto() {
    let mut p = obra();
    let antes = p.clone();
    assert!(p.aplicar_dificultad("OBRA", dec!(0)).is_err());
    assert!(p.aplicar_dificultad("OBRA", dec!(1)).is_err());
    assert!(p.aplicar_dificultad("MAT", dec!(1.2)).is_err());
    assert!(p.aplicar_dificultad("NADA", dec!(1.2)).is_err());
    assert_eq!(p, antes);
    // C02 solo tiene P2 (compartida con C01) y P3 → P3 sí cambia; pero un capítulo
    // vacío de mano de obra propia da error
    p.nuevo_capitulo("OBRA", "C03", "Vacío").unwrap();
    let antes_c03 = p.clone();
    assert!(p.aplicar_dificultad("C03", dec!(1.2)).is_err());
    assert_eq!(p, antes_c03);
}
