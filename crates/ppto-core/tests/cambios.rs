// SPDX-License-Identifier: GPL-3.0-or-later
//! Propuestas de cambios de la obra (quitar, cantidad, textos, mover): se
//! aplican todas o ninguna y solo al aceptarlas.

use ppto_core::LineaMedicion;
use ppto_core::ejemplos::suelo_radiante;
use ppto_core::propuesta::{Cambio, Confianza, Contenido, EstadoPropuesta, Propuesta};
use rust_decimal_macros::dec;

fn cambios(c: Vec<Cambio>) -> Propuesta {
    Propuesta {
        id: 1,
        partida: "cambios".into(),
        contenido: Contenido::Cambios(c),
        confianza: Confianza::Media,
        justificacion: "prueba".into(),
        autor: "IA".into(),
        estado: EstadoPropuesta::Pendiente,
        descuentos: Default::default(),
        nueva: None,
    }
}

fn cantidad_en(p: &ppto_core::Presupuesto, cap: &str, partida: &str) -> rust_decimal::Decimal {
    p.concepto(cap)
        .unwrap()
        .descomposicion
        .iter()
        .find(|l| l.hijo == partida)
        .map(|l| l.cantidad())
        .unwrap()
}

/// Obra de ejemplo con un capítulo C02 y dos partidas de precio directo.
fn obra() -> ppto_core::Presupuesto {
    let mut p = suelo_radiante();
    let raiz = p.raiz.clone();
    p.nuevo_capitulo(&raiz, "C02", "Varios").unwrap();
    p.nueva_partida("C02", "P.INERCIA", "ud", "Depósito de inercia")
        .unwrap();
    p.fijar_precio("P.INERCIA", dec!(500)).unwrap();
    p.nueva_partida("C02", "P.LINEA", "m", "Línea frigorífica").unwrap();
    p.fijar_precio("P.LINEA", dec!(10)).unwrap();
    p
}

#[test]
fn quitar_cantidad_texto_y_mover_en_una_propuesta() {
    let mut p = obra();
    let pem = p.pem().unwrap();
    let prop = cambios(vec![
        Cambio::Borrar {
            capitulo: String::new(),
            partida: "P.INERCIA".into(),
        },
        Cambio::Cantidad {
            capitulo: "C02".into(),
            partida: "P.LINEA".into(),
            cantidad: dec!(49.5),
            medicion: vec![],
        },
        Cambio::Texto {
            partida: "P.LINEA".into(),
            resumen: Some("Línea frigorífica 3 tubos".into()),
            texto: Some("Cobre 3/8-5/8-3/4 aislado.".into()),
            unidad: None,
        },
        Cambio::Mover {
            de: String::new(),
            partida: "P.LINEA".into(),
            a: "C01".into(),
            antes_de: Some("SR.COL8".into()),
        },
    ]);
    // Simular no cambia nada
    let sim = p.simular_propuesta(&prop).unwrap();
    assert_eq!(p.pem().unwrap(), pem);
    assert!(p.conceptos.contains_key("P.INERCIA"));
    let a = p.aplicar_propuesta(&prop).unwrap();
    assert_eq!(a, sim);
    // PEM: −500 × 1 (inercia) + 10 × (49,5 − 1) (línea) = −15
    assert_eq!(a.pem_antes, pem);
    assert_eq!(a.pem_despues, pem - dec!(500) + dec!(485));
    assert_eq!(p.pem().unwrap(), a.pem_despues);
    assert_eq!(a.cambios.len(), 4);
    assert!(!p.conceptos.contains_key("P.INERCIA"), "se purga al quedar sin usar");
    assert_eq!(p.concepto("P.LINEA").unwrap().resumen, "Línea frigorífica 3 tubos");
    let c01: Vec<_> = p
        .concepto("C01")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .collect();
    assert_eq!(c01, ["SR.M2", "P.LINEA", "SR.COL8"]);
    assert_eq!(cantidad_en(&p, "C01", "P.LINEA"), dec!(49.5));
    assert!(p.concepto("C02").unwrap().descomposicion.is_empty());
}

#[test]
fn cantidad_con_medicion_y_sin_ella() {
    let mut p = obra();
    // 10 m cubierta + 19,5 m patinillo + 20 m falso techo = 49,5 m
    let n = |c: &str, l| LineaMedicion::normal(c, Some(dec!(1)), Some(l), None, None);
    let prop = cambios(vec![Cambio::Cantidad {
        capitulo: "C02".into(),
        partida: "P.LINEA".into(),
        cantidad: dec!(49.5),
        medicion: vec![
            n("Cubierta", dec!(10)),
            n("Patinillo", dec!(19.5)),
            n("Falso techo", dec!(20)),
        ],
    }]);
    p.aplicar_propuesta(&prop).unwrap();
    assert_eq!(cantidad_en(&p, "C02", "P.LINEA"), dec!(49.5));
    assert_eq!(p.mediciones[&("C02".into(), "P.LINEA".into())].lineas.len(), 3);
    // SR.M2 tiene medición en C01: una cantidad sin líneas la sustituye y quita la hoja
    let prop = cambios(vec![Cambio::Cantidad {
        capitulo: "C01".into(),
        partida: "SR.M2".into(),
        cantidad: dec!(100),
        medicion: vec![],
    }]);
    p.aplicar_propuesta(&prop).unwrap();
    assert_eq!(cantidad_en(&p, "C01", "SR.M2"), dec!(100));
    assert!(!p.mediciones.contains_key(&("C01".into(), "SR.M2".into())));
}

#[test]
fn si_un_cambio_falla_no_se_aplica_ninguno() {
    let p0 = obra();
    let malos = [
        // El primero vale; el segundo no existe
        vec![
            Cambio::Borrar {
                capitulo: String::new(),
                partida: "P.INERCIA".into(),
            },
            Cambio::Borrar {
                capitulo: String::new(),
                partida: "NO.EXISTE".into(),
            },
        ],
        // Capítulo equivocado
        vec![Cambio::Borrar {
            capitulo: "C01".into(),
            partida: "P.INERCIA".into(),
        }],
        // La medición no suma la cantidad indicada
        vec![Cambio::Cantidad {
            capitulo: "C02".into(),
            partida: "P.LINEA".into(),
            cantidad: dec!(50),
            medicion: vec![LineaMedicion::normal("x", Some(dec!(2)), Some(dec!(10)), None, None)],
        }],
        vec![Cambio::Cantidad {
            capitulo: String::new(),
            partida: "P.LINEA".into(),
            cantidad: dec!(-1),
            medicion: vec![],
        }],
        // Texto sin nada que cambiar, resumen vacío
        vec![Cambio::Texto {
            partida: "P.LINEA".into(),
            resumen: None,
            texto: None,
            unidad: None,
        }],
        vec![Cambio::Texto {
            partida: "P.LINEA".into(),
            resumen: Some("  ".into()),
            texto: None,
            unidad: None,
        }],
        // Mover a algo que no es capítulo, la raíz no se borra, lista vacía
        vec![Cambio::Mover {
            de: String::new(),
            partida: "P.LINEA".into(),
            a: "SR.M2".into(),
            antes_de: None,
        }],
        vec![Cambio::Borrar {
            capitulo: String::new(),
            partida: p0.raiz.clone(),
        }],
        vec![],
    ];
    for m in malos {
        let mut p = p0.clone();
        let prop = cambios(m.clone());
        assert!(p.aplicar_propuesta(&prop).is_err(), "{m:?}");
        assert_eq!(p.pem().unwrap(), p0.pem().unwrap(), "{m:?}");
        assert!(p.conceptos.contains_key("P.INERCIA"), "{m:?}");
        assert_eq!(cantidad_en(&p, "C02", "P.LINEA"), dec!(1), "{m:?}");
    }
}

#[test]
fn partida_en_dos_capitulos_pide_el_capitulo() {
    let mut p = obra();
    p.copiar_linea("C02", "P.LINEA", "C01").unwrap();
    let sin = cambios(vec![Cambio::Borrar {
        capitulo: String::new(),
        partida: "P.LINEA".into(),
    }]);
    let e = p.simular_propuesta(&sin).unwrap_err().to_string();
    assert!(e.contains("indique el capítulo"), "{e}");
    let con = cambios(vec![Cambio::Borrar {
        capitulo: "C01".into(),
        partida: "P.LINEA".into(),
    }]);
    p.aplicar_propuesta(&con).unwrap();
    // Sigue en C02: no se purga
    assert!(p.conceptos.contains_key("P.LINEA"));
    assert_eq!(p.padres("P.LINEA"), ["C02"]);
}

#[test]
fn las_propuestas_de_cambios_se_guardan_y_leen() {
    let prop = cambios(vec![Cambio::Texto {
        partida: "P.LINEA".into(),
        resumen: Some("x".into()),
        texto: None,
        unidad: Some("m".into()),
    }]);
    let json = serde_json::to_string(&prop).unwrap();
    let leida: Propuesta = serde_json::from_str(&json).unwrap();
    assert_eq!(leida, prop);
}
