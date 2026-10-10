// SPDX-License-Identifier: GPL-3.0-or-later
//! Edición estructural (P-014): crear, añadir, quitar, ordenar y borrar,
//! con cifras calculadas a mano.

use ppto_core::ejemplos::suelo_radiante;
use ppto_core::{Naturaleza, Presupuesto};
use rust_decimal_macros::dec;

#[test]
fn presupuesto_desde_cero() {
    // Obra nueva: C01 con una partida de 2 recursos y un 3 % de medios auxiliares.
    // P1 = 0,5 h × 20,00 + 2 m × 3,10 = 10,00 + 6,20 = 16,20; 3 % de 16,20 = 0,486 → 0,49
    // (Presto: 16,20/100 = 0,162 × 3 = 0,486 → 0,49). P1 = 16,69 €/m.
    // Medición 12 m → importe 200,28 €.
    let mut p = Presupuesto::new("Nueva", "OBRA", "Obra nueva");
    p.nuevo_capitulo("OBRA", "C01", "Fontanería").unwrap();
    p.nueva_partida("C01", "P1", "m", "Tubería PPR 20 mm").unwrap();
    p.nuevo_recurso("MO1", "h", "Oficial fontanero", Naturaleza::ManoObra, dec!(20))
        .unwrap();
    p.nuevo_recurso("MT1", "m", "Tubo PPR 20", Naturaleza::Material, dec!(3.10))
        .unwrap();
    p.nuevo_recurso("%MA", "%", "Medios auxiliares", Naturaleza::Porcentaje, dec!(0))
        .unwrap();
    p.anadir_linea("P1", "MO1", dec!(0.5)).unwrap();
    p.anadir_linea("P1", "MT1", dec!(2)).unwrap();
    p.anadir_linea("P1", "%MA", dec!(3)).unwrap();
    p.fijar_rendimiento("C01", "P1", dec!(12)).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(16.69));
    assert_eq!(p.pem().unwrap(), dec!(200.28));

    // Reordenar: %MA arriba del todo no tiene base → P1 = 16,20
    p.mover_linea("P1", "%MA", true).unwrap();
    p.mover_linea("P1", "%MA", true).unwrap();
    p.mover_linea("P1", "%MA", true).unwrap(); // ya es la primera: no hace nada
    assert_eq!(p.concepto("P1").unwrap().descomposicion[0].hijo, "%MA");
    assert_eq!(p.precio("P1").unwrap(), dec!(16.20));
    p.mover_linea("P1", "%MA", false).unwrap();
    p.mover_linea("P1", "%MA", false).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(16.69));
    assert_eq!(p.codigo_libre("C", 2), "C02");
}

#[test]
fn reglas_que_protegen_el_presupuesto() {
    let mut p = suelo_radiante();
    let antes = p.clone();
    let err = |r: Result<(), ppto_core::ErrorMotor>| r.unwrap_err().to_string();
    // Partida dentro de sí misma (directa o indirecta) → circular, y no se toca nada
    assert!(err(p.anadir_linea("SR.M2", "SR.M2", dec!(1))).contains("circular"));
    // Recurso en un capítulo, capítulo en una partida, recurso descompuesto
    assert!(err(p.anadir_linea("C01", "MO.OF1F", dec!(1))).contains("solo puede haber"));
    assert!(err(p.anadir_linea("SR.M2", "C01", dec!(1))).contains("no puede formar parte"));
    assert!(err(p.anadir_linea("MO.OF1F", "SR.M2", dec!(1))).contains("no se descompone"));
    // Repetido, código duplicado o con caracteres reservados de BC3
    assert!(err(p.anadir_linea("C01", "SR.M2", dec!(1))).contains("ya está"));
    assert!(err(p.nueva_partida("C01", "SR.M2", "m2", "x")).contains("duplicado"));
    assert!(err(p.nueva_partida("C01", "A|B", "m2", "x")).contains("reservado"));
    assert!(err(p.nueva_partida("C01", " ", "m2", "x")).contains("vacío"));
    assert!(err(p.nueva_partida("SR.M2", "NUEVA", "m2", "x")).contains("no es un capítulo"));
    // Borrar algo en uso o la raíz
    assert!(err(p.borrar_concepto("MO.OF1F")).contains("se usa en"));
    assert!(err(p.borrar_concepto(&p.raiz.clone())).contains("raíz"));
    assert_eq!(p, antes);
}

#[test]
fn quitar_y_purgar() {
    // Quitar SR.COL8 (3 × 242,75 = 728,25) de C01: PEM 7.005,15 → 6.276,90.
    let mut p = suelo_radiante();
    let total = p.conceptos.len();
    let borrados = p.quitar_y_purgar("C01", "SR.COL8").unwrap();
    assert_eq!(p.pem().unwrap(), dec!(6276.90));
    // Se borra la partida y los recursos que solo usaba ella; los compartidos quedan
    assert!(borrados.contains(&"SR.COL8".to_owned()));
    for b in &borrados {
        assert!(!p.conceptos.contains_key(b));
        assert!(p.padres(b).is_empty());
    }
    assert_eq!(p.conceptos.len(), total - borrados.len());
    assert!(p.conceptos.contains_key("MO.OF1F"), "lo usa SR.M2");
    assert!(!p.mediciones.contains_key(&("C01".into(), "SR.COL8".into())));
    p.validar().unwrap();

    // quitar_linea no borra el concepto: se puede volver a colgar
    let mut q = suelo_radiante();
    q.quitar_linea("C01", "SR.COL8").unwrap();
    assert!(q.conceptos.contains_key("SR.COL8"));
    q.anadir_linea("C01", "SR.COL8", dec!(3)).unwrap();
    assert_eq!(q.pem().unwrap(), dec!(7005.15));
}
