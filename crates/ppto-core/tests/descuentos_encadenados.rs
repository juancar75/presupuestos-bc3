// SPDX-License-Identifier: GPL-3.0-or-later
//! Descuentos encadenados de fabricante («40+8», hasta tres escalones).

use ppto_core::Presupuesto;
use ppto_core::concepto::{Concepto, Naturaleza};
use ppto_core::descuentos::{equivalente, leer_cadena, texto_cadena};
use rust_decimal_macros::dec;

#[test]
fn leer_y_equivalente() {
    assert_eq!(leer_cadena("45").unwrap(), [dec!(45)]);
    assert_eq!(leer_cadena("40+8").unwrap(), [dec!(40), dec!(8)]);
    assert_eq!(
        leer_cadena(" 50 % + 10 % + 5 %").unwrap(),
        [dec!(50), dec!(10), dec!(5)]
    );
    assert_eq!(leer_cadena("65+7,5").unwrap(), [dec!(65), dec!(7.5)]);
    assert!(leer_cadena("40+8+5+2").is_err(), "como mucho tres");
    assert!(leer_cadena("100").is_err());
    assert!(leer_cadena("40+").is_err());
    assert!(leer_cadena("").is_err());

    assert_eq!(equivalente(&[dec!(40), dec!(8)]), dec!(44.8));
    assert_eq!(equivalente(&[dec!(50), dec!(10)]), dec!(55));
    assert_eq!(equivalente(&[dec!(50), dec!(10), dec!(5)]), dec!(57.25));
    assert_eq!(equivalente(&[dec!(45)]), dec!(45));
    assert_eq!(texto_cadena(&[dec!(40), dec!(8)]), "40+8");
}

fn obra() -> Presupuesto {
    let mut p = Presupuesto::new("dto", "OBRA", "Obra");
    p.insertar(Concepto::basico("M1", "ud", "Válvula", Naturaleza::Material, dec!(0)))
        .unwrap();
    p.fijar_tarifa("M1", "Faba", dec!(100)).unwrap();
    p
}

#[test]
fn neto_con_descuento_encadenado() {
    let mut p = obra();
    let r = p.fijar_descuento_texto("faba", "40+8").unwrap();
    assert_eq!(r, ["M1"]);
    // 100 × 0,60 × 0,92
    assert_eq!(p.concepto("M1").unwrap().precio, dec!(55.20));
    assert_eq!(p.descuentos.get("FABA"), Some(&dec!(44.8)));
    assert_eq!(p.texto_descuento("FABA"), "40+8");
    let f = &p.fabricantes()[0];
    assert_eq!((f.descuento, f.cadena.clone()), (dec!(44.8), vec![dec!(40), dec!(8)]));

    // Tres escalones (p. ej. FABB)
    p.fijar_descuento_encadenado("FABA", &[dec!(50), dec!(10), dec!(5)])
        .unwrap();
    assert_eq!(p.concepto("M1").unwrap().precio, dec!(42.75));
    assert_eq!(p.texto_descuento("FABA"), "50+10+5");

    // Un descuento simple borra la cadena
    p.fijar_descuento("FABA", dec!(45)).unwrap();
    assert!(p.descuentos_encadenados.is_empty());
    assert_eq!(p.texto_descuento("FABA"), "45");

    // Quitarlo borra también la cadena
    p.fijar_descuento_texto("FABA", "40+8").unwrap();
    p.quitar_descuento("FABA");
    assert!(p.descuentos_encadenados.is_empty());
    assert_eq!(p.concepto("M1").unwrap().precio, dec!(100));
}

#[test]
fn la_cadena_se_guarda_en_json() {
    let mut p = obra();
    p.fijar_descuento_texto("FABA/G1", "40+8").unwrap();
    let j = serde_json::to_string(&p).unwrap();
    let q: Presupuesto = serde_json::from_str(&j).unwrap();
    assert_eq!(q.texto_descuento("FABA/G1"), "40+8");
    // Un JSON antiguo (sin el campo) sigue leyéndose
    let mut v: serde_json::Value = serde_json::from_str(&j).unwrap();
    v.as_object_mut().unwrap().remove("descuentos_encadenados");
    let q: Presupuesto = serde_json::from_value(v).unwrap();
    assert_eq!(q.texto_descuento("FABA/G1"), "44.8");
}
